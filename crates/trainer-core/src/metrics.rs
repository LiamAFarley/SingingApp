//! Note segmentation and per-note vocal metrics (note centre, settling, drift, stability, vibrato).

use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::pitch::{PitchFrame, PitchTrack};

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct NoteSegment {
    /// First frame index (inclusive).
    pub start: usize,
    /// Last frame index (exclusive).
    pub end: usize,
    pub onset_s: f32,
    pub offset_s: f32,
    pub median_midi: f32,
}

impl NoteSegment {
    pub fn duration_s(&self) -> f32 {
        self.offset_s - self.onset_s
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct SegmentOptions {
    /// A pitch change larger than this (semitones), held for `change_frames`, starts a new note.
    pub change_semitones: f32,
    pub change_frames: usize,
    /// Unvoiced gaps up to this many frames inside a note are bridged.
    pub max_gap_frames: usize,
    /// Segments shorter than this are merged into a neighbour.
    pub min_note_s: f32,
    /// A level dip of this many dB followed by a rise splits repeated same-pitch notes.
    pub dip_db: f32,
}

impl Default for SegmentOptions {
    fn default() -> Self {
        SegmentOptions {
            change_semitones: 0.6,
            change_frames: 4,
            max_gap_frames: 3,
            min_note_s: 0.12,
            dip_db: 9.0,
        }
    }
}

fn median(values: &mut [f32]) -> f32 {
    if values.is_empty() {
        return f32::NAN;
    }
    values.sort_by(|a, b| a.total_cmp(b));
    let n = values.len();
    if n % 2 == 1 {
        values[n / 2]
    } else {
        0.5 * (values[n / 2 - 1] + values[n / 2])
    }
}

fn median_midi(frames: &[PitchFrame]) -> f32 {
    let mut v: Vec<f32> = frames.iter().filter_map(|f| f.midi).collect();
    median(&mut v)
}

/// Splits a pitch track into sung notes.
pub fn segment_notes(track: &PitchTrack, opts: &SegmentOptions) -> Vec<NoteSegment> {
    let frames = &track.frames;
    let hop = track.hop_s;
    let n = frames.len();

    // 1. Voiced runs, bridging short gaps where the pitch continues.
    let mut runs: Vec<(usize, usize)> = Vec::new();
    let mut i = 0;
    while i < n {
        if frames[i].midi.is_none() {
            i += 1;
            continue;
        }
        let start = i;
        let mut j = i;
        let end = loop {
            while j < n && frames[j].midi.is_some() {
                j += 1;
            }
            let end = j;
            let gap_start = j;
            while j < n && frames[j].midi.is_none() {
                j += 1;
            }
            let bridged = j < n
                && j - gap_start <= opts.max_gap_frames
                && (frames[j].midi.unwrap() - frames[end - 1].midi.unwrap()).abs()
                    < opts.change_semitones;
            if !bridged {
                break end;
            }
        };
        runs.push((start, end));
        i = end;
    }

    // 2. Split runs at sustained pitch changes and at level dips. Pitch changes are judged on a
    //    low-passed contour so vibrato (4–8 Hz) cannot look like a new note.
    let frame_rate = 1.0 / hop;
    let mut segs: Vec<(usize, usize)> = Vec::new();
    for (start, end) in runs {
        let mut held = Vec::with_capacity(end - start);
        let mut last = frames[start].midi.unwrap();
        for f in &frames[start..end] {
            if let Some(m) = f.midi {
                last = m;
            }
            held.push(last);
        }
        let contour = if held.len() >= 8 {
            lowpass_zero_phase(&held, 4.0, frame_rate)
        } else {
            held.clone()
        };
        let mut cur = start;
        let mut k = start + 2;
        while k < end {
            let mut window: Vec<f32> = contour[cur - start..k - start].to_vec();
            let ref_midi = median(&mut window);
            let deviates = |x: usize| (contour[x - start] - ref_midi).abs() > opts.change_semitones;
            let split = if deviates(k) {
                let hold_end = (k + opts.change_frames).min(end);
                hold_end - k >= opts.change_frames.min(end - k).max(2)
                    && (k..hold_end).all(deviates)
            } else {
                is_level_dip(frames, k, cur, end, opts.dip_db)
            };
            if split {
                segs.push((cur, k));
                cur = k;
                k = cur + 2;
            } else {
                k += 1;
            }
        }
        segs.push((cur, end));
    }

    // 3. Merge very short segments: onto the previous note when adjacent and close in pitch
    //    (a release), otherwise onto the following adjacent note (a scoop into it).
    let min_frames = ((opts.min_note_s / hop).round() as usize).max(1);
    let mut out: Vec<(usize, usize)> = Vec::new();
    let mut pending: Option<(usize, usize)> = None;
    for &(seg_start, e) in &segs {
        let mut s = seg_start;
        if let Some((ps, pe)) = pending.take() {
            if pe == s {
                s = ps;
            }
        }
        if e - s >= min_frames {
            out.push((s, e));
            continue;
        }
        let m = median_midi(&frames[s..e]);
        if let Some(last) = out.last_mut() {
            if last.1 == s && (median_midi(&frames[last.0..last.1]) - m).abs() < 1.5 {
                last.1 = e;
                continue;
            }
        }
        pending = Some((s, e));
    }
    if let (Some((ps, pe)), Some(last)) = (pending, out.last_mut()) {
        if last.1 == ps {
            last.1 = pe;
        }
    }

    out.into_iter()
        .filter(|(s, e)| e > s)
        .map(|(s, e)| NoteSegment {
            start: s,
            end: e,
            onset_s: frames[s].t,
            offset_s: frames[e - 1].t + hop,
            median_midi: median_midi(&frames[s..e]),
        })
        .collect()
}

/// A level minimum at `k` at least `dip_db` below the louder of the preceding peak and the
/// following recovery, with voiced material on both sides.
fn is_level_dip(
    frames: &[PitchFrame],
    k: usize,
    seg_start: usize,
    end: usize,
    dip_db: f32,
) -> bool {
    if k < seg_start + 4 || k + 4 > end {
        return false;
    }
    let lvl = frames[k].rms_db;
    let is_min = frames[k - 1].rms_db >= lvl && frames[k + 1].rms_db > lvl;
    if !is_min {
        return false;
    }
    let before = frames[seg_start..k]
        .iter()
        .map(|f| f.rms_db)
        .fold(f32::MIN, f32::max);
    let after = frames[k + 1..(k + 7).min(end)]
        .iter()
        .map(|f| f.rms_db)
        .fold(f32::MIN, f32::max);
    before - lvl >= dip_db && after - lvl >= dip_db * 0.67
}

// ---------------------------------------------------------------------------------------------
// Filters and statistics
// ---------------------------------------------------------------------------------------------

/// Zero-phase 2nd-order Butterworth low-pass (forward-backward), with reflected padding.
pub fn lowpass_zero_phase(x: &[f32], cutoff_hz: f32, sample_rate: f32) -> Vec<f32> {
    let n = x.len();
    if n < 4 {
        return x.to_vec();
    }
    let k = (std::f32::consts::PI * cutoff_hz / sample_rate).tan();
    let q = std::f32::consts::FRAC_1_SQRT_2;
    let norm = 1.0 / (1.0 + k / q + k * k);
    let b0 = k * k * norm;
    let b1 = 2.0 * b0;
    let b2 = b0;
    let a1 = 2.0 * (k * k - 1.0) * norm;
    let a2 = (1.0 - k / q + k * k) * norm;
    let pad = (n - 1).min(30);
    let mut padded = Vec::with_capacity(n + 2 * pad);
    for i in (1..=pad).rev() {
        padded.push(2.0 * x[0] - x[i]);
    }
    padded.extend_from_slice(x);
    for i in 1..=pad {
        padded.push(2.0 * x[n - 1] - x[n - 1 - i]);
    }
    let run = |v: &mut Vec<f32>| {
        let (mut x1, mut x2, mut y1, mut y2) = (v[0], v[0], v[0], v[0]);
        for s in v.iter_mut() {
            let x0 = *s;
            let y0 = b0 * x0 + b1 * x1 + b2 * x2 - a1 * y1 - a2 * y2;
            x2 = x1;
            x1 = x0;
            y2 = y1;
            y1 = y0;
            *s = y0;
        }
    };
    run(&mut padded);
    padded.reverse();
    run(&mut padded);
    padded.reverse();
    padded[pad..pad + n].to_vec()
}

/// Least-squares line through (t, y); returns (slope, intercept).
pub fn linear_fit(t: &[f32], y: &[f32]) -> (f32, f32) {
    let n = t.len().min(y.len());
    if n < 2 {
        return (0.0, y.first().copied().unwrap_or(0.0));
    }
    let mt = t[..n].iter().sum::<f32>() / n as f32;
    let my = y[..n].iter().sum::<f32>() / n as f32;
    let mut num = 0.0;
    let mut den = 0.0;
    for i in 0..n {
        num += (t[i] - mt) * (y[i] - my);
        den += (t[i] - mt) * (t[i] - mt);
    }
    let slope = if den > 1e-12 { num / den } else { 0.0 };
    (slope, my - slope * mt)
}

fn std_dev(v: &[f32]) -> f32 {
    if v.len() < 2 {
        return 0.0;
    }
    let m = v.iter().sum::<f32>() / v.len() as f32;
    (v.iter().map(|x| (x - m) * (x - m)).sum::<f32>() / (v.len() - 1) as f32).sqrt()
}

// ---------------------------------------------------------------------------------------------
// Per-note metrics
// ---------------------------------------------------------------------------------------------

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, TS)]
pub struct Vibrato {
    pub rate_hz: f32,
    /// Half the peak-to-peak excursion, in cents.
    pub extent_cents: f32,
    /// Cycle-to-cycle coefficient of variation of the half-period (lower is more regular).
    pub irregularity: f32,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, TS)]
pub struct NoteMetrics {
    pub onset_s: f32,
    pub offset_s: f32,
    pub duration_ms: f32,
    /// The target the errors are measured against (after octave or relative adjustment).
    pub target_midi: f32,
    /// Median pitch of the whole note.
    pub sung_midi: f32,
    pub initial_cents: f32,
    /// The first reliable pitch: median error 40–140 ms after voicing starts, past the onset
    /// transient and before most corrections.
    pub attack_cents: f32,
    /// How far the note moved toward the target after the attack, |attack| − |centre|;
    /// negative when it moved away.
    pub correction_cents: f32,
    /// Error of the note centre over the settled part (vibrato excluded).
    pub centre_cents: f32,
    pub settled: bool,
    pub settling_ms: Option<f32>,
    pub drift_cents_per_s: f32,
    /// SD of the detrended note centre over the settled part.
    pub stability_cents: f32,
    pub vibrato: Option<Vibrato>,
    pub voiced_fraction: f32,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct NoteMetricOptions {
    pub stable_tol_cents: f32,
    pub hold_ms: f32,
}

impl Default for NoteMetricOptions {
    fn default() -> Self {
        NoteMetricOptions {
            stable_tol_cents: 25.0,
            hold_ms: 400.0,
        }
    }
}

/// Metrics for one note (`frames` covers just the note) against `target_midi`.
pub fn note_metrics(
    frames: &[PitchFrame],
    hop_s: f32,
    target_midi: f32,
    opts: &NoteMetricOptions,
) -> Option<NoteMetrics> {
    let voiced: Vec<(f32, f32)> = frames
        .iter()
        .filter_map(|f| f.midi.map(|m| (f.t, m)))
        .collect();
    if voiced.len() < 3 {
        return None;
    }
    let onset_s = voiced[0].0;
    let offset_s = voiced[voiced.len() - 1].0 + hop_s;
    let duration_s = offset_s - onset_s;
    // Fill unvoiced gaps by holding the previous value so filters see a continuous series.
    let mut cents = Vec::with_capacity(frames.len());
    let mut times = Vec::with_capacity(frames.len());
    let mut last = voiced[0].1;
    let first_idx = frames.iter().position(|f| f.midi.is_some()).unwrap();
    let last_idx = frames.iter().rposition(|f| f.midi.is_some()).unwrap();
    for f in &frames[first_idx..=last_idx] {
        if let Some(m) = f.midi {
            last = m;
        }
        cents.push((last - target_midi) * 100.0);
        times.push(f.t);
    }
    let frame_rate = 1.0 / hop_s;
    let centre = if duration_s >= 0.4 {
        lowpass_zero_phase(&cents, 3.0, frame_rate)
    } else {
        let mut c = cents.clone();
        vec![median(&mut c); cents.len()]
    };

    let initial_frames = ((0.08 / hop_s).round() as usize).clamp(2, cents.len());
    let initial_cents = median(&mut cents[..initial_frames].to_vec());
    let mut attack: Vec<f32> = times
        .iter()
        .zip(&cents)
        .filter(|(t, _)| (0.04..0.14).contains(&(**t - onset_s)))
        .map(|(_, c)| *c)
        .collect();
    let attack_cents = if attack.len() >= 2 {
        median(&mut attack)
    } else {
        initial_cents
    };

    let hold_frames = ((opts.hold_ms / 1000.0 / hop_s).round() as usize).max(1);
    let hold_frames = hold_frames
        .min(((cents.len() as f32) * 0.6).ceil() as usize)
        .max(1);
    let mut settle_idx = None;
    'outer: for i in 0..cents.len() {
        if i + hold_frames > cents.len() {
            break;
        }
        for c in &centre[i..i + hold_frames] {
            if c.abs() > opts.stable_tol_cents {
                continue 'outer;
            }
        }
        settle_idx = Some(i);
        break;
    }
    let region_start = settle_idx.unwrap_or(((cents.len() as f32) * 0.4) as usize);
    let region_centre = &centre[region_start..];
    let region_times = &times[region_start..];
    let mut rc = region_centre.to_vec();
    let centre_cents = median(&mut rc);
    let (drift, intercept) = linear_fit(region_times, region_centre);
    let detrended: Vec<f32> = region_times
        .iter()
        .zip(region_centre)
        .map(|(t, c)| c - (drift * t + intercept))
        .collect();
    let stability = std_dev(&detrended);
    let residual: Vec<f32> = cents[region_start..]
        .iter()
        .zip(region_centre)
        .map(|(r, c)| r - c)
        .collect();
    let vibrato = detect_vibrato(&residual, frame_rate);

    let voiced_fraction = voiced.len() as f32 / (last_idx - first_idx + 1) as f32;
    let mut all: Vec<f32> = voiced.iter().map(|v| v.1).collect();
    Some(NoteMetrics {
        onset_s,
        offset_s,
        duration_ms: duration_s * 1000.0,
        target_midi,
        sung_midi: median(&mut all),
        initial_cents,
        attack_cents,
        correction_cents: attack_cents.abs() - centre_cents.abs(),
        centre_cents,
        settled: settle_idx.is_some(),
        settling_ms: settle_idx.map(|i| (times[i] - onset_s) * 1000.0),
        drift_cents_per_s: drift,
        stability_cents: stability,
        vibrato,
        voiced_fraction,
    })
}

/// Periodic modulation of the residual in the 3.5–9 Hz band.
pub fn detect_vibrato(residual: &[f32], frame_rate: f32) -> Option<Vibrato> {
    let n = residual.len();
    if (n as f32) < 0.6 * frame_rate {
        return None;
    }
    let mean = residual.iter().sum::<f32>() / n as f32;
    let r: Vec<f32> = residual.iter().map(|v| v - mean).collect();
    let energy: f32 = r.iter().map(|v| v * v).sum();
    if energy <= 1e-6 {
        return None;
    }
    let lag_min = (frame_rate / 9.0).floor().max(2.0) as usize;
    let lag_max = ((frame_rate / 3.5).ceil() as usize).min(n / 2);
    if lag_max <= lag_min + 1 {
        return None;
    }
    let ac = |lag: usize| -> f32 {
        let mut s = 0.0;
        let mut e1 = 0.0;
        let mut e2 = 0.0;
        for i in 0..n - lag {
            s += r[i] * r[i + lag];
            e1 += r[i] * r[i];
            e2 += r[i + lag] * r[i + lag];
        }
        if e1 > 0.0 && e2 > 0.0 {
            s / (e1 * e2).sqrt()
        } else {
            0.0
        }
    };
    let acs: Vec<f32> = (lag_min - 1..=lag_max + 1).map(ac).collect();
    let mut best = 0usize;
    for i in 1..acs.len() - 1 {
        if acs[i] > acs[best.max(1)] && acs[i] >= acs[i - 1] && acs[i] >= acs[i + 1] {
            best = i;
        }
    }
    if best == 0 || acs[best] < 0.4 {
        return None;
    }
    let (a, b, c) = (acs[best - 1], acs[best], acs[best + 1]);
    let denom = a - 2.0 * b + c;
    let delta = if denom.abs() > 1e-9 {
        (0.5 * (a - c) / denom).clamp(-0.5, 0.5)
    } else {
        0.0
    };
    let lag = (lag_min - 1 + best) as f32 + delta;
    let rate_hz = frame_rate / lag;
    let rms = (energy / n as f32).sqrt();
    let extent = rms * std::f32::consts::SQRT_2;
    if extent < 12.0 {
        return None;
    }
    // Half-period regularity from zero crossings.
    let mut crossings = Vec::new();
    for i in 1..n {
        if (r[i - 1] < 0.0) != (r[i] < 0.0) {
            let frac = r[i - 1] / (r[i - 1] - r[i]);
            crossings.push(i as f32 - 1.0 + frac);
        }
    }
    let halves: Vec<f32> = crossings.windows(2).map(|w| w[1] - w[0]).collect();
    let irregularity = if halves.len() >= 3 {
        let m = halves.iter().sum::<f32>() / halves.len() as f32;
        std_dev(&halves) / m
    } else {
        0.0
    };
    Some(Vibrato {
        rate_hz,
        extent_cents: extent,
        irregularity,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pitch::synth::*;
    use crate::pitch::{TrackOptions, track};

    const SR: f32 = 48_000.0;

    fn padded(mut sig: Vec<f32>) -> Vec<f32> {
        let mut out = silence(SR, 0.2);
        out.append(&mut sig);
        out.extend(silence(SR, 0.2));
        out
    }

    fn single_note(sig: Vec<f32>, target: f32) -> NoteMetrics {
        let tr = track(&padded(sig), &TrackOptions::new(SR));
        let segs = segment_notes(&tr, &SegmentOptions::default());
        assert_eq!(segs.len(), 1, "{segs:?}");
        let s = segs[0];
        note_metrics(
            &tr.frames[s.start..s.end],
            tr.hop_s,
            target,
            &NoteMetricOptions::default(),
        )
        .unwrap()
    }

    #[test]
    fn known_offset_is_measured() {
        // A4 sung at 444.01 Hz: +15.7 cents.
        let m = single_note(harmonic_tone(SR, 1.2, 440.0, |_| 69.157), 69.0);
        assert!((m.centre_cents - 15.7).abs() < 1.0, "{m:?}");
        assert!(m.settled);
        assert!(m.drift_cents_per_s.abs() < 2.0);
        assert!(m.vibrato.is_none());
    }

    #[test]
    fn scoop_settling_time() {
        // Starts 80 cents flat and reaches the target linearly over 150 ms.
        let m = single_note(
            harmonic_tone(SR, 1.5, 440.0, |t| {
                if t < 0.15 {
                    60.0 - 0.8 * (1.0 - t / 0.15)
                } else {
                    60.0
                }
            }),
            60.0,
        );
        assert!(m.initial_cents < -40.0, "{m:?}");
        let settle = m.settling_ms.expect("should settle");
        assert!((60.0..=220.0).contains(&settle), "{settle}");
        assert!(m.centre_cents.abs() < 3.0, "{m:?}");
    }

    #[test]
    fn attack_and_correction() {
        // Starts 60 cents flat for 250 ms, then slides onto the target over 250 ms.
        let m = single_note(
            harmonic_tone(SR, 1.5, 440.0, |t| {
                if t < 0.25 {
                    59.4
                } else if t < 0.5 {
                    59.4 + 0.6 * (t - 0.25) / 0.25
                } else {
                    60.0
                }
            }),
            60.0,
        );
        assert!((m.attack_cents - -60.0).abs() < 8.0, "{m:?}");
        assert!(m.centre_cents.abs() < 5.0, "{m:?}");
        assert!((m.correction_cents - 60.0).abs() < 10.0, "{m:?}");
    }

    #[test]
    fn drift_is_measured() {
        let m = single_note(harmonic_tone(SR, 2.5, 440.0, |t| 64.0 - 0.04 * t), 64.0);
        assert!((m.drift_cents_per_s - -4.0).abs() < 1.0, "{m:?}");
    }

    #[test]
    fn vibrato_does_not_hurt_intonation() {
        let m = single_note(
            harmonic_tone(SR, 2.0, 440.0, |t| {
                62.0 + 0.5 * (2.0 * std::f32::consts::PI * 5.5 * t).sin()
            }),
            62.0,
        );
        assert!(m.centre_cents.abs() < 4.0, "{m:?}");
        let v = m.vibrato.expect("vibrato should be detected");
        assert!((v.rate_hz - 5.5).abs() < 0.25, "{v:?}");
        assert!((v.extent_cents - 50.0).abs() < 12.0, "{v:?}");
        assert!(m.settled, "a well-centred vibrato note counts as settled");
    }

    #[test]
    fn segments_legato_melody() {
        let notes = [60.0f32, 62.0, 64.0, 65.0];
        let sig = harmonic_tone(SR, 1.6, 440.0, |t| notes[((t / 0.4) as usize).min(3)]);
        let tr = track(&padded(sig), &TrackOptions::new(SR));
        let segs = segment_notes(&tr, &SegmentOptions::default());
        assert_eq!(segs.len(), 4, "{segs:?}");
        for (s, n) in segs.iter().zip(notes) {
            assert!((s.median_midi - n).abs() < 0.1, "{s:?}");
        }
    }

    #[test]
    fn separates_repeated_notes_with_gaps() {
        let mut sig = Vec::new();
        for _ in 0..3 {
            sig.extend(harmonic_tone(SR, 0.35, 440.0, |_| 67.0));
            sig.extend(silence(SR, 0.06));
        }
        let tr = track(&padded(sig), &TrackOptions::new(SR));
        let segs = segment_notes(&tr, &SegmentOptions::default());
        assert_eq!(segs.len(), 3, "{segs:?}");
    }

    #[test]
    fn lowpass_removes_vibrato_band() {
        let x: Vec<f32> = (0..300)
            .map(|i| 50.0 * (2.0 * std::f32::consts::PI * 6.0 * i as f32 / 100.0).sin())
            .collect();
        let y = lowpass_zero_phase(&x, 3.0, 100.0);
        let peak = y[50..250].iter().fold(0.0f32, |m, v| m.max(v.abs()));
        assert!(peak < 15.0, "{peak}");
    }
}
