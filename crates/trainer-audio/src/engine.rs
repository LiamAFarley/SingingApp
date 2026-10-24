//! The real-time side: what runs inside the audio callbacks. Nothing here allocates, locks,
//! blocks or logs. Buffers arrive through a command ring and leave through a garbage ring so the
//! audio thread never frees memory.

use std::sync::Arc;
use std::sync::atomic::Ordering;

use rtrb::{Consumer, Producer};

use crate::buffer::SoundBuffer;
use crate::shared::Shared;

pub const MAX_VOICES: usize = 128;
pub const COMMAND_CAPACITY: usize = 1024;

#[derive(
    Clone, Copy, Debug, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize, ts_rs::TS,
)]
#[serde(rename_all = "snake_case")]
pub enum Bus {
    /// Prompts, masks, references.
    Prompt,
    /// Backing tracks, drones, chords.
    Backing,
    /// Guide melody (can be muted or faded independently).
    Guide,
    /// Metronome clicks played on their own (tuner).
    Click,
}

impl Bus {
    pub const COUNT: usize = 4;

    fn index(self) -> usize {
        match self {
            Bus::Prompt => 0,
            Bus::Backing => 1,
            Bus::Guide => 2,
            Bus::Click => 3,
        }
    }
}

pub enum Command {
    /// Start `buf` at absolute output frame `at` (or immediately if `at` is in the past).
    Play {
        id: u64,
        buf: Arc<SoundBuffer>,
        at: u64,
        gain: f32,
        bus: Bus,
    },
    Stop {
        id: u64,
    },
    StopBus(Bus),
    StopAll,
    SetBusGain(Bus, f32),
    SetMasterGain(f32),
}

struct Voice {
    id: u64,
    buf: Arc<SoundBuffer>,
    start: u64,
    gain: f32,
    bus: Bus,
    /// Fade-out in progress: remaining frames.
    fading: Option<u32>,
}

const FADE_FRAMES: u32 = 256;

pub struct OutputCore {
    commands: Consumer<Command>,
    garbage: Producer<Arc<SoundBuffer>>,
    voices: Vec<Voice>,
    /// Buffers that could not be returned because the garbage ring was full; returned later.
    held: Vec<Arc<SoundBuffer>>,
    frame: u64,
    bus_gain: [f32; Bus::COUNT],
    bus_target: [f32; Bus::COUNT],
    master: f32,
    shared: Arc<Shared>,
    /// Device channels and the first channel of the stereo pair we write to.
    channels: usize,
    pair: usize,
}

impl OutputCore {
    pub fn new(
        commands: Consumer<Command>,
        garbage: Producer<Arc<SoundBuffer>>,
        shared: Arc<Shared>,
        channels: usize,
        pair: usize,
    ) -> Self {
        OutputCore {
            commands,
            garbage,
            voices: Vec::with_capacity(MAX_VOICES),
            held: Vec::with_capacity(MAX_VOICES * 2),
            frame: 0,
            bus_gain: [1.0; Bus::COUNT],
            bus_target: [1.0; Bus::COUNT],
            master: 1.0,
            shared,
            channels: channels.max(1),
            pair: if pair + 1 < channels.max(1) { pair } else { 0 },
        }
    }

    pub fn frame(&self) -> u64 {
        self.frame
    }

    fn release(&mut self, buf: Arc<SoundBuffer>) {
        if let Err(rtrb::PushError::Full(buf)) = self.garbage.push(buf) {
            if self.held.len() < self.held.capacity() {
                self.held.push(buf);
            } else {
                // Never free on the audio thread; leaking one buffer is the lesser evil.
                std::mem::forget(buf);
            }
        }
    }

    fn drain_commands(&mut self) {
        // Retry returning held buffers first.
        while let Some(buf) = self.held.pop() {
            if let Err(rtrb::PushError::Full(buf)) = self.garbage.push(buf) {
                self.held.push(buf);
                break;
            }
        }
        for _ in 0..64 {
            let Ok(cmd) = self.commands.pop() else { break };
            match cmd {
                Command::Play {
                    id,
                    buf,
                    at,
                    gain,
                    bus,
                } => {
                    if self.voices.len() < self.voices.capacity() {
                        self.voices.push(Voice {
                            id,
                            buf,
                            start: at.max(self.frame),
                            gain,
                            bus,
                            fading: None,
                        });
                    } else {
                        self.shared.voices_dropped.fetch_add(1, Ordering::Relaxed);
                        self.release(buf);
                    }
                }
                Command::Stop { id } => {
                    for v in self.voices.iter_mut().filter(|v| v.id == id) {
                        v.fading.get_or_insert(FADE_FRAMES);
                    }
                }
                Command::StopBus(bus) => {
                    for v in self.voices.iter_mut().filter(|v| v.bus == bus) {
                        v.fading.get_or_insert(FADE_FRAMES);
                    }
                }
                Command::StopAll => {
                    for v in self.voices.iter_mut() {
                        v.fading.get_or_insert(FADE_FRAMES);
                    }
                }
                Command::SetBusGain(bus, g) => self.bus_target[bus.index()] = g.clamp(0.0, 4.0),
                Command::SetMasterGain(g) => self.master = g.clamp(0.0, 4.0),
            }
        }
    }

    /// Fills `out` (interleaved, `channels` per frame) and advances the clock.
    pub fn process(&mut self, out: &mut [f32]) {
        self.drain_commands();
        out.fill(0.0);
        let ch = self.channels;
        let frames = out.len() / ch;
        if frames == 0 {
            return;
        }
        let start = self.frame;
        let end = start + frames as u64;
        // Smooth bus gain changes across the buffer.
        let mut gains_start = [0.0f32; Bus::COUNT];
        let mut gains_step = [0.0f32; Bus::COUNT];
        for b in 0..Bus::COUNT {
            gains_start[b] = self.bus_gain[b];
            gains_step[b] = (self.bus_target[b] - self.bus_gain[b]) / frames as f32;
            self.bus_gain[b] = self.bus_target[b];
        }
        let (l_ch, r_ch) = (self.pair, if ch > 1 { self.pair + 1 } else { self.pair });
        let mut i = 0;
        while i < self.voices.len() {
            let v = &mut self.voices[i];
            let len = v.buf.frames() as u64;
            let v_end = v.start + len;
            let mut finished = v_end <= start;
            if !finished && v.start < end {
                let from = v.start.max(start);
                let to = v_end.min(end);
                let bi = v.bus.index();
                for f in from..to {
                    let k = (f - start) as usize;
                    let src = ((f - v.start) as usize) * 2;
                    let mut g =
                        v.gain * (gains_start[bi] + gains_step[bi] * k as f32) * self.master;
                    if let Some(rem) = v.fading.as_mut() {
                        g *= *rem as f32 / FADE_FRAMES as f32;
                        if *rem == 0 {
                            finished = true;
                            break;
                        }
                        *rem -= 1;
                    }
                    out[k * ch + l_ch] += v.buf.data[src] * g;
                    if r_ch != l_ch {
                        out[k * ch + r_ch] += v.buf.data[src + 1] * g;
                    } else {
                        out[k * ch + l_ch] += v.buf.data[src + 1] * g;
                    }
                }
                if to == v_end {
                    finished = true;
                }
            } else if v.fading.is_some() && v.start >= end {
                // Stopped before it started.
                finished = true;
            }
            if finished {
                let v = self.voices.swap_remove(i);
                self.release(v.buf);
            } else {
                i += 1;
            }
        }
        for s in out.iter_mut() {
            *s = s.clamp(-1.0, 1.0);
        }
        self.frame = end;
        self.shared.out_frame.store(end, Ordering::Release);
        self.shared
            .voices_active
            .store(self.voices.len() as u32, Ordering::Relaxed);
    }
}

pub struct InputCore {
    ring: Producer<f32>,
    shared: Arc<Shared>,
    channels: usize,
    channel: usize,
    frames: u64,
}

impl InputCore {
    pub fn new(ring: Producer<f32>, shared: Arc<Shared>, channels: usize, channel: usize) -> Self {
        let channels = channels.max(1);
        InputCore {
            ring,
            shared,
            channels,
            channel: channel.min(channels - 1),
            frames: 0,
        }
    }

    pub fn frames(&self) -> u64 {
        self.frames
    }

    /// Takes interleaved input already converted to f32 and pushes the selected channel.
    pub fn process(&mut self, input: &[f32]) {
        let ch = self.channels;
        let frames = input.len() / ch;
        let take = frames.min(self.ring.slots());
        if take < frames {
            self.shared
                .input_overflows
                .fetch_add((frames - take) as u64, Ordering::Relaxed);
        }
        if take > 0 {
            if let Ok(chunk) = self.ring.write_chunk_uninit(take) {
                let channel = self.channel;
                chunk.fill_from_iter(input.chunks_exact(ch).take(take).map(|f| f[channel]));
            }
        }
        // The clock counts every frame the device delivered, pushed or not, so timing stays exact.
        self.frames += frames as u64;
        self.shared.in_frames.store(self.frames, Ordering::Release);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn core(
        channels: usize,
    ) -> (
        Producer<Command>,
        Consumer<Arc<SoundBuffer>>,
        OutputCore,
        Arc<Shared>,
    ) {
        let (cmd_tx, cmd_rx) = rtrb::RingBuffer::new(COMMAND_CAPACITY);
        let (g_tx, g_rx) = rtrb::RingBuffer::new(256);
        let shared = Arc::new(Shared::default());
        (
            cmd_tx,
            g_rx,
            OutputCore::new(cmd_rx, g_tx, shared.clone(), channels, 0),
            shared,
        )
    }

    fn ramp(frames: usize) -> Arc<SoundBuffer> {
        let mono: Vec<f32> = (0..frames).map(|i| (i + 1) as f32 / 1000.0).collect();
        Arc::new(SoundBuffer::from_mono(48_000, &mono))
    }

    #[test]
    fn plays_buffer_at_exact_frame() {
        let (mut tx, mut garbage, mut out, shared) = core(2);
        tx.push(Command::Play {
            id: 1,
            buf: ramp(10),
            at: 70,
            gain: 1.0,
            bus: Bus::Prompt,
        })
        .unwrap();
        let mut buf = vec![0.0; 64 * 2];
        out.process(&mut buf);
        assert!(buf.iter().all(|&s| s == 0.0));
        out.process(&mut buf);
        // Frame 70 is index 6 of the second buffer.
        assert_eq!(buf[5 * 2], 0.0);
        assert!((buf[6 * 2] - 0.001).abs() < 1e-6);
        assert!((buf[6 * 2 + 1] - 0.001).abs() < 1e-6);
        assert!((buf[15 * 2] - 0.010).abs() < 1e-6);
        assert_eq!(buf[16 * 2], 0.0);
        assert_eq!(shared.out_frame.load(Ordering::Relaxed), 128);
        // The finished buffer comes back through the garbage ring.
        assert!(garbage.pop().is_ok());
    }

    #[test]
    fn writes_to_selected_pair_of_multichannel_device() {
        let (cmd_tx, cmd_rx) = rtrb::RingBuffer::new(16);
        let (g_tx, _g_rx) = rtrb::RingBuffer::new(16);
        let mut tx = cmd_tx;
        let mut out = OutputCore::new(cmd_rx, g_tx, Arc::new(Shared::default()), 4, 2);
        tx.push(Command::Play {
            id: 1,
            buf: ramp(4),
            at: 0,
            gain: 1.0,
            bus: Bus::Prompt,
        })
        .unwrap();
        let mut buf = vec![0.0; 4 * 4];
        out.process(&mut buf);
        assert_eq!(buf[0], 0.0);
        assert_eq!(buf[1], 0.0);
        assert!((buf[2] - 0.001).abs() < 1e-6);
        assert!((buf[3] - 0.001).abs() < 1e-6);
    }

    #[test]
    fn stop_fades_out_and_releases() {
        let (mut tx, mut garbage, mut out, _) = core(2);
        tx.push(Command::Play {
            id: 7,
            buf: ramp(48_000),
            at: 0,
            gain: 1.0,
            bus: Bus::Backing,
        })
        .unwrap();
        let mut buf = vec![0.0; 128 * 2];
        out.process(&mut buf);
        tx.push(Command::StopBus(Bus::Backing)).unwrap();
        for _ in 0..4 {
            out.process(&mut buf);
        }
        assert!(buf.iter().all(|&s| s == 0.0));
        assert!(garbage.pop().is_ok());
    }

    #[test]
    fn input_core_selects_channel_and_counts_frames() {
        let (tx, mut rx) = rtrb::RingBuffer::new(8);
        let shared = Arc::new(Shared::default());
        let mut input = InputCore::new(tx, shared.clone(), 2, 1);
        input.process(&[0.1, 0.2, 0.3, 0.4, 0.5, 0.6]);
        assert_eq!(rx.pop().unwrap(), 0.2);
        assert_eq!(rx.pop().unwrap(), 0.4);
        assert_eq!(rx.pop().unwrap(), 0.6);
        assert_eq!(shared.in_frames.load(Ordering::Relaxed), 3);
        // Overflow drops samples but keeps the clock.
        input.process(&vec![0.0; 2 * 20]);
        assert_eq!(shared.in_frames.load(Ordering::Relaxed), 23);
        assert!(shared.input_overflows.load(Ordering::Relaxed) > 0);
    }
}
