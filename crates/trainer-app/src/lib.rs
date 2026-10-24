//! Singing Trainer desktop app: Tauri shell around the audio engine, session runner and
//! database. The UI lives in `ui/`.

mod commands;
mod live;
mod runner;
mod state;
pub mod views;

use std::sync::Arc;

use tauri::Manager;

use crate::runner::Runner;
use crate::state::Core;

pub struct AppState {
    pub core: Arc<Core>,
    pub runner: Arc<Runner>,
}

pub fn run() {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .init();

    tauri::Builder::default()
        .setup(|app| {
            let resource_soundfonts = app.path().resource_dir().ok().map(|d| d.join("soundfonts"));
            let core = Core::new(state::data_dir(), resource_soundfonts)?;
            let _ = core.app.set(app.handle().clone());
            let runner = Arc::new(Runner::spawn(core.clone()));
            app.manage(AppState {
                core: core.clone(),
                runner,
            });
            live::spawn_live_forwarder(core.clone());
            live::spawn_queue_watcher(core.clone());
            // Load instruments and open the audio device without blocking the window.
            std::thread::spawn(move || {
                core.load_soundfonts();
                if let Err(e) = core.start_audio() {
                    tracing::warn!("audio did not start: {e:#}");
                }
                core.emit("audio-changed", ());
                let days = core.settings.read().recording_retention_days as i64;
                if let Ok(paths) = core.db.lock().expire_recordings(days) {
                    for p in paths {
                        let _ = std::fs::remove_file(p);
                    }
                }
            });
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::app_info,
            commands::get_settings,
            commands::save_settings,
            commands::list_audio_hosts,
            commands::audio_state,
            commands::start_audio,
            commands::stop_audio,
            commands::play_note,
            commands::set_drone,
            commands::subscribe_live,
            commands::calibrate,
            commands::list_presets,
            commands::list_queue,
            commands::remove_queued,
            commands::start_session,
            commands::runner_command,
            commands::runner_state,
            commands::list_sessions,
            commands::session_report,
            commands::trial_detail,
            commands::play_recording,
            commands::get_range_profile,
            commands::set_range_profile,
            commands::add_comfort_report,
            commands::get_stats,
            commands::list_anchors,
            commands::save_anchor,
            commands::remove_anchor,
        ])
        .run(tauri::generate_context!())
        .expect("error while running the Singing Trainer");
}
