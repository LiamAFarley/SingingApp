//! Rendered audio held in memory and shared with the audio thread.

use std::sync::Arc;

/// Interleaved stereo samples at the engine sample rate.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct SoundBuffer {
    pub sample_rate: u32,
    /// Interleaved L/R.
    pub data: Vec<f32>,
}

impl SoundBuffer {
    pub fn silent(sample_rate: u32, frames: usize) -> Self {
        SoundBuffer {
            sample_rate,
            data: vec![0.0; frames * 2],
        }
    }

    pub fn from_mono(sample_rate: u32, mono: &[f32]) -> Self {
        let mut data = Vec::with_capacity(mono.len() * 2);
        for &s in mono {
            data.push(s);
            data.push(s);
        }
        SoundBuffer { sample_rate, data }
    }

    pub fn frames(&self) -> usize {
        self.data.len() / 2
    }

    pub fn duration_s(&self) -> f64 {
        self.frames() as f64 / self.sample_rate.max(1) as f64
    }

    /// Mixes `other` into `self` starting at `offset` frames, growing `self` if needed.
    pub fn mix(&mut self, other: &SoundBuffer, offset: usize, gain: f32) {
        let need = (offset + other.frames()) * 2;
        if self.data.len() < need {
            self.data.resize(need, 0.0);
        }
        for (d, s) in self.data[offset * 2..need].iter_mut().zip(&other.data) {
            *d += s * gain;
        }
    }

    pub fn peak(&self) -> f32 {
        self.data.iter().fold(0.0f32, |m, v| m.max(v.abs()))
    }

    /// Scales down to `limit` peak if louder (keeps quiet buffers unchanged).
    pub fn limit(&mut self, limit: f32) {
        let p = self.peak();
        if p > limit && p > 0.0 {
            let g = limit / p;
            for v in &mut self.data {
                *v *= g;
            }
        }
    }

    pub fn into_shared(self) -> Arc<SoundBuffer> {
        Arc::new(self)
    }
}
