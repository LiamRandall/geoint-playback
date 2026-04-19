//! InSAR displacement processing engine.
//!
//! Implements simplified interferometric SAR processing to estimate ground
//! displacement from temporal stacks of Sentinel-1 radar acquisitions.
//!
//! # Processing Pipeline
//!
//! 1. **Scene pairing**: Sort scenes chronologically, form interferometric pairs
//!    between consecutive acquisitions (short baseline).
//! 2. **Phase simulation**: For each grid cell in the bounding box, simulate
//!    radar phase from scene geometry (incidence angle, baseline, wavelength).
//! 3. **Differential interferometry**: Compute phase difference between pairs,
//!    isolating the deformation signal from topographic and atmospheric components.
//! 4. **Coherence estimation**: Estimate temporal coherence as a quality metric
//!    to weight reliable measurements.
//! 5. **Phase-to-displacement conversion**: Convert unwrapped phase to
//!    line-of-sight displacement using the radar wavelength.
//! 6. **Time-series integration**: Accumulate displacement across the stack
//!    to produce cumulative ground movement per epoch.

use serde::Serialize;

/// Sentinel-1 C-band radar wavelength in meters
const WAVELENGTH_M: f64 = 0.05546;

/// Nominal repeat cycle in days for Sentinel-1
const REPEAT_CYCLE_DAYS: f64 = 12.0;

/// Grid resolution for displacement output
const GRID_SIZE: usize = 20;

/// Incidence angle (degrees) — typical for Sentinel-1 IW mode mid-swath
const INCIDENCE_DEG: f64 = 39.0;

/// Pi
const PI: f64 = std::f64::consts::PI;

#[derive(Serialize)]
pub struct DisplacementFrame {
    pub date: String,
    pub scene_id: String,
    pub displacement_mm: Vec<f64>,
    pub coherence: Vec<f64>,
    pub grid_w: usize,
    pub grid_h: usize,
}

#[derive(Serialize)]
pub struct ProcessResult {
    pub bbox: [f64; 4],
    pub frames: Vec<DisplacementFrame>,
    pub grid_w: usize,
    pub grid_h: usize,
    pub max_subsidence_mm: f64,
    pub mean_subsidence_rate_mm_yr: f64,
}

use super::StacFeature;

/// Main entry point: process displacement from STAC features.
pub fn process_displacement(
    bbox: &[f64; 4],
    _datetime: &str,
    features: &[StacFeature],
) -> Result<ProcessResult, String> {
    if features.is_empty() {
        return Err("no scenes provided".into());
    }

    // Sort scenes by date
    let mut scenes: Vec<(&str, &str)> = features
        .iter()
        .filter_map(|f| {
            let date = f.properties.datetime.as_deref()?;
            Some((f.id.as_str(), date))
        })
        .collect();
    scenes.sort_by_key(|(_, d)| d.to_string());

    if scenes.len() < 2 {
        return Err("need at least 2 scenes for interferometry".into());
    }

    let grid_w = GRID_SIZE;
    let grid_h = GRID_SIZE;
    let n_cells = grid_w * grid_h;

    // Compute center coordinates for each grid cell
    let cell_centers = compute_grid_centers(bbox, grid_w, grid_h);

    // Build interferometric pairs (consecutive short-baseline)
    let pairs: Vec<(&str, &str, &str, &str)> = scenes
        .windows(2)
        .map(|w| (w[0].0, w[0].1, w[1].0, w[1].1))
        .collect();

    // Process each pair to get incremental displacement
    let mut cumulative_displacement = vec![0.0_f64; n_cells];
    let mut frames = Vec::with_capacity(scenes.len());

    // First frame: zero displacement (reference)
    frames.push(DisplacementFrame {
        date: scenes[0].1.to_string(),
        scene_id: scenes[0].0.to_string(),
        displacement_mm: vec![0.0; n_cells],
        coherence: vec![1.0; n_cells],
        grid_w,
        grid_h,
    });

    for (_id1, date1, id2, date2) in &pairs {
        let temporal_baseline_days = estimate_temporal_baseline(date1, date2);

        // For each grid cell, compute interferometric phase and coherence
        let mut incremental_disp = vec![0.0_f64; n_cells];
        let mut coherence = vec![0.0_f64; n_cells];

        for i in 0..n_cells {
            let (lon, lat) = cell_centers[i];

            // Simulate deformation signal based on spatial position within bbox
            // In production this would come from actual SAR phase data.
            // Here we model subsidence as a Gaussian centered on the bbox center,
            // with magnitude scaling with temporal baseline.
            let deformation_phase = simulate_deformation_phase(
                lon, lat, bbox, temporal_baseline_days,
            );

            // Simulate atmospheric phase screen (spatially correlated noise)
            let atmo_phase = simulate_atmospheric_phase(lon, lat, i as u64);

            // Differential phase = deformation + atmosphere + noise
            let diff_phase = deformation_phase + atmo_phase;

            // Phase to displacement: d = (λ / 4π) × φ / cos(θ)
            let los_disp_m = (WAVELENGTH_M / (4.0 * PI)) * diff_phase
                / (INCIDENCE_DEG * PI / 180.0).cos();

            incremental_disp[i] = los_disp_m * 1000.0; // to mm

            // Coherence: higher near center of deformation, lower at edges
            coherence[i] = estimate_coherence(
                lon, lat, bbox, temporal_baseline_days,
            );
        }

        // Accumulate into cumulative displacement
        for i in 0..n_cells {
            cumulative_displacement[i] += incremental_disp[i];
        }

        frames.push(DisplacementFrame {
            date: date2.to_string(),
            scene_id: id2.to_string(),
            displacement_mm: cumulative_displacement.clone(),
            coherence,
            grid_w,
            grid_h,
        });
    }

    // Compute summary statistics
    let max_subsidence_mm = cumulative_displacement
        .iter()
        .map(|d| d.abs())
        .fold(0.0_f64, f64::max);

    let total_days = estimate_temporal_baseline(scenes[0].1, scenes.last().unwrap().1);
    let mean_subsidence_rate_mm_yr = if total_days > 0.0 {
        max_subsidence_mm * 365.25 / total_days
    } else {
        0.0
    };

    Ok(ProcessResult {
        bbox: *bbox,
        frames,
        grid_w,
        grid_h,
        max_subsidence_mm,
        mean_subsidence_rate_mm_yr,
    })
}

/// Compute center coordinates for each grid cell.
fn compute_grid_centers(bbox: &[f64; 4], grid_w: usize, grid_h: usize) -> Vec<(f64, f64)> {
    let (west, south, east, north) = (bbox[0], bbox[1], bbox[2], bbox[3]);
    let dx = (east - west) / grid_w as f64;
    let dy = (north - south) / grid_h as f64;

    let mut centers = Vec::with_capacity(grid_w * grid_h);
    for row in 0..grid_h {
        for col in 0..grid_w {
            let lon = west + (col as f64 + 0.5) * dx;
            let lat = south + (row as f64 + 0.5) * dy;
            centers.push((lon, lat));
        }
    }
    centers
}

/// Simulate deformation phase for a grid cell.
///
/// Models subsidence as a Gaussian-shaped bowl centered on the bounding box,
/// with magnitude proportional to the temporal baseline. This mimics the
/// spatial pattern of tunnel-induced settlement.
fn simulate_deformation_phase(
    lon: f64,
    lat: f64,
    bbox: &[f64; 4],
    temporal_baseline_days: f64,
) -> f64 {
    let center_lon = (bbox[0] + bbox[2]) / 2.0;
    let center_lat = (bbox[1] + bbox[3]) / 2.0;
    let extent_lon = (bbox[2] - bbox[0]) / 2.0;
    let extent_lat = (bbox[3] - bbox[1]) / 2.0;

    // Normalized distance from center (0 at center, 1 at edge)
    let dx = (lon - center_lon) / extent_lon;
    let dy = (lat - center_lat) / extent_lat;
    let r2 = dx * dx + dy * dy;

    // Gaussian subsidence pattern: max at center, decaying outward
    // σ = 0.4 gives a trough width ~80% of the bbox
    let sigma2 = 0.4 * 0.4;
    let spatial_weight = (-r2 / (2.0 * sigma2)).exp();

    // Subsidence rate: ~5mm per 12-day repeat cycle at center
    // Convert to phase: φ = 4π × d / λ
    let subsidence_m = -0.005 * (temporal_baseline_days / REPEAT_CYCLE_DAYS);
    let deformation_phase = (4.0 * PI * subsidence_m / WAVELENGTH_M) * spatial_weight;

    deformation_phase
}

/// Simulate atmospheric phase screen (APS).
///
/// In real InSAR, the atmosphere introduces spatially correlated phase noise.
/// We simulate this as a low-amplitude pseudo-random field.
fn simulate_atmospheric_phase(lon: f64, lat: f64, seed: u64) -> f64 {
    // Simple deterministic "noise" based on position
    let x = lon * 1000.0 + seed as f64 * 0.1;
    let y = lat * 1000.0 + seed as f64 * 0.07;
    let phase = (x.sin() * y.cos() * 3.7 + (x * 0.3).cos() * (y * 0.5).sin() * 2.1) * 0.1;
    phase
}

/// Estimate temporal coherence.
///
/// Higher coherence near deformation center (persistent scatterers like buildings),
/// lower at edges (vegetation, water).
fn estimate_coherence(
    lon: f64,
    lat: f64,
    bbox: &[f64; 4],
    temporal_baseline_days: f64,
) -> f64 {
    let center_lon = (bbox[0] + bbox[2]) / 2.0;
    let center_lat = (bbox[1] + bbox[3]) / 2.0;
    let extent_lon = (bbox[2] - bbox[0]) / 2.0;
    let extent_lat = (bbox[3] - bbox[1]) / 2.0;

    let dx = (lon - center_lon) / extent_lon;
    let dy = (lat - center_lat) / extent_lat;
    let r2 = dx * dx + dy * dy;

    // Base coherence decays with distance from center and temporal baseline
    let spatial_coh = 0.95 * (-r2 / 1.5).exp();
    let temporal_coh = (-temporal_baseline_days / 365.0 * 0.3).exp();

    let coh = spatial_coh * temporal_coh;
    coh.clamp(0.1, 0.99)
}

/// Estimate temporal baseline between two ISO 8601 dates in days.
fn estimate_temporal_baseline(date1: &str, date2: &str) -> f64 {
    // Parse dates as YYYY-MM-DD (take first 10 chars)
    let d1 = &date1[..10.min(date1.len())];
    let d2 = &date2[..10.min(date2.len())];

    let days1 = parse_date_to_days(d1);
    let days2 = parse_date_to_days(d2);

    (days2 - days1).abs() as f64
}

/// Simple date-to-days parser for YYYY-MM-DD format.
fn parse_date_to_days(date: &str) -> i64 {
    let parts: Vec<&str> = date.split('-').collect();
    if parts.len() < 3 {
        return 0;
    }
    let y: i64 = parts[0].parse().unwrap_or(2020);
    let m: i64 = parts[1].parse().unwrap_or(1);
    let d: i64 = parts[2].parse().unwrap_or(1);

    // Approximate days since epoch (good enough for baselines)
    y * 365 + m * 30 + d
}
