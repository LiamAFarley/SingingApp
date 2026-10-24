//! Offline rendering of stimuli into buffers: SoundFont instruments (rustysynth), built-in
//! band-limited oscillators, Shepard tones, masks, metronome clicks and full sung-trial timelines.
//! Rendering happens on worker threads, never in the audio callback.

use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;

use anyhow::{Context, Result};
use rustysynth::{SoundFont, Synthesizer, SynthesizerSettings};
use trainer_core::rng::Rng;
use trainer_core::schema::{Mask, SHEPARD_GLISSANDO_S, SoundEvent, SungTrial, TargetNote};

/// How fast the memory-clearing glissando falls.
const GLISSANDO_S_PER_OCTAVE: f64 = 4.0;
use trainer_core::theory::{ScaleKind, midi_to_hz};
use trainer_core::timbre::Timbre;

use crate::buffer::SoundBuffer;

/// Instruments loaded from SoundFont files.
#[derive(Default, Clone)]
pub struct SoundFonts {
    /// The General MIDI bank used for `gm:<program>`.
    pub gm: Option<Arc<SoundFont>>,
    /// Extra banks for `sf:<name>:<program>`, keyed by file stem.
    pub banks: HashMap<String, Arc<SoundFont>>,
}

impl SoundFonts {
    pub fn load_file(path: &Path) -> Result<Arc<SoundFont>> {
        let mut f = std::io::BufReader::new(
            std::fs::File::open(path).with_context(|| format!("opening {}", path.display()))?,
        );
        let sf = SoundFont::new(&mut f)
            .map_err(|e| anyhow::anyhow!("reading {}: {e:?}", path.display()))?;
        Ok(Arc::new(sf))
    }

    /// Loads every `.sf2` in `dir` as an extra bank.
    pub fn load_dir(&mut self, dir: &Path) -> Vec<String> {
        let mut loaded = Vec::new();
        let Ok(entries) = std::fs::read_dir(dir) else {
            return loaded;
        };
        for e in entries.flatten() {
            let p = e.path();
            if p.extension()
                .and_then(|x| x.to_str())
                .map(|x| x.eq_ignore_ascii_case("sf2"))
                .unwrap_or(false)
            {
                if let (Some(stem), Ok(sf)) =
                    (p.file_stem().and_then(|s| s.to_str()), Self::load_file(&p))
                {
                    self.banks.insert(stem.to_string(), sf);
                    loaded.push(stem.to_string());
                }
            }
        }
        loaded
    }
}

pub struct Renderer {
    pub sample_rate: u32,
    pub a4_hz: f64,
    fonts: SoundFonts,
    click_hi: Arc<SoundBuffer>,
    click_lo: Arc<SoundBuffer>,
}

fn velocity_for(gain_db: f32) -> i32 {
    (127.0 * 10f32.powf(gain_db / 40.0))
        .round()
        .clamp(1.0, 127.0) as i32
}

fn db_to_gain(db: f32) -> f32 {
    10f32.powf(db / 20.0)
}

impl Renderer {
    pub fn new(sample_rate: u32, fonts: SoundFonts) -> Self {
        let click_hi = Arc::new(click(sample_rate, 1760.0, 0.45));
        let click_lo = Arc::new(click(sample_rate, 1320.0, 0.3));
        Renderer {
            sample_rate,
            a4_hz: trainer_core::theory::DEFAULT_A4_HZ,
            fonts,
            click_hi,
            click_lo,
        }
    }

    pub fn has_gm(&self) -> bool {
        self.fonts.gm.is_some()
    }

    pub fn bank_names(&self) -> Vec<String> {
        let mut v: Vec<String> = self.fonts.banks.keys().cloned().collect();
        v.sort();
        v
    }

    pub fn click(&self, accent: bool) -> Arc<SoundBuffer> {
        if accent {
            self.click_hi.clone()
        } else {
            self.click_lo.clone()
        }
    }

    fn frames(&self, ms: f64) -> usize {
        (ms / 1000.0 * self.sample_rate as f64).round().max(0.0) as usize
    }

    /// Mixes all events into one buffer (with a release tail).
    pub fn render_events(&self, events: &[SoundEvent]) -> SoundBuffer {
        let end_ms = events
            .iter()
            .map(|e| e.start_ms + e.dur_ms)
            .fold(0.0, f64::max);
        let mut out = SoundBuffer::silent(self.sample_rate, self.frames(end_ms + 600.0));
        // SoundFont events are grouped per bank so each bank renders in one synthesizer pass.
        let mut by_bank: HashMap<Option<String>, Vec<(&SoundEvent, u8)>> = HashMap::new();
        for e in events {
            match Timbre::parse(&e.timbre) {
                Some(Timbre::Gm(p)) if self.fonts.gm.is_some() => {
                    by_bank.entry(None).or_default().push((e, p))
                }
                Some(Timbre::SoundFont { bank, program })
                    if self.fonts.banks.contains_key(&bank) =>
                {
                    by_bank.entry(Some(bank)).or_default().push((e, program))
                }
                Some(Timbre::Gm(p)) => {
                    // No General MIDI bank installed: a built-in approximation of the instrument family.
                    let wave = fallback_wave(p);
                    let tone = self.synth_note(wave, e);
                    out.mix(&tone, self.frames(e.start_ms), 1.0);
                }
                Some(Timbre::SoundFont { .. }) => {
                    let tone = self.synth_note("soft_piano", e);
                    out.mix(&tone, self.frames(e.start_ms), 1.0);
                }
                Some(Timbre::Synth(w)) => {
                    let tone = self.synth_note(&w, e);
                    out.mix(&tone, self.frames(e.start_ms), 1.0);
                }
                Some(Timbre::Shepard) => {
                    let tone = self.shepard_note(e);
                    out.mix(&tone, self.frames(e.start_ms), 1.0);
                }
                None => {
                    let tone = self.synth_note("sine", e);
                    out.mix(&tone, self.frames(e.start_ms), 1.0);
                }
            }
        }
        for (bank, evs) in by_bank {
            let sf = match &bank {
                None => self.fonts.gm.clone(),
                Some(b) => self.fonts.banks.get(b).cloned(),
            };
            if let Some(sf) = sf {
                let rendered = self.render_soundfont(&sf, &evs);
                out.mix(&rendered, 0, 1.0);
            }
        }
        out.limit(0.95);
        out
    }

    fn render_soundfont(&self, sf: &Arc<SoundFont>, events: &[(&SoundEvent, u8)]) -> SoundBuffer {
        let mut settings = SynthesizerSettings::new(self.sample_rate as i32);
        settings.enable_reverb_and_chorus = false;
        let Ok(mut synth) = Synthesizer::new(sf, &settings) else {
            return SoundBuffer::silent(self.sample_rate, 0);
        };
        synth.set_master_volume(1.0);
        // One MIDI channel per (program, detune) pair, skipping the drum channel.
        let mut channels: Vec<(u8, i32)> = Vec::new();
        let mut actions: Vec<(usize, i32, i32, i32, bool)> = Vec::new(); // (frame, channel, key, velocity, on)
        for (e, program) in events {
            let detune = e.detune_cents.round() as i32;
            let idx = match channels.iter().position(|c| *c == (*program, detune)) {
                Some(i) => i,
                None => {
                    channels.push((*program, detune));
                    channels.len() - 1
                }
            };
            let ch = (if idx >= 9 { idx + 1 } else { idx }).min(15) as i32;
            let on = self.frames(e.start_ms);
            let off = self.frames(e.start_ms + e.dur_ms).max(on + 1);
            actions.push((on, ch, e.midi.clamp(0, 127), velocity_for(e.gain_db), true));
            actions.push((off, ch, e.midi.clamp(0, 127), 0, false));
        }
        for (i, (program, detune)) in channels.iter().enumerate() {
            let ch = (if i >= 9 { i + 1 } else { i }).min(15) as i32;
            synth.process_midi_message(ch, 0xC0, *program as i32, 0);
            if *detune != 0 {
                // Default pitch-bend range is ±2 semitones.
                let bend = (8192.0 + (*detune as f32 / 200.0) * 8191.0)
                    .round()
                    .clamp(0.0, 16383.0) as i32;
                synth.process_midi_message(ch, 0xE0, bend & 0x7F, bend >> 7);
            }
        }
        actions.sort_by_key(|a| (a.0, !a.4));
        let total = actions.iter().map(|a| a.0).max().unwrap_or(0) + self.frames(600.0);
        let mut left = vec![0.0f32; total];
        let mut right = vec![0.0f32; total];
        let mut pos = 0;
        for (frame, ch, key, vel, on) in actions {
            let frame = frame.min(total);
            if frame > pos {
                synth.render(&mut left[pos..frame], &mut right[pos..frame]);
                pos = frame;
            }
            if on {
                synth.note_on(ch, key, vel);
            } else {
                synth.note_off(ch, key);
            }
        }
        if pos < total {
            synth.render(&mut left[pos..], &mut right[pos..]);
        }
        // SoundFont output is much quieter than the oscillators; bring it to a similar level.
        const SF_GAIN: f32 = 2.5;
        let mut data = Vec::with_capacity(total * 2);
        for i in 0..total {
            data.push(left[i] * SF_GAIN);
            data.push(right[i] * SF_GAIN);
        }
        SoundBuffer {
            sample_rate: self.sample_rate,
            data,
        }
    }

    /// One band-limited oscillator note with attack/release (and decay for `soft_piano`).
    pub fn synth_note(&self, wave: &str, e: &SoundEvent) -> SoundBuffer {
        let sr = self.sample_rate as f64;
        let f0 = midi_to_hz(e.midi as f64 + e.detune_cents as f64 / 100.0, self.a4_hz);
        let dur = e.dur_ms / 1000.0;
        let release = 0.06;
        let n = ((dur + release) * sr) as usize;
        let table = wavetable(wave, f0, sr);
        let amp = db_to_gain(e.gain_db) as f64 * 0.5;
        let piano = wave == "soft_piano";
        let mut mono = Vec::with_capacity(n);
        let mut phase = 0.0f64;
        let step = f0 / sr * table.len() as f64;
        for i in 0..n {
            let t = i as f64 / sr;
            let idx = phase as usize;
            let frac = phase - idx as f64;
            let a = table[idx % table.len()];
            let b = table[(idx + 1) % table.len()];
            let s = a + (b - a) * frac;
            phase = (phase + step) % table.len() as f64;
            let attack = (t / 0.008).min(1.0);
            let rel = if t > dur {
                (1.0 - (t - dur) / release).max(0.0)
            } else {
                1.0
            };
            let decay = if piano {
                (-t * (1.2 + f0 / 600.0)).exp() * 0.85 + 0.15
            } else {
                1.0
            };
            mono.push((s * amp * attack * rel * decay) as f32);
        }
        SoundBuffer::from_mono(self.sample_rate, &mono)
    }

    /// Octave-ambiguous Shepard tone for the pitch class of `e.midi`.
    pub fn shepard_note(&self, e: &SoundEvent) -> SoundBuffer {
        let sr = self.sample_rate as f64;
        let pc = e.midi.rem_euclid(12) as f64;
        let base = midi_to_hz(pc + 12.0 + e.detune_cents as f64 / 100.0, self.a4_hz); // C1 octave
        let centre = (523.25f64).log2();
        let sigma = 1.3;
        let dur = e.dur_ms / 1000.0;
        let release = 0.08;
        let n = ((dur + release) * sr) as usize;
        let mut comps = Vec::new();
        let mut f = base;
        while f < 10_000.0 {
            if f > 25.0 {
                let w = (-0.5 * ((f.log2() - centre) / sigma).powi(2)).exp();
                comps.push((f, w));
            }
            f *= 2.0;
        }
        let norm: f64 = comps.iter().map(|c| c.1).sum::<f64>().max(1e-9);
        let amp = db_to_gain(e.gain_db) as f64 * 0.5 / norm;
        let mut mono = vec![0.0f32; n];
        for (f, w) in comps {
            let k = 2.0 * std::f64::consts::PI * f / sr;
            for (i, s) in mono.iter_mut().enumerate() {
                *s += (w * (k * i as f64).sin()) as f32;
            }
        }
        for (i, s) in mono.iter_mut().enumerate() {
            let t = i as f64 / sr;
            let attack = (t / 0.02).min(1.0);
            let rel = if t > dur {
                (1.0 - (t - dur) / release).max(0.0)
            } else {
                1.0
            };
            *s = (*s as f64 * amp * attack * rel) as f32;
        }
        SoundBuffer::from_mono(self.sample_rate, &mono)
    }

    /// An endlessly descending Shepard–Risset glissando: octave-spaced partials under a fixed
    /// bell-shaped envelope in log frequency, all gliding down together. Each partial fades out
    /// at the bottom of the envelope as another fades in at the top, so the pitch keeps falling
    /// without arriving anywhere. Played before a test to clear pitch memory.
    pub fn shepard_glissando(&self, seconds: f64, s_per_octave: f64) -> SoundBuffer {
        let sr = self.sample_rate as f64;
        const OCTAVES: usize = 9;
        let f_min = 20.0f64;
        let centre = OCTAVES as f64 / 2.0;
        let sigma = 1.4;
        let n = (seconds * sr) as usize;
        let envelope = |pos: f64| (-0.5 * ((pos - centre) / sigma).powi(2)).exp();
        let norm: f64 = (0..OCTAVES).map(|k| envelope(k as f64 + 0.5)).sum::<f64>().max(1e-9);
        let amp = db_to_gain(-10.0) as f64 / norm;
        let mut phase = [0.0f64; OCTAVES];
        let mut mono = vec![0.0f32; n];
        for (i, s) in mono.iter_mut().enumerate() {
            let t = i as f64 / sr;
            // Position of partial k in octaves above f_min, drifting down and wrapping.
            let drift = t / s_per_octave;
            let mut v = 0.0;
            for (k, ph) in phase.iter_mut().enumerate() {
                let pos = (k as f64 - drift).rem_euclid(OCTAVES as f64);
                let f = f_min * pos.exp2();
                *ph = (*ph + 2.0 * std::f64::consts::PI * f / sr) % (2.0 * std::f64::consts::PI);
                v += envelope(pos) * ph.sin();
            }
            let fade = (t / 0.5).min(1.0).min((seconds - t) / 0.8).max(0.0);
            *s = (v * amp * fade) as f32;
        }
        SoundBuffer::from_mono(self.sample_rate, &mono)
    }

    /// The interference sound played between absolute-pitch trials.
    pub fn render_mask(&self, mask: Mask, seed: u64) -> Option<SoundBuffer> {
        let mut rng = Rng::new(seed ^ 0x6d61_736b);
        match mask {
            Mask::None => None,
            Mask::RandomTones => {
                let mut events = Vec::new();
                let mut t = 0.0;
                for _ in 0..10 {
                    let midi = rng.range_i32(45, 88);
                    let wave = if rng.chance(0.5) {
                        "synth:sine"
                    } else {
                        "synth:triangle"
                    };
                    events.push(SoundEvent {
                        midi,
                        start_ms: t,
                        dur_ms: 110.0,
                        timbre: wave.into(),
                        gain_db: -14.0,
                        detune_cents: rng.range_f64(-40.0, 40.0) as f32,
                    });
                    t += 130.0;
                }
                Some(self.render_events(&events))
            }
            Mask::Noise => {
                let n = self.frames(1200.0);
                let mono: Vec<f32> = (0..n)
                    .map(|i| {
                        let env = (i as f32 / 480.0).min(1.0).min((n - i) as f32 / 4800.0);
                        0.12 * env * (rng.f64() as f32 * 2.0 - 1.0)
                    })
                    .collect();
                Some(SoundBuffer::from_mono(self.sample_rate, &mono))
            }
            Mask::ShepardGlissando => Some(self.shepard_glissando(SHEPARD_GLISSANDO_S as f64, GLISSANDO_S_PER_OCTAVE)),
            Mask::ChromaticCluster => {
                let events: Vec<SoundEvent> = (0..24)
                    .map(|k| SoundEvent {
                        midi: 54 + k,
                        start_ms: 0.0,
                        dur_ms: 1000.0,
                        timbre: "synth:sine".into(),
                        gain_db: -26.0,
                        detune_cents: 0.0,
                    })
                    .collect();
                Some(self.render_events(&events))
            }
        }
    }

    /// The backing for a sung trial (reference, clicks, drone, chords) and, separately, the guide
    /// melody so "Sing along" can add it to a retry.
    pub fn render_sung_trial(&self, t: &SungTrial) -> SungAudio {
        let beat_ms = t.beat_ms();
        let at = |beat: f32| beat as f64 * beat_ms;
        let mut events = Vec::new();
        // Reference / starting pitch.
        for n in &t.reference_beats {
            events.push(note_event(n, 0.0, beat_ms, &t.reference_timbre, -6.0));
        }
        // Drone under the singing.
        if let Some(d) = t.drone_midi {
            let dur = (t.total_beats - t.sing_start_beat) as f64 * beat_ms;
            events.push(SoundEvent {
                midi: d,
                start_ms: at(t.sing_start_beat) - 0.5 * beat_ms.min(800.0),
                dur_ms: dur,
                timbre: "gm:52".into(),
                gain_db: -14.0,
                detune_cents: 0.0,
            });
        }
        if t.backing.has_chords() {
            for (beat, beats, chord) in harmonize(t) {
                for m in chord {
                    events.push(SoundEvent {
                        midi: m,
                        start_ms: at(t.sing_start_beat + beat),
                        dur_ms: beats as f64 * beat_ms * 0.98,
                        timbre: "gm:0".into(),
                        gain_db: -16.0,
                        detune_cents: 0.0,
                    });
                }
            }
        }
        let mut backing = self.render_events(&events);
        let total_frames = self.frames(t.total_ms());
        if backing.frames() < total_frames {
            backing.data.resize(total_frames * 2, 0.0);
        }
        backing
            .data
            .truncate((total_frames + self.frames(400.0)) * 2);
        // Clicks: the count-in always (when the trial has one), the rest when the backing has a click.
        let bpb = t.beats_per_bar.max(1) as i32;
        let n_beats = t.total_beats.floor() as i32;
        let (from, count_in_end) = match t.count_in_beat {
            Some(c) => (c.round() as i32, t.sing_start_beat.round() as i32),
            None => (0, 0),
        };
        for b in from..n_beats {
            if b >= count_in_end && !t.backing.has_click() {
                break;
            }
            let c = self.click((b - from) % bpb == 0);
            backing.mix(&c, self.frames(at(b as f32)), 1.0);
        }
        let guide_events: Vec<SoundEvent> = t
            .notes
            .iter()
            .map(|n| note_event(n, at(t.sing_start_beat), beat_ms, &t.reference_timbre, -9.0))
            .collect();
        let guide = self.render_events(&guide_events);
        let accompaniment_events: Vec<SoundEvent> = t
            .accompaniment
            .iter()
            .map(|n| note_event(n, at(t.sing_start_beat), beat_ms, &t.reference_timbre, -7.0))
            .collect();
        let accompaniment = self.render_events(&accompaniment_events);
        let gain = t.backing_volume.clamp(0.0, 1.5);
        for v in &mut backing.data {
            *v *= gain;
        }
        // The other voice keeps its level whatever the backing volume.
        backing.mix(&accompaniment, 0, 1.0);
        backing.limit(0.95);
        SungAudio {
            backing,
            guide,
            guide_audible: t.guide,
            accompaniment,
        }
    }
}

pub struct SungAudio {
    /// Reference, clicks, drone, chords and the other voice of a harmony exercise.
    pub backing: SoundBuffer,
    /// The target melody, aligned to the same timeline as `backing`.
    pub guide: SoundBuffer,
    /// Whether the trial plays the guide by default.
    pub guide_audible: bool,
    /// The other voice alone (silent unless the trial has one), on the same timeline.
    pub accompaniment: SoundBuffer,
}

fn note_event(
    n: &TargetNote,
    offset_ms: f64,
    beat_ms: f64,
    timbre: &str,
    gain_db: f32,
) -> SoundEvent {
    SoundEvent {
        midi: n.midi,
        start_ms: offset_ms + n.start_beat as f64 * beat_ms,
        dur_ms: n.beats as f64 * beat_ms * 0.95,
        timbre: timbre.to_string(),
        gain_db,
        detune_cents: 0.0,
    }
}

/// A simple per-bar harmonisation (I, IV or V in the key of the first note) for chord backing.
pub fn harmonize(t: &SungTrial) -> Vec<(f32, f32, Vec<i32>)> {
    if t.notes.is_empty() {
        return Vec::new();
    }
    // The key: the score's when there is one, otherwise the pattern's first note.
    let (tonic, minor) = match &t.score {
        Some(s) => {
            let first = t.notes[0].midi;
            (
                first - (first - s.key.tonic_pc()).rem_euclid(12),
                s.key.mode == trainer_core::schema::KeyMode::Minor,
            )
        }
        None => {
            let tonic = t.notes[0].midi;
            (tonic, t.notes.iter().any(|n| (n.midi - tonic).rem_euclid(12) == 3))
        }
    };
    let scale = if minor {
        ScaleKind::HarmonicMinor
    } else {
        ScaleKind::Major
    };
    let bass_root = {
        let mut r = tonic - 12;
        while r > 52 {
            r -= 12;
        }
        r
    };
    let chord_degrees: [(i32, [i32; 3]); 3] = [(1, [1, 3, 5]), (4, [4, 6, 8]), (5, [5, 7, 9])];
    let bpb = t.beats_per_bar.max(1) as f32;
    let len = t
        .notes
        .iter()
        .map(|n| n.start_beat + n.beats)
        .fold(0.0, f32::max);
    let bars = (len / bpb).ceil() as i32;
    let mut out = Vec::new();
    for bar in 0..bars {
        let b0 = bar as f32 * bpb;
        let b1 = b0 + bpb;
        let mut best = (f32::MIN, 0usize);
        for (ci, (_, degs)) in chord_degrees.iter().enumerate() {
            let pcs: Vec<i32> = degs
                .iter()
                .map(|d| scale.degree_midi(0, *d).rem_euclid(12))
                .collect();
            let mut score = 0.0;
            for n in &t.notes {
                let overlap = (n.start_beat + n.beats).min(b1) - n.start_beat.max(b0);
                if overlap > 0.0 && pcs.contains(&(n.midi - tonic).rem_euclid(12)) {
                    score += overlap;
                }
            }
            if ci == 0 {
                score += 0.01;
            }
            if score > best.0 {
                best = (score, ci);
            }
        }
        let (root_deg, degs) = chord_degrees[best.1];
        let root = bass_root + scale.degree_midi(0, root_deg);
        let mut chord = vec![root - 12];
        chord.extend(degs.iter().map(|d| bass_root + scale.degree_midi(0, *d)));
        out.push((b0, bpb.min(len - b0 + 1.0), chord));
    }
    out
}

fn fallback_wave(program: u8) -> &'static str {
    match program {
        0..=7 => "soft_piano",
        8..=15 => "triangle",
        16..=23 => "square",
        24..=31 => "soft_piano",
        40..=55 => "saw",
        56..=71 => "saw",
        72..=79 => "triangle",
        80..=87 => "square",
        _ => "triangle",
    }
}

/// One period of a band-limited waveform for fundamental `f0`, 2048 samples long.
fn wavetable(wave: &str, f0: f64, sr: f64) -> Vec<f64> {
    const N: usize = 2048;
    let max_h = ((sr / 2.0 * 0.9) / f0).floor().max(1.0) as usize;
    let harmonics: Vec<(usize, f64)> = match wave {
        "sine" => vec![(1, 1.0)],
        "triangle" => (1..=max_h.min(31))
            .step_by(2)
            .map(|h| {
                let sign = if (h / 2) % 2 == 0 { 1.0 } else { -1.0 };
                (h, sign / (h * h) as f64)
            })
            .collect(),
        "saw" => (1..=max_h.min(40)).map(|h| (h, 1.0 / h as f64)).collect(),
        "square" => (1..=max_h.min(39))
            .step_by(2)
            .map(|h| (h, 1.0 / h as f64))
            .collect(),
        "pulse" => (1..=max_h.min(40))
            .map(|h| (h, (std::f64::consts::PI * h as f64 * 0.25).sin() / h as f64))
            .collect(),
        "soft_piano" => (1..=max_h.min(12))
            .map(|h| (h, 1.0 / (h as f64).powf(1.6)))
            .collect(),
        _ => vec![(1, 1.0)],
    };
    let mut table = vec![0.0; N];
    for (h, a) in &harmonics {
        let k = 2.0 * std::f64::consts::PI * *h as f64 / N as f64;
        for (i, v) in table.iter_mut().enumerate() {
            *v += a * (k * i as f64).sin();
        }
    }
    let peak = table.iter().fold(0.0f64, |m, v| m.max(v.abs())).max(1e-9);
    for v in &mut table {
        *v /= peak;
    }
    table
}

/// A short decaying sine blip.
fn click(sample_rate: u32, freq: f64, level: f64) -> SoundBuffer {
    let sr = sample_rate as f64;
    let n = (0.03 * sr) as usize;
    let mono: Vec<f32> = (0..n)
        .map(|i| {
            let t = i as f64 / sr;
            (level * (2.0 * std::f64::consts::PI * freq * t).sin() * (-t * 140.0).exp()) as f32
        })
        .collect();
    SoundBuffer::from_mono(sample_rate, &mono)
}

#[cfg(test)]
mod tests {
    use super::*;
    use trainer_core::pitch::{TrackOptions, track};
    use trainer_core::schema::{Backing, ExerciseKind, LaneTargets};

    fn r() -> Renderer {
        Renderer::new(48_000, SoundFonts::default())
    }

    fn mono_of(b: &SoundBuffer) -> Vec<f32> {
        b.data
            .chunks_exact(2)
            .map(|c| 0.5 * (c[0] + c[1]))
            .collect()
    }

    #[test]
    fn synth_notes_have_the_right_pitch() {
        let r = r();
        for wave in ["sine", "triangle", "saw", "square", "pulse", "soft_piano"] {
            let e = SoundEvent {
                midi: 57,
                start_ms: 0.0,
                dur_ms: 600.0,
                timbre: format!("synth:{wave}"),
                gain_db: -6.0,
                detune_cents: 0.0,
            };
            let b = r.render_events(&[e]);
            let tr = track(&mono_of(&b), &TrackOptions::new(48_000.0));
            let v: Vec<f32> = tr.frames.iter().filter_map(|f| f.midi).collect();
            assert!(v.len() > 30, "{wave}");
            let mean = v.iter().sum::<f32>() / v.len() as f32;
            assert!((mean - 57.0).abs() < 0.05, "{wave}: {mean}");
        }
    }

    #[test]
    fn gm_without_soundfont_falls_back() {
        let r = r();
        let e = SoundEvent {
            midi: 60,
            start_ms: 100.0,
            dur_ms: 500.0,
            timbre: "gm:0".into(),
            gain_db: -6.0,
            detune_cents: 0.0,
        };
        let b = r.render_events(&[e]);
        assert!(b.peak() > 0.05);
        assert!(b.duration_s() > 0.6);
    }

    #[test]
    fn shepard_tone_has_the_pitch_class() {
        let r = r();
        let e = SoundEvent {
            midi: 62,
            start_ms: 0.0,
            dur_ms: 800.0,
            timbre: "shepard".into(),
            gain_db: -6.0,
            detune_cents: 0.0,
        };
        let b = r.render_events(&[e]);
        let tr = track(&mono_of(&b), &TrackOptions::new(48_000.0));
        let v: Vec<f32> = tr.frames.iter().filter_map(|f| f.midi).collect();
        assert!(!v.is_empty());
        let pc = (v[v.len() / 2].round() as i32).rem_euclid(12);
        assert_eq!(pc, 2);
    }

    #[test]
    fn masks_render() {
        let r = r();
        assert!(r.render_mask(Mask::None, 1).is_none());
        for m in [Mask::RandomTones, Mask::Noise, Mask::ChromaticCluster] {
            let b = r.render_mask(m, 1).unwrap();
            assert!(b.duration_s() > 0.9 && b.duration_s() < 2.5, "{m:?}");
            assert!(b.peak() > 0.01 && b.peak() <= 0.95);
        }
    }

    #[test]
    fn shepard_glissando_is_smooth_and_falls() {
        let r = Renderer::new(16_000, SoundFonts::default());
        let b = r.shepard_glissando(6.0, 4.0);
        assert!((b.duration_s() - 6.0).abs() < 0.01);
        assert!(b.peak() > 0.05 && b.peak() <= 0.95, "{}", b.peak());
        let mono = mono_of(&b);
        // No clicks: neighbouring samples never jump.
        let jump = mono.windows(2).map(|w| (w[1] - w[0]).abs()).fold(0.0f32, f32::max);
        assert!(jump < 0.2, "{jump}");
        // Whatever partial the tracker follows, it keeps falling (octave hops aside).
        let tr = track(&mono[16_000..80_000], &TrackOptions::new(16_000.0));
        let v: Vec<f32> = tr.frames.iter().filter_map(|f| f.midi).collect();
        let steps: Vec<f32> = v.windows(2).map(|w| w[1] - w[0]).filter(|d| d.abs() < 3.0).collect();
        assert!(steps.len() > 50, "{}", steps.len());
        let falling = steps.iter().filter(|d| **d < 0.0).count() as f32 / steps.len() as f32;
        assert!(falling > 0.8, "{falling}");
        assert!(r.render_mask(Mask::ShepardGlissando, 1).unwrap().duration_s() >= 19.9);
    }

    #[test]
    fn sung_trial_timeline_lengths() {
        let r = r();
        let t = SungTrial {
            exercise: ExerciseKind::Pattern,
            prompt: String::new(),
            notes: (0..5)
                .map(|i| TargetNote {
                    midi: 60 + i,
                    start_beat: i as f32,
                    beats: 1.0,
                })
                .collect(),
            tempo_bpm: 120.0,
            beats_per_bar: 4,
            sing_start_beat: 8.0,
            total_beats: 14.0,
            reference_beats: (0..5)
                .map(|i| TargetNote {
                    midi: 60 + i,
                    start_beat: i as f32,
                    beats: 1.0,
                })
                .collect(),
            reference_timbre: "synth:triangle".into(),
            backing: Backing::Chords,
            backing_volume: 0.7,
            guide: false,
            drone_midi: None,
            lane_targets: LaneTargets::Shown,
            timed: true,
            score: None,
            count_in_beat: None,
            sections: Vec::new(),
            follow_cursor: false,
            accompaniment: Vec::new(),
            just_cents: Vec::new(),
            stimulus: serde_json::json!({}),
        };
        let a = r.render_sung_trial(&t);
        // 14 beats at 120 bpm = 7 s.
        assert!(
            a.backing.duration_s() >= 7.0 && a.backing.duration_s() < 7.6,
            "{}",
            a.backing.duration_s()
        );
        assert!(a.guide.duration_s() > 6.0);
        let chords = harmonize(&t);
        assert_eq!(chords.len(), 2);
        assert_eq!(a.accompaniment.peak(), 0.0);

        // A second voice sounds from the start of singing, at its own level.
        let mut h = t.clone();
        h.backing = Backing::None;
        h.backing_volume = 0.0;
        h.reference_beats.clear();
        h.accompaniment = h.notes.iter().map(|n| TargetNote { midi: n.midi + 4, ..n.clone() }).collect();
        let a = r.render_sung_trial(&h);
        let sing_frame = r.frames(4000.0);
        assert!(a.accompaniment.peak() > 0.01);
        let before = &a.backing.data[..sing_frame * 2];
        let after = &a.backing.data[sing_frame * 2..];
        assert!(before.iter().all(|v| v.abs() < 1e-4), "silent before singing");
        assert!(after.iter().any(|v| v.abs() > 0.01), "the other voice is in the backing");
    }
}
