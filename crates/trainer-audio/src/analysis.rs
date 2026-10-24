//! The analysis thread: drains captured input, keeps a rolling history for trial captures, runs
//! the live pitch tracker for the tuner/pitch lane, and tracks the input/output latency.

use std::collections::VecDeque;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicI64, Ordering};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use crossbeam_channel::Sender;
use parking_lot::{Condvar, Mutex};
use rtrb::Consumer;
use serde::{Deserialize, Serialize};
use trainer_core::pitch::LivePitch;
use ts_rs::TS;

use crate::shared::Shared;

/// Rolling buffer of the most recent input samples, indexed by absolute input frame.
pub struct InputHistory {
    buf: Vec<f32>,
    write: usize,
    len: usize,
    /// Absolute frame index of the next sample to be written.
    end: u64,
}

impl InputHistory {
    pub fn new(capacity: usize) -> Self {
        InputHistory {
            buf: vec![0.0; capacity.max(1)],
            write: 0,
            len: 0,
            end: 0,
        }
    }

    pub fn push(&mut self, samples: &[f32]) {
        let cap = self.buf.len();
        for &s in samples {
            self.buf[self.write] = s;
            self.write = (self.write + 1) % cap;
        }
        self.len = (self.len + samples.len()).min(cap);
        self.end += samples.len() as u64;
    }

    pub fn push_silence(&mut self, n: usize) {
        let zeros = vec![0.0; n.min(self.buf.len())];
        self.push(&zeros);
        if n > zeros.len() {
            self.end += (n - zeros.len()) as u64;
        }
    }

    pub fn start(&self) -> u64 {
        self.end - self.len as u64
    }

    pub fn end(&self) -> u64 {
        self.end
    }

    /// Samples for absolute frames `[a, b)`, or `None` if part of the range is no longer held.
    pub fn copy_range(&self, a: u64, b: u64) -> Option<Vec<f32>> {
        if b < a || a < self.start() || b > self.end {
            return None;
        }
        let cap = self.buf.len();
        let n = (b - a) as usize;
        let back = (self.end - a) as usize;
        let first = (self.write + cap - back % cap) % cap;
        let mut out = Vec::with_capacity(n);
        for i in 0..n {
            out.push(self.buf[(first + i) % cap]);
        }
        Some(out)
    }
}

pub type SharedHistory = Arc<(Mutex<InputHistory>, Condvar)>;

/// One live analysis frame (every 10 ms).
#[derive(Clone, Copy, Debug, Serialize, Deserialize, TS)]
pub struct LiveFrame {
    /// Absolute input frame at the end of the analysis window.
    pub in_frame: u64,
    pub hz: Option<f32>,
    pub midi: Option<f32>,
    pub clarity: f32,
    pub rms_db: f32,
    /// Peak absolute input sample since the previous frame.
    pub peak: f32,
}

/// Running estimate of the output→input round trip from stream timestamps.
#[derive(Default)]
pub struct LatencyTracker {
    estimate: AtomicI64,
    valid: AtomicBool,
}

impl LatencyTracker {
    /// Frames between a sound being scheduled at output frame F and it arriving at input frame
    /// F + estimate (before any calibration residual).
    pub fn estimate(&self) -> Option<i64> {
        self.valid
            .load(Ordering::Acquire)
            .then(|| self.estimate.load(Ordering::Acquire))
    }

    fn set(&self, v: i64) {
        self.estimate.store(v, Ordering::Release);
        self.valid.store(true, Ordering::Release);
    }
}

pub struct AnalysisConfig {
    pub sample_rate: u32,
    pub fmin: f32,
    pub fmax: f32,
    pub a4_hz: f64,
    pub history_seconds: u32,
}

pub struct AnalysisHandle {
    pub history: SharedHistory,
    pub latency: Arc<LatencyTracker>,
    stop: Arc<AtomicBool>,
    thread: Option<JoinHandle<()>>,
}

impl AnalysisHandle {
    pub fn stop(&mut self) {
        self.stop.store(true, Ordering::Release);
        if let Some(t) = self.thread.take() {
            let _ = t.join();
        }
    }
}

impl Drop for AnalysisHandle {
    fn drop(&mut self) {
        self.stop();
    }
}

pub fn spawn_analysis(
    mut ring: Consumer<f32>,
    shared: Arc<Shared>,
    cfg: AnalysisConfig,
    live_tx: Sender<LiveFrame>,
) -> AnalysisHandle {
    let history: SharedHistory = Arc::new((
        Mutex::new(InputHistory::new(
            (cfg.sample_rate * cfg.history_seconds.max(10)) as usize,
        )),
        Condvar::new(),
    ));
    let latency = Arc::new(LatencyTracker::default());
    let stop = Arc::new(AtomicBool::new(false));
    let (h, l, s) = (history.clone(), latency.clone(), stop.clone());
    let thread = std::thread::Builder::new()
        .name("trainer-analysis".into())
        .spawn(move || {
            let sr = cfg.sample_rate as f64;
            let mut live = LivePitch::new(cfg.sample_rate as f32, cfg.fmin, cfg.fmax, cfg.a4_hz);
            let mut chunk = vec![0.0f32; 8192];
            let mut dropped_seen = 0u64;
            let mut peak = 0.0f32;
            let mut estimates: VecDeque<i64> = VecDeque::with_capacity(64);
            let mut last_latency = Instant::now();
            while !s.load(Ordering::Acquire) {
                let available = ring.slots();
                if available == 0 {
                    std::thread::sleep(Duration::from_millis(2));
                } else {
                    let n = available.min(chunk.len());
                    if let Ok(read) = ring.read_chunk(n) {
                        let (a, b) = read.as_slices();
                        chunk[..a.len()].copy_from_slice(a);
                        chunk[a.len()..a.len() + b.len()].copy_from_slice(b);
                        read.commit_all();
                    }
                    let samples = &chunk[..n];
                    for &v in samples {
                        peak = peak.max(v.abs());
                    }
                    {
                        let (lock, cv) = &*h;
                        lock.lock().push(samples);
                        cv.notify_all();
                    }
                    live.push(samples, |f| {
                        let _ = live_tx.try_send(LiveFrame {
                            in_frame: f.end_sample,
                            hz: f.hz,
                            midi: f.midi,
                            clarity: f.clarity,
                            rms_db: f.rms_db,
                            peak,
                        });
                        shared
                            .input_peak_bits
                            .store(peak.to_bits(), Ordering::Relaxed);
                        peak = 0.0;
                    });
                }
                // Samples the callback could not queue keep their place as silence.
                let dropped = shared.input_overflows.load(Ordering::Relaxed);
                if dropped > dropped_seen {
                    let d = (dropped - dropped_seen) as usize;
                    dropped_seen = dropped;
                    h.0.lock().push_silence(d);
                    live.push(&vec![0.0; d.min(1 << 16)], |_| {});
                }
                if last_latency.elapsed() >= Duration::from_millis(50) {
                    last_latency = Instant::now();
                    if let (Some((of, p)), Some((inf, c))) =
                        (shared.out_stamp.load(), shared.in_stamp.load())
                    {
                        let r = (inf as i64 - of as i64) as f64 + (p as f64 - c as f64) * sr / 1e9;
                        if estimates.len() == 64 {
                            estimates.pop_front();
                        }
                        estimates.push_back(r.round() as i64);
                        let mut sorted: Vec<i64> = estimates.iter().copied().collect();
                        sorted.sort_unstable();
                        l.set(sorted[sorted.len() / 2]);
                    }
                }
            }
        })
        .expect("spawn analysis thread");
    AnalysisHandle {
        history,
        latency,
        stop,
        thread: Some(thread),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn history_copies_absolute_ranges() {
        let mut h = InputHistory::new(10);
        h.push(&[1.0, 2.0, 3.0, 4.0, 5.0, 6.0]);
        assert_eq!(h.copy_range(1, 4), Some(vec![2.0, 3.0, 4.0]));
        h.push(&[7.0, 8.0, 9.0, 10.0, 11.0, 12.0]);
        assert_eq!(h.start(), 2);
        assert_eq!(h.end(), 12);
        assert_eq!(h.copy_range(1, 3), None);
        assert_eq!(h.copy_range(8, 12), Some(vec![9.0, 10.0, 11.0, 12.0]));
        assert_eq!(h.copy_range(2, 4), Some(vec![3.0, 4.0]));
        h.push_silence(3);
        assert_eq!(h.copy_range(12, 15), Some(vec![0.0, 0.0, 0.0]));
    }

    #[test]
    fn analysis_thread_feeds_history_and_live_frames() {
        let (mut tx, rx) = rtrb::RingBuffer::new(48_000);
        let shared = Arc::new(Shared::default());
        let (live_tx, live_rx) = crossbeam_channel::bounded(1024);
        let mut handle = spawn_analysis(
            rx,
            shared.clone(),
            AnalysisConfig {
                sample_rate: 48_000,
                fmin: 70.0,
                fmax: 1200.0,
                a4_hz: 440.0,
                history_seconds: 10,
            },
            live_tx,
        );
        let tone = trainer_core::pitch::synth::harmonic_tone(48_000.0, 0.5, 440.0, |_| 64.0);
        for chunk in tone.chunks(256) {
            for &s in chunk {
                tx.push(s).unwrap();
            }
        }
        let deadline = Instant::now() + Duration::from_secs(5);
        let mut frames = Vec::new();
        while Instant::now() < deadline && frames.len() < 20 {
            if let Ok(f) = live_rx.recv_timeout(Duration::from_millis(100)) {
                frames.push(f);
            }
        }
        while Instant::now() < deadline && handle.history.0.lock().end() < tone.len() as u64 {
            std::thread::sleep(Duration::from_millis(5));
        }
        handle.stop();
        assert!(frames.len() >= 20);
        assert!(
            frames
                .iter()
                .filter_map(|f| f.midi)
                .any(|m| (m - 64.0).abs() < 0.05)
        );
        let hist = handle.history.0.lock();
        assert_eq!(hist.end(), tone.len() as u64);
        assert_eq!(hist.copy_range(100, 110).unwrap(), tone[100..110].to_vec());
    }
}
