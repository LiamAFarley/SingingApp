//! Latency calibration: find scheduled clicks in the captured input.

use serde::{Deserialize, Serialize};
use ts_rs::TS;

/// Sample indices where the signal rises above a threshold relative to its noise floor,
/// at least `min_gap_ms` apart.
pub fn detect_onsets(x: &[f32], sample_rate: u32, min_gap_ms: f32) -> Vec<usize> {
    if x.is_empty() {
        return Vec::new();
    }
    let win = (sample_rate as usize / 1000).max(1);
    let env: Vec<f32> = x
        .chunks(win)
        .map(|c| c.iter().fold(0.0f32, |m, v| m.max(v.abs())))
        .collect();
    let mut sorted = env.clone();
    sorted.sort_by(|a, b| a.total_cmp(b));
    let floor = sorted[sorted.len() / 5];
    let peak = sorted[sorted.len() - 1];
    let threshold = (floor * 8.0).max(peak * 0.25).max(1e-4);
    let gap = ((min_gap_ms as usize).max(1)) * sample_rate as usize / 1000;
    let mut onsets = Vec::new();
    let mut last: Option<usize> = None;
    for (w, &e) in env.iter().enumerate() {
        if e < threshold {
            continue;
        }
        // Refine within the window to the first sample over the threshold.
        let start = w * win;
        let idx = x[start..(start + win).min(x.len())]
            .iter()
            .position(|v| v.abs() >= threshold)
            .map(|p| start + p)
            .unwrap_or(start);
        if last.map(|l| idx >= l + gap).unwrap_or(true) {
            onsets.push(idx);
            last = Some(idx);
        }
    }
    onsets
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, TS)]
pub struct CalibrationResult {
    /// Measured round trip minus the timestamp estimate, in frames. Add it to the estimate.
    pub residual_frames: i64,
    /// Median absolute deviation of the measurements, in ms.
    pub spread_ms: f64,
    pub matched: usize,
    pub expected: usize,
}

/// Matches clicks scheduled at output frames `scheduled` against onsets found in a capture that
/// starts at absolute input frame `capture_start`, given the current round-trip `estimate`.
pub fn analyze_clicks(
    capture: &[f32],
    capture_start: u64,
    scheduled: &[u64],
    estimate: i64,
    sample_rate: u32,
    window_ms: f64,
) -> Option<CalibrationResult> {
    let spacing_ms = scheduled
        .windows(2)
        .map(|w| (w[1] - w[0]) as f64 * 1000.0 / sample_rate as f64)
        .fold(f64::MAX, f64::min);
    let min_gap = (spacing_ms * 0.5).clamp(20.0, 400.0) as f32;
    let onsets: Vec<i64> = detect_onsets(capture, sample_rate, min_gap)
        .into_iter()
        .map(|o| capture_start as i64 + o as i64)
        .collect();
    let window = (window_ms * sample_rate as f64 / 1000.0) as i64;
    let mut residuals = Vec::new();
    for &f in scheduled {
        let expected = f as i64 + estimate;
        if let Some(best) = onsets.iter().copied().min_by_key(|o| (o - expected).abs()) {
            if (best - expected).abs() <= window {
                residuals.push(best - expected);
            }
        }
    }
    if residuals.len() * 10 < scheduled.len() * 6 || residuals.is_empty() {
        return None;
    }
    residuals.sort_unstable();
    let median = residuals[residuals.len() / 2];
    let mut dev: Vec<i64> = residuals.iter().map(|r| (r - median).abs()).collect();
    dev.sort_unstable();
    let mad = dev[dev.len() / 2];
    Some(CalibrationResult {
        residual_frames: median,
        spread_ms: mad as f64 * 1000.0 / sample_rate as f64,
        matched: residuals.len(),
        expected: scheduled.len(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_delayed_clicks() {
        let sr = 48_000u32;
        let delay = 731i64; // true round trip
        let estimate = 600i64; // what timestamps claimed
        let scheduled: Vec<u64> = (0..8).map(|i| 10_000 + i * 24_000).collect();
        let capture_start = 5_000u64;
        let mut cap = vec![0.0f32; 220_000];
        let mut rng = trainer_core::rng::Rng::new(1);
        for v in cap.iter_mut() {
            *v = 0.002 * (rng.f64() as f32 - 0.5);
        }
        for &f in &scheduled {
            let at = (f as i64 + delay - capture_start as i64) as usize;
            for k in 0..200 {
                cap[at + k] +=
                    0.5 * (-(k as f32) / 40.0).exp() * if k % 2 == 0 { 1.0 } else { -1.0 };
            }
        }
        let r = analyze_clicks(&cap, capture_start, &scheduled, estimate, sr, 250.0).unwrap();
        assert_eq!(r.matched, 8);
        assert_eq!(r.residual_frames, delay - estimate);
        assert!(r.spread_ms < 0.1);
    }

    #[test]
    fn fails_without_clicks() {
        let cap = vec![0.0f32; 48_000];
        assert!(analyze_clicks(&cap, 0, &[1000, 25_000], 0, 48_000, 200.0).is_none());
    }
}
