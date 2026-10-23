//! Pitch detection.
//!
//! - [`MpmDetector`]: McLeod Pitch Method on one window (normalised square difference function).
//! - [`LivePitch`]: streaming wrapper used for the tuner and live pitch line.
//! - [`track`]: post-trial tracker. Per-frame MPM candidates are smoothed with a Viterbi pass so a
//!   single octave error or noisy frame cannot win against its neighbours.

use std::sync::Arc;

use rustfft::num_complex::Complex;
use rustfft::{Fft, FftPlanner};
use serde::{Deserialize, Serialize};

use crate::theory::hz_to_midi;

/// MPM threshold: the first key maximum within this fraction of the highest one is the period.
const MPM_K: f32 = 0.9;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Candidate {
    /// Interpolated period in samples.
    pub tau: f32,
    /// NSDF value at the peak (0..1), a periodicity confidence.
    pub clarity: f32,
}

pub struct MpmDetector {
    size: usize,
    fft_len: usize,
    fft: Arc<dyn Fft<f32>>,
    ifft: Arc<dyn Fft<f32>>,
    buf: Vec<Complex<f32>>,
    scratch: Vec<Complex<f32>>,
    frame: Vec<f32>,
    nsdf: Vec<f32>,
}

impl MpmDetector {
    pub fn new(size: usize) -> Self {
        let fft_len = (2 * size).next_power_of_two();
        let mut planner = FftPlanner::<f32>::new();
        let fft = planner.plan_fft_forward(fft_len);
        let ifft = planner.plan_fft_inverse(fft_len);
        let scratch_len = fft
            .get_inplace_scratch_len()
            .max(ifft.get_inplace_scratch_len());
        MpmDetector {
            size,
            fft_len,
            fft,
            ifft,
            buf: vec![Complex::new(0.0, 0.0); fft_len],
            scratch: vec![Complex::new(0.0, 0.0); scratch_len],
            frame: vec![0.0; size],
            nsdf: vec![0.0; size],
        }
    }

    pub fn size(&self) -> usize {
        self.size
    }

    fn compute_nsdf(&mut self, x: &[f32]) {
        let n = self.size;
        debug_assert_eq!(x.len(), n);
        let mean = x.iter().sum::<f32>() / n as f32;
        for (dst, &src) in self.frame.iter_mut().zip(x) {
            *dst = src - mean;
        }
        for (i, c) in self.buf.iter_mut().enumerate() {
            *c = Complex::new(if i < n { self.frame[i] } else { 0.0 }, 0.0);
        }
        self.fft
            .process_with_scratch(&mut self.buf, &mut self.scratch);
        for c in self.buf.iter_mut() {
            *c = Complex::new(c.norm_sqr(), 0.0);
        }
        self.ifft
            .process_with_scratch(&mut self.buf, &mut self.scratch);
        let scale = 1.0 / self.fft_len as f32;
        let mut m: f32 = 2.0 * self.frame.iter().map(|v| v * v).sum::<f32>();
        self.nsdf[0] = if m > 1e-12 { 1.0 } else { 0.0 };
        for tau in 1..n {
            let a = self.frame[tau - 1];
            let b = self.frame[n - tau];
            m -= a * a + b * b;
            let r = self.buf[tau].re * scale;
            self.nsdf[tau] = if m > 1e-9 {
                (2.0 * r / m).clamp(-1.0, 1.0)
            } else {
                0.0
            };
        }
    }

    /// Key maxima of the NSDF with period in `[tau_min, tau_max]`, in order of increasing period.
    pub fn candidates(
        &mut self,
        x: &[f32],
        tau_min: usize,
        tau_max: usize,
        out: &mut Vec<Candidate>,
    ) {
        out.clear();
        if x.len() != self.size {
            return;
        }
        self.compute_nsdf(x);
        let n = self.size;
        let tau_max = tau_max.min(n - 2);
        let nsdf = &self.nsdf;
        // Skip the zero-lag lobe.
        let mut tau = 1;
        while tau < n - 1 && nsdf[tau] > 0.0 {
            tau += 1;
        }
        let mut in_lobe = false;
        let mut best_tau = 0usize;
        let mut best_val = f32::MIN;
        while tau <= tau_max + 1 && tau < n - 1 {
            let v = nsdf[tau];
            if v > 0.0 {
                if !in_lobe {
                    in_lobe = true;
                    best_val = f32::MIN;
                }
                if v > best_val {
                    best_val = v;
                    best_tau = tau;
                }
            } else if in_lobe {
                in_lobe = false;
                push_peak(nsdf, best_tau, tau_min, tau_max, out);
            }
            tau += 1;
        }
        if in_lobe {
            push_peak(nsdf, best_tau, tau_min, tau_max, out);
        }
    }

    /// Returns `(frequency_hz, clarity)` using the MPM selection rule.
    pub fn detect(
        &mut self,
        x: &[f32],
        sample_rate: f32,
        fmin: f32,
        fmax: f32,
    ) -> Option<(f32, f32)> {
        let mut cands = Vec::with_capacity(8);
        let tau_min = (sample_rate / fmax).floor().max(2.0) as usize;
        let tau_max = (sample_rate / fmin).ceil() as usize;
        self.candidates(x, tau_min, tau_max, &mut cands);
        mpm_pick(&cands).map(|c| (sample_rate / c.tau, c.clarity))
    }
}

fn push_peak(nsdf: &[f32], t: usize, tau_min: usize, tau_max: usize, out: &mut Vec<Candidate>) {
    if t < tau_min || t > tau_max || t == 0 || t + 1 >= nsdf.len() {
        return;
    }
    let (a, b, c) = (nsdf[t - 1], nsdf[t], nsdf[t + 1]);
    let denom = a - 2.0 * b + c;
    let delta = if denom.abs() > 1e-12 {
        (0.5 * (a - c) / denom).clamp(-0.5, 0.5)
    } else {
        0.0
    };
    let value = b - 0.25 * (a - c) * delta;
    out.push(Candidate {
        tau: t as f32 + delta,
        clarity: value.clamp(0.0, 1.0),
    });
}

/// The first candidate whose clarity is within `MPM_K` of the best one.
pub fn mpm_pick(cands: &[Candidate]) -> Option<Candidate> {
    let max = cands.iter().map(|c| c.clarity).fold(0.0f32, f32::max);
    if max <= 0.0 {
        return None;
    }
    cands.iter().copied().find(|c| c.clarity >= MPM_K * max)
}

pub fn rms_db(x: &[f32]) -> f32 {
    if x.is_empty() {
        return -120.0;
    }
    let ms = x.iter().map(|v| v * v).sum::<f32>() / x.len() as f32;
    10.0 * ms.max(1e-12).log10()
}

/// Window length suited to the lowest expected pitch (at least ~2.2 periods), as a power of two.
pub fn window_for(sample_rate: f32, fmin: f32) -> usize {
    let needed = (2.2 * sample_rate / fmin.max(20.0)) as usize;
    needed.next_power_of_two().clamp(1024, 4096)
}

// ---------------------------------------------------------------------------------------------
// Live pitch
// ---------------------------------------------------------------------------------------------

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct LiveFrame {
    /// Absolute index (in input samples) of the sample just after the analysis window.
    pub end_sample: u64,
    pub hz: Option<f32>,
    pub midi: Option<f32>,
    pub clarity: f32,
    pub rms_db: f32,
}

pub struct LivePitch {
    det: MpmDetector,
    window: usize,
    hop: usize,
    ring: Vec<f32>,
    write: usize,
    filled: usize,
    since_hop: usize,
    total: u64,
    sample_rate: f32,
    fmin: f32,
    fmax: f32,
    a4_hz: f64,
    linear: Vec<f32>,
    cands: Vec<Candidate>,
    last_midi: Option<f32>,
    pending_jump: Option<(f32, u8)>,
    /// Frames below this level are reported as unvoiced.
    pub gate_db: f32,
    pub min_clarity: f32,
}

impl LivePitch {
    pub fn new(sample_rate: f32, fmin: f32, fmax: f32, a4_hz: f64) -> Self {
        let window = window_for(sample_rate, fmin);
        let hop = (sample_rate / 100.0).round() as usize;
        LivePitch {
            det: MpmDetector::new(window),
            window,
            hop,
            ring: vec![0.0; window],
            write: 0,
            filled: 0,
            since_hop: 0,
            total: 0,
            sample_rate,
            fmin,
            fmax,
            a4_hz,
            linear: vec![0.0; window],
            cands: Vec::with_capacity(16),
            last_midi: None,
            pending_jump: None,
            gate_db: -55.0,
            min_clarity: 0.8,
        }
    }

    pub fn hop(&self) -> usize {
        self.hop
    }

    pub fn window(&self) -> usize {
        self.window
    }

    pub fn total_samples(&self) -> u64 {
        self.total
    }

    /// Feeds samples; `emit` is called once per hop with the newest frame.
    pub fn push(&mut self, samples: &[f32], mut emit: impl FnMut(LiveFrame)) {
        for &s in samples {
            self.ring[self.write] = s;
            self.write = (self.write + 1) % self.window;
            self.filled = (self.filled + 1).min(self.window);
            self.total += 1;
            self.since_hop += 1;
            if self.since_hop >= self.hop && self.filled == self.window {
                self.since_hop = 0;
                let frame = self.analyze();
                emit(frame);
            }
        }
    }

    fn analyze(&mut self) -> LiveFrame {
        let w = self.window;
        let (tail, head) = self.ring.split_at(self.write);
        self.linear[..head.len()].copy_from_slice(head);
        self.linear[head.len()..].copy_from_slice(tail);
        let level = rms_db(&self.linear);
        let tau_min = (self.sample_rate / self.fmax).floor().max(2.0) as usize;
        let tau_max = ((self.sample_rate / self.fmin).ceil() as usize).min(w / 2);
        self.det
            .candidates(&self.linear, tau_min, tau_max, &mut self.cands);
        let pick = mpm_pick(&self.cands);
        let mut out = LiveFrame {
            end_sample: self.total,
            hz: None,
            midi: None,
            clarity: 0.0,
            rms_db: level,
        };
        if let Some(c) = pick {
            out.clarity = c.clarity;
            if c.clarity >= self.min_clarity && level >= self.gate_db {
                let hz = self.sample_rate / c.tau;
                let midi = hz_to_midi(hz as f64, self.a4_hz) as f32;
                if self.accept(midi) {
                    out.hz = Some(hz);
                    out.midi = Some(midi);
                }
            } else {
                self.last_midi = None;
                self.pending_jump = None;
            }
        } else {
            self.last_midi = None;
            self.pending_jump = None;
        }
        out
    }

    /// Octave-jump guard: a jump of more than ~7 semitones from the previous frame must persist
    /// for three frames before it is shown.
    fn accept(&mut self, midi: f32) -> bool {
        match self.last_midi {
            Some(prev) if (midi - prev).abs() > 7.0 => {
                let count = match self.pending_jump {
                    Some((p, n)) if (p - midi).abs() < 1.0 => n + 1,
                    _ => 1,
                };
                if count >= 3 {
                    self.last_midi = Some(midi);
                    self.pending_jump = None;
                    true
                } else {
                    self.pending_jump = Some((midi, count));
                    false
                }
            }
            _ => {
                self.last_midi = Some(midi);
                self.pending_jump = None;
                true
            }
        }
    }
}

// ---------------------------------------------------------------------------------------------
// Post-trial tracker
// ---------------------------------------------------------------------------------------------

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct TrackOptions {
    pub sample_rate: f32,
    pub fmin: f32,
    pub fmax: f32,
    pub a4_hz: f64,
    /// Hop in seconds (default 10 ms).
    pub hop_s: f32,
}

impl TrackOptions {
    pub fn new(sample_rate: f32) -> Self {
        TrackOptions {
            sample_rate,
            fmin: 60.0,
            fmax: 1400.0,
            a4_hz: crate::theory::DEFAULT_A4_HZ,
            hop_s: 0.01,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct PitchFrame {
    /// Centre of the analysis window, seconds from the start of the capture.
    pub t: f32,
    pub midi: Option<f32>,
    pub clarity: f32,
    pub rms_db: f32,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct PitchTrack {
    pub hop_s: f32,
    pub frames: Vec<PitchFrame>,
}

impl PitchTrack {
    pub fn voiced_count(&self) -> usize {
        self.frames.iter().filter(|f| f.midi.is_some()).count()
    }

    /// Frame index nearest to time `t` (clamped).
    pub fn index_at(&self, t: f32) -> usize {
        if self.frames.is_empty() {
            return 0;
        }
        let t0 = self.frames[0].t;
        (((t - t0) / self.hop_s).round().max(0.0) as usize).min(self.frames.len() - 1)
    }
}

struct FrameCands {
    midis: Vec<f32>,
    clarities: Vec<f32>,
    pick: Option<usize>,
    rms_db: f32,
}

pub fn track(samples: &[f32], opts: &TrackOptions) -> PitchTrack {
    let sr = opts.sample_rate;
    let window = window_for(sr, opts.fmin);
    let hop = ((opts.hop_s * sr).round() as usize).max(1);
    let hop_s = hop as f32 / sr;
    if samples.len() < window {
        return PitchTrack {
            hop_s,
            frames: Vec::new(),
        };
    }
    let mut det = MpmDetector::new(window);
    let tau_min = (sr / opts.fmax).floor().max(2.0) as usize;
    let tau_max = ((sr / opts.fmin).ceil() as usize).min(window / 2);
    let mut cands = Vec::with_capacity(16);
    let mut per_frame: Vec<FrameCands> = Vec::new();
    let mut times = Vec::new();
    let mut start = 0;
    while start + window <= samples.len() {
        let x = &samples[start..start + window];
        let level = rms_db(x);
        det.candidates(x, tau_min, tau_max, &mut cands);
        // Keep the MPM pick plus the strongest few alternatives (octave errors live among them),
        // then restore period order. Periodic signals have equally clear peaks at every multiple
        // of the period, so the pick must be kept explicitly rather than ranked by clarity.
        let mpm = mpm_pick(&cands);
        let mut ranked: Vec<Candidate> =
            cands.iter().copied().filter(|c| c.clarity >= 0.5).collect();
        ranked.sort_by(|a, b| b.clarity.total_cmp(&a.clarity));
        let mut sorted: Vec<Candidate> = mpm.into_iter().collect();
        for c in ranked {
            if sorted.len() >= 5 {
                break;
            }
            if !sorted.iter().any(|k| (k.tau - c.tau).abs() < 0.5) {
                sorted.push(c);
            }
        }
        sorted.sort_by(|a, b| a.tau.total_cmp(&b.tau));
        let pick = mpm.and_then(|p| sorted.iter().position(|c| c.tau == p.tau));
        per_frame.push(FrameCands {
            midis: sorted
                .iter()
                .map(|c| hz_to_midi((sr / c.tau) as f64, opts.a4_hz) as f32)
                .collect(),
            clarities: sorted.iter().map(|c| c.clarity).collect(),
            pick,
            rms_db: level,
        });
        times.push((start + window / 2) as f32 / sr);
        start += hop;
    }

    let gate = level_gate(&per_frame.iter().map(|f| f.rms_db).collect::<Vec<_>>());
    let path = viterbi(&per_frame, gate);

    let mut frames: Vec<PitchFrame> = per_frame
        .iter()
        .zip(&times)
        .zip(&path)
        .map(|((f, &t), state)| {
            let best_clarity = f.clarities.iter().copied().fold(0.0, f32::max);
            match state {
                Some(i) => PitchFrame {
                    t,
                    midi: Some(f.midis[*i]),
                    clarity: f.clarities[*i],
                    rms_db: f.rms_db,
                },
                None => PitchFrame {
                    t,
                    midi: None,
                    clarity: best_clarity,
                    rms_db: f.rms_db,
                },
            }
        })
        .collect();
    remove_short_runs(&mut frames, 3);
    PitchTrack { hop_s, frames }
}

/// Level below which frames are treated as silence: 10 dB above the noise floor, but never more
/// than 25 dB below the loudest frame and never below -65 dBFS.
fn level_gate(levels: &[f32]) -> f32 {
    if levels.is_empty() {
        return -65.0;
    }
    let mut sorted = levels.to_vec();
    sorted.sort_by(|a, b| a.total_cmp(b));
    let floor = sorted[sorted.len() / 10];
    let peak = sorted[sorted.len() - 1];
    (floor + 10.0).min(peak - 25.0).max(-65.0)
}

fn viterbi(frames: &[FrameCands], gate_db: f32) -> Vec<Option<usize>> {
    const W_CLARITY: f32 = 6.0;
    const NOT_PICK: f32 = 0.8;
    const QUIET: f32 = 6.0;
    const SWITCH: f32 = 2.0;
    const STEP: f32 = 0.35;
    const JUMP: f32 = 1.2;
    let n = frames.len();
    if n == 0 {
        return Vec::new();
    }
    // State 0 = unvoiced, state k = candidate k-1.
    let emission = |f: &FrameCands, s: usize| -> f32 {
        let quiet = f.rms_db < gate_db;
        if s == 0 {
            let best = f.clarities.iter().copied().fold(0.0, f32::max);
            let mut c = W_CLARITY * (best - 0.55).max(0.0);
            if !quiet {
                c += 0.3;
            }
            c
        } else {
            let k = s - 1;
            let mut c = W_CLARITY * (1.0 - f.clarities[k]);
            if f.pick != Some(k) {
                c += NOT_PICK;
            }
            if quiet {
                c += QUIET;
            }
            c
        }
    };
    let transition = |a: Option<f32>, b: Option<f32>| -> f32 {
        match (a, b) {
            (None, None) => 0.0,
            (Some(_), None) | (None, Some(_)) => SWITCH,
            (Some(x), Some(y)) => {
                let d = (x - y).abs();
                if d <= 1.0 {
                    STEP * d
                } else {
                    STEP + JUMP * (d - 1.0)
                }
            }
        }
    };
    let state_midi = |f: &FrameCands, s: usize| -> Option<f32> {
        if s == 0 { None } else { Some(f.midis[s - 1]) }
    };

    let mut cost: Vec<f32> = (0..=frames[0].midis.len())
        .map(|s| emission(&frames[0], s))
        .collect();
    let mut back: Vec<Vec<usize>> = vec![vec![0; cost.len()]];
    for t in 1..n {
        let prev = &frames[t - 1];
        let cur = &frames[t];
        let mut next = vec![f32::INFINITY; cur.midis.len() + 1];
        let mut bp = vec![0usize; cur.midis.len() + 1];
        for s in 0..next.len() {
            let e = emission(cur, s);
            let m = state_midi(cur, s);
            for (p, &pc) in cost.iter().enumerate() {
                let c = pc + transition(state_midi(prev, p), m) + e;
                if c < next[s] {
                    next[s] = c;
                    bp[s] = p;
                }
            }
        }
        cost = next;
        back.push(bp);
    }
    let mut s = cost
        .iter()
        .enumerate()
        .min_by(|a, b| a.1.total_cmp(b.1))
        .map(|(i, _)| i)
        .unwrap_or(0);
    let mut path = vec![None; n];
    for t in (0..n).rev() {
        path[t] = if s == 0 { None } else { Some(s - 1) };
        s = back[t][s];
    }
    path
}

/// Voiced runs shorter than `min_frames` become unvoiced.
fn remove_short_runs(frames: &mut [PitchFrame], min_frames: usize) {
    let mut i = 0;
    while i < frames.len() {
        if frames[i].midi.is_some() {
            let start = i;
            while i < frames.len() && frames[i].midi.is_some() {
                i += 1;
            }
            if i - start < min_frames {
                for f in &mut frames[start..i] {
                    f.midi = None;
                }
            }
        } else {
            i += 1;
        }
    }
}

// ---------------------------------------------------------------------------------------------
// Test signals (also used by other modules' tests and the audio crate's examples)
// ---------------------------------------------------------------------------------------------

pub mod synth {
    //! Deterministic voice-like test signals with known pitch trajectories.

    use crate::theory::midi_to_hz;

    /// A harmonic tone whose pitch follows `midi_at(t)`. Harmonic amplitudes fall off like a
    /// sung vowel, with a gentle attack and release.
    pub fn harmonic_tone(
        sample_rate: f32,
        duration_s: f32,
        a4_hz: f64,
        midi_at: impl Fn(f32) -> f32,
    ) -> Vec<f32> {
        let n = (duration_s * sample_rate) as usize;
        let mut out = Vec::with_capacity(n);
        let mut phase = 0.0f64;
        let amps = [1.0, 0.6, 0.45, 0.3, 0.2, 0.12, 0.08, 0.05];
        let norm: f64 = amps.iter().sum();
        for i in 0..n {
            let t = i as f32 / sample_rate;
            let f = midi_to_hz(midi_at(t) as f64, a4_hz);
            phase += 2.0 * std::f64::consts::PI * f / sample_rate as f64;
            let mut s = 0.0;
            for (k, a) in amps.iter().enumerate() {
                if f * (k as f64 + 1.0) < sample_rate as f64 / 2.0 {
                    s += a * ((k as f64 + 1.0) * phase).sin();
                }
            }
            let env = (t / 0.02).min(1.0).min(((duration_s - t) / 0.02).max(0.0));
            out.push((0.5 * s / norm) as f32 * env);
        }
        out
    }

    pub fn silence(sample_rate: f32, duration_s: f32) -> Vec<f32> {
        vec![0.0; (duration_s * sample_rate) as usize]
    }

    /// Low-level white noise from a fixed-seed generator.
    pub fn noise(sample_rate: f32, duration_s: f32, level: f32, seed: u64) -> Vec<f32> {
        let mut rng = crate::rng::Rng::new(seed);
        (0..(duration_s * sample_rate) as usize)
            .map(|_| level * (rng.f64() as f32 * 2.0 - 1.0))
            .collect()
    }

    pub fn mix_into(dst: &mut [f32], src: &[f32]) {
        for (d, s) in dst.iter_mut().zip(src) {
            *d += s;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::synth::*;
    use super::*;

    const SR: f32 = 48_000.0;

    fn midi_of(hz: f64) -> f32 {
        hz_to_midi(hz, 440.0) as f32
    }

    #[test]
    fn mpm_detects_pure_and_harmonic_tones_precisely() {
        let mut det = MpmDetector::new(2048);
        for &target in &[45.0f32, 52.0, 57.0, 64.0, 69.0, 76.0, 84.0] {
            let sig = harmonic_tone(SR, 0.2, 440.0, |_| target + 0.157);
            let (hz, clarity) = det.detect(&sig[2000..4048], SR, 60.0, 1400.0).unwrap();
            let err = (midi_of(hz as f64) - (target + 0.157)) * 100.0;
            assert!(err.abs() < 1.0, "target {target}: error {err} cents");
            assert!(clarity > 0.9);
        }
    }

    #[test]
    fn live_pitch_reports_frames_every_hop() {
        let sig = harmonic_tone(SR, 0.5, 440.0, |_| 60.0);
        let mut live = LivePitch::new(SR, 80.0, 1200.0, 440.0);
        let mut frames = Vec::new();
        for chunk in sig.chunks(128) {
            live.push(chunk, |f| frames.push(f));
        }
        let expected = (sig.len() - live.window()) / live.hop();
        assert!(
            frames.len() >= expected - 1 && frames.len() <= expected + 1,
            "{}",
            frames.len()
        );
        let voiced: Vec<f32> = frames.iter().filter_map(|f| f.midi).collect();
        assert!(voiced.len() > frames.len() / 2);
        for m in voiced {
            assert!((m - 60.0).abs() < 0.05, "{m}");
        }
    }

    #[test]
    fn live_pitch_is_unvoiced_for_silence_and_noise() {
        let mut live = LivePitch::new(SR, 80.0, 1200.0, 440.0);
        let mut sig = silence(SR, 0.3);
        sig.extend(noise(SR, 0.3, 0.05, 3));
        let mut voiced = 0;
        live.push(&sig, |f| voiced += f.midi.is_some() as usize);
        assert!(voiced <= 2, "{voiced}");
    }

    #[test]
    fn tracker_follows_steps_and_rejects_silence() {
        let mut sig = silence(SR, 0.3);
        sig.extend(harmonic_tone(SR, 0.6, 440.0, |_| 57.0));
        sig.extend(harmonic_tone(SR, 0.6, 440.0, |_| 64.0));
        sig.extend(silence(SR, 0.3));
        let tr = track(&sig, &TrackOptions::new(SR));
        let voiced: Vec<&PitchFrame> = tr.frames.iter().filter(|f| f.midi.is_some()).collect();
        assert!(voiced.len() > 90, "{}", voiced.len());
        for f in &tr.frames {
            if f.t < 0.25 || f.t > 1.55 {
                assert!(f.midi.is_none(), "voiced in silence at {}", f.t);
            }
            if (0.4..0.85).contains(&f.t) {
                assert!((f.midi.unwrap() - 57.0).abs() < 0.05, "{:?}", f);
            }
            if (1.0..1.45).contains(&f.t) {
                assert!((f.midi.unwrap() - 64.0).abs() < 0.05, "{:?}", f);
            }
        }
    }

    #[test]
    fn tracker_survives_noise_floor() {
        let mut sig = harmonic_tone(SR, 1.0, 440.0, |_| 50.0);
        mix_into(&mut sig, &noise(SR, 1.0, 0.01, 11));
        let tr = track(&sig, &TrackOptions::new(SR));
        let v: Vec<f32> = tr.frames.iter().filter_map(|f| f.midi).collect();
        assert!(v.len() > 80);
        let mean = v.iter().sum::<f32>() / v.len() as f32;
        assert!((mean - 50.0).abs() < 0.05, "{mean}");
    }
}
