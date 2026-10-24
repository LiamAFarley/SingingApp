//! Device enumeration and stream setup with cpal. ASIO is preferred when available.

use std::sync::Arc;
use std::sync::atomic::Ordering;
use std::time::Instant;

use anyhow::{Result, anyhow};
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{
    BufferSize, Device, FromSample, Host, Sample, SampleFormat, SizedSample, StreamConfig,
    SupportedBufferSize,
};
use crossbeam_channel::Sender;
use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::engine::{InputCore, OutputCore};
use crate::shared::Shared;

#[derive(Clone, Debug, Serialize, Deserialize, TS)]
pub struct DeviceInfo {
    /// cpal device id (`host:device`), stable across restarts.
    pub id: String,
    pub name: String,
    pub input_channels: u16,
    pub output_channels: u16,
    pub default_sample_rate: Option<u32>,
    pub buffer_min: Option<u32>,
    pub buffer_max: Option<u32>,
    pub is_default_input: bool,
    pub is_default_output: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize, TS)]
pub struct HostInfo {
    /// cpal host name, e.g. `ASIO`, `WASAPI`.
    pub name: String,
    pub devices: Vec<DeviceInfo>,
}

fn max_channels<I: Iterator<Item = cpal::SupportedStreamConfigRange>>(
    it: Result<I, cpal::Error>,
) -> u16 {
    it.map(|i| i.map(|c| c.channels()).max().unwrap_or(0))
        .unwrap_or(0)
}

pub fn host_by_name(name: &str) -> Option<Host> {
    cpal::available_hosts()
        .into_iter()
        .find(|h| h.name().eq_ignore_ascii_case(name))
        .and_then(|id| cpal::host_from_id(id).ok())
}

/// ASIO when compiled in and a driver is installed, otherwise the platform default.
pub fn preferred_host() -> Host {
    #[cfg(all(windows, feature = "asio"))]
    {
        if cpal::available_hosts().contains(&cpal::HostId::Asio) {
            if let Ok(h) = cpal::host_from_id(cpal::HostId::Asio) {
                if h.devices().map(|mut d| d.next().is_some()).unwrap_or(false) {
                    return h;
                }
            }
        }
    }
    cpal::default_host()
}

pub fn device_name(d: &Device) -> String {
    d.description()
        .map(|desc| desc.name().to_string())
        .unwrap_or_else(|_| d.to_string())
}

pub fn device_id(d: &Device) -> String {
    d.id()
        .map(|id| id.to_string())
        .unwrap_or_else(|_| device_name(d))
}

pub fn list_hosts() -> Vec<HostInfo> {
    let mut out = Vec::new();
    for id in cpal::available_hosts() {
        let Ok(host) = cpal::host_from_id(id) else {
            continue;
        };
        let def_in = host.default_input_device().map(|d| device_id(&d));
        let def_out = host.default_output_device().map(|d| device_id(&d));
        let mut devices = Vec::new();
        if let Ok(list) = host.devices() {
            for d in list {
                let id = device_id(&d);
                let (buffer_min, buffer_max) = d
                    .default_output_config()
                    .or_else(|_| d.default_input_config())
                    .ok()
                    .and_then(|c| match c.buffer_size() {
                        SupportedBufferSize::Range { min, max } => Some((Some(*min), Some(*max))),
                        SupportedBufferSize::Unknown => None,
                    })
                    .unwrap_or((None, None));
                devices.push(DeviceInfo {
                    name: device_name(&d),
                    input_channels: max_channels(d.supported_input_configs()),
                    output_channels: max_channels(d.supported_output_configs()),
                    default_sample_rate: d
                        .default_output_config()
                        .or_else(|_| d.default_input_config())
                        .ok()
                        .map(|c| c.sample_rate()),
                    buffer_min,
                    buffer_max,
                    is_default_input: def_in.as_deref() == Some(id.as_str()),
                    is_default_output: def_out.as_deref() == Some(id.as_str()),
                    id,
                });
            }
        }
        out.push(HostInfo {
            name: id.name().to_string(),
            devices,
        });
    }
    out
}

/// Finds a device by id, falling back to one whose name mentions the interface, then the default.
pub fn pick_device(host: &Host, wanted: Option<&str>, input: bool) -> Option<Device> {
    let devices: Vec<Device> = host.devices().map(|d| d.collect()).unwrap_or_default();
    let usable = |d: &&Device| {
        if input {
            max_channels(d.supported_input_configs()) > 0
        } else {
            max_channels(d.supported_output_configs()) > 0
        }
    };
    if let Some(w) = wanted {
        if let Some(d) = devices
            .iter()
            .filter(usable)
            .find(|d| device_id(d) == w || device_name(d) == w)
        {
            return Some(d.clone());
        }
    }
    if let Some(d) = devices.iter().filter(usable).find(|d| {
        let n = device_name(d).to_lowercase();
        n.contains("focusrite") || n.contains("scarlett")
    }) {
        return Some(d.clone());
    }
    if input {
        host.default_input_device()
    } else {
        host.default_output_device()
    }
}

pub struct StreamPlan {
    pub sample_rate: u32,
    pub buffer: BufferSize,
    pub in_channels: u16,
    pub out_channels: u16,
    pub in_format: SampleFormat,
    pub out_format: SampleFormat,
}

pub fn plan_streams(
    input: &Device,
    output: &Device,
    sample_rate: Option<u32>,
    buffer_frames: Option<u32>,
) -> Result<StreamPlan> {
    let in_def = input
        .default_input_config()
        .map_err(|e| anyhow!("input device: {e}"))?;
    let out_def = output
        .default_output_config()
        .map_err(|e| anyhow!("output device: {e}"))?;
    let wanted = sample_rate.unwrap_or(48_000);
    let supports = |ranges: Result<Vec<cpal::SupportedStreamConfigRange>, cpal::Error>,
                    rate: u32,
                    fmt: SampleFormat| {
        ranges
            .map(|r| {
                r.iter()
                    .any(|c| c.sample_format() == fmt && c.contains_rate(rate))
            })
            .unwrap_or(false)
    };
    let in_ranges = || {
        input
            .supported_input_configs()
            .map(|i| i.collect::<Vec<_>>())
    };
    let out_ranges = || {
        output
            .supported_output_configs()
            .map(|i| i.collect::<Vec<_>>())
    };
    let rate = if supports(in_ranges(), wanted, in_def.sample_format())
        && supports(out_ranges(), wanted, out_def.sample_format())
    {
        wanted
    } else if in_def.sample_rate() == out_def.sample_rate() {
        in_def.sample_rate()
    } else {
        out_def.sample_rate()
    };
    let buffer = match (buffer_frames, out_def.buffer_size()) {
        (Some(n), SupportedBufferSize::Range { min, max }) if n >= *min && n <= *max => {
            BufferSize::Fixed(n)
        }
        (Some(n), SupportedBufferSize::Unknown) => BufferSize::Fixed(n),
        _ => BufferSize::Default,
    };
    Ok(StreamPlan {
        sample_rate: rate,
        buffer,
        in_channels: in_def.channels(),
        out_channels: out_def.channels(),
        in_format: in_def.sample_format(),
        out_format: out_def.sample_format(),
    })
}

fn report_error(errors: &Sender<String>, shared: &Shared, e: cpal::Error) {
    if e.kind() == cpal::ErrorKind::Xrun {
        shared.xruns.fetch_add(1, Ordering::Relaxed);
    } else {
        let _ = errors.try_send(e.to_string());
    }
}

const SCRATCH_FRAMES: usize = 8192;

fn build_output_typed<T>(
    device: &Device,
    config: StreamConfig,
    mut core: OutputCore,
    shared: Arc<Shared>,
    errors: Sender<String>,
) -> Result<cpal::Stream>
where
    T: SizedSample + FromSample<f32>,
{
    let ch = config.channels.max(1) as usize;
    let sr = config.sample_rate as f64;
    let mut scratch = vec![0.0f32; SCRATCH_FRAMES * ch];
    let shared_cb = shared.clone();
    let stream = device.build_output_stream::<T, _, _>(
        config,
        move |data: &mut [T], info: &cpal::OutputCallbackInfo| {
            let started = Instant::now();
            let frames = data.len() / ch;
            shared_cb
                .out_stamp
                .store(core.frame(), info.timestamp().playback.as_nanos() as u64);
            for chunk in data.chunks_mut(SCRATCH_FRAMES * ch) {
                let s = &mut scratch[..chunk.len()];
                core.process(s);
                for (d, v) in chunk.iter_mut().zip(s.iter()) {
                    *d = T::from_sample(*v);
                }
            }
            shared_cb
                .buffer_frames
                .store(frames as u32, Ordering::Relaxed);
            shared_cb.out_callbacks.fetch_add(1, Ordering::Relaxed);
            if frames > 0 {
                let period = frames as f64 / sr;
                let load = (started.elapsed().as_secs_f64() / period * 1000.0) as u32;
                let prev = shared_cb.load_permille.load(Ordering::Relaxed);
                // Peak-hold with slow decay.
                let next = if load > prev {
                    load
                } else {
                    prev - (prev - load) / 64
                };
                shared_cb.load_permille.store(next, Ordering::Relaxed);
            }
        },
        move |e| report_error(&errors, &shared, e),
        None,
    )?;
    Ok(stream)
}

fn build_input_typed<T>(
    device: &Device,
    config: StreamConfig,
    mut core: InputCore,
    shared: Arc<Shared>,
    errors: Sender<String>,
) -> Result<cpal::Stream>
where
    T: SizedSample,
    f32: FromSample<T>,
{
    let ch = config.channels.max(1) as usize;
    let mut scratch = vec![0.0f32; SCRATCH_FRAMES * ch];
    let shared_cb = shared.clone();
    let stream = device.build_input_stream::<T, _, _>(
        config,
        move |data: &[T], info: &cpal::InputCallbackInfo| {
            shared_cb
                .in_stamp
                .store(core.frames(), info.timestamp().capture.as_nanos() as u64);
            for chunk in data.chunks(SCRATCH_FRAMES * ch) {
                let s = &mut scratch[..chunk.len()];
                for (d, v) in s.iter_mut().zip(chunk.iter()) {
                    *d = f32::from_sample(*v);
                }
                core.process(s);
            }
            shared_cb.in_callbacks.fetch_add(1, Ordering::Relaxed);
        },
        move |e| report_error(&errors, &shared, e),
        None,
    )?;
    Ok(stream)
}

pub fn build_output(
    device: &Device,
    config: StreamConfig,
    format: SampleFormat,
    core: OutputCore,
    shared: Arc<Shared>,
    errors: Sender<String>,
) -> Result<cpal::Stream> {
    match format {
        SampleFormat::F32 => build_output_typed::<f32>(device, config, core, shared, errors),
        SampleFormat::F64 => build_output_typed::<f64>(device, config, core, shared, errors),
        SampleFormat::I16 => build_output_typed::<i16>(device, config, core, shared, errors),
        SampleFormat::I24 => build_output_typed::<cpal::I24>(device, config, core, shared, errors),
        SampleFormat::I32 => build_output_typed::<i32>(device, config, core, shared, errors),
        SampleFormat::U16 => build_output_typed::<u16>(device, config, core, shared, errors),
        SampleFormat::U8 => build_output_typed::<u8>(device, config, core, shared, errors),
        other => Err(anyhow!("unsupported output sample format {other}")),
    }
}

pub fn build_input(
    device: &Device,
    config: StreamConfig,
    format: SampleFormat,
    core: InputCore,
    shared: Arc<Shared>,
    errors: Sender<String>,
) -> Result<cpal::Stream> {
    match format {
        SampleFormat::F32 => build_input_typed::<f32>(device, config, core, shared, errors),
        SampleFormat::F64 => build_input_typed::<f64>(device, config, core, shared, errors),
        SampleFormat::I16 => build_input_typed::<i16>(device, config, core, shared, errors),
        SampleFormat::I24 => build_input_typed::<cpal::I24>(device, config, core, shared, errors),
        SampleFormat::I32 => build_input_typed::<i32>(device, config, core, shared, errors),
        SampleFormat::U16 => build_input_typed::<u16>(device, config, core, shared, errors),
        SampleFormat::U8 => build_input_typed::<u8>(device, config, core, shared, errors),
        other => Err(anyhow!("unsupported input sample format {other}")),
    }
}

pub fn play(stream: &cpal::Stream) -> Result<()> {
    stream.play().map_err(|e| anyhow!("starting stream: {e}"))
}
