# GeointPlayback

A wasmCloud-native InSAR (Interferometric Synthetic Aperture Radar) ground subsidence detector and visualizer.

## Project Overview

GeointPlayback is a geospatial application designed to detect ground subsidence patterns using Sentinel-1 radar data. It operates as a set of WebAssembly components on the [wasmCloud](https://wasmcloud.com) 2.0 host, leveraging NATS for inter-component communication and WASI 0.2 (`wasm32-wasip2`) for capability access.

### Main Technologies
- **Language:** Rust (compiled to `wasm32-wasip2`).
- **Runtime:** [wasmCloud](https://wasmcloud.com) with NATS messaging.
- **Interfaces:** [WIT](https://component-model.bytecodealliance.org/) (WebAssembly Interface Types).
- **Frontend:** MapLibre GL JS, served from a single embedded HTML file.
- **Data Source:** [Earth Search STAC API](https://earth-search.aws.element84.com/v1) (Sentinel-1 GRD).
- **Libraries:** `wstd` (WASI standard library), `wit-bindgen`, `serde`, `MapLibre GL JS`.

## Architecture

The system consists of two primary WebAssembly components:

1.  **`http-api`**: An HTTP gateway that:
    - Serves the single-page MapLibre UI (`ui.html`).
    - Proxies STAC catalog queries to Earth Search.
    - Forwards InSAR processing requests to `task-insar` via NATS (subject: `tasks.insar`).
2.  **`task-insar`**: A background worker that:
    - Receives processing requests via NATS.
    - Implements the InSAR displacement engine (multi-looking, Goldstein filtering, SBAS stacking).
    - Returns georeferenced displacement grids and coherence data.

## Building and Running

### Prerequisites
- [Rust](https://rustup.rs/) with the `wasm32-wasip2` target: `rustup target add wasm32-wasip2`
- [wash CLI](https://wasmcloud.com/docs/installation) (wasmCloud Shell)

### Key Commands
- **Build All:** `wash build` (Compiles both components to Wasm).
- **Local Development:** `wash dev` (Starts wasmCloud host, NATS, and enables hot-reload).
- **Run Tests:** `./tests/test_api.sh` (Requires `wash dev` to be running).
- **Access UI:** [http://localhost:8000](http://localhost:8000)

## Development Conventions

### Wasm Component Development
- **Target:** Always use `wasm32-wasip2`.
- **WIT Definitions:** Shared interface definitions are located in `wit/world.wit`.
- **Component Linking:** Capabilities (HTTP, Messaging, etc.) are declared in WIT and managed by the wasmCloud host.
- **Small Footprint:** Aim for minimal Wasm sizes (currently ~150KB - 400KB).

### Coding Style
- **Rust Workspace:** The project is a Cargo workspace with `http-api` and `task-insar` as members.
- **Embedded UI:** The frontend (`ui.html`) is embedded directly into the `http-api` binary using `include_str!`.
- **Error Handling:** Use `anyhow` for high-level error management.
- **Documentation:** Use module-level and function-level doc comments to explain complex SAR processing logic.

### Testing
- **Integration over Unit:** There is no standard `cargo test` suite. Testing is performed via `./tests/test_api.sh`, which uses `curl` to exercise the HTTP API end-to-end.
- **Reproduction:** When fixing bugs, verify them by running the integration test suite while `wash dev` is active.

## Project Structure
- `http-api/`: HTTP gateway and UI host.
- `task-insar/`: InSAR processing engine.
- `wit/`: WIT world and interface definitions.
- `tests/`: Integration test scripts.
- `.wash/`: wasmCloud build and deployment configuration.
