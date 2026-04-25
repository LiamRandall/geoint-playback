# Project Constraints & Technical Debt

This document outlines the known limitations, architectural constraints, and technical inaccuracies identified during the code review of the GeointPlayback project.

## 1. Geospatial & Scientific Accuracy

### Synthetic InSAR Engine
The current implementation of the InSAR engine in `task-insar/src/insar.rs` is a **simulation**. It generates synthetic displacement data using Gaussian functions rather than processing real Interferometric Synthetic Aperture Radar (SAR) phase data.
- **Data Product Mismatch:** The project queries Sentinel-1 **GRD** (Ground Range Detected) products. These products do not contain the phase information required for actual InSAR processing. Real interferometry requires **SLC** (Single Look Complex) products.
- **Simplified LIA:** Local Incidence Angle (LIA) correction is based on a linear interpolation across the grid coordinates rather than an actual Digital Elevation Model (DEM). This is insufficient for real-world vertical projection.

## 2. Architectural Scalability

### Synchronous Processing Bottleneck
The `http-api` uses a synchronous NATS request-reply pattern (`consumer::request`) with a hardcoded 60-second timeout.
- **Constraint:** Long-running InSAR jobs or high-density grids will likely exceed this timeout, leading to `504 Gateway Timeout` errors and blocking Wasm instances.
- **Requirement:** A production-grade implementation should transition to an asynchronous job-status pattern (Accept -> Poll/Callback).

### Frontend Delivery
The `ui.html` file is embedded into the `http-api` binary via `include_str!`.
- **Constraint:** This increases the cold-start size of the Wasm component and prevents the browser from caching the frontend assets independently of the API logic.

## 3. Security & Implementation Risks

### CORS Configuration
The `http-api` hardcodes `Access-Control-Allow-Origin: *`.
- **Risk:** This allows any origin to access the API. In a deployed environment, this should be restricted to the specific domain hosting the frontend.

### Fragile Datetime Handling
Date normalization is performed using manual string manipulation in `http-api/src/lib.rs`.
- **Debt:** This is error-prone. A robust implementation should use the `chrono` crate for RFC3339 validation and conversion.

## 4. Error Handling & Observability

### Inconsistent Error Patterns
The `task-insar` worker uses `Result<(), String>` for message handling, while the `http-api` uses `anyhow::Result`.
- **Debt:** Returning raw strings from the worker loses structured context and stack traces, making debugging in a distributed `wasmCloud` environment more difficult.
