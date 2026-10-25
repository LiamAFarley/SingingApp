//! Shared application state: settings, the audio engine, instruments and the database.

use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, OnceLock};
use std::time::Duration;

use anyhow::{Context, Result, anyhow};
use parking_lot::{Mutex, RwLock};
use tauri::{AppHandle, Emitter, ipc::Channel};
use trainer_audio::render::{Renderer, SoundFonts};
use trainer_audio::{AudioEngine, Bus, EngineInfo, LiveRange, SoundBuffer};
use trainer_core::generate::GenContext;
use trainer_core::storage::{CalibrationRow, Db, db_path};
use trainer_core::theory::midi_to_hz;

use crate::views::{AppSettings, LiveBatch};

pub const SETTINGS_KEY: &str = "app_settings";

pub struct Core {
    pub data_dir: PathBuf,
    pub resource_soundfonts: Option<PathBuf>,
    pub db: Mutex<Db>,
    pub settings: RwLock<AppSettings>,
    engine: RwLock<Option<Arc<AudioEngine>>>,
    pub engine_error: RwLock<Option<String>>,
    pub engine_gen: AtomicU64,
    fonts: RwLock<SoundFonts>,
    gm_name: RwLock<Option<String>>,
    renderer: RwLock<Arc<Renderer>>,
    pub live_channel: Mutex<Option<Channel<LiveBatch>>>,
    pub drone: Mutex<Option<u64>>,
    /// A library piece playing on the Library page.
    pub preview: Mutex<Option<u64>>,
    pub app: OnceLock<AppHandle>,
}

impl Core {
    pub fn new(data_dir: PathBuf, resource_soundfonts: Option<PathBuf>) -> Result<Arc<Core>> {
        std::fs::create_dir_all(&data_dir)
            .with_context(|| format!("creating {}", data_dir.display()))?;
        let db = Db::open(&db_path(&data_dir))?;
        db.abandon_stale_running()?;
        let settings: AppSettings = db.get_setting(SETTINGS_KEY)?.unwrap_or_default();
        let core = Arc::new(Core {
            data_dir,
            resource_soundfonts,
            db: Mutex::new(db),
            settings: RwLock::new(settings),
            engine: RwLock::new(None),
            engine_error: RwLock::new(None),
            engine_gen: AtomicU64::new(0),
            fonts: RwLock::new(SoundFonts::default()),
            gm_name: RwLock::new(None),
            renderer: RwLock::new(Arc::new(Renderer::new(48_000, SoundFonts::default()))),
            live_channel: Mutex::new(None),
            drone: Mutex::new(None),
            preview: Mutex::new(None),
            app: OnceLock::new(),
        });
        let _ = std::fs::create_dir_all(core.library_dir());
        Ok(core)
    }

    pub fn emit<S: serde::Serialize + Clone>(&self, event: &str, payload: S) {
        if let Some(app) = self.app.get() {
            let _ = app.emit(event, payload);
        }
    }

    pub fn soundfont_dir(&self) -> PathBuf {
        self.data_dir.join("soundfonts")
    }

    pub fn recordings_dir(&self) -> PathBuf {
        self.data_dir.join("recordings")
    }

    /// Loads the General MIDI bank (bundled resource, or the first SF2 in the data folder) and any
    /// extra banks. Slow (tens of ms per bank), so it runs off the UI thread at startup.
    pub fn load_soundfonts(&self) {
        let mut fonts = SoundFonts::default();
        let mut gm_name = None;
        let mut candidates: Vec<PathBuf> = Vec::new();
        for dir in [self.resource_soundfonts.clone(), Some(self.soundfont_dir())]
            .into_iter()
            .flatten()
        {
            if let Ok(entries) = std::fs::read_dir(&dir) {
                let mut v: Vec<PathBuf> = entries
                    .flatten()
                    .map(|e| e.path())
                    .filter(|p| {
                        p.extension()
                            .and_then(|x| x.to_str())
                            .map(|x| x.eq_ignore_ascii_case("sf2"))
                            .unwrap_or(false)
                    })
                    .collect();
                v.sort();
                candidates.extend(v);
            }
        }
        // Prefer a bank whose name says General MIDI / GS.
        let gm_pos = candidates
            .iter()
            .position(|p| {
                let n = p
                    .file_name()
                    .and_then(|n| n.to_str())
                    .unwrap_or("")
                    .to_lowercase();
                n.contains("generaluser") || n.contains("gm") || n.contains("fluid")
            })
            .or(if candidates.is_empty() { None } else { Some(0) });
        for (i, p) in candidates.iter().enumerate() {
            match SoundFonts::load_file(p) {
                Ok(sf) => {
                    let stem = p
                        .file_stem()
                        .and_then(|s| s.to_str())
                        .unwrap_or("bank")
                        .to_string();
                    if Some(i) == gm_pos {
                        fonts.gm = Some(sf.clone());
                        gm_name = Some(stem.clone());
                    }
                    fonts.banks.insert(stem, sf);
                }
                Err(e) => tracing::warn!("could not load {}: {e:#}", p.display()),
            }
        }
        *self.gm_name.write() = gm_name;
        *self.fonts.write() = fonts;
        self.rebuild_renderer();
    }

    pub fn gm_name(&self) -> Option<String> {
        self.gm_name.read().clone()
    }

    pub fn bank_names(&self) -> Vec<String> {
        let mut v: Vec<String> = self.fonts.read().banks.keys().cloned().collect();
        v.sort();
        v
    }

    fn rebuild_renderer(&self) {
        let sr = self.engine().map(|e| e.sample_rate()).unwrap_or(48_000);
        let mut r = Renderer::new(sr, self.fonts.read().clone());
        r.a4_hz = self.settings.read().a4_hz;
        *self.renderer.write() = Arc::new(r);
    }

    pub fn renderer(&self) -> Arc<Renderer> {
        self.renderer.read().clone()
    }

    pub fn engine(&self) -> Option<Arc<AudioEngine>> {
        self.engine.read().clone()
    }

    /// The generation context, after reading any library files that changed.
    pub fn gen_context(&self) -> GenContext {
        let db = self.db.lock();
        if let Err(e) = db.sync_library(&self.library_dir()) {
            tracing::warn!("library sync failed: {e:#}");
        }
        let mut ctx = db.gen_context().unwrap_or_default();
        let dir = self.soundfont_dir();
        let mut dirs = vec![dir.as_path()];
        if let Some(r) = &self.resource_soundfonts {
            dirs.push(r.as_path());
        }
        ctx.soundfonts = Some(trainer_core::timbre::installed_soundfonts(&dirs));
        ctx
    }

    pub fn library_dir(&self) -> PathBuf {
        trainer_core::library::folder(&self.data_dir)
    }

    /// Live tracker range from the singer's range profile.
    fn live_range(&self) -> LiveRange {
        let a4 = self.settings.read().a4_hz;
        let range = self
            .db
            .lock()
            .range_profile()
            .map(|r| r.comfortable())
            .unwrap_or_else(|_| GenContext::default().comfortable);
        let fmin = (midi_to_hz(range.low.0 as f64 - 10.0, a4) as f32).clamp(55.0, 400.0);
        let fmax = (midi_to_hz(range.high.0 as f64 + 12.0, a4) as f32).clamp(400.0, 1600.0);
        LiveRange {
            fmin,
            fmax,
            a4_hz: a4,
        }
    }

    pub fn start_audio(&self) -> Result<EngineInfo> {
        self.stop_audio();
        let settings = self.settings.read().audio.clone();
        match AudioEngine::start(&settings, self.live_range()) {
            Ok(engine) => {
                let info = engine.info().clone();
                *self.engine.write() = Some(Arc::new(engine));
                *self.engine_error.write() = None;
                self.engine_gen.fetch_add(1, Ordering::AcqRel);
                self.rebuild_renderer();
                Ok(info)
            }
            Err(e) => {
                let msg = format!("{e:#}");
                *self.engine_error.write() = Some(msg.clone());
                Err(anyhow!(msg))
            }
        }
    }

    pub fn stop_audio(&self) {
        let old = self.engine.write().take();
        if let Some(e) = old {
            e.stop_all();
            std::thread::sleep(Duration::from_millis(20));
            e.shutdown();
        }
        *self.drone.lock() = None;
        self.engine_gen.fetch_add(1, Ordering::AcqRel);
    }

    /// The running engine, starting it if needed.
    pub fn ensure_engine(&self) -> Result<Arc<AudioEngine>> {
        if let Some(e) = self.engine() {
            if e.is_running() {
                return Ok(e);
            }
        }
        self.start_audio()?;
        self.engine()
            .ok_or_else(|| anyhow!("audio engine did not start"))
    }

    pub fn device_key(&self, engine: &AudioEngine) -> String {
        engine.info().device_key(engine.buffer_frames())
    }

    pub fn calibration(&self, engine: &AudioEngine) -> Option<CalibrationRow> {
        self.db
            .lock()
            .latest_calibration(&self.device_key(engine))
            .ok()
            .flatten()
    }

    /// Frames from an output frame to the input frame where its sound arrives, and whether a
    /// stored calibration was applied.
    pub fn roundtrip(&self, engine: &AudioEngine) -> (i64, bool) {
        let estimate = engine
            .roundtrip_estimate()
            .unwrap_or(2 * engine.buffer_frames() as i64 + 64);
        match self.calibration(engine) {
            Some(c) => (estimate + c.residual_frames, true),
            None => (estimate, false),
        }
    }

    pub fn save_settings(&self, s: AppSettings) -> Result<()> {
        self.db.lock().set_setting(SETTINGS_KEY, &s)?;
        let a4_changed = self.settings.read().a4_hz != s.a4_hz;
        *self.settings.write() = s;
        if a4_changed {
            self.rebuild_renderer();
        }
        Ok(())
    }

    /// Plays a short buffer immediately on the prompt bus.
    pub fn play_now(&self, buf: SoundBuffer) -> Result<()> {
        let engine = self.ensure_engine()?;
        engine.play_now(Arc::new(buf), 1.0, Bus::Prompt)?;
        Ok(())
    }
}

/// The app's `data` folder, unless `SINGING_TRAINER_DATA_DIR` is set.
pub fn data_dir() -> PathBuf {
    trainer_core::storage::default_data_dir()
}
