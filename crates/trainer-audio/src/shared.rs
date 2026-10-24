//! State shared between the audio callbacks and the rest of the app, using only atomics.

use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering};

/// A (frame, nanoseconds) pair written by one thread and read by others without tearing.
#[derive(Default)]
pub struct StampCell {
    seq: AtomicU64,
    frame: AtomicU64,
    nanos: AtomicU64,
}

impl StampCell {
    /// Writer side (one writer only).
    pub fn store(&self, frame: u64, nanos: u64) {
        let s = self.seq.load(Ordering::Relaxed);
        self.seq.store(s.wrapping_add(1), Ordering::Release);
        std::sync::atomic::fence(Ordering::Release);
        self.frame.store(frame, Ordering::Relaxed);
        self.nanos.store(nanos, Ordering::Relaxed);
        self.seq.store(s.wrapping_add(2), Ordering::Release);
    }

    /// Returns `None` if nothing has been written yet.
    pub fn load(&self) -> Option<(u64, u64)> {
        for _ in 0..64 {
            let a = self.seq.load(Ordering::Acquire);
            if a == 0 {
                return None;
            }
            if a % 2 == 1 {
                std::hint::spin_loop();
                continue;
            }
            let f = self.frame.load(Ordering::Relaxed);
            let n = self.nanos.load(Ordering::Relaxed);
            std::sync::atomic::fence(Ordering::Acquire);
            if self.seq.load(Ordering::Relaxed) == a {
                return Some((f, n));
            }
        }
        None
    }
}

#[derive(Default)]
pub struct Shared {
    /// Output frames processed so far (the master clock).
    pub out_frame: AtomicU64,
    /// Input frames received so far.
    pub in_frames: AtomicU64,
    /// Output buffer start frame and the instant its first frame is heard.
    pub out_stamp: StampCell,
    /// Input buffer start frame and the instant its first frame was captured.
    pub in_stamp: StampCell,
    pub out_callbacks: AtomicU64,
    pub in_callbacks: AtomicU64,
    /// Frames in the most recent output callback.
    pub buffer_frames: AtomicU32,
    /// Worst callback processing time as a fraction of the buffer period, in 1/1000.
    pub load_permille: AtomicU32,
    pub xruns: AtomicU32,
    pub input_overflows: AtomicU64,
    pub voices_active: AtomicU32,
    pub voices_dropped: AtomicU32,
    /// Peak of the input in the last analysis window, as f32 bits.
    pub input_peak_bits: AtomicU32,
    pub running: AtomicBool,
}

impl Shared {
    pub fn input_peak(&self) -> f32 {
        f32::from_bits(self.input_peak_bits.load(Ordering::Relaxed))
    }
}
