mod insar;

wit_bindgen::generate!({
    path: "../wit",
    world: "task",
    with: {
        "wasmcloud:messaging/types@0.2.0": generate,
        "wasmcloud:messaging/consumer@0.2.0": generate,
    },
});

use crate::wasmcloud::messaging::types::BrokerMessage;
use wasmcloud::messaging::consumer;
#[allow(unused)]
use wstd::prelude::*;

use serde::Deserialize;

struct Component;
export!(Component);

/// Request to process InSAR displacement for a given area and time range.
#[derive(Deserialize)]
struct ProcessRequest {
    /// Bounding box [west, south, east, north]
    bbox: [f64; 4],
    /// ISO 8601 date range "start/end"
    datetime: String,
    /// STAC feature collection (the search results passed through)
    features: Vec<StacFeature>,
}

#[derive(Deserialize)]
struct StacFeature {
    id: String,
    properties: FeatureProperties,
}

#[derive(Deserialize)]
struct FeatureProperties {
    datetime: Option<String>,
    #[serde(default, rename = "sar:instrument_mode")]
    _instrument_mode: Option<String>,
}

impl exports::wasmcloud::messaging::handler::Guest for Component {
    fn handle_message(msg: BrokerMessage) -> Result<(), String> {
        let Some(subject) = msg.reply_to else {
            return Err("missing reply_to".to_string());
        };

        let request: ProcessRequest = serde_json::from_slice(&msg.body)
            .map_err(|e| format!("invalid request: {e}"))?;

        let result = insar::process_displacement(&request.bbox, &request.datetime, &request.features)
            .map_err(|e| format!("processing failed: {e}"))?;

        let response_bytes = serde_json::to_vec(&result)
            .map_err(|e| format!("serialize result: {e}"))?;

        let reply = BrokerMessage {
            subject,
            body: response_bytes,
            reply_to: None,
        };

        consumer::publish(&reply)
    }
}
