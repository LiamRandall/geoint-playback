# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project Overview

GeointPlayback is a geospatial application for detecting and visualizing ground subsidence using InSAR (Interferometric Synthetic Aperture Radar) data. It runs on [wasmCloud](https://wasmcloud.com) as WebAssembly components targeting `wasm32-wasip2`.

## Build & Run Commands

```bash
# Prerequisites
rustup target add wasm32-wasip2

# Build all components
wash build

# Run locally (starts NATS + wasmCloud host)
wash dev

# Run tests (requires wash dev running)
wash dev &
./tests/test_api.sh

# App is served at http://localhost:8000
```

There is no `cargo test` — the test suite is a bash script (`tests/test_api.sh`) that hits the HTTP API with curl. The server must be running first via `wash dev`.

## Architecture

Two Wasm components communicate over NATS messaging:

- **http-api** — HTTP server (port 8000). Routes: `/` (UI), `/api/sites`, `/api/stac/search`, `/api/process`. Proxies STAC queries to the Earth Search API and forwards InSAR processing to `task-insar` via NATS subject `tasks.insar`.
- **task-insar** — NATS message handler that receives processing requests and runs the InSAR displacement engine. Responds on the reply subject.

The flow: Browser → http-api (HTTP) → NATS → task-insar → NATS → http-api → Browser.

## Key Technical Details

- **Cargo workspace** with two `cdylib` crate members: `http-api/` and `task-insar/`.
- **WIT interfaces** are in `wit/world.wit`. Two worlds: `http-api` (imports messaging consumer, exports HTTP handler via `wstd`) and `task` (imports messaging consumer, exports messaging handler).
- **wit-bindgen** generates bindings. `http-api` uses `wstd::http_server` proc macro for HTTP export; `task-insar` uses `export!()` macro with manual `Guest` impl.
- **UI** is a single `http-api/ui.html` file embedded at compile time via `include_str!`. Uses MapLibre GL JS.
- **wasmCloud config** is in `.wash/config.yaml` — defines the build command and dev component wiring.
- The InSAR engine (`task-insar/src/insar.rs`) simulates displacement using a Gaussian subsidence model rather than processing real SAR phase data. It produces 20x20 grids.
- External dependency: Earth Search STAC API at `https://earth-search.aws.element84.com/v1`.
