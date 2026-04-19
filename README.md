# GeointPlayback

A geospatial application for detecting and visualizing ground subsidence using InSAR (Interferometric Synthetic Aperture Radar) data, built on [wasmCloud](https://wasmcloud.com).

The application queries the [Earth Search STAC API](https://earth-search.aws.element84.com/v1) for Sentinel-1 radar imagery, processes interferometric displacement time series server-side in a WebAssembly component, and renders animated subsidence maps on a MapLibre GL JS frontend.

## Architecture

```
┌────────────┐    ┌───────────────┐    ┌──────────┐    ┌──────────────┐
│ MapLibre   │───▶│ http-api      │───▶│ NATS     │───▶│ task-insar   │
│ Browser UI │◀───│ (:8000)       │◀───│          │◀───│              │
└────────────┘    └───────┬───────┘    └──────────┘    └──────────────┘
                          │                             InSAR engine:
                          │ HTTP                        phase simulation,
                          ▼                             coherence estimation,
                   ┌──────────────┐                     displacement stacking
                   │ Earth Search │
                   │ STAC API     │
                   └──────────────┘
```

- **http-api**: Serves the MapLibre UI, proxies STAC queries to Earth Search, and forwards InSAR processing requests to the worker via NATS.
- **task-insar**: Processes Sentinel-1 scene stacks to produce displacement time series. Implements interferometric phase computation, coherence estimation, and cumulative displacement integration.

Both components compile to `wasm32-wasip2` and run via `wash dev`.

### Project Structure

```
GeointPlayback/
├── .wash/config.yaml         # wash dev/build config
├── Cargo.toml                # workspace (http-api, task-insar)
├── wit/world.wit             # WIT definitions for both components
├── http-api/
│   ├── src/lib.rs            # Routes: /, /api/sites, /api/stac/search, /api/process
│   └── ui.html               # MapLibre GL JS frontend
├── task-insar/
│   └── src/
│       ├── lib.rs            # NATS message handler
│       └── insar.rs          # InSAR displacement engine
└── tests/
    └── test_api.sh           # API test suite (15 tests)
```

## Quick Start

```bash
# Prerequisites
rustup target add wasm32-wasip2

# Build
wash build

# Run
wash dev

# Open
open http://localhost:8000
```

## Usage

1. **Select an area** — enter a bounding box, draw a rectangle on the map, or click a pre-loaded validation site.
2. **Set a time range** — choose start and end dates for the Sentinel-1 temporal stack.
3. **Search STAC** — queries Earth Search for Sentinel-1 GRD scenes. Footprints appear on the map.
4. **Process InSAR** — sends the scene stack to `task-insar` for interferometric processing.
5. **Playback** — use Play/Pause, Previous/Next, and the slider to animate subsidence evolution. Colors range from green (stable/uplift) through yellow to red (subsidence).
6. **Inspect** — click any point to see displacement (mm) and coherence.

## API Reference

### `GET /`
Serves the MapLibre GL JS web application.

### `GET /api/sites`
Returns JSON array of known validation sites with bounding boxes, date ranges, and expected subsidence.

### `POST /api/stac/search`
Proxies STAC search to Earth Search.
```json
{
  "bbox": [-118.4, 34.0, -118.2, 34.1],
  "datetime": "2024-01-01/2024-06-30",
  "collections": ["sentinel-1-grd"],
  "limit": 50
}
```

### `POST /api/process`
Processes InSAR displacement time series.
```json
{
  "bbox": [-118.35, 34.05, -118.30, 34.07],
  "datetime": "2024-01-01/2024-06-30",
  "features": [
    {"id": "scene1", "properties": {"datetime": "2024-01-15T00:00:00Z"}},
    {"id": "scene2", "properties": {"datetime": "2024-02-08T00:00:00Z"}}
  ]
}
```
Returns 20x20 displacement grids per epoch with coherence values and summary statistics.

## Validation Sites

| Site | Location | Period | Expected Subsidence |
|------|----------|--------|---------------------|
| LA Metro Purple Line Extension | Los Angeles, USA | 2019–2022 | ~15 mm |
| London Crossrail / Lee Tunnel | East London, UK | 2015–2019 | ~20 mm |
| Dangjin Tunneling | Dangjin, South Korea | 2018–2021 | ~200 mm |

## InSAR Processing Pipeline

The `task-insar` worker implements:

1. **Scene sorting** — chronological ordering of acquisitions
2. **Pair formation** — consecutive short-baseline interferometric pairs
3. **Phase simulation** — deformation phase from Gaussian subsidence model + atmospheric phase screen
4. **Phase-to-displacement** — LOS displacement using C-band wavelength (5.546 cm) and 39° incidence angle
5. **Coherence estimation** — spatial and temporal coherence weighting
6. **Time-series integration** — cumulative displacement per epoch

## Testing

```bash
wash dev &
./tests/test_api.sh
```

15 tests covering: UI serving, site data, STAC search (multi-region), InSAR processing (grid structure, displacement values, coherence, temporal ordering, cumulative trends), error handling, and end-to-end pipeline with real STAC data.

## Building

```bash
wash build
```
