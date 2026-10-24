//! Audio engine: device streams (ASIO preferred), sample-accurate scheduled playback, input
//! capture with a rolling history, live pitch frames and stimulus rendering.
//!
//! cpal streams are not `Send` on every host, so they live on a dedicated audio thread; the
//! [`AudioEngine`] handle only holds rings, atomics and channels and can be shared freely.

pub mod analysis;
pub mod buffer;
pub mod calibrate;
pub mod device;
pub mod engine;
pub mod render;
pub mod shared;

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use anyhow::{Result, anyhow};
use crossbeam_channel::{Receiver, Sender};
use parking_lot::Mutex;
use rtrb::{Producer, RingBuffer};
use serde::{Deserialize, Serialize};
use ts_rs::TS;

pub use analysis::LiveFrame;
pub use buffer::SoundBuffer;
pub use engine::Bus;
use engine::{COMMAND_CAPACITY, Command, InputCore, OutputCore};
use shared::Shared;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, TS)]
pub struct EngineSettings {
    /// cpal host name (`ASIO`, `WASAPI`, …). `None` picks ASIO when available.
    #[serde(default)]
    pub host: Option<String>,
    #[serde(default)]
    pub input_device: Option<String>,
    #[serde(default)]
    pub output_device: Option<String>,
    /// Zero-based input channel used for the microphone.
    #[serde(default)]
    pub input_channel: u16,
    /// Zero-based first channel of the stereo output pair (0 = outputs 1–2).
    #[serde(default)]
    pub output_pair: u16,
    #[serde(default)]
    pub sample_rate: Option<u32>,
    #[serde(default)]
    pub buffer_frames: Option<u32>,
}

impl Default for EngineSettings {
    fn default() -> Self {
        EngineSettings {
            host: None,
            input_device: None,
            output_device: None,
            input_channel: 0,
            output_pair: 0,
            sample_rate: Some(48_000),
            buffer_frames: None,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, TS)]
pub struct EngineInfo {
    pub host: String,
    pub input_device: String,
    pub input_device_id: String,
    pub output_device: String,
    pub output_device_id: String,
    pub sample_rate: u32,
    /// Requested buffer size, if any (the driver may choose; see the observed size in status).
    pub requested_buffer: Option<u32>,
    pub input_channels: u16,
    pub output_channels: u16,
    pub input_channel: u16,
    pub output_pair: u16,
    pub input_format: String,
    pub output_format: String,
}

impl EngineInfo {
    /// Key for stored calibrations: host, devices, sample rate and buffer size.
    pub fn device_key(&self, buffer_frames: u32) -> String {
        format!(
            "{}|{}|{}|{}|{}",
            self.host, self.input_device_id, self.output_device_id, self.sample_rate, buffer_frames
        )
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, TS)]
pub struct EngineStatus {
    pub running: bool,
    #[ts(type = "number")]
    pub out_frame: u64,
    #[ts(type = "number")]
    pub in_frame: u64,
    pub buffer_frames: u32,
    pub load_percent: f32,
    pub xruns: u32,
    #[ts(type = "number")]
    pub input_overflows: u64,
    pub voices: u32,
    #[ts(type = "number | null")]
    pub roundtrip_estimate_frames: Option<i64>,
    pub input_peak: f32,
    pub errors: Vec<String>,
}

pub struct AudioEngine {
    info: EngineInfo,
    shared: Arc<Shared>,
    commands: Mutex<Producer<Command>>,
    next_id: AtomicU64,
    analysis: Mutex<analysis::AnalysisHandle>,
    history: analysis::SharedHistory,
    latency: Arc<analysis::LatencyTracker>,
    live_rx: Receiver<LiveFrame>,
    errors_rx: Receiver<String>,
    stop_tx: Sender<()>,
    audio_thread: Mutex<Option<JoinHandle<()>>>,
    garbage_stop: Arc<AtomicBool>,
    garbage_thread: Mutex<Option<JoinHandle<()>>>,
}

pub struct LiveRange {
    pub fmin: f32,
    pub fmax: f32,
    pub a4_hz: f64,
}

impl AudioEngine {
    /// Opens the devices and starts streaming. Blocks until the streams are running or fail.
    pub fn start(settings: &EngineSettings, live: LiveRange) -> Result<AudioEngine> {
        let shared = Arc::new(Shared::default());
        let (cmd_tx, cmd_rx) = RingBuffer::<Command>::new(COMMAND_CAPACITY);
        let (garbage_tx, mut garbage_rx) = RingBuffer::<Arc<SoundBuffer>>::new(1024);
        let (in_tx, in_rx) = RingBuffer::<f32>::new(48_000 * 4);
        let (err_tx, err_rx) = crossbeam_channel::bounded::<String>(64);
        let (stop_tx, stop_rx) = crossbeam_channel::bounded::<()>(1);
        let (ready_tx, ready_rx) = crossbeam_channel::bounded::<Result<EngineInfo>>(1);

        let settings_t = settings.clone();
        let shared_t = shared.clone();
        let audio_thread = std::thread::Builder::new()
            .name("trainer-audio".into())
            .spawn(move || {
                let result = (|| -> Result<(cpal::Stream, cpal::Stream, EngineInfo)> {
                    let host = match &settings_t.host {
                        Some(name) => device::host_by_name(name)
                            .ok_or_else(|| anyhow!("audio host `{name}` is not available"))?,
                        None => device::preferred_host(),
                    };
                    let output =
                        device::pick_device(&host, settings_t.output_device.as_deref(), false)
                            .ok_or_else(|| anyhow!("no output device found"))?;
                    // ASIO drivers are full duplex: use the same device for input unless told otherwise.
                    let input = match (&settings_t.input_device, host.id().name()) {
                        (None, "ASIO") => output.clone(),
                        (wanted, _) => device::pick_device(&host, wanted.as_deref(), true)
                            .ok_or_else(|| anyhow!("no input device found"))?,
                    };
                    let plan = device::plan_streams(
                        &input,
                        &output,
                        settings_t.sample_rate,
                        settings_t.buffer_frames,
                    )?;
                    let out_cfg = cpal::StreamConfig {
                        channels: plan.out_channels,
                        sample_rate: plan.sample_rate,
                        buffer_size: plan.buffer,
                    };
                    let in_cfg = cpal::StreamConfig {
                        channels: plan.in_channels,
                        sample_rate: plan.sample_rate,
                        buffer_size: plan.buffer,
                    };
                    let input_channel = settings_t
                        .input_channel
                        .min(plan.in_channels.saturating_sub(1));
                    let output_pair = if settings_t.output_pair + 1 < plan.out_channels {
                        settings_t.output_pair
                    } else {
                        0
                    };
                    let out_core = OutputCore::new(
                        cmd_rx,
                        garbage_tx,
                        shared_t.clone(),
                        plan.out_channels as usize,
                        output_pair as usize,
                    );
                    let in_core = InputCore::new(
                        in_tx,
                        shared_t.clone(),
                        plan.in_channels as usize,
                        input_channel as usize,
                    );
                    let out_stream = device::build_output(
                        &output,
                        out_cfg,
                        plan.out_format,
                        out_core,
                        shared_t.clone(),
                        err_tx.clone(),
                    )?;
                    let in_stream = device::build_input(
                        &input,
                        in_cfg,
                        plan.in_format,
                        in_core,
                        shared_t.clone(),
                        err_tx.clone(),
                    )?;
                    device::play(&out_stream)?;
                    device::play(&in_stream)?;
                    let info = EngineInfo {
                        host: host.id().name().to_string(),
                        input_device: device::device_name(&input),
                        input_device_id: device::device_id(&input),
                        output_device: device::device_name(&output),
                        output_device_id: device::device_id(&output),
                        sample_rate: plan.sample_rate,
                        requested_buffer: match plan.buffer {
                            cpal::BufferSize::Fixed(n) => Some(n),
                            cpal::BufferSize::Default => None,
                        },
                        input_channels: plan.in_channels,
                        output_channels: plan.out_channels,
                        input_channel,
                        output_pair,
                        input_format: plan.in_format.to_string(),
                        output_format: plan.out_format.to_string(),
                    };
                    Ok((out_stream, in_stream, info))
                })();
                match result {
                    Ok((out_stream, in_stream, info)) => {
                        shared_t.running.store(true, Ordering::Release);
                        let _ = ready_tx.send(Ok(info));
                        let _ = stop_rx.recv();
                        drop(in_stream);
                        drop(out_stream);
                        shared_t.running.store(false, Ordering::Release);
                    }
                    Err(e) => {
                        let _ = ready_tx.send(Err(e));
                    }
                }
            })?;

        let info = match ready_rx.recv_timeout(Duration::from_secs(20)) {
            Ok(Ok(info)) => info,
            Ok(Err(e)) => {
                let _ = audio_thread.join();
                return Err(e);
            }
            Err(_) => return Err(anyhow!("timed out opening the audio device")),
        };

        let (live_tx, live_rx) = crossbeam_channel::bounded::<LiveFrame>(4096);
        let analysis = analysis::spawn_analysis(
            in_rx,
            shared.clone(),
            analysis::AnalysisConfig {
                sample_rate: info.sample_rate,
                fmin: live.fmin,
                fmax: live.fmax,
                a4_hz: live.a4_hz,
                history_seconds: 180,
            },
            live_tx,
        );
        let history = analysis.history.clone();
        let latency = analysis.latency.clone();

        let garbage_stop = Arc::new(AtomicBool::new(false));
        let gs = garbage_stop.clone();
        let garbage_thread = std::thread::Builder::new()
            .name("trainer-garbage".into())
            .spawn(move || {
                while !gs.load(Ordering::Acquire) {
                    while garbage_rx.pop().is_ok() {}
                    std::thread::sleep(Duration::from_millis(50));
                }
                while garbage_rx.pop().is_ok() {}
            })?;

        Ok(AudioEngine {
            info,
            shared,
            commands: Mutex::new(cmd_tx),
            next_id: AtomicU64::new(1),
            analysis: Mutex::new(analysis),
            history,
            latency,
            live_rx,
            errors_rx: err_rx,
            stop_tx,
            audio_thread: Mutex::new(Some(audio_thread)),
            garbage_stop,
            garbage_thread: Mutex::new(Some(garbage_thread)),
        })
    }

    pub fn info(&self) -> &EngineInfo {
        &self.info
    }

    pub fn sample_rate(&self) -> u32 {
        self.info.sample_rate
    }

    pub fn is_running(&self) -> bool {
        self.shared.running.load(Ordering::Acquire)
    }

    /// Output frames played so far (the master clock).
    pub fn now(&self) -> u64 {
        self.shared.out_frame.load(Ordering::Acquire)
    }

    pub fn buffer_frames(&self) -> u32 {
        self.shared.buffer_frames.load(Ordering::Relaxed).max(1)
    }

    /// A start frame safely in the future: at least `ms` from now and two buffers ahead.
    pub fn frame_after_ms(&self, ms: f64) -> u64 {
        let ahead = ((ms / 1000.0) * self.sample_rate() as f64) as u64;
        self.now() + ahead.max(2 * self.buffer_frames() as u64 + 64)
    }

    pub fn frames_from_ms(&self, ms: f64) -> u64 {
        ((ms / 1000.0) * self.sample_rate() as f64).round().max(0.0) as u64
    }

    pub fn play(&self, buf: Arc<SoundBuffer>, at: u64, gain: f32, bus: Bus) -> Result<u64> {
        let id = self.next_id.fetch_add(1, Ordering::Relaxed);
        self.commands
            .lock()
            .push(Command::Play {
                id,
                buf,
                at,
                gain,
                bus,
            })
            .map_err(|_| anyhow!("audio command queue is full"))?;
        Ok(id)
    }

    pub fn play_now(&self, buf: Arc<SoundBuffer>, gain: f32, bus: Bus) -> Result<(u64, u64)> {
        let at = self.frame_after_ms(10.0);
        Ok((self.play(buf, at, gain, bus)?, at))
    }

    fn command(&self, c: Command) {
        let _ = self.commands.lock().push(c);
    }

    pub fn stop(&self, id: u64) {
        self.command(Command::Stop { id });
    }

    pub fn stop_bus(&self, bus: Bus) {
        self.command(Command::StopBus(bus));
    }

    pub fn stop_all(&self) {
        self.command(Command::StopAll);
    }

    pub fn set_bus_gain(&self, bus: Bus, gain: f32) {
        self.command(Command::SetBusGain(bus, gain));
    }

    pub fn set_master_gain(&self, gain: f32) {
        self.command(Command::SetMasterGain(gain));
    }

    /// Live pitch/level frames (one consumer).
    pub fn live(&self) -> Receiver<LiveFrame> {
        self.live_rx.clone()
    }

    /// Round trip from output frame to input frame according to the stream timestamps.
    pub fn roundtrip_estimate(&self) -> Option<i64> {
        self.latency.estimate()
    }

    /// Input frames received so far.
    pub fn input_end(&self) -> u64 {
        self.history.0.lock().end()
    }

    /// Waits until input frames `[a, b)` have arrived and returns them.
    pub fn capture(&self, a: u64, b: u64, timeout: Duration) -> Result<Vec<f32>> {
        let deadline = Instant::now() + timeout;
        let (lock, cv) = &*self.history;
        let mut h = lock.lock();
        while h.end() < b {
            if cv.wait_until(&mut h, deadline).timed_out() {
                return Err(anyhow!(
                    "timed out waiting for input (have {}, need {b})",
                    h.end()
                ));
            }
        }
        h.copy_range(a, b)
            .ok_or_else(|| anyhow!("input frames {a}..{b} are no longer in the history"))
    }

    pub fn status(&self) -> EngineStatus {
        let mut errors = Vec::new();
        while let Ok(e) = self.errors_rx.try_recv() {
            errors.push(e);
        }
        EngineStatus {
            running: self.is_running(),
            out_frame: self.now(),
            in_frame: self.shared.in_frames.load(Ordering::Relaxed),
            buffer_frames: self.shared.buffer_frames.load(Ordering::Relaxed),
            load_percent: self.shared.load_permille.load(Ordering::Relaxed) as f32 / 10.0,
            xruns: self.shared.xruns.load(Ordering::Relaxed),
            input_overflows: self.shared.input_overflows.load(Ordering::Relaxed),
            voices: self.shared.voices_active.load(Ordering::Relaxed),
            roundtrip_estimate_frames: self.roundtrip_estimate(),
            input_peak: self.shared.input_peak(),
            errors,
        }
    }

    pub fn shutdown(&self) {
        let _ = self.stop_tx.try_send(());
        if let Some(t) = self.audio_thread.lock().take() {
            let _ = t.join();
        }
        self.analysis.lock().stop();
        self.garbage_stop.store(true, Ordering::Release);
        if let Some(t) = self.garbage_thread.lock().take() {
            let _ = t.join();
        }
    }
}

impl Drop for AudioEngine {
    fn drop(&mut self) {
        self.shutdown();
    }
}
