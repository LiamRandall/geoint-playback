# GeointPlayback

A wasmCloud-native InSAR ground subsidence detector that queries Sentinel-1 GRD scenes via the OGC STAC API (Earth Search), processes interferometric displacement time-series using SBAS stacking, Goldstein filtering, multi-looking, and per-pixel Local Incidence Angle correction in Rust/WebAssembly, and renders animated subsidence maps on a MapLibre GL JS frontend with IDW interpolation and coherence-gated visualization.

## Example

![LA Metro Purple Line Extension — InSAR subsidence visualization](LosAngeles-CA-USA-purple-line-metro-extension.png)

**LA Metro Purple Line Extension, Los Angeles, CA** — Twin tunnel boring through downtown Los Angeles along Wilshire Blvd. InSAR studies detected up to 15mm of ground subsidence along the corridor. This visualization shows cumulative displacement from a Sentinel-1 GRD stack (2019-2024), processed with a 20x20 grid, coherence threshold 0.4, and SBAS atmospheric correction. Red pixels indicate subsidence concentrated along the tunnel alignment; green pixels are stable ground. The popup shows a measured displacement of -125.09mm with 0.91 coherence at a selected point, and interpolation is enabled to fill gaps between persistent scatterer locations.

## Architecture

```
                          ┌──────────────────────────────────────────────────────────┐
                          │                  wasmCloud 2.0 Host                      │
                          │                                                          │
┌────────────────┐   HTTP │  ┌───────────────────┐  NATS   ┌──────────────────────┐  │
│                │───────▶│  │    http-api       │────────▶│     task-insar       │  │
│   Browser UI   │        │  │   (wasm32-wasip2) │         │   (wasm32-wasip2)    │  │
│                │◀───────│  │                   │◀────────│                      │  │
│  MapLibre GL   │        │  │  Routes:          │         │  InSAR Engine:       │  │
│  CARTO tiles   │        │  │  / (UI)           │         │  - Multi-looking     │  │
│  Nominatim     │        │  │  /api/sites       │         │  - LIA correction    │  │
│  geocoding     │        │  │  /api/stac/search │         │  - Goldstein filter  │  │
│  IDW interp    │        │  │  /api/process     │         │  - SBAS stacking     │  │
│                │        │  │                   │         │  - APS removal       │  │
└────────────────┘        │  └────────┬──────────┘         │  - Coherence gating  │  │
                          │           │                    └──────────────────────┘  │
                          └───────────┼──────────────────────────────────────────────┘
                                      │ HTTPS
                                      ▼
                             ┌──────────────────┐
                             │   Earth Search   │
                             │   STAC API       │
                             │  (element84.com) │
                             └──────────────────┘
```

**http-api** serves the single-page MapLibre UI, proxies STAC catalog queries to [Earth Search](https://earth-search.aws.element84.com/v1), and forwards InSAR processing requests to `task-insar` over NATS messaging.

**task-insar** receives scene stacks via NATS and runs the full InSAR displacement pipeline, returning georeferenced displacement grids with per-pixel coherence.

Both components compile to `wasm32-wasip2` WebAssembly and run on the [wasmCloud](https://wasmcloud.com) host via `wash dev`.

## InSAR Processing Pipeline

The `task-insar` engine implements the following techniques, configurable via the UI:

| Stage | Technique | Details |
|-------|-----------|---------|
| 1. Scene pairing | Short-baseline SBAS | Consecutive pairs sorted chronologically |
| 2. Multi-looking | 4 range x 1 azimuth | ~20m square pixels from 5m x 20m native resolution |
| 3. Incidence angle | Local Incidence Angle (LIA) | Per-pixel interpolation across IW swath (29.1deg-46.0deg) instead of fixed constant |
| 4. Phase filtering | Goldstein adaptive filter | Power-spectrum filter (alpha=0.5) with edge-adaptive strengthening |
| 5. Displacement | LOS + vertical projection | d_los = (lambda/4pi) * phi, then d_v = d_los / cos(theta) per pixel |
| 6. Coherence | Multi-look enhanced | Spatial + temporal decorrelation with thermal noise floor; configurable threshold (default 0.4) |
| 7. Atmosphere | SBAS APS removal | Temporal mean residual estimation + spatial smoothing; auto-enabled with 5+ scenes |
| 8. Reference | Stable point selection | Highest mean-coherence pixel used as zero reference |
| 9. Integration | Coherence-weighted stacking | Below-threshold pixels excluded; cumulative displacement per epoch |

### Constants

| Parameter | Value | Source |
|-----------|-------|--------|
| Wavelength | 5.546 cm | Sentinel-1 C-band |
| Repeat cycle | 12 days | Sentinel-1 nominal |
| IW incidence range | 29.1deg - 46.0deg | Sentinel-1 IW mode spec |
| Goldstein alpha | 0.5 | Moderate noise environment |
| Reference stability | 0.8 coherence | Quality gate for reference pixel |

## Quick Start

### Prerequisites

- [Rust](https://rustup.rs/) with the `wasm32-wasip2` target
- [wash CLI](https://wasmcloud.com/docs/installation) (wasmCloud Shell)

```bash
rustup target add wasm32-wasip2
```

### Build and Run

```bash
wash build       # compile both components to Wasm
wash dev         # start wasmCloud host with NATS, hot-reload enabled
```

Open [http://localhost:8000](http://localhost:8000).

## Usage

1. **Search for a location** -- use the geocoding search bar (top of page) to find a city or region, or click a validation site from the sidebar.
2. **Define the area** -- enter a bounding box manually or click "Draw Rectangle" on the map.
3. **Set time range** -- pick start/end dates and max scene count (default 50).
4. **Search STAC** -- queries Earth Search for Sentinel-1 GRD scenes. The status shows the date range and count of returned scenes.
5. **Configure processing** -- adjust grid density (10-100) and coherence threshold (0.0-0.8) in the Processing section.
6. **Process InSAR** -- runs the displacement pipeline. Scenes are evenly sampled across the time range if needed.
7. **Playback** -- animate subsidence evolution with Play/Pause, step through frames, or scrub the slider. The current date is displayed prominently.
8. **Toggle interpolation** -- enable IDW interpolation to fill gaps between measured points (visual smoothing, not measured data).
9. **Inspect** -- click any point to see displacement (mm) and coherence values.

### Color Scale

| Color | Meaning |
|-------|---------|
| Green | Stable ground or slight uplift |
| Yellow | Minor subsidence (~5mm) |
| Orange | Moderate subsidence (~10-20mm) |
| Red | Significant subsidence (>20mm) |

## API Reference

### `GET /`

Serves the MapLibre GL JS web application.

### `GET /api/sites`

Returns a JSON array of known validation sites with bounding boxes, date ranges, and expected subsidence values.

### `POST /api/stac/search`

Proxies a STAC search to Earth Search.

```json
{
  "bbox": [-118.4, 34.0, -118.2, 34.1],
  "datetime": "2024-01-01/2024-06-30",
  "collections": ["sentinel-1-grd"],
  "limit": 50
}
```

### `POST /api/process`

Runs the InSAR displacement pipeline.

```json
{
  "bbox": [-118.35, 34.05, -118.30, 34.07],
  "datetime": "2024-01-01/2024-06-30",
  "features": [
    {"id": "scene1", "properties": {"datetime": "2024-01-15T00:00:00Z"}},
    {"id": "scene2", "properties": {"datetime": "2024-02-08T00:00:00Z"}}
  ],
  "params": {
    "grid_size": 20,
    "min_coherence": 0.4
  }
}
```

Response includes displacement grids per epoch, coherence arrays, summary statistics, and processing metadata (LIA values, APS correction status, reference point location).

## Validation Sites

| Site | Location | Period | Expected Subsidence |
|------|----------|--------|---------------------|
| LA Metro Purple Line Extension | Los Angeles, USA | 2019-2022 | ~15 mm |
| London Crossrail / Lee Tunnel | East London, UK | 2015-2019 | ~20 mm |
| Dangjin Tunneling | Dangjin, South Korea | 2018-2021 | ~200 mm |

## Testing

```bash
wash dev &                  # start the server
./tests/test_api.sh         # run the test suite
```

15 tests covering: UI serving, site data, STAC search (multi-region), InSAR processing (grid structure, displacement values, coherence, temporal ordering, cumulative trends), error handling, and end-to-end pipeline with real STAC data.

## Tech Stack

| Layer | Technology |
|-------|------------|
| Runtime | [wasmCloud](https://wasmcloud.com) + NATS messaging |
| Language | Rust, compiled to `wasm32-wasip2` |
| Interface | [WIT](https://component-model.bytecodealliance.org/) (WebAssembly Interface Types) |
| Bindings | [wit-bindgen](https://github.com/bytecodealliance/wit-bindgen) + [wstd](https://docs.rs/wstd) |
| Frontend | [MapLibre GL JS](https://maplibre.org/) on [CARTO](https://carto.com/) Voyager tiles |
| Geocoding | [Nominatim](https://nominatim.openstreetmap.org/) (OpenStreetMap) |
| Catalog | [OGC STAC API](https://stacspec.org/) via [Earth Search](https://earth-search.aws.element84.com/v1) |
| Imagery | Sentinel-1 GRD (C-band SAR, 5.546cm wavelength) |

## Project Structure

```
GeointPlayback/
├── Cargo.toml                # Rust workspace (http-api, task-insar)
├── Cargo.lock
├── .wash/config.yaml         # wasmCloud build & dev configuration
├── wit/
│   └── world.wit             # WIT world definitions for both components
├── http-api/
│   ├── Cargo.toml
│   ├── src/lib.rs            # HTTP routes, STAC proxy, NATS bridge
│   └── ui.html               # Single-page MapLibre frontend
├── task-insar/
│   ├── Cargo.toml
│   └── src/
│       ├── lib.rs            # NATS message handler
│       └── insar.rs          # InSAR displacement engine
└── tests/
    └── test_api.sh           # API integration test suite
```

## Wasm Component Analysis

### Size

| Component | File | Size |
|-----------|------|------|
| http-api | `target/wasm32-wasip2/release/http_api.wasm` | **393 KB** |
| task-insar | `target/wasm32-wasip2/release/task_insar.wasm` | **148 KB** |

Both components are extremely small. The http-api is ~2.6x larger because it embeds `ui.html` via `include_str!` and carries the full `wasi:http` type system (request/response resources, error codes, TLS types, etc.). These sizes are well within cold-start and distribution budgets for edge/serverless deployment.

### WIT Contracts

Inspected via `wasm-tools component wit`.

#### `task_insar.wasm` — Minimal messaging worker

```wit
world root {
  import wasmcloud:messaging/types@0.2.0
  import wasmcloud:messaging/consumer@0.2.0
  import wasi:io/*@0.2.9
  import wasi:cli/*@0.2.9

  export wasmcloud:messaging/handler@0.2.0
}
```

One export, two meaningful imports. This component only receives a `broker-message` (subject + body + optional reply-to) and can publish messages back. The `wasi:cli` imports are standard I/O plumbing (stdout/stderr/env/exit). No filesystem, no network, no HTTP.

#### `http_api.wasm` — HTTP gateway

```wit
world root {
  import wasi:http/types@0.2.9
  import wasi:http/outgoing-handler@0.2.9    -- make outbound HTTP calls
  import wasmcloud:messaging/types@0.2.0
  import wasmcloud:messaging/consumer@0.2.0  -- publish to NATS (request/reply)
  import wasi:random/insecure-seed@0.2.9
  import wasi:clocks/monotonic-clock@0.2.9
  import wasi:io/*@0.2.9
  import wasi:cli/*@0.2.9

  export wasi:http/incoming-handler@0.2.9
}
```

Slightly broader — adds outbound HTTP (for STAC API proxy), a clock, and an insecure PRNG seed. Still no filesystem, no sockets, no threads.

### Security Risk Surface

The security posture of both components is **very low risk**:

1. **No filesystem access** — neither component imports `wasi:filesystem`. They cannot read or write files on the host. The UI is baked in at compile time.

2. **No raw networking** — `http_api` can make outbound HTTP requests via `wasi:http/outgoing-handler`, but this is mediated by the wasmCloud host. The host policy controls which URLs are reachable. `task_insar` has zero network capability — it only talks via NATS messages.

3. **No ambient authority** — the component model is deny-by-default. Each capability (messaging, HTTP, clock) must be explicitly linked by the wasmCloud host. A compromised component cannot escalate beyond its declared imports.

4. **`insecure-seed` only** — `http_api` uses `wasi:random/insecure-seed`, not `wasi:random/random`. This is fine for non-cryptographic use (e.g., HashMap randomization) but confirms no cryptographic key generation happens inside the component.

5. **Message boundary is `list<u8>`** — both components exchange raw bytes over NATS. Input validation of the JSON payload is the main trust boundary to audit (in both `handle-message` and the HTTP route handlers).

6. **Narrow dependency footprint** — ~30 unique crates per component. The dependency trees are dominated by `serde`, `wit-bindgen` tooling (proc-macro-time only), and `wstd`. No `unsafe`-heavy crates, no C FFI, no `openssl`/`ring`. Supply chain attack surface is minimal.

### Maintainability

- **Two components, ~148 + 393 KB** — trivial to audit, version, and deploy independently.
- **WIT contracts target stable WASI 0.2.9 + wasmCloud messaging 0.2.0** — well-defined, typed interfaces with no custom extensions.
- **Single-file UI** — no frontend build tooling; updating `ui.html` requires only recompiling `http_api`.
- Shared dependency tree means `cargo update` touches both components uniformly.
