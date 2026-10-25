//! Load a speech file (WAV, FLAC or MP3), trim silence, normalise and resample.

use std::path::Path;

use anyhow::{Context, Result, anyhow};
use symphonia::core::audio::{AudioBufferRef, Signal};
use symphonia::core::codecs::{CODEC_TYPE_NULL, DecoderOptions};
use symphonia::core::errors::Error as SymError;
use symphonia::core::formats::FormatOptions;
use symphonia::core::io::MediaSourceStream;
use symphonia::core::meta::MetadataOptions;
use symphonia::core::probe::Hint;
use symphonia::core::sample::Sample;

use crate::SoundBuffer;

/// Decodes `path` to a stereo buffer at `target_sr`.
pub fn load(path: &Path, target_sr: u32) -> Result<SoundBuffer> {
    let file = std::fs::File::open(path).with_context(|| format!("opening {}", path.display()))?;
    let mss = MediaSourceStream::new(Box::new(file), Default::default());
    let mut hint = Hint::new();
    if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
        hint.with_extension(ext);
    }
    let probed = symphonia::default::get_probe()
        .format(
            &hint,
            mss,
            &FormatOptions::default(),
            &MetadataOptions::default(),
        )
        .with_context(|| format!("reading {}", path.display()))?;
    let mut format = probed.format;
    let track = format
        .tracks()
        .iter()
        .find(|t| t.codec_params.codec != CODEC_TYPE_NULL)
        .context("the speech file has no audio")?;
    let track_id = track.id;
    let sr = track
        .codec_params
        .sample_rate
        .context("the speech file has no sample rate")?;
    let mut decoder = symphonia::default::get_codecs()
        .make(&track.codec_params, &DecoderOptions::default())
        .context("could not decode the speech file")?;
    let mut mono = Vec::new();
    loop {
        let packet = match format.next_packet() {
            Ok(p) => p,
            Err(SymError::IoError(e)) if e.kind() == std::io::ErrorKind::UnexpectedEof => break,
            Err(SymError::ResetRequired) => {
                decoder.reset();
                continue;
            }
            Err(SymError::IoError(_)) => break,
            Err(_) => break,
        };
        if packet.track_id() != track_id {
            continue;
        }
        match decoder.decode(&packet) {
            Ok(decoded) => append_mono(decoded, &mut mono),
            Err(SymError::DecodeError(_)) => continue,
            Err(_) => break,
        }
    }
    if mono.is_empty() {
        return Err(anyhow!("{} has no samples", path.display()));
    }
    let mono = prepare(&mono, sr, target_sr.max(1));
    Ok(SoundBuffer::from_mono(target_sr.max(1), &mono))
}

fn append_mono(decoded: AudioBufferRef<'_>, out: &mut Vec<f32>) {
    match decoded {
        AudioBufferRef::U8(b) => mix(&b, out, |s| (s as f32 - 128.0) / 128.0),
        AudioBufferRef::U16(b) => mix(&b, out, |s| (s as f32 - 32768.0) / 32768.0),
        AudioBufferRef::U32(b) => mix(&b, out, |s| (s as f32 / u32::MAX as f32) * 2.0 - 1.0),
        AudioBufferRef::S8(b) => mix(&b, out, |s| s as f32 / 128.0),
        AudioBufferRef::S16(b) => mix(&b, out, |s| s as f32 / i16::MAX as f32),
        AudioBufferRef::S32(b) => mix(&b, out, |s| s as f32 / i32::MAX as f32),
        AudioBufferRef::F32(b) => mix(&b, out, |s| s),
        AudioBufferRef::F64(b) => mix(&b, out, |s| s as f32),
        _ => {}
    }
}

fn mix<S: Sample + Copy>(
    buf: &symphonia::core::audio::AudioBuffer<S>,
    out: &mut Vec<f32>,
    map: impl Fn(S) -> f32,
) {
    let chans = buf.spec().channels.count();
    let n = buf.frames();
    if chans == 0 || n == 0 {
        return;
    }
    for i in 0..n {
        let mut sum = 0.0f32;
        for c in 0..chans {
            sum += map(buf.chan(c)[i]);
        }
        out.push(sum / chans as f32);
    }
}

/// Drops leading and trailing silence, sets a steady loudness, and resamples.
fn prepare(input: &[f32], from_sr: u32, to_sr: u32) -> Vec<f32> {
    let thresh = 0.01f32;
    let start = input.iter().position(|s| s.abs() > thresh).unwrap_or(0);
    let end = input
        .iter()
        .rposition(|s| s.abs() > thresh)
        .unwrap_or(input.len().saturating_sub(1));
    let pad = (from_sr as usize / 100).max(1);
    let a = start.saturating_sub(pad);
    let b = (end + 1 + pad).min(input.len());
    let mut samples = input[a..b].to_vec();
    if samples.is_empty() {
        return samples;
    }
    let rms = (samples.iter().map(|s| s * s).sum::<f32>() / samples.len() as f32).sqrt();
    if rms > 1e-4 {
        let g = 0.1 / rms;
        for s in &mut samples {
            *s *= g;
        }
    }
    let peak = samples.iter().fold(0.0f32, |m, s| m.max(s.abs()));
    if peak > 0.98 {
        let g = 0.98 / peak;
        for s in &mut samples {
            *s *= g;
        }
    }
    resample(&samples, from_sr, to_sr)
}

fn resample(input: &[f32], from: u32, to: u32) -> Vec<f32> {
    if input.is_empty() || from == 0 || to == 0 || from == to {
        return input.to_vec();
    }
    let ratio = from as f64 / to as f64;
    let out_len = ((input.len() as f64) / ratio).round().max(1.0) as usize;
    let mut out = Vec::with_capacity(out_len);
    for i in 0..out_len {
        let x = i as f64 * ratio;
        let i0 = (x.floor() as usize).min(input.len() - 1);
        let i1 = (i0 + 1).min(input.len() - 1);
        let frac = (x - i0 as f64) as f32;
        out.push(input[i0] * (1.0 - frac) + input[i1] * frac);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_wav_is_trimmed_normalised_and_resampled() {
        let dir = std::env::temp_dir().join(format!("speech-load-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("C.wav");
        let spec = hound::WavSpec {
            channels: 1,
            sample_rate: 8000,
            bits_per_sample: 16,
            sample_format: hound::SampleFormat::Int,
        };
        let mut w = hound::WavWriter::create(&path, spec).unwrap();
        for _ in 0..4000 {
            w.write_sample(0i16).unwrap();
        }
        for i in 0..8000 {
            let s = (i as f32 / 40.0).sin() * 8000.0;
            w.write_sample(s as i16).unwrap();
        }
        for _ in 0..4000 {
            w.write_sample(0i16).unwrap();
        }
        w.finalize().unwrap();
        let buf = load(&path, 16000).unwrap();
        assert_eq!(buf.sample_rate, 16000);
        // The tone was one second; silence is trimmed, so the result is near two seconds at the new rate, not four.
        assert!(buf.frames() < 16000 * 3, "{}", buf.frames());
        assert!(buf.frames() > 8000, "{}", buf.frames());
        assert!(buf.peak() <= 0.99);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
