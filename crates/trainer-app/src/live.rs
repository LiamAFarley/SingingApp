//! Background threads: forwarding live pitch frames to the UI, and noticing sessions the MCP
//! server queues.

use std::sync::Arc;
use std::sync::atomic::Ordering;
use std::time::{Duration, Instant};

use crossbeam_channel::RecvTimeoutError;
use trainer_core::pitch::window_for;
use trainer_core::storage::{Db, db_path};

use crate::state::Core;
use crate::views::{LiveBatch, LivePoint};

pub fn spawn_live_forwarder(core: Arc<Core>) {
    std::thread::Builder::new()
        .name("live-forwarder".into())
        .spawn(move || {
            loop {
                let Some(engine) = core.engine() else {
                    std::thread::sleep(Duration::from_millis(100));
                    continue;
                };
                let generation = core.engine_gen.load(Ordering::Acquire);
                let rx = engine.live();
                let sr = engine.sample_rate() as f64;
                // Live frames are stamped at the end of the analysis window; place them at its centre.
                let half_window = (window_for(sr as f32, 55.0) / 2) as f64;
                let mut roundtrip = core.roundtrip(&engine).0 as f64;
                let mut roundtrip_at = Instant::now();
                let mut points: Vec<LivePoint> = Vec::new();
                let mut peak = 0.0f32;
                let mut last_send = Instant::now();
                while core.engine_gen.load(Ordering::Acquire) == generation {
                    match rx.recv_timeout(Duration::from_millis(20)) {
                        Ok(f) => {
                            peak = peak.max(f.peak);
                            points.push(LivePoint {
                                t: (f.in_frame as f64 - half_window - roundtrip) / sr,
                                midi: f.midi,
                                clarity: f.clarity,
                                rms_db: f.rms_db,
                            });
                            while let Ok(f) = rx.try_recv() {
                                peak = peak.max(f.peak);
                                points.push(LivePoint {
                                    t: (f.in_frame as f64 - half_window - roundtrip) / sr,
                                    midi: f.midi,
                                    clarity: f.clarity,
                                    rms_db: f.rms_db,
                                });
                            }
                        }
                        Err(RecvTimeoutError::Timeout) => {}
                        Err(RecvTimeoutError::Disconnected) => break,
                    }
                    if roundtrip_at.elapsed() > Duration::from_millis(500) {
                        roundtrip = core.roundtrip(&engine).0 as f64;
                        roundtrip_at = Instant::now();
                    }
                    if last_send.elapsed() >= Duration::from_millis(16) && !points.is_empty() {
                        let batch = LiveBatch {
                            now_s: engine.now() as f64 / sr,
                            points: std::mem::take(&mut points),
                            peak,
                        };
                        peak = 0.0;
                        last_send = Instant::now();
                        let mut ch = core.live_channel.lock();
                        if let Some(c) = ch.as_ref() {
                            if c.send(batch).is_err() {
                                *ch = None;
                            }
                        }
                    }
                }
            }
        })
        .expect("spawn live forwarder");
}

/// Emits `queue-changed` when another process (the MCP server) writes to the database.
pub fn spawn_queue_watcher(core: Arc<Core>) {
    std::thread::Builder::new()
        .name("queue-watcher".into())
        .spawn(move || {
            let Ok(db) = Db::open(&db_path(&core.data_dir)) else {
                return;
            };
            let mut last = db.data_version().unwrap_or(0);
            loop {
                std::thread::sleep(Duration::from_millis(800));
                if let Ok(v) = db.data_version() {
                    if v != last {
                        last = v;
                        core.emit("queue-changed", ());
                    }
                }
            }
        })
        .expect("spawn queue watcher");
}
