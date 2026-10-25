//! SQLite storage shared by the app and the MCP server. Trials are append-only; everything else
//! the agent reads is computed from them.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, anyhow};
use chrono::{DateTime, Utc};
use rusqlite::{Connection, OptionalExtension, Row, params};
use rusqlite_migration::{M, Migrations};
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use ts_rs::TS;

use crate::generate::GenContext;
use crate::library::{self, LibraryPiece};
use crate::pitch::{PitchFrame, PitchTrack};
use crate::schema::{Anchor, NoteRange, SessionSpec};
use crate::theory::Note;

const MIGRATION_1: &str = r#"
CREATE TABLE settings (
  key TEXT PRIMARY KEY,
  value TEXT NOT NULL
);

CREATE TABLE sessions (
  id TEXT PRIMARY KEY,
  title TEXT NOT NULL,
  source TEXT NOT NULL,
  protocol TEXT,
  spec_json TEXT NOT NULL,
  status TEXT NOT NULL,
  queued_at TEXT,
  started_at TEXT,
  completed_at TEXT,
  queue_note TEXT,
  summary_json TEXT
);
CREATE INDEX sessions_status ON sessions(status);

CREATE TABLE trials (
  id TEXT PRIMARY KEY,
  session_id TEXT NOT NULL REFERENCES sessions(id) ON DELETE CASCADE,
  block_idx INTEGER NOT NULL,
  trial_idx INTEGER NOT NULL,
  attempt INTEGER NOT NULL DEFAULT 0,
  type TEXT NOT NULL,
  protocol TEXT,
  tags_json TEXT NOT NULL,
  stimulus_json TEXT NOT NULL,
  spec_json TEXT NOT NULL,
  pool TEXT,
  feedback_mode TEXT NOT NULL,
  response_json TEXT NOT NULL,
  metrics_json TEXT NOT NULL,
  summary TEXT NOT NULL,
  correct INTEGER,
  score REAL,
  rt_ms REAL,
  assisted INTEGER NOT NULL DEFAULT 0,
  assessment INTEGER NOT NULL DEFAULT 0,
  labeled_reference INTEGER NOT NULL DEFAULT 0,
  started_at TEXT NOT NULL,
  start_frame INTEGER,
  device_profile TEXT,
  calibrated INTEGER NOT NULL DEFAULT 0,
  secs_since_labeled_reference REAL,
  interrupted INTEGER NOT NULL DEFAULT 0
);
CREATE INDEX trials_session ON trials(session_id, block_idx, trial_idx);
CREATE INDEX trials_type_time ON trials(type, started_at);

CREATE TABLE traces (
  trial_id TEXT PRIMARY KEY REFERENCES trials(id) ON DELETE CASCADE,
  hop_s REAL NOT NULL,
  t0_s REAL NOT NULL,
  midi BLOB NOT NULL,
  clarity BLOB NOT NULL,
  level BLOB NOT NULL
);

CREATE TABLE recordings (
  trial_id TEXT PRIMARY KEY REFERENCES trials(id) ON DELETE CASCADE,
  path TEXT NOT NULL,
  sample_rate INTEGER NOT NULL,
  frames INTEGER NOT NULL,
  pinned INTEGER NOT NULL DEFAULT 0,
  created_at TEXT NOT NULL
);

CREATE TABLE regrades (
  id TEXT PRIMARY KEY,
  trial_id TEXT NOT NULL REFERENCES trials(id) ON DELETE CASCADE,
  grading_json TEXT NOT NULL,
  metrics_json TEXT NOT NULL,
  correct INTEGER,
  score REAL,
  created_at TEXT NOT NULL
);

CREATE TABLE coach_notes (
  id TEXT PRIMARY KEY,
  created_at TEXT NOT NULL,
  tags_json TEXT NOT NULL,
  text TEXT NOT NULL
);

CREATE TABLE comfort_reports (
  id TEXT PRIMARY KEY,
  session_id TEXT,
  phase TEXT NOT NULL,
  level TEXT NOT NULL,
  note TEXT,
  created_at TEXT NOT NULL
);

CREATE TABLE range_profile (
  id INTEGER PRIMARY KEY CHECK (id = 1),
  lowest_reachable INTEGER,
  highest_reachable INTEGER,
  comfortable_low INTEGER NOT NULL,
  comfortable_high INTEGER NOT NULL,
  voice_type TEXT,
  updated_at TEXT NOT NULL,
  source TEXT NOT NULL
);

CREATE TABLE calibrations (
  id TEXT PRIMARY KEY,
  device_key TEXT NOT NULL,
  residual_frames INTEGER NOT NULL,
  sample_rate INTEGER NOT NULL,
  buffer_frames INTEGER,
  method TEXT NOT NULL,
  spread_ms REAL,
  created_at TEXT NOT NULL
);
CREATE INDEX calibrations_device ON calibrations(device_key, created_at);

CREATE TABLE presets (
  name TEXT PRIMARY KEY,
  spec_json TEXT NOT NULL,
  source TEXT NOT NULL,
  created_at TEXT NOT NULL
);
"#;

const MIGRATION_2: &str = r#"
CREATE TABLE anchors (
  id TEXT PRIMARY KEY,
  title TEXT NOT NULL,
  first_note TEXT NOT NULL,
  melody_json TEXT,
  created_at TEXT NOT NULL,
  source TEXT NOT NULL
);
"#;

/// The melody library index. The library folder is the source; `library_files` remembers what
/// each file looked like when it was read, so unchanged files are not read again.
const MIGRATION_3: &str = r#"
CREATE TABLE library_files (
  file TEXT PRIMARY KEY,
  stamp TEXT NOT NULL,
  pieces INTEGER NOT NULL,
  error TEXT,
  read_at TEXT NOT NULL
);
CREATE TABLE library_pieces (
  id TEXT PRIMARY KEY,
  file TEXT NOT NULL,
  position INTEGER NOT NULL,
  piece_json TEXT NOT NULL
);
CREATE INDEX library_pieces_file ON library_pieces(file);
"#;

/// Courses: each course's current level, and a log of the levels passed.
const MIGRATION_4: &str = r#"
CREATE TABLE course_state (
  course TEXT PRIMARY KEY,
  level INTEGER NOT NULL,
  updated_at TEXT NOT NULL,
  source TEXT NOT NULL
);
CREATE TABLE course_passes (
  course TEXT NOT NULL,
  level INTEGER NOT NULL,
  passed_at TEXT NOT NULL,
  source TEXT NOT NULL,
  evidence_json TEXT NOT NULL,
  PRIMARY KEY (course, level)
);
"#;

fn migrations() -> Migrations<'static> {
    Migrations::new(vec![
        M::up(MIGRATION_1),
        M::up(MIGRATION_2),
        M::up(MIGRATION_3),
        M::up(MIGRATION_4),
    ])
}

pub fn now() -> DateTime<Utc> {
    Utc::now()
}

pub fn new_id() -> String {
    uuid::Uuid::now_v7().to_string()
}

/// The app's own `data` folder, overridable with `SINGING_TRAINER_DATA_DIR`.
///
/// A build launched from `target\release` or `target\debug` keeps its files in the project
/// folder (`SingingApp\data`). A copy of the exe keeps them in `data` beside that exe.
/// An older folder in `%APPDATA%\SingingTrainer` is moved across on first use.
pub fn default_data_dir() -> PathBuf {
    if let Ok(dir) = std::env::var("SINGING_TRAINER_DATA_DIR") {
        if !dir.trim().is_empty() {
            return PathBuf::from(dir);
        }
    }
    let dir = data_dir_for_exe(
        &std::env::current_exe().unwrap_or_else(|_| PathBuf::from("singing-trainer.exe")),
    );
    adopt_legacy_appdata(&dir);
    dir
}

/// `data` under the project when `exe` lives in a Cargo `target` directory, otherwise `data`
/// beside the exe.
pub fn data_dir_for_exe(exe: &Path) -> PathBuf {
    for ancestor in exe.ancestors().skip(1) {
        if ancestor.file_name().and_then(|n| n.to_str()) == Some("target") {
            if let Some(root) = ancestor.parent().filter(|p| !p.as_os_str().is_empty()) {
                return root.join("data");
            }
        }
    }
    exe.parent().unwrap_or_else(|| Path::new(".")).join("data")
}

/// Copies an existing `%APPDATA%\SingingTrainer` tree into `dest` when `dest` has no database yet,
/// then removes the AppData copy.
fn adopt_legacy_appdata(dest: &Path) {
    let Some(src) = dirs::data_dir().map(|d| d.join("SingingTrainer")) else {
        return;
    };
    if src == dest || !src.join("trainer.sqlite").exists() || dest.join("trainer.sqlite").exists() {
        return;
    }
    if copy_tree(&src, dest).is_ok() && dest.join("trainer.sqlite").exists() {
        let _ = std::fs::remove_dir_all(&src);
    }
}

fn copy_tree(src: &Path, dest: &Path) -> std::io::Result<()> {
    std::fs::create_dir_all(dest)?;
    for entry in std::fs::read_dir(src)? {
        let entry = entry?;
        let to = dest.join(entry.file_name());
        if entry.file_type()?.is_dir() {
            copy_tree(&entry.path(), &to)?;
        } else if !to.exists() {
            std::fs::copy(entry.path(), &to)?;
        }
    }
    Ok(())
}

pub fn db_path(data_dir: &Path) -> PathBuf {
    data_dir.join("trainer.sqlite")
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
pub enum SessionStatus {
    Queued,
    Running,
    Completed,
    Abandoned,
}

impl SessionStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            SessionStatus::Queued => "queued",
            SessionStatus::Running => "running",
            SessionStatus::Completed => "completed",
            SessionStatus::Abandoned => "abandoned",
        }
    }

    pub fn parse(s: &str) -> Self {
        match s {
            "queued" => SessionStatus::Queued,
            "running" => SessionStatus::Running,
            "completed" => SessionStatus::Completed,
            _ => SessionStatus::Abandoned,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, TS)]
pub struct SessionRow {
    pub id: String,
    pub title: String,
    /// `preset`, `llm`, `user` or `repeat`.
    pub source: String,
    pub protocol: Option<String>,
    #[ts(type = "Record<string, unknown>")]
    pub spec: serde_json::Value,
    pub status: SessionStatus,
    pub queued_at: Option<String>,
    pub started_at: Option<String>,
    pub completed_at: Option<String>,
    pub queue_note: Option<String>,
    #[ts(type = "Record<string, unknown> | null")]
    pub summary: Option<serde_json::Value>,
}

impl SessionRow {
    pub fn spec(&self) -> Result<SessionSpec> {
        SessionSpec::from_value(self.spec.clone())
            .map_err(|e| anyhow!("stored session spec is invalid: {e}"))
    }
}

/// A trial as stored. Written once, never updated.
#[derive(Clone, Debug, Serialize, Deserialize, TS)]
pub struct TrialRow {
    pub id: String,
    pub session_id: String,
    pub block_idx: u32,
    pub trial_idx: u32,
    /// Retry number for the same trial (0 = first attempt).
    pub attempt: u32,
    #[serde(rename = "type")]
    pub kind: String,
    pub protocol: Option<String>,
    pub tags: Vec<String>,
    #[ts(type = "Record<string, unknown>")]
    pub stimulus: serde_json::Value,
    /// The full trial spec as played.
    #[ts(type = "Record<string, unknown>")]
    pub spec: serde_json::Value,
    pub pool: Option<String>,
    pub feedback_mode: String,
    #[ts(type = "Record<string, unknown>")]
    pub response: serde_json::Value,
    #[ts(type = "Record<string, unknown>")]
    pub metrics: serde_json::Value,
    pub summary: String,
    pub correct: Option<bool>,
    pub score: Option<f32>,
    pub rt_ms: Option<f64>,
    /// Retried with a guide (Sing along) or other help.
    pub assisted: bool,
    pub assessment: bool,
    /// The trial played a tone whose name the user was told (feedback or pairing).
    pub labeled_reference: bool,
    pub started_at: String,
    #[ts(type = "number | null")]
    pub start_frame: Option<i64>,
    pub device_profile: Option<String>,
    pub calibrated: bool,
    pub secs_since_labeled_reference: Option<f64>,
    pub interrupted: bool,
}

const TRIAL_COLUMNS: &str = "id, session_id, block_idx, trial_idx, attempt, type, protocol, tags_json, stimulus_json, \
    spec_json, pool, feedback_mode, response_json, metrics_json, summary, correct, score, rt_ms, assisted, assessment, \
    labeled_reference, started_at, start_frame, device_profile, calibrated, secs_since_labeled_reference, interrupted";

fn json_col(row: &Row, idx: usize) -> rusqlite::Result<serde_json::Value> {
    let s: String = row.get(idx)?;
    Ok(serde_json::from_str(&s).unwrap_or(serde_json::Value::Null))
}

fn trial_from_row(row: &Row) -> rusqlite::Result<TrialRow> {
    let tags: serde_json::Value = json_col(row, 7)?;
    Ok(TrialRow {
        id: row.get(0)?,
        session_id: row.get(1)?,
        block_idx: row.get(2)?,
        trial_idx: row.get(3)?,
        attempt: row.get(4)?,
        kind: row.get(5)?,
        protocol: row.get(6)?,
        tags: serde_json::from_value(tags).unwrap_or_default(),
        stimulus: json_col(row, 8)?,
        spec: json_col(row, 9)?,
        pool: row.get(10)?,
        feedback_mode: row.get(11)?,
        response: json_col(row, 12)?,
        metrics: json_col(row, 13)?,
        summary: row.get(14)?,
        correct: row.get::<_, Option<i64>>(15)?.map(|v| v != 0),
        score: row.get::<_, Option<f64>>(16)?.map(|v| v as f32),
        rt_ms: row.get(17)?,
        assisted: row.get::<_, i64>(18)? != 0,
        assessment: row.get::<_, i64>(19)? != 0,
        labeled_reference: row.get::<_, i64>(20)? != 0,
        started_at: row.get(21)?,
        start_frame: row.get(22)?,
        device_profile: row.get(23)?,
        calibrated: row.get::<_, i64>(24)? != 0,
        secs_since_labeled_reference: row.get(25)?,
        interrupted: row.get::<_, i64>(26)? != 0,
    })
}

fn session_from_row(row: &Row) -> rusqlite::Result<SessionRow> {
    let summary: Option<String> = row.get(10)?;
    Ok(SessionRow {
        id: row.get(0)?,
        title: row.get(1)?,
        source: row.get(2)?,
        protocol: row.get(3)?,
        spec: json_col(row, 4)?,
        status: SessionStatus::parse(&row.get::<_, String>(5)?),
        queued_at: row.get(6)?,
        started_at: row.get(7)?,
        completed_at: row.get(8)?,
        queue_note: row.get(9)?,
        summary: summary.and_then(|s| serde_json::from_str(&s).ok()),
    })
}

const SESSION_COLUMNS: &str = "id, title, source, protocol, spec_json, status, queued_at, started_at, completed_at, queue_note, summary_json";

/// Filters for trial queries (all optional, combined with AND).
#[derive(Clone, Debug, Default, Serialize, Deserialize, schemars::JsonSchema, TS)]
pub struct TrialFilter {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub session_id: Option<String>,
    /// Exercise types, e.g. `["pitch_id"]`.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub types: Vec<String>,
    /// RFC 3339 timestamp or date (`2026-10-01`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub since: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub until: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub protocol: Option<String>,
    /// Trials carrying this tag (prefix match, so `ap.id` also matches `ap.id.timbre_transfer`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tag: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pool: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub assisted: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub assessment: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub feedback_mode: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub correct: Option<bool>,
    /// Include interrupted trials (default false).
    #[serde(default)]
    pub include_interrupted: bool,
}

/// A file of the library folder as last read.
#[derive(Clone, Debug, Serialize, Deserialize, TS)]
pub struct LibraryFile {
    /// Path relative to the library folder, with `/`.
    pub file: String,
    pub pieces: u32,
    /// Why the file could not be read.
    pub error: Option<String>,
    pub read_at: String,
}

/// What a library sync changed.
#[derive(Clone, Debug, Default, Serialize, Deserialize, TS)]
pub struct LibrarySync {
    /// Files read (new or changed).
    pub read: Vec<String>,
    pub removed: Vec<String>,
    /// Files that could not be read, with the reason.
    pub errors: Vec<(String, String)>,
}

impl LibrarySync {
    pub fn changed(&self) -> bool {
        !(self.read.is_empty() && self.removed.is_empty() && self.errors.is_empty())
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, TS)]
pub struct CoachNote {
    pub id: String,
    pub created_at: String,
    pub tags: Vec<String>,
    pub text: String,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, schemars::JsonSchema, TS)]
pub struct RangeProfile {
    pub comfortable_low: Note,
    pub comfortable_high: Note,
    #[serde(default)]
    pub lowest_reachable: Option<Note>,
    #[serde(default)]
    pub highest_reachable: Option<Note>,
    /// e.g. `baritone`; informational.
    #[serde(default)]
    pub voice_type: Option<String>,
    #[serde(default)]
    pub updated_at: Option<String>,
    /// `default`, `manual`, `range_finder` or `llm`.
    #[serde(default)]
    pub source: Option<String>,
}

impl RangeProfile {
    pub fn default_profile() -> Self {
        RangeProfile {
            comfortable_low: Note(48),
            comfortable_high: Note(67),
            lowest_reachable: None,
            highest_reachable: None,
            voice_type: None,
            updated_at: None,
            source: Some("default".into()),
        }
    }

    pub fn comfortable(&self) -> NoteRange {
        NoteRange::new(self.comfortable_low.0, self.comfortable_high.0)
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, TS)]
pub struct CalibrationRow {
    pub id: String,
    pub device_key: String,
    /// Measured round trip minus the driver-reported estimate, in frames.
    #[ts(type = "number")]
    pub residual_frames: i64,
    pub sample_rate: u32,
    pub buffer_frames: Option<u32>,
    pub method: String,
    pub spread_ms: Option<f64>,
    pub created_at: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, TS)]
pub struct ComfortReport {
    pub id: String,
    pub session_id: Option<String>,
    /// `pre` or `post`.
    pub phase: String,
    /// `comfortable`, `mild_fatigue`, `discomfort`, `pain` or `hoarseness`.
    pub level: String,
    pub note: Option<String>,
    pub created_at: String,
}

pub struct Db {
    conn: Connection,
}

fn f32s_to_blob(v: impl Iterator<Item = f32>) -> Vec<u8> {
    v.flat_map(|x| x.to_le_bytes()).collect()
}

fn blob_to_f32s(b: &[u8]) -> Vec<f32> {
    b.chunks_exact(4)
        .map(|c| f32::from_le_bytes([c[0], c[1], c[2], c[3]]))
        .collect()
}

impl Db {
    pub fn open(path: &Path) -> Result<Db> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)
                .with_context(|| format!("creating {}", parent.display()))?;
        }
        let conn = Connection::open(path).with_context(|| format!("opening {}", path.display()))?;
        Self::init(conn)
    }

    pub fn open_in_memory() -> Result<Db> {
        Self::init(Connection::open_in_memory()?)
    }

    fn init(mut conn: Connection) -> Result<Db> {
        conn.busy_timeout(std::time::Duration::from_secs(5))?;
        conn.pragma_update(None, "journal_mode", "WAL")?;
        conn.pragma_update(None, "foreign_keys", "ON")?;
        conn.pragma_update(None, "synchronous", "NORMAL")?;
        migrations()
            .to_latest(&mut conn)
            .context("migrating database")?;
        Ok(Db { conn })
    }

    pub fn conn(&self) -> &Connection {
        &self.conn
    }

    /// Changes whenever another connection commits; used to notice sessions queued by the MCP server.
    pub fn data_version(&self) -> Result<i64> {
        Ok(self
            .conn
            .query_row("PRAGMA data_version", [], |r| r.get(0))?)
    }

    // --- settings ------------------------------------------------------------------------

    pub fn get_setting<T: DeserializeOwned>(&self, key: &str) -> Result<Option<T>> {
        let v: Option<String> = self
            .conn
            .query_row("SELECT value FROM settings WHERE key = ?1", [key], |r| {
                r.get(0)
            })
            .optional()?;
        Ok(match v {
            Some(s) => serde_json::from_str(&s).ok(),
            None => None,
        })
    }

    pub fn set_setting<T: Serialize>(&self, key: &str, value: &T) -> Result<()> {
        self.conn.execute(
            "INSERT INTO settings(key, value) VALUES (?1, ?2) ON CONFLICT(key) DO UPDATE SET value = excluded.value",
            params![key, serde_json::to_string(value)?],
        )?;
        Ok(())
    }

    // --- sessions ------------------------------------------------------------------------

    /// Adds a session to the queue ("Up next").
    pub fn queue_session(
        &self,
        spec: &SessionSpec,
        source: &str,
        note: Option<&str>,
    ) -> Result<String> {
        let id = new_id();
        self.conn.execute(
            "INSERT INTO sessions(id, title, source, protocol, spec_json, status, queued_at, queue_note)
             VALUES (?1, ?2, ?3, ?4, ?5, 'queued', ?6, ?7)",
            params![id, spec.title, source, spec.protocol, serde_json::to_string(spec)?, now().to_rfc3339(), note],
        )?;
        Ok(id)
    }

    /// Creates a session that starts immediately (presets, repeats).
    pub fn start_new_session(&self, spec: &SessionSpec, source: &str) -> Result<String> {
        let id = new_id();
        self.conn.execute(
            "INSERT INTO sessions(id, title, source, protocol, spec_json, status, started_at)
             VALUES (?1, ?2, ?3, ?4, ?5, 'running', ?6)",
            params![
                id,
                spec.title,
                source,
                spec.protocol,
                serde_json::to_string(spec)?,
                now().to_rfc3339()
            ],
        )?;
        Ok(id)
    }

    pub fn mark_session_running(&self, id: &str) -> Result<()> {
        self.conn.execute(
            "UPDATE sessions SET status = 'running', started_at = COALESCE(started_at, ?2) WHERE id = ?1",
            params![id, now().to_rfc3339()],
        )?;
        Ok(())
    }

    pub fn finish_session(
        &self,
        id: &str,
        status: SessionStatus,
        summary: &serde_json::Value,
    ) -> Result<()> {
        self.conn.execute(
            "UPDATE sessions SET status = ?2, completed_at = ?3, summary_json = ?4 WHERE id = ?1",
            params![
                id,
                status.as_str(),
                now().to_rfc3339(),
                serde_json::to_string(summary)?
            ],
        )?;
        Ok(())
    }

    pub fn delete_queued_session(&self, id: &str) -> Result<bool> {
        Ok(self.conn.execute(
            "DELETE FROM sessions WHERE id = ?1 AND status = 'queued'",
            [id],
        )? > 0)
    }

    pub fn get_session(&self, id: &str) -> Result<Option<SessionRow>> {
        Ok(self
            .conn
            .query_row(
                &format!("SELECT {SESSION_COLUMNS} FROM sessions WHERE id = ?1"),
                [id],
                session_from_row,
            )
            .optional()?)
    }

    /// Sessions newest first. `status` filters when given.
    pub fn list_sessions(
        &self,
        status: Option<SessionStatus>,
        limit: u32,
    ) -> Result<Vec<SessionRow>> {
        let order = "ORDER BY COALESCE(started_at, queued_at) DESC";
        let mut out = Vec::new();
        match status {
            Some(s) => {
                let mut stmt = self.conn.prepare(&format!(
                    "SELECT {SESSION_COLUMNS} FROM sessions WHERE status = ?1 {order} LIMIT ?2"
                ))?;
                for r in stmt.query_map(params![s.as_str(), limit], session_from_row)? {
                    out.push(r?);
                }
            }
            None => {
                let mut stmt = self.conn.prepare(&format!(
                    "SELECT {SESSION_COLUMNS} FROM sessions {order} LIMIT ?1"
                ))?;
                for r in stmt.query_map(params![limit], session_from_row)? {
                    out.push(r?);
                }
            }
        }
        Ok(out)
    }

    /// Queued sessions oldest first (the order they will be offered).
    pub fn queued_sessions(&self) -> Result<Vec<SessionRow>> {
        let mut stmt = self.conn.prepare(&format!(
            "SELECT {SESSION_COLUMNS} FROM sessions WHERE status = 'queued' ORDER BY queued_at ASC"
        ))?;
        let rows = stmt.query_map([], session_from_row)?;
        Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
    }

    /// Sessions left `running` by a crash are marked abandoned at startup.
    pub fn abandon_stale_running(&self) -> Result<usize> {
        Ok(self.conn.execute(
            "UPDATE sessions SET status = 'abandoned', completed_at = ?1 WHERE status = 'running'",
            [now().to_rfc3339()],
        )?)
    }

    // --- trials --------------------------------------------------------------------------

    pub fn insert_trial(&self, t: &TrialRow) -> Result<()> {
        self.conn.execute(
            &format!("INSERT INTO trials({TRIAL_COLUMNS}) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18, ?19, ?20, ?21, ?22, ?23, ?24, ?25, ?26, ?27)"),
            params![
                t.id,
                t.session_id,
                t.block_idx,
                t.trial_idx,
                t.attempt,
                t.kind,
                t.protocol,
                serde_json::to_string(&t.tags)?,
                serde_json::to_string(&t.stimulus)?,
                serde_json::to_string(&t.spec)?,
                t.pool,
                t.feedback_mode,
                serde_json::to_string(&t.response)?,
                serde_json::to_string(&t.metrics)?,
                t.summary,
                t.correct.map(|c| c as i64),
                t.score.map(|s| s as f64),
                t.rt_ms,
                t.assisted as i64,
                t.assessment as i64,
                t.labeled_reference as i64,
                t.started_at,
                t.start_frame,
                t.device_profile,
                t.calibrated as i64,
                t.secs_since_labeled_reference,
                t.interrupted as i64,
            ],
        )?;
        Ok(())
    }

    pub fn get_trial(&self, id: &str) -> Result<Option<TrialRow>> {
        Ok(self
            .conn
            .query_row(
                &format!("SELECT {TRIAL_COLUMNS} FROM trials WHERE id = ?1"),
                [id],
                trial_from_row,
            )
            .optional()?)
    }

    pub fn trials_for_session(&self, session_id: &str) -> Result<Vec<TrialRow>> {
        let mut stmt = self.conn.prepare(&format!(
            "SELECT {TRIAL_COLUMNS} FROM trials WHERE session_id = ?1 ORDER BY block_idx, trial_idx, attempt"
        ))?;
        let rows = stmt.query_map([session_id], trial_from_row)?;
        Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
    }

    /// Trials matching `f`, newest first.
    pub fn query_trials(&self, f: &TrialFilter, limit: u32, offset: u32) -> Result<Vec<TrialRow>> {
        let mut sql = format!("SELECT {TRIAL_COLUMNS} FROM trials WHERE 1 = 1");
        let mut args: Vec<Box<dyn rusqlite::ToSql>> = Vec::new();
        fn push(
            sql: &mut String,
            args: &mut Vec<Box<dyn rusqlite::ToSql>>,
            clause: &str,
            v: Box<dyn rusqlite::ToSql>,
        ) {
            args.push(v);
            sql.push_str(&clause.replace('?', &format!("?{}", args.len())));
        }
        if let Some(s) = &f.session_id {
            push(
                &mut sql,
                &mut args,
                " AND session_id = ?",
                Box::new(s.clone()),
            );
        }
        if !f.types.is_empty() {
            let mut parts = Vec::new();
            for t in &f.types {
                args.push(Box::new(t.clone()));
                parts.push(format!("?{}", args.len()));
            }
            sql.push_str(&format!(" AND type IN ({})", parts.join(", ")));
        }
        if let Some(s) = &f.since {
            push(
                &mut sql,
                &mut args,
                " AND started_at >= ?",
                Box::new(s.clone()),
            );
        }
        if let Some(s) = &f.until {
            push(
                &mut sql,
                &mut args,
                " AND started_at <= ?",
                Box::new(s.clone()),
            );
        }
        if let Some(s) = &f.protocol {
            push(
                &mut sql,
                &mut args,
                " AND protocol = ?",
                Box::new(s.clone()),
            );
        }
        if let Some(s) = &f.pool {
            push(&mut sql, &mut args, " AND pool = ?", Box::new(s.clone()));
        }
        if let Some(b) = f.assisted {
            push(&mut sql, &mut args, " AND assisted = ?", Box::new(b as i64));
        }
        if let Some(b) = f.assessment {
            push(
                &mut sql,
                &mut args,
                " AND assessment = ?",
                Box::new(b as i64),
            );
        }
        if let Some(s) = &f.feedback_mode {
            push(
                &mut sql,
                &mut args,
                " AND feedback_mode = ?",
                Box::new(s.clone()),
            );
        }
        if let Some(b) = f.correct {
            push(&mut sql, &mut args, " AND correct = ?", Box::new(b as i64));
        }
        if let Some(tag) = &f.tag {
            // Exact tag or a dotted child of it, inside the JSON array text.
            args.push(Box::new(format!("%\"{tag}\"%")));
            let exact = args.len();
            args.push(Box::new(format!("%\"{tag}.%")));
            let child = args.len();
            sql.push_str(&format!(
                " AND (tags_json LIKE ?{exact} OR tags_json LIKE ?{child})"
            ));
        }
        if !f.include_interrupted {
            sql.push_str(" AND interrupted = 0");
        }
        args.push(Box::new(limit as i64));
        let l = args.len();
        args.push(Box::new(offset as i64));
        let o = args.len();
        sql.push_str(&format!(
            " ORDER BY started_at DESC, trial_idx DESC LIMIT ?{l} OFFSET ?{o}"
        ));
        let mut stmt = self.conn.prepare(&sql)?;
        let refs: Vec<&dyn rusqlite::ToSql> = args.iter().map(|b| b.as_ref()).collect();
        let rows = stmt.query_map(refs.as_slice(), trial_from_row)?;
        Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
    }

    pub fn count_trials(&self) -> Result<i64> {
        Ok(self
            .conn
            .query_row("SELECT COUNT(*) FROM trials", [], |r| r.get(0))?)
    }

    /// When the app last played a tone whose name the user was told.
    pub fn last_labeled_reference_at(&self) -> Result<Option<DateTime<Utc>>> {
        let s: Option<String> = self
            .conn
            .query_row(
                "SELECT MAX(started_at) FROM trials WHERE labeled_reference = 1",
                [],
                |r| r.get(0),
            )
            .optional()?
            .flatten();
        Ok(s.and_then(|s| {
            DateTime::parse_from_rfc3339(&s)
                .ok()
                .map(|d| d.with_timezone(&Utc))
        }))
    }

    // --- traces and recordings -----------------------------------------------------------

    pub fn insert_trace(&self, trial_id: &str, track: &PitchTrack) -> Result<()> {
        let t0 = track.frames.first().map(|f| f.t).unwrap_or(0.0);
        self.conn.execute(
            "INSERT OR REPLACE INTO traces(trial_id, hop_s, t0_s, midi, clarity, level) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![
                trial_id,
                track.hop_s,
                t0,
                f32s_to_blob(track.frames.iter().map(|f| f.midi.unwrap_or(f32::NAN))),
                f32s_to_blob(track.frames.iter().map(|f| f.clarity)),
                f32s_to_blob(track.frames.iter().map(|f| f.rms_db)),
            ],
        )?;
        Ok(())
    }

    pub fn get_trace(&self, trial_id: &str) -> Result<Option<PitchTrack>> {
        let row = self
            .conn
            .query_row(
                "SELECT hop_s, t0_s, midi, clarity, level FROM traces WHERE trial_id = ?1",
                [trial_id],
                |r| {
                    Ok((
                        r.get::<_, f64>(0)?,
                        r.get::<_, f64>(1)?,
                        r.get::<_, Vec<u8>>(2)?,
                        r.get::<_, Vec<u8>>(3)?,
                        r.get::<_, Vec<u8>>(4)?,
                    ))
                },
            )
            .optional()?;
        Ok(row.map(|(hop, t0, midi, clarity, level)| {
            let midi = blob_to_f32s(&midi);
            let clarity = blob_to_f32s(&clarity);
            let level = blob_to_f32s(&level);
            let hop = hop as f32;
            PitchTrack {
                hop_s: hop,
                frames: midi
                    .iter()
                    .enumerate()
                    .map(|(i, m)| PitchFrame {
                        t: t0 as f32 + i as f32 * hop,
                        midi: if m.is_nan() { None } else { Some(*m) },
                        clarity: clarity.get(i).copied().unwrap_or(0.0),
                        rms_db: level.get(i).copied().unwrap_or(-120.0),
                    })
                    .collect(),
            }
        }))
    }

    pub fn insert_recording(
        &self,
        trial_id: &str,
        path: &Path,
        sample_rate: u32,
        frames: u64,
        pinned: bool,
    ) -> Result<()> {
        self.conn.execute(
            "INSERT OR REPLACE INTO recordings(trial_id, path, sample_rate, frames, pinned, created_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![trial_id, path.to_string_lossy(), sample_rate, frames as i64, pinned as i64, now().to_rfc3339()],
        )?;
        Ok(())
    }

    pub fn recording_path(&self, trial_id: &str) -> Result<Option<PathBuf>> {
        let p: Option<String> = self
            .conn
            .query_row(
                "SELECT path FROM recordings WHERE trial_id = ?1",
                [trial_id],
                |r| r.get(0),
            )
            .optional()?;
        Ok(p.map(PathBuf::from))
    }

    /// Deletes unpinned recordings older than `days`; returns the files to remove.
    pub fn expire_recordings(&self, days: i64) -> Result<Vec<PathBuf>> {
        let cutoff = (now() - chrono::Duration::days(days)).to_rfc3339();
        let mut stmt = self
            .conn
            .prepare("SELECT path FROM recordings WHERE pinned = 0 AND created_at < ?1")?;
        let paths: Vec<PathBuf> = stmt
            .query_map([&cutoff], |r| r.get::<_, String>(0))?
            .filter_map(|r| r.ok())
            .map(PathBuf::from)
            .collect();
        self.conn.execute(
            "DELETE FROM recordings WHERE pinned = 0 AND created_at < ?1",
            [&cutoff],
        )?;
        Ok(paths)
    }

    pub fn insert_regrade(
        &self,
        trial_id: &str,
        grading: &serde_json::Value,
        outcome: &crate::grade::TrialOutcome,
    ) -> Result<String> {
        let id = new_id();
        self.conn.execute(
            "INSERT INTO regrades(id, trial_id, grading_json, metrics_json, correct, score, created_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            params![
                id,
                trial_id,
                serde_json::to_string(grading)?,
                serde_json::to_string(&outcome.metrics)?,
                outcome.correct.map(|c| c as i64),
                outcome.score.map(|s| s as f64),
                now().to_rfc3339()
            ],
        )?;
        Ok(id)
    }

    // --- coach notes, comfort, range ------------------------------------------------------

    pub fn add_coach_note(&self, text: &str, tags: &[String]) -> Result<CoachNote> {
        let note = CoachNote {
            id: new_id(),
            created_at: now().to_rfc3339(),
            tags: tags.to_vec(),
            text: text.to_string(),
        };
        self.conn.execute(
            "INSERT INTO coach_notes(id, created_at, tags_json, text) VALUES (?1, ?2, ?3, ?4)",
            params![
                note.id,
                note.created_at,
                serde_json::to_string(&note.tags)?,
                note.text
            ],
        )?;
        Ok(note)
    }

    // --- anchor songs ---------------------------------------------------------------------------

    pub fn anchors(&self) -> Result<Vec<Anchor>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, title, first_note, melody_json, created_at, source FROM anchors ORDER BY title COLLATE NOCASE",
        )?;
        let rows = stmt.query_map([], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, String>(2)?,
                r.get::<_, Option<String>>(3)?,
                r.get::<_, String>(4)?,
                r.get::<_, String>(5)?,
            ))
        })?;
        let mut out = Vec::new();
        for row in rows {
            let (id, title, first, melody, created_at, source) = row?;
            let Ok(first_note) = first.parse() else {
                continue;
            };
            out.push(Anchor {
                id,
                title,
                first_note,
                melody: melody.and_then(|m| serde_json::from_str(&m).ok()),
                created_at,
                source,
            });
        }
        Ok(out)
    }

    /// Adds an anchor, or replaces the one with the same id (or, without an id, the same title).
    pub fn save_anchor(&self, a: &Anchor, source: &str) -> Result<Anchor> {
        if a.title.trim().is_empty() {
            return Err(anyhow!("an anchor needs a title"));
        }
        if let Some(m) = &a.melody {
            crate::score::Score::from_compact(m, 90.0).map_err(|e| anyhow!("melody: {e}"))?;
        }
        let existing = self.anchors()?.into_iter().find(|x| {
            (!a.id.is_empty() && x.id == a.id)
                || (a.id.is_empty() && x.title.eq_ignore_ascii_case(a.title.trim()))
        });
        let saved = Anchor {
            id: existing.as_ref().map(|x| x.id.clone()).unwrap_or_else(|| {
                if a.id.is_empty() {
                    new_id()
                } else {
                    a.id.clone()
                }
            }),
            title: a.title.trim().to_string(),
            first_note: a.first_note,
            melody: a.melody.clone(),
            created_at: existing
                .map(|x| x.created_at)
                .unwrap_or_else(|| now().to_rfc3339()),
            source: source.to_string(),
        };
        self.conn.execute(
            "INSERT INTO anchors(id, title, first_note, melody_json, created_at, source) VALUES (?1, ?2, ?3, ?4, ?5, ?6)
             ON CONFLICT(id) DO UPDATE SET title = excluded.title, first_note = excluded.first_note,
               melody_json = excluded.melody_json, source = excluded.source",
            params![
                saved.id,
                saved.title,
                saved.first_note.name(),
                saved.melody.as_ref().map(serde_json::to_string).transpose()?,
                saved.created_at,
                saved.source
            ],
        )?;
        Ok(saved)
    }

    pub fn remove_anchor(&self, id: &str) -> Result<bool> {
        Ok(self
            .conn
            .execute("DELETE FROM anchors WHERE id = ?1", params![id])?
            > 0)
    }

    /// Everything trial generation needs from the database: range, stimulus pools, anchors and
    /// the melody library.
    pub fn gen_context(&self) -> Result<GenContext> {
        Ok(GenContext {
            comfortable: self.range_profile()?.comfortable(),
            pools: self.get_setting("stimulus_pools")?.unwrap_or_default(),
            anchors: self.anchors()?,
            library: self.library()?,
            labels: crate::labels::sets_from_db(self)?,
            soundfonts: None,
        })
    }

    // --- melody library -------------------------------------------------------------------------

    /// Every library piece, in natural order of id.
    pub fn library(&self) -> Result<Vec<LibraryPiece>> {
        let mut stmt = self.conn.prepare("SELECT piece_json FROM library_pieces")?;
        let rows = stmt.query_map([], |r| r.get::<_, String>(0))?;
        let mut out: Vec<LibraryPiece> = Vec::new();
        for row in rows {
            if let Ok(p) = serde_json::from_str(&row?) {
                out.push(p);
            }
        }
        out.sort_by(|a, b| library::natural_cmp(&a.id, &b.id));
        Ok(out)
    }

    pub fn library_piece(&self, id: &str) -> Result<Option<LibraryPiece>> {
        let json: Option<String> = self
            .conn
            .query_row(
                "SELECT piece_json FROM library_pieces WHERE id = ?1",
                params![id],
                |r| r.get(0),
            )
            .optional()?;
        Ok(json.and_then(|j| serde_json::from_str(&j).ok()))
    }

    /// Every file of the library folder that was read, with its number of pieces or its error.
    pub fn library_files(&self) -> Result<Vec<LibraryFile>> {
        let mut stmt = self
            .conn
            .prepare("SELECT file, pieces, error, read_at FROM library_files")?;
        let rows = stmt.query_map([], |r| {
            Ok(LibraryFile {
                file: r.get(0)?,
                pieces: r.get::<_, i64>(1)?.max(0) as u32,
                error: r.get(2)?,
                read_at: r.get(3)?,
            })
        })?;
        let mut out = rows.collect::<rusqlite::Result<Vec<_>>>()?;
        out.sort_by(|a, b| library::natural_cmp(&a.file, &b.file));
        Ok(out)
    }

    /// Brings the index up to date with the library folder: reads new and changed files, drops
    /// the pieces of deleted ones, and retries files that failed when anything else changed (an
    /// id clash may have gone).
    pub fn sync_library(&self, dir: &Path) -> Result<LibrarySync> {
        let on_disk = library::scan(dir);
        let mut known: HashMap<String, (String, bool)> = HashMap::new();
        {
            let mut stmt = self
                .conn
                .prepare("SELECT file, stamp, error IS NOT NULL FROM library_files")?;
            let rows = stmt.query_map([], |r| {
                Ok((
                    r.get::<_, String>(0)?,
                    (r.get::<_, String>(1)?, r.get::<_, bool>(2)?),
                ))
            })?;
            for row in rows {
                let (file, v) = row?;
                known.insert(file, v);
            }
        }
        let mut sync = LibrarySync::default();
        for file in known.keys() {
            if !on_disk.iter().any(|(f, _)| f == file) {
                self.conn
                    .execute("DELETE FROM library_pieces WHERE file = ?1", params![file])?;
                self.conn
                    .execute("DELETE FROM library_files WHERE file = ?1", params![file])?;
                sync.removed.push(file.clone());
            }
        }
        let changed: Vec<&(String, String)> = on_disk
            .iter()
            .filter(|(f, stamp)| known.get(f).is_none_or(|(s, _)| s != stamp))
            .collect();
        let retry = !changed.is_empty() || !sync.removed.is_empty();
        for (file, stamp) in &on_disk {
            let failed = known.get(file).is_some_and(|(_, failed)| *failed);
            if changed.iter().any(|(f, _)| f == file) || (retry && failed) {
                self.read_library_file(dir, file, stamp, &mut sync)?;
            }
        }
        Ok(sync)
    }

    fn read_library_file(
        &self,
        dir: &Path,
        file: &str,
        stamp: &str,
        sync: &mut LibrarySync,
    ) -> Result<()> {
        let result = std::fs::read(dir.join(file))
            .map_err(|e| e.to_string())
            .and_then(|bytes| library::read_file(file, &bytes))
            .and_then(|pieces| {
                for p in &pieces {
                    let other: Option<String> = self
                        .conn
                        .query_row(
                            "SELECT file FROM library_pieces WHERE id = ?1 AND file != ?2",
                            params![p.id, file],
                            |r| r.get(0),
                        )
                        .optional()
                        .map_err(|e| e.to_string())?;
                    if let Some(other) = other {
                        return Err(format!("the id `{}` is already used in {other}", p.id));
                    }
                }
                Ok(pieces)
            });
        let tx = self.conn.unchecked_transaction()?;
        tx.execute("DELETE FROM library_pieces WHERE file = ?1", params![file])?;
        if let Ok(pieces) = &result {
            for (k, p) in pieces.iter().enumerate() {
                tx.execute(
                    "INSERT OR REPLACE INTO library_pieces(id, file, position, piece_json) VALUES (?1, ?2, ?3, ?4)",
                    params![p.id, file, k as i64, serde_json::to_string(p)?],
                )?;
            }
        }
        tx.execute(
            "INSERT OR REPLACE INTO library_files(file, stamp, pieces, error, read_at) VALUES (?1, ?2, ?3, ?4, ?5)",
            params![
                file,
                stamp,
                result.as_ref().map(|p| p.len() as i64).unwrap_or(0),
                result.as_ref().err(),
                now().to_rfc3339()
            ],
        )?;
        tx.commit()?;
        match result {
            Ok(_) => sync.read.push(file.to_string()),
            Err(e) => sync.errors.push((file.to_string(), e)),
        }
        Ok(())
    }

    pub fn coach_notes(&self, tag: Option<&str>, limit: u32) -> Result<Vec<CoachNote>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, created_at, tags_json, text FROM coach_notes
             WHERE (?1 IS NULL OR tags_json LIKE '%\"' || ?1 || '%') ORDER BY created_at DESC LIMIT ?2",
        )?;
        let rows = stmt.query_map(params![tag, limit], |r| {
            let tags: String = r.get(2)?;
            Ok(CoachNote {
                id: r.get(0)?,
                created_at: r.get(1)?,
                tags: serde_json::from_str(&tags).unwrap_or_default(),
                text: r.get(3)?,
            })
        })?;
        Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
    }

    pub fn add_comfort_report(
        &self,
        session_id: Option<&str>,
        phase: &str,
        level: &str,
        note: Option<&str>,
    ) -> Result<()> {
        self.conn.execute(
            "INSERT INTO comfort_reports(id, session_id, phase, level, note, created_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![new_id(), session_id, phase, level, note, now().to_rfc3339()],
        )?;
        Ok(())
    }

    pub fn comfort_reports(&self, limit: u32) -> Result<Vec<ComfortReport>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, session_id, phase, level, note, created_at FROM comfort_reports ORDER BY created_at DESC LIMIT ?1",
        )?;
        let rows = stmt.query_map([limit], |r| {
            Ok(ComfortReport {
                id: r.get(0)?,
                session_id: r.get(1)?,
                phase: r.get(2)?,
                level: r.get(3)?,
                note: r.get(4)?,
                created_at: r.get(5)?,
            })
        })?;
        Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
    }

    pub fn range_profile(&self) -> Result<RangeProfile> {
        let row = self
            .conn
            .query_row(
                "SELECT comfortable_low, comfortable_high, lowest_reachable, highest_reachable, voice_type, updated_at, source
                 FROM range_profile WHERE id = 1",
                [],
                |r| {
                    Ok(RangeProfile {
                        comfortable_low: Note(r.get(0)?),
                        comfortable_high: Note(r.get(1)?),
                        lowest_reachable: r.get::<_, Option<i32>>(2)?.map(Note),
                        highest_reachable: r.get::<_, Option<i32>>(3)?.map(Note),
                        voice_type: r.get(4)?,
                        updated_at: r.get(5)?,
                        source: r.get(6)?,
                    })
                },
            )
            .optional()?;
        Ok(row.unwrap_or_else(RangeProfile::default_profile))
    }

    pub fn set_range_profile(&self, p: &RangeProfile, source: &str) -> Result<()> {
        if p.comfortable_low.0 >= p.comfortable_high.0 {
            return Err(anyhow!("comfortable_low must be below comfortable_high"));
        }
        self.conn.execute(
            "INSERT INTO range_profile(id, comfortable_low, comfortable_high, lowest_reachable, highest_reachable, voice_type, updated_at, source)
             VALUES (1, ?1, ?2, ?3, ?4, ?5, ?6, ?7)
             ON CONFLICT(id) DO UPDATE SET comfortable_low = excluded.comfortable_low, comfortable_high = excluded.comfortable_high,
               lowest_reachable = excluded.lowest_reachable, highest_reachable = excluded.highest_reachable,
               voice_type = excluded.voice_type, updated_at = excluded.updated_at, source = excluded.source",
            params![
                p.comfortable_low.0,
                p.comfortable_high.0,
                p.lowest_reachable.map(|n| n.0),
                p.highest_reachable.map(|n| n.0),
                p.voice_type,
                now().to_rfc3339(),
                source
            ],
        )?;
        Ok(())
    }

    // --- calibration -------------------------------------------------------------------------

    pub fn save_calibration(&self, c: &CalibrationRow) -> Result<()> {
        self.conn.execute(
            "INSERT INTO calibrations(id, device_key, residual_frames, sample_rate, buffer_frames, method, spread_ms, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
            params![c.id, c.device_key, c.residual_frames, c.sample_rate, c.buffer_frames, c.method, c.spread_ms, c.created_at],
        )?;
        Ok(())
    }

    pub fn latest_calibration(&self, device_key: &str) -> Result<Option<CalibrationRow>> {
        Ok(self
            .conn
            .query_row(
                "SELECT id, device_key, residual_frames, sample_rate, buffer_frames, method, spread_ms, created_at
                 FROM calibrations WHERE device_key = ?1 ORDER BY created_at DESC LIMIT 1",
                [device_key],
                |r| {
                    Ok(CalibrationRow {
                        id: r.get(0)?,
                        device_key: r.get(1)?,
                        residual_frames: r.get(2)?,
                        sample_rate: r.get(3)?,
                        buffer_frames: r.get(4)?,
                        method: r.get(5)?,
                        spread_ms: r.get(6)?,
                        created_at: r.get(7)?,
                    })
                },
            )
            .optional()?)
    }

    // --- courses -------------------------------------------------------------------------------

    /// A course's current level (1-based) and how it got there (`passed` or `manual`).
    pub fn course_level(&self, course: &str) -> Result<Option<(u32, String)>> {
        Ok(self
            .course_state(course)?
            .map(|(level, source, _)| (level, source)))
    }

    /// Level, how it was set, and when that happened.
    pub fn course_state(&self, course: &str) -> Result<Option<(u32, String, String)>> {
        Ok(self
            .conn
            .query_row(
                "SELECT level, source, updated_at FROM course_state WHERE course = ?1",
                [course],
                |r| {
                    Ok((
                        r.get::<_, u32>(0)?,
                        r.get::<_, String>(1)?,
                        r.get::<_, String>(2)?,
                    ))
                },
            )
            .optional()?)
    }

    pub fn set_course_level(&self, course: &str, level: u32, source: &str) -> Result<()> {
        self.conn.execute(
            "INSERT INTO course_state(course, level, updated_at, source) VALUES (?1, ?2, ?3, ?4)
             ON CONFLICT(course) DO UPDATE SET level = excluded.level, updated_at = excluded.updated_at, source = excluded.source",
            params![course, level, now().to_rfc3339(), source],
        )?;
        Ok(())
    }

    /// Records that `level` was passed (the first time only).
    pub fn record_course_pass(
        &self,
        course: &str,
        level: u32,
        source: &str,
        evidence: &serde_json::Value,
    ) -> Result<()> {
        self.conn.execute(
            "INSERT OR IGNORE INTO course_passes(course, level, passed_at, source, evidence_json) VALUES (?1, ?2, ?3, ?4, ?5)",
            params![course, level, now().to_rfc3339(), source, serde_json::to_string(evidence)?],
        )?;
        Ok(())
    }

    /// The levels of a course passed so far: (level, passed_at).
    pub fn course_passes(&self, course: &str) -> Result<Vec<(u32, String)>> {
        let mut stmt = self.conn.prepare(
            "SELECT level, passed_at FROM course_passes WHERE course = ?1 ORDER BY level",
        )?;
        let rows = stmt.query_map([course], |r| {
            Ok((r.get::<_, u32>(0)?, r.get::<_, String>(1)?))
        })?;
        Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
    }

    // --- presets -------------------------------------------------------------------------------

    pub fn save_preset(&self, name: &str, spec: &SessionSpec, source: &str) -> Result<()> {
        self.conn.execute(
            "INSERT INTO presets(name, spec_json, source, created_at) VALUES (?1, ?2, ?3, ?4)
             ON CONFLICT(name) DO UPDATE SET spec_json = excluded.spec_json, source = excluded.source, created_at = excluded.created_at",
            params![name, serde_json::to_string(spec)?, source, now().to_rfc3339()],
        )?;
        Ok(())
    }

    pub fn user_presets(&self) -> Result<Vec<(String, SessionSpec, String)>> {
        let mut stmt = self
            .conn
            .prepare("SELECT name, spec_json, source FROM presets ORDER BY name")?;
        let rows = stmt.query_map([], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, String>(2)?,
            ))
        })?;
        let mut out = Vec::new();
        for r in rows {
            let (name, json, source) = r?;
            if let Ok(spec) = SessionSpec::from_json(&json) {
                out.push((name, spec, source));
            }
        }
        Ok(out)
    }
}

#[cfg(test)]
pub(crate) mod tests_support {
    use super::*;
    use serde_json::json;

    pub fn sample_trial(session_id: &str, idx: u32, correct: bool) -> TrialRow {
        TrialRow {
            id: new_id(),
            session_id: session_id.into(),
            block_idx: 0,
            trial_idx: idx,
            attempt: 0,
            kind: "pitch_id".into(),
            protocol: Some("p1".into()),
            tags: vec!["ap.id".into()],
            stimulus: json!({"pitch_class": if idx % 2 == 0 { "C" } else { "E" }, "timbre": "gm:0"}),
            spec: json!({}),
            pool: Some("training".into()),
            feedback_mode: "after_trial".into(),
            response: json!({"answer": "C"}),
            metrics: json!({"expected": "C"}),
            summary: String::new(),
            correct: Some(correct),
            score: Some(if correct { 1.0 } else { 0.0 }),
            rt_ms: Some(800.0 + idx as f64),
            assisted: false,
            assessment: false,
            labeled_reference: true,
            started_at: now().to_rfc3339(),
            start_frame: Some(1000),
            device_profile: None,
            calibrated: false,
            secs_since_labeled_reference: None,
            interrupted: false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::tests_support::sample_trial;
    use super::*;
    use serde_json::json;

    #[test]
    fn data_stays_in_the_app_directory() {
        let built = Path::new(r"F:\Documents\music app\SingingApp\target\release\singing-trainer.exe");
        assert_eq!(
            data_dir_for_exe(built),
            PathBuf::from(r"F:\Documents\music app\SingingApp\data")
        );
        let tests = Path::new(r"C:\proj\SingingApp\target\debug\deps\trainer_core.exe");
        assert_eq!(data_dir_for_exe(tests), PathBuf::from(r"C:\proj\SingingApp\data"));
        let installed = Path::new(r"D:\Apps\Singing Trainer\singing-trainer.exe");
        assert_eq!(
            data_dir_for_exe(installed),
            PathBuf::from(r"D:\Apps\Singing Trainer\data")
        );
    }

    fn spec() -> SessionSpec {
        SessionSpec::from_json(
            r#"{"title": "Test", "protocol": "p1", "blocks": [{"type": "pitch_id", "count": 2}]}"#,
        )
        .unwrap()
    }

    #[test]
    fn session_lifecycle_and_trials() {
        let db = Db::open_in_memory().unwrap();
        let id = db
            .queue_session(&spec(), "llm", Some("focus on E"))
            .unwrap();
        assert_eq!(db.queued_sessions().unwrap().len(), 1);
        db.mark_session_running(&id).unwrap();
        assert!(db.queued_sessions().unwrap().is_empty());
        for i in 0..4 {
            db.insert_trial(&sample_trial(&id, i, i != 1)).unwrap();
        }
        let trials = db.trials_for_session(&id).unwrap();
        assert_eq!(trials.len(), 4);
        assert_eq!(trials[1].correct, Some(false));
        db.finish_session(&id, SessionStatus::Completed, &json!({"accuracy": 0.75}))
            .unwrap();
        let s = db.get_session(&id).unwrap().unwrap();
        assert_eq!(s.status, SessionStatus::Completed);
        assert_eq!(s.spec().unwrap().title, "Test");
        assert!(db.last_labeled_reference_at().unwrap().is_some());

        let f = TrialFilter {
            types: vec!["pitch_id".into()],
            correct: Some(true),
            tag: Some("ap".into()),
            ..Default::default()
        };
        // `ap` matches its dotted children such as `ap.id`.
        assert_eq!(db.query_trials(&f, 100, 0).unwrap().len(), 3);
        let f2 = TrialFilter {
            tag: Some("ap.id".into()),
            ..Default::default()
        };
        assert_eq!(db.query_trials(&f2, 100, 0).unwrap().len(), 4);
        let f3 = TrialFilter {
            tag: Some("rp".into()),
            ..Default::default()
        };
        assert_eq!(db.query_trials(&f3, 100, 0).unwrap().len(), 0);
    }

    #[test]
    fn traces_round_trip() {
        let db = Db::open_in_memory().unwrap();
        let id = db.start_new_session(&spec(), "preset").unwrap();
        let t = sample_trial(&id, 0, true);
        db.insert_trial(&t).unwrap();
        let track = PitchTrack {
            hop_s: 0.01,
            frames: vec![
                PitchFrame {
                    t: 0.5,
                    midi: None,
                    clarity: 0.1,
                    rms_db: -70.0,
                },
                PitchFrame {
                    t: 0.51,
                    midi: Some(60.25),
                    clarity: 0.97,
                    rms_db: -20.0,
                },
            ],
        };
        db.insert_trace(&t.id, &track).unwrap();
        let back = db.get_trace(&t.id).unwrap().unwrap();
        assert_eq!(back.frames.len(), 2);
        assert_eq!(back.frames[0].midi, None);
        assert_eq!(back.frames[1].midi, Some(60.25));
        assert!((back.frames[1].t - 0.51).abs() < 1e-6);
    }

    #[test]
    fn settings_range_and_notes() {
        let db = Db::open_in_memory().unwrap();
        db.set_setting("a4_hz", &442.0).unwrap();
        assert_eq!(db.get_setting::<f64>("a4_hz").unwrap(), Some(442.0));
        assert_eq!(
            db.range_profile().unwrap().source.as_deref(),
            Some("default")
        );
        let mut p = RangeProfile::default_profile();
        p.comfortable_low = Note(45);
        p.comfortable_high = Note(64);
        db.set_range_profile(&p, "manual").unwrap();
        assert_eq!(db.range_profile().unwrap().comfortable_low, Note(45));
        assert!(
            db.set_range_profile(
                &RangeProfile {
                    comfortable_low: Note(70),
                    ..p
                },
                "manual"
            )
            .is_err()
        );
        db.add_coach_note("Descending m6 is weak", &["rp.interval".into()])
            .unwrap();
        assert_eq!(db.coach_notes(None, 10).unwrap().len(), 1);
        assert_eq!(db.coach_notes(Some("rp"), 10).unwrap().len(), 1);
        assert_eq!(db.coach_notes(Some("ap"), 10).unwrap().len(), 0);
    }

    #[test]
    fn file_database_sees_other_connection_writes() {
        let dir = tempfile::tempdir().unwrap();
        let path = db_path(dir.path());
        let a = Db::open(&path).unwrap();
        let b = Db::open(&path).unwrap();
        let v0 = a.data_version().unwrap();
        b.queue_session(&spec(), "llm", None).unwrap();
        assert_ne!(a.data_version().unwrap(), v0);
        assert_eq!(a.queued_sessions().unwrap().len(), 1);
    }

    #[test]
    fn library_follows_the_folder() {
        let dir = tempfile::tempdir().unwrap();
        let lib = dir.path().join("library");
        let db = Db::open_in_memory().unwrap();
        assert!(!db.sync_library(&lib).unwrap().changed());

        library::write_file(
            &lib,
            "Book/one.json",
            br#"{"key": "C", "notes": "C4:2 E4:2 | G4:4"}"#,
        )
        .unwrap();
        library::write_file(
            &lib,
            "two.json",
            br#"[{"key": "G", "notes": "G4:4"}, {"key": "F", "notes": "F4:4"}]"#,
        )
        .unwrap();
        let sync = db.sync_library(&lib).unwrap();
        assert_eq!(sync.read.len(), 2);
        let ids: Vec<String> = db.library().unwrap().into_iter().map(|p| p.id).collect();
        assert_eq!(ids, ["Book/one", "two#1", "two#2"]);
        assert_eq!(db.gen_context().unwrap().library.len(), 3);
        // Nothing changed: nothing is read again.
        assert!(!db.sync_library(&lib).unwrap().changed());

        // A clashing id is reported and retried once the other file is gone.
        library::write_file(
            &lib,
            "clash.json",
            br#"{"id": "Book/one", "key": "D", "notes": "D4:4"}"#,
        )
        .unwrap();
        let sync = db.sync_library(&lib).unwrap();
        assert!(
            sync.errors[0].1.contains("already used in Book/one.json"),
            "{:?}",
            sync.errors
        );
        assert!(
            db.library_files()
                .unwrap()
                .iter()
                .any(|f| f.file == "clash.json" && f.error.is_some())
        );
        library::remove_file(&lib, "Book/one.json").unwrap();
        let sync = db.sync_library(&lib).unwrap();
        assert_eq!(sync.removed, ["Book/one.json"]);
        assert!(sync.errors.is_empty());
        assert_eq!(
            db.library_piece("Book/one").unwrap().unwrap().key,
            "D major"
        );

        // Hidden temporary files are ignored.
        std::fs::write(lib.join(".x.json.tmp"), "{").unwrap();
        assert!(!db.sync_library(&lib).unwrap().changed());
    }

    #[test]
    fn anchors_round_trip_and_reach_the_generation_context() {
        let db = Db::open_in_memory().unwrap();
        assert!(db.anchors().unwrap().is_empty());
        let a: Anchor = serde_json::from_value(serde_json::json!({
            "title": "Happy Birthday",
            "first_note": "G3",
            "melody": {"key": "C major", "time": "3/4", "notes": [{"note": "G3", "beats": 0.75}, {"note": "G3", "beats": 0.25}, {"note": "A3", "beats": 1}]}
        }))
        .unwrap();
        let saved = db.save_anchor(&a, "manual").unwrap();
        assert!(!saved.id.is_empty());
        // Saving the same title again updates instead of duplicating.
        let mut b = a.clone();
        b.first_note = "A3".parse().unwrap();
        let again = db.save_anchor(&b, "llm").unwrap();
        assert_eq!(again.id, saved.id);
        let all = db.anchors().unwrap();
        assert_eq!(all.len(), 1);
        assert_eq!(all[0].first_note.name(), "A3");
        assert!(all[0].melody.is_some());
        assert_eq!(db.gen_context().unwrap().anchors.len(), 1);
        assert!(db.remove_anchor(&saved.id).unwrap());
        assert!(db.anchors().unwrap().is_empty());
        let bad = Anchor {
            title: " ".into(),
            ..a
        };
        assert!(db.save_anchor(&bad, "manual").is_err());
    }
}
