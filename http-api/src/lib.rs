mod bindings {
    wit_bindgen::generate!({
        path: "../wit",
        world: "http-api",
        generate_all,
    });
}

use bindings::wasmcloud::messaging::consumer;
use serde::{Deserialize, Serialize};
use wstd::{
    http::{Body, Client, Request, Response, StatusCode},
    time::Duration,
};

static UI_HTML: &str = include_str!("../ui.html");

const STAC_BASE: &str = "https://earth-search.aws.element84.com/v1";

#[wstd::http_server]
async fn main(req: Request<Body>) -> anyhow::Result<Response<Body>> {
    let method = req.method().as_str().to_string();
    let path = req.uri().path().to_string();

    match (method.as_str(), path.as_str()) {
        (_, "/") => serve_ui().await,
        ("POST", "/api/stac/search") => stac_search(req).await,
        ("POST", "/api/process") => process_insar(req).await,
        ("GET", p) if p.starts_with("/api/sites") => known_sites().await,
        _ => Response::builder()
            .status(StatusCode::NOT_FOUND)
            .body("Not found\n".into())
            .map_err(Into::into),
    }
}

async fn serve_ui() -> anyhow::Result<Response<Body>> {
    Response::builder()
        .status(StatusCode::OK)
        .header("Content-Type", "text/html")
        .body(UI_HTML.into())
        .map_err(Into::into)
}

// ── STAC Search: proxy to Earth Search API ──

#[derive(Deserialize)]
struct StacSearchRequest {
    bbox: [f64; 4],
    datetime: String,
    collections: Option<Vec<String>>,
    limit: Option<u32>,
}

async fn stac_search(mut req: Request<Body>) -> anyhow::Result<Response<Body>> {
    let search_req: StacSearchRequest = req
        .body_mut()
        .json()
        .await
        .map_err(|e| anyhow::anyhow!("invalid request: {e}"))?;

    let collections = search_req
        .collections
        .unwrap_or_else(|| vec!["sentinel-1-grd".to_string()]);

    // Ensure datetime is RFC3339 compliant
    let datetime = normalize_datetime(&search_req.datetime);

    let stac_body = serde_json::json!({
        "collections": collections,
        "bbox": search_req.bbox,
        "datetime": datetime,
        "limit": search_req.limit.unwrap_or(50),
    });

    let stac_url = format!("{STAC_BASE}/search");
    let outgoing_req = Request::builder()
        .method("POST")
        .uri(&stac_url)
        .header("Content-Type", "application/json")
        .header("Accept", "application/geo+json")
        .body(Body::from_json(&stac_body)?)
        .map_err(|e| anyhow::anyhow!("build request: {e}"))?;

    let client = Client::new();
    let mut resp = client.send(outgoing_req).await
        .map_err(|e| anyhow::anyhow!("STAC request failed: {e}"))?;

    let body_bytes = resp.body_mut().contents().await
        .map_err(|e| anyhow::anyhow!("read STAC response: {e}"))?
        .to_vec();

    Response::builder()
        .status(StatusCode::OK)
        .header("Content-Type", "application/json")
        .header("Access-Control-Allow-Origin", "*")
        .body(body_bytes.into())
        .map_err(Into::into)
}

// ── InSAR Processing: forward to task-insar worker via NATS ──

async fn process_insar(mut req: Request<Body>) -> anyhow::Result<Response<Body>> {
    let body_bytes = req.body_mut().contents().await
        .map_err(|e| anyhow::anyhow!("read body: {e}"))?
        .to_vec();

    let timeout = Duration::from_secs(60).as_millis() as u32;

    match consumer::request("tasks.insar", &body_bytes, timeout) {
        Ok(resp) => Response::builder()
            .status(StatusCode::OK)
            .header("Content-Type", "application/json")
            .header("Access-Control-Allow-Origin", "*")
            .body(resp.body.into())
            .map_err(Into::into),
        Err(err) => Response::builder()
            .status(StatusCode::BAD_GATEWAY)
            .body(format!("worker error: {err}").into())
            .map_err(Into::into),
    }
}

// ── Known validation sites ──

#[derive(Serialize)]
struct ValidationSite {
    name: &'static str,
    description: &'static str,
    bbox: [f64; 4],
    date_range: &'static str,
    expected_subsidence_mm: f64,
}

async fn known_sites() -> anyhow::Result<Response<Body>> {
    let sites = vec![
        ValidationSite {
            name: "LA Metro Purple Line Extension",
            description: "Twin tunnel boring through downtown Los Angeles. InSAR studies detected up to 15mm subsidence along Wilshire Blvd corridor.",
            bbox: [-118.38, 34.05, -118.26, 34.07],
            date_range: "2019-01-01/2022-12-31",
            expected_subsidence_mm: 15.0,
        },
        ValidationSite {
            name: "London Crossrail - East London",
            description: "Lee Tunnel and Crossrail construction in East London. Sentinel-1 PS-InSAR detected subsidence patterns including a drift-filled hollow discovered during tunnelling.",
            bbox: [-0.02, 51.48, 0.12, 51.53],
            date_range: "2015-01-01/2019-12-31",
            expected_subsidence_mm: 20.0,
        },
        ValidationSite {
            name: "Dangjin Tunneling, South Korea",
            description: "TBM tunneling in reclaimed land near Dangjin. PS-InSAR measured maximum subsidence rate exceeding 40mm/yr with cumulative subsidence of ~200mm.",
            bbox: [126.55, 36.95, 126.72, 37.02],
            date_range: "2018-01-01/2021-12-31",
            expected_subsidence_mm: 200.0,
        },
    ];

    let json = serde_json::to_vec(&sites)?;
    Response::builder()
        .status(StatusCode::OK)
        .header("Content-Type", "application/json")
        .header("Access-Control-Allow-Origin", "*")
        .body(json.into())
        .map_err(Into::into)
}

/// Normalize datetime range to RFC3339 format required by Earth Search.
/// Converts "2020-01-01/2020-12-31" to "2020-01-01T00:00:00Z/2020-12-31T23:59:59Z"
fn normalize_datetime(dt: &str) -> String {
    let parts: Vec<&str> = dt.split('/').collect();
    let fix = |s: &str| -> String {
        if s.contains('T') {
            s.to_string()
        } else if s.len() == 10 {
            format!("{s}T00:00:00Z")
        } else {
            s.to_string()
        }
    };
    if parts.len() == 2 {
        let start = fix(parts[0]);
        let mut end = fix(parts[1]);
        // Make end inclusive
        if end.ends_with("T00:00:00Z") {
            end = format!("{}T23:59:59Z", &end[..10]);
        }
        format!("{start}/{end}")
    } else {
        fix(dt)
    }
}
