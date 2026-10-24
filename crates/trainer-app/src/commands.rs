//! Tauri commands called by the UI.

use std::sync::Arc;
use std::time::Duration;

use serde_json::Value;
use tauri::State;
use tauri::ipc::Channel;
use trainer_audio::calibrate::{CalibrationResult, analyze_clicks};
use trainer_audio::device::{HostInfo, list_hosts};
use trainer_audio::{Bus, EngineInfo, SoundBuffer};
use trainer_core::presets::{builtin_presets, find_builtin};
use trainer_core::reports::{self, SessionReport};
use trainer_core::schema::{SessionSpec, SoundEvent};
use trainer_core::stats::{StatsQuery, StatsRow};
use trainer_core::storage::{CalibrationRow, RangeProfile, SessionRow, SessionStatus, new_id, now};
use trainer_core::validate::validate_session;

use crate::AppState;
use crate::views::*;

type Res<T> = Result<T, String>;

fn err(e: impl std::fmt::Display) -> String {
    e.to_string()
}

fn anyhow_err(e: anyhow::Error) -> String {
    format!("{e:#}")
}

#[tauri::command]
pub fn app_info(state: State<'_, AppState>) -> Res<AppInfo> {
    let exe = std::env::current_exe()
        .map(|p| p.display().to_string())
        .unwrap_or_default();
    Ok(AppInfo {
        version: env!("CARGO_PKG_VERSION").to_string(),
        data_dir: state.core.data_dir.display().to_string(),
        mcp_command: exe,
        mcp_args: vec!["--mcp".to_string()],
        soundfont: state.core.gm_name(),
        banks: state.core.bank_names(),
    })
}

#[tauri::command]
pub fn get_settings(state: State<'_, AppState>) -> AppSettings {
    state.core.settings.read().clone()
}

#[tauri::command]
pub async fn save_settings(state: State<'_, AppState>, settings: AppSettings) -> Res<AudioView> {
    let core = state.core.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let audio_changed = core.settings.read().audio != settings.audio;
        let a4_changed = core.settings.read().a4_hz != settings.a4_hz;
        core.save_settings(settings).map_err(anyhow_err)?;
        if (audio_changed || a4_changed) && core.engine().is_some() {
            let _ = core.start_audio();
        }
        Ok(audio_view(&core))
    })
    .await
    .map_err(err)?
}

#[tauri::command]
pub async fn list_audio_hosts() -> Res<Vec<HostInfo>> {
    tauri::async_runtime::spawn_blocking(list_hosts)
        .await
        .map_err(err)
}

fn audio_view(core: &crate::state::Core) -> AudioView {
    let engine = core.engine();
    let (roundtrip_ms, calibration) = match &engine {
        Some(e) => {
            let (frames, _) = core.roundtrip(e);
            (
                Some(frames as f64 * 1000.0 / e.sample_rate() as f64),
                core.calibration(e),
            )
        }
        None => (None, None),
    };
    AudioView {
        settings: core.settings.read().audio.clone(),
        info: engine.as_ref().map(|e| e.info().clone()),
        status: engine.as_ref().map(|e| e.status()),
        error: core.engine_error.read().clone(),
        roundtrip_ms,
        calibration,
    }
}

#[tauri::command]
pub fn audio_state(state: State<'_, AppState>) -> AudioView {
    audio_view(&state.core)
}

#[tauri::command]
pub async fn start_audio(state: State<'_, AppState>) -> Res<EngineInfo> {
    let core = state.core.clone();
    tauri::async_runtime::spawn_blocking(move || core.start_audio().map_err(anyhow_err))
        .await
        .map_err(err)?
}

#[tauri::command]
pub async fn stop_audio(state: State<'_, AppState>) -> Res<()> {
    let core = state.core.clone();
    tauri::async_runtime::spawn_blocking(move || core.stop_audio())
        .await
        .map_err(err)
}

/// A short reference note (tuner, settings test).
#[tauri::command]
pub fn play_note(
    state: State<'_, AppState>,
    midi: i32,
    timbre: Option<String>,
    ms: Option<f64>,
) -> Res<()> {
    let r = state.core.renderer();
    let buf = r.render_events(&[SoundEvent {
        midi,
        start_ms: 0.0,
        dur_ms: ms.unwrap_or(1200.0),
        timbre: timbre.unwrap_or_else(|| "gm:0".into()),
        gain_db: -6.0,
        detune_cents: 0.0,
    }]);
    state.core.play_now(buf).map_err(anyhow_err)
}

/// Starts (or replaces) a sustained drone, or stops it with `midi: null`.
#[tauri::command]
pub fn set_drone(state: State<'_, AppState>, midi: Option<i32>, timbre: Option<String>) -> Res<()> {
    let engine = state.core.ensure_engine().map_err(anyhow_err)?;
    if let Some(id) = state.core.drone.lock().take() {
        engine.stop(id);
    }
    if let Some(m) = midi {
        let r = state.core.renderer();
        let buf = r.render_events(&[SoundEvent {
            midi: m,
            start_ms: 0.0,
            dur_ms: 120_000.0,
            timbre: timbre.unwrap_or_else(|| "gm:52".into()),
            gain_db: -12.0,
            detune_cents: 0.0,
        }]);
        let (id, _) = engine
            .play_now(Arc::new(buf), 1.0, Bus::Backing)
            .map_err(anyhow_err)?;
        *state.core.drone.lock() = Some(id);
    }
    Ok(())
}

#[tauri::command]
pub fn subscribe_live(state: State<'_, AppState>, channel: Channel<LiveBatch>) {
    *state.core.live_channel.lock() = Some(channel);
}

/// Loopback (`loopback`: cable from an output to an input) or tap-along (`tap`: tap the mic on
/// each click) latency calibration.
#[tauri::command]
pub async fn calibrate(state: State<'_, AppState>, method: String) -> Res<CalibrationResult> {
    let core = state.core.clone();
    tauri::async_runtime::spawn_blocking(move || -> Res<CalibrationResult> {
        let engine = core.ensure_engine().map_err(anyhow_err)?;
        let sr = engine.sample_rate();
        let tap = method == "tap";
        let spacing = engine.frames_from_ms(if tap { 750.0 } else { 500.0 });
        let count = if tap { 10 } else { 8 };
        let first = engine.frame_after_ms(if tap { 1500.0 } else { 400.0 });
        let click = core.renderer().click(true);
        let scheduled: Vec<u64> = (0..count).map(|i| first + i as u64 * spacing).collect();
        for &f in &scheduled {
            engine.play(click.clone(), f, if tap { 1.0 } else { 0.8 }, Bus::Click).map_err(anyhow_err)?;
        }
        let estimate = engine.roundtrip_estimate().unwrap_or(2 * engine.buffer_frames() as i64 + 64);
        let a = (first as i64 + estimate - engine.frames_from_ms(300.0) as i64).max(0) as u64;
        let b = scheduled.last().unwrap() + spacing + estimate.max(0) as u64;
        let capture = engine.capture(a, b, Duration::from_secs(20)).map_err(anyhow_err)?;
        // Taps come early or late; skip the first two while the user finds the beat.
        let (sched, window) = if tap { (&scheduled[2..], 250.0) } else { (&scheduled[..], 150.0) };
        let result = analyze_clicks(&capture, a, sched, estimate, sr, window)
            .ok_or_else(|| "No clicks were detected in the input. Check the cable (loopback) or tap closer to the microphone.".to_string())?;
        let row = CalibrationRow {
            id: new_id(),
            device_key: core.device_key(&engine),
            residual_frames: result.residual_frames,
            sample_rate: sr,
            buffer_frames: Some(engine.buffer_frames()),
            method: method.clone(),
            spread_ms: Some(result.spread_ms),
            created_at: now().to_rfc3339(),
        };
        core.db.lock().save_calibration(&row).map_err(anyhow_err)?;
        Ok(result)
    })
    .await
    .map_err(err)?
}

#[tauri::command]
pub fn list_presets(state: State<'_, AppState>) -> Vec<PresetView> {
    let ctx = state.core.gen_context();
    let view =
        |id: String, category: String, description: String, source: &str, spec: &SessionSpec| {
            let r = validate_session(spec, &ctx);
            PresetView {
                id,
                title: spec.title.clone(),
                category,
                description,
                source: source.to_string(),
                trial_count: spec.total_trials(),
                estimated_minutes: r.estimated_minutes,
                block_titles: spec.blocks.iter().map(|b| b.display_title()).collect(),
            }
        };
    let mut out = Vec::new();
    for p in builtin_presets() {
        if let Ok(spec) = SessionSpec::from_value(p.spec.clone()) {
            out.push(view(p.id, p.category, p.description, "builtin", &spec));
        }
    }
    if let Ok(saved) = state.core.db.lock().user_presets() {
        for (name, spec, source) in saved {
            let desc = spec.notes.clone().unwrap_or_default();
            out.push(view(name, "Saved".into(), desc, &source, &spec));
        }
    }
    out
}

#[tauri::command]
pub fn list_queue(state: State<'_, AppState>) -> Res<Vec<SessionRow>> {
    state.core.db.lock().queued_sessions().map_err(anyhow_err)
}

#[tauri::command]
pub fn remove_queued(state: State<'_, AppState>, session_id: String) -> Res<bool> {
    state
        .core
        .db
        .lock()
        .delete_queued_session(&session_id)
        .map_err(anyhow_err)
}

#[tauri::command]
pub async fn start_session(state: State<'_, AppState>, request: StartRequest) -> Res<String> {
    let core = state.core.clone();
    let runner = state.runner.clone();
    tauri::async_runtime::spawn_blocking(move || -> Res<String> {
        if runner.is_active() {
            return Err("A session is already running.".into());
        }
        core.ensure_engine()
            .map_err(|e| format!("Audio is not available: {e:#}"))?;
        let db = core.db.lock();
        let (id, spec) = match request {
            StartRequest::Preset { id } => {
                let spec = find_builtin(&id)
                    .or_else(|| {
                        db.user_presets()
                            .ok()?
                            .into_iter()
                            .find(|(n, _, _)| *n == id)
                            .map(|(_, s, _)| s)
                    })
                    .ok_or_else(|| format!("unknown preset {id}"))?;
                (
                    db.start_new_session(&spec, "preset").map_err(anyhow_err)?,
                    spec,
                )
            }
            StartRequest::Queued { session_id } => {
                let row = db
                    .get_session(&session_id)
                    .map_err(anyhow_err)?
                    .ok_or("session not found")?;
                if row.status != SessionStatus::Queued {
                    return Err("that session is not queued any more".into());
                }
                let spec = row.spec().map_err(anyhow_err)?;
                (session_id, spec)
            }
            StartRequest::Repeat { session_id } => {
                let row = db
                    .get_session(&session_id)
                    .map_err(anyhow_err)?
                    .ok_or("session not found")?;
                let mut spec = row.spec().map_err(anyhow_err)?;
                spec.seed = None;
                (
                    db.start_new_session(&spec, "repeat").map_err(anyhow_err)?,
                    spec,
                )
            }
            StartRequest::Spec { spec } => {
                let spec = SessionSpec::from_value(spec).map_err(err)?;
                (
                    db.start_new_session(&spec, "user").map_err(anyhow_err)?,
                    spec,
                )
            }
        };
        drop(db);
        runner.start(id.clone(), spec).map_err(anyhow_err)?;
        Ok(id)
    })
    .await
    .map_err(err)?
}

#[tauri::command]
pub fn runner_command(state: State<'_, AppState>, command: RunnerCommand) {
    state.runner.command(command);
}

#[tauri::command]
pub fn runner_state(state: State<'_, AppState>) -> RunnerState {
    state.runner.state()
}

#[tauri::command]
pub fn list_sessions(state: State<'_, AppState>, limit: Option<u32>) -> Res<Vec<SessionRow>> {
    let rows = state
        .core
        .db
        .lock()
        .list_sessions(None, limit.unwrap_or(50))
        .map_err(anyhow_err)?;
    Ok(rows
        .into_iter()
        .filter(|s| s.status != SessionStatus::Queued)
        .collect())
}

#[tauri::command]
pub fn session_report(state: State<'_, AppState>, session_id: String) -> Res<SessionReport> {
    reports::session_report(&state.core.db.lock(), &session_id).map_err(anyhow_err)
}

#[tauri::command]
pub fn trial_detail(state: State<'_, AppState>, trial_id: String) -> Res<Value> {
    reports::trial_detail(&state.core.db.lock(), &trial_id).map_err(anyhow_err)
}

/// Plays the stored recording of a trial.
#[tauri::command]
pub fn play_recording(state: State<'_, AppState>, trial_id: String) -> Res<()> {
    let path = state
        .core
        .db
        .lock()
        .recording_path(&trial_id)
        .map_err(anyhow_err)?
        .ok_or("no recording for this trial")?;
    let mut reader = hound::WavReader::open(&path).map_err(err)?;
    let spec = reader.spec();
    let samples: Vec<f32> = reader
        .samples::<i16>()
        .filter_map(|s| s.ok())
        .map(|s| s as f32 / i16::MAX as f32)
        .collect();
    let engine = state.core.ensure_engine().map_err(anyhow_err)?;
    let mono = if spec.sample_rate == engine.sample_rate() {
        samples
    } else {
        // Nearest-sample conversion is enough for listening back.
        let ratio = spec.sample_rate as f64 / engine.sample_rate() as f64;
        let n = (samples.len() as f64 / ratio) as usize;
        (0..n)
            .map(|i| samples[((i as f64 * ratio) as usize).min(samples.len() - 1)])
            .collect()
    };
    let buf = SoundBuffer::from_mono(engine.sample_rate(), &mono);
    engine
        .play_now(Arc::new(buf), 1.0, Bus::Prompt)
        .map_err(anyhow_err)?;
    Ok(())
}

#[tauri::command]
pub fn get_range_profile(state: State<'_, AppState>) -> Res<RangeProfile> {
    state.core.db.lock().range_profile().map_err(anyhow_err)
}

#[tauri::command]
pub fn set_range_profile(state: State<'_, AppState>, profile: RangeProfile) -> Res<RangeProfile> {
    let db = state.core.db.lock();
    db.set_range_profile(&profile, "manual")
        .map_err(anyhow_err)?;
    db.range_profile().map_err(anyhow_err)
}

#[tauri::command]
pub fn add_comfort_report(
    state: State<'_, AppState>,
    session_id: Option<String>,
    phase: String,
    level: String,
    note: Option<String>,
) -> Res<()> {
    state
        .core
        .db
        .lock()
        .add_comfort_report(session_id.as_deref(), &phase, &level, note.as_deref())
        .map_err(anyhow_err)
}

#[tauri::command]
pub fn get_stats(state: State<'_, AppState>, query: StatsQuery) -> Res<Vec<StatsRow>> {
    trainer_core::stats::get_stats(&state.core.db.lock(), &query).map_err(anyhow_err)
}
