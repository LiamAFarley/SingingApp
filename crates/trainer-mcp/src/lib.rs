//! MCP server over the trainer's database: results, statistics, the exercise catalogue and the
//! session queue the desktop app picks up.

use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use rmcp::handler::server::router::tool::ToolRouter;
use rmcp::handler::server::wrapper::Parameters;
use rmcp::model::{CallToolResult, ContentBlock, Implementation, ServerCapabilities, ServerConfig};
use rmcp::{ErrorData, ServerHandler, schemars, tool, tool_handler, tool_router};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use trainer_core::courses::{self, ApPrefs, DailyPlan};
use trainer_core::generate::GenContext;
use trainer_core::labels::LabelSet;
use trainer_core::library;
use trainer_core::presets::{builtin_presets, find_builtin};
use trainer_core::reports;
use trainer_core::stats::{StatsQuery, confusion, get_stats};
use trainer_core::storage::{Db, RangeProfile, SessionStatus, TrialFilter};
use trainer_core::timbre::{GM_PROGRAM_NAMES, PoolConfig, SYNTH_WAVES, Timbre};
use trainer_core::validate::validate_value;

pub const INSTRUCTIONS: &str = "\
You are coaching a singer who trains singing, ear training and absolute pitch in a Windows desktop app. \
This server exposes the app's results and lets you queue sessions the app runs.\n\
\n\
How to work:\n\
- Start with get_overview. Use get_stats (group_by any stimulus/metrics field) and get_confusion_matrix to find weak spots; \
use get_session_report and get_trial_detail for specifics. Save your plan and observations with save_coach_note so the next \
conversation can continue from them.\n\
- Build sessions from the exercise types in list_exercise_types and queue them with queue_session (use dry_run first). \
The user starts queued sessions from \"Up next\" in the app.\n\
\n\
Rules:\n\
- The app grades every attempt deterministically. Use its metrics; never re-judge pitch or rhythm yourself.\n\
- Holdout stimuli (pool \"holdout\") are for assessment only. Tests run first in the day, before any named note, with no \
feedback. Absolute pitch is measured only by the four test courses below; docs/AP_TESTING.md says what each measures, how \
often to run it and how to read it. Do not build your own absolute pitch assessments.\n\
- Fade feedback as accuracy stabilises: continuous -> after_trial -> none. Test transfer (new octaves, timbres, keys) before \
calling something learned, and retest after gaps for retention.\n\
- pitch_pairing, incidental_pairing and other exposure are not evidence of absolute pitch.\n\
- Absolute pitch is organised in courses (list_courses, get_ap_dashboard): ap_naming (a growing set of notes over four \
octaves with Other for notes outside the set, a check without feedback per level, the last level needing four checks at 90% \
with the last one at least 12 h after the third), ap_brady (all twelve notes, C weighted 40% fading to even, paced streams from \
5 s to 1.4 s apart, starting by singing C from memory), ap_speed (pitch_detect streams, d′ on the last streams), \
ap_note_colors (Taneda colours, errorless at first), ap_chord_colors (Eguchi's 14 chord colours; the next chord is due only \
after 100% and 14 days), ap_contrast (E♭ against F♯, then the black keys, answered with Gervain names or the user's own set), \
the tests ap_benchmark (12 notes sung from memory, then 60 piano notes; every 2 weeks), ap_weekly (120 notes, five \
timbres, on the test day), ap_transfer (60 notes on untrained instruments and 24 Shepard tones; every 4 weeks) and \
ap_ucsf_style (36 piano, 36 sine; every 4 weeks), all with 5 s per note and no replay, and ap_maintenance (twice a week once \
naming is complete). ap_screening is the aptitude screening (implicit note memory, tone N-back at 2 and 3 back, span; \
no feedback, about 9 minutes; due at once, then every 4 weeks); read it with get_aptitude_profile and use it to weight the \
daily set (lower capacity: more chord colours, Brady and errorless colour work; near ceiling: the multi-octave naming \
ladder). It informs the plan and gates nothing. ap_production trains singing named notes from memory (classes grow as in \
ap_naming; each level's check comes first, before any note is heard, then practice with hold and compare); it reports \
cents error and settling time. sing_pitch can be asked for by label (prompt label, color or spoken, with labels) and \
hold_and_compare fades the target in under the voice after the graded window. The absolute-pitch daily set starts with \
singing C from memory (internal anchor accuracy, a trend on the dashboard) on days no test is due. Tests come up in \
\"Today's set\" when due; to run one now, queue its preset (get_preset with the test's \
id). Read them with get_ap_profile. The app moves a course to its next level when the level's checks pass, and the user's \
\"Today's set\" is composed from the active courses in the daily plan (preview_daily_set, set_daily_plan). New courses start \
switched off. Use set_course_level only to correct a level. The dashboard pools every method per note: mastered means at least \
90% correct within 2 s over the recent trials. Before any unassisted check, clear pitch memory with a block pre_mask \
\"shepard_glissando\" and keep consecutive tones apart: min_jump_semitones 13 and, over four or more octaves, \
min_octave_step 2 (each tone two octaves from the last, so its octave gives nothing away). Masks between trials (mask, \
pre_mask) are off by default: the jump rule and an answer limit do that job without the wait. Do not put sounds between \
trials; the user finds them useless. feedback \"errorless\" stores \
the true result but shows no error mark; it is the default on early colour, chord and contrast levels unless the user turns \
\"Errorless early levels\" off (set_ap_prefs). Label sets (list_label_sets, set_label_set) are note_names, taneda_colors, \
gervain_names, my_colors and eguchi_chords. Trials always store the pitch class, so the set does not change mastery. \
Every absolute-pitch block that shows results also shows each note's Taneda colour (tinted keys, and the note's colour \
with the result); blocks without feedback show none. Do not \
treat a block without feedback as passed if the tones were within an octave of each other or a named note was heard in the \
previous few minutes without the clearing sound.\n\
- Relative pitch starts from rp_core, the daily core (on in the plan by default): twelve short mixed sessions in four \
tiers, built on the aural-skills research in docs/RELATIVE_PITCH.md: tonal context (find home: sing or name the resting \
tone of a phrase that stops; scale degrees; same/different and probe memory), \
tonal patterns and working memory (Gordon pattern echo with teaching and evaluation passes, the I–V dialogue, musical \
N-back and canon, additive and complementary interval singing), inner hearing and lines (silent bars, flash scores, \
melodic updating, bass and guide-tone singing), structure and error detection (single-pass score-against-sound, \
mislabeled syllables, cadences, singback). Each session passes when every checked part is at 80% in one go. Prefer the \
core for daily work and the single-exercise workshops for targeted practice where the dashboard shows a weakness: \
rp_home, rp_patterns, rp_memory, rp_inner, rp_singback, rp_harmony, rp_errors, rp_walk, rp_match, rp_degrees, rp_intervals_key, rp_sing, \
rp_intervals, rp_compare, rp_chords, rp_inversions, rp_scales, rp_dictation, rp_sight. A course's levels are lessons \
grouped into modules (get_workshop shows them with scores); one run at 90% passes a lesson (N-back recognition: d′ 2). \
The category has its own \"Today's set\" (preview_daily_set with category \"Relative pitch\"); set_daily_plan and \
set_course_level work as for absolute pitch. To give the user a drill the lessons lack, save it as a custom level of a \
single-exercise activity (save_custom_level). New exercises: resting_tone, tonal_pattern, pattern_dialogue, n_back, \
melodic_updating, pattern_memory, error_detection, syllable_check, cadence_id, bass_line; sight_sing's mode (read, \
singback, flash) and silent (bars, strong_beats, weak_beats); sing_interval's scaffold (additive, complementary); \
scale_degree_id's think_ms (an optional pause before answering, off by default) and adaptive (degrees missed lately come up more). See list_exercise_types.\n\
- Other graded courses are built-in presets: harmony_01–08 (degrees_01–08 is the older scale-degree ladder; prefer \
rp_degrees). Suggest the next level after at least 90% over two sessions.\n\
- Sight singing (sight_sing) and dictation (melodic_dictation) use staff notation; melodies are generated, written by you \
({\"given\": [{key, time, notes: [{note, beats}]}]}), or taken from the melody library ({\"library\": {collection | ids | tags}}). \
Transcribe the user's books into the library with add_library_pieces (or MIDI files with import_library_files) and reuse them. \
Harmony singing (harmony) sings one voice against the others: fixed intervals (source.parallel: a third, fourth, fifth or \
sixth from a melody; tritones become perfect fourths) or free part-writing you compose (source.given, or library pieces): \
two-voice or SATB, sung exactly as written with any intervals, tritones included; play_parts picks the accompanying voices. \
practice: true plays the singer's part along (stored as assisted); grading {\"tuning\": \"just\"} scores against pure \
intervals. Presets harmony_01–08 are a course, one interval each. \
Anchor songs need anchors the user confirmed (list_anchors, save_anchor).\n\
- Singing diagnostics: before advising on sung accuracy, read docs/SINGING_DIAGNOSTICS.md. Every single sung note stores \
attack_cents (first reliable pitch), centre_cents (settled), correction_cents, settling_ms, stability_cents and \
error_semitones; block grading {\"grade_on\": \"attack\"} grades the first attempt, block self_judge asks flat, in tune \
or sharp before the result, and pitch_match delay_ms and distractor_tones hold a note through silence or other notes. \
rp_match is the pitch-matching workshop (first attempt, hold the note, through other notes, judge it first, long notes \
with and without a drone); rp_walk is the silent pitch walk (imagery with no voice). Keep each ability's measure \
separate rather than one score, and test one hypothesis at a time.\n\
- Respect comfort reports: mild fatigue -> fewer sung blocks; discomfort -> no range or intensity work; pain or new hoarseness \
-> no demanding vocal work, and suggest professional evaluation if it persists. Never diagnose; a correct pitch is not evidence \
of healthy technique.\n\
- Keep sung material inside the comfortable range (get_range_profile) unless deliberately extending it.";

#[derive(Clone)]
pub struct TrainerServer {
    db: Arc<Mutex<Db>>,
    data_dir: PathBuf,
    tool_router: ToolRouter<Self>,
}

fn text(v: &impl Serialize) -> Result<CallToolResult, ErrorData> {
    let s = serde_json::to_string_pretty(v)
        .map_err(|e| ErrorData::internal_error(e.to_string(), None))?;
    Ok(CallToolResult::success(vec![ContentBlock::text(s)]))
}

fn tool_error(msg: impl Into<String>) -> Result<CallToolResult, ErrorData> {
    Ok(CallToolResult::error(vec![ContentBlock::text(msg.into())]))
}

fn session_schema(_: &mut schemars::SchemaGenerator) -> schemars::Schema {
    let v = reports::session_schema();
    schemars::Schema::try_from(v).unwrap_or_else(|_| schemars::json_schema!({"type": "object"}))
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
struct DailySetRequest {
    /// `Absolute pitch` (default) or `Relative pitch`.
    #[serde(default)]
    category: Option<String>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct SessionIdRequest {
    pub session_id: String,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct TrialIdRequest {
    pub trial_id: String,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct ListSessionsRequest {
    /// queued, running, completed or abandoned.
    #[serde(default)]
    pub status: Option<String>,
    #[serde(default)]
    pub limit: Option<u32>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct QueryTrialsRequest {
    #[serde(default)]
    pub filter: TrialFilter,
    /// Default 50, at most 500.
    #[serde(default)]
    pub limit: Option<u32>,
    #[serde(default)]
    pub offset: Option<u32>,
    /// Include full spec/metrics JSON (default false returns a compact row).
    #[serde(default)]
    pub full: bool,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct ConfusionRequest {
    /// Usually `{"types": ["pitch_id"]}`; any trial filter.
    #[serde(default)]
    pub filter: TrialFilter,
    #[serde(default)]
    pub max_trials: Option<u32>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct QueueSessionRequest {
    /// The session to queue: `{"title": "...", "blocks": [{"type": "...", "count": n, "params": {...}}]}`.
    #[schemars(schema_with = "session_schema")]
    pub session: Value,
    /// Shown to the user in "Up next".
    #[serde(default)]
    pub note: Option<String>,
    /// Only validate and preview; nothing is queued.
    #[serde(default)]
    pub dry_run: bool,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct SavePresetRequest {
    pub name: String,
    /// A session spec, same shape as in queue_session.
    pub session: Value,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct PresetIdRequest {
    /// A built-in preset id from list_presets, or a saved preset name.
    pub id: String,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct CourseLevelRequest {
    /// A course id from list_courses, e.g. `ap_naming`.
    pub course: String,
    /// 1-based. For set_course_level, one past the last level marks the course complete.
    pub level: u32,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct WorkshopRequest {
    /// A relative-pitch course id, e.g. `rp_degrees`.
    pub course: String,
    /// Include each lesson's exercise params (large).
    #[serde(default)]
    pub include_params: bool,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct CourseRequest {
    /// A relative-pitch course id, e.g. `rp_intervals`.
    pub course: String,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct DeleteCustomLevelRequest {
    pub id: String,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct SaveAnchorRequest {
    pub anchor: trainer_core::schema::Anchor,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct RemoveAnchorRequest {
    pub id: String,
}

#[derive(Debug, Default, Deserialize, schemars::JsonSchema)]
pub struct ListLibraryRequest {
    /// Only this collection (a folder of the library) and its sub-collections.
    #[serde(default)]
    pub collection: Option<String>,
    /// Only pieces with this tag.
    #[serde(default)]
    pub tag: Option<String>,
    /// Only pieces whose id or title contains this text.
    #[serde(default)]
    pub query: Option<String>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct LibraryPieceRequest {
    pub id: String,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct AddLibraryPiecesRequest {
    /// Pieces in the library JSON format (see the tool description).
    pub pieces: Vec<Value>,
    /// Folder of the library to write to, e.g. "Ottman/Chapter 3". It is the pieces' collection.
    #[serde(default)]
    pub collection: Option<String>,
    /// File name (".json" is added); defaults to the first piece's title. Writing the same file
    /// again replaces it.
    #[serde(default)]
    pub file_name: Option<String>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct ImportLibraryFilesRequest {
    /// Absolute paths of .json, .mid or .midi files.
    pub paths: Vec<String>,
    /// Folder of the library to copy them to.
    #[serde(default)]
    pub collection: Option<String>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct LibraryFileRequest {
    /// Path inside the library folder, as list_library shows it.
    pub file: String,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct CoachNoteRequest {
    pub text: String,
    #[serde(default)]
    pub tags: Vec<String>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct CoachNotesRequest {
    #[serde(default)]
    pub tag: Option<String>,
    #[serde(default)]
    pub limit: Option<u32>,
}

impl TrainerServer {
    pub fn new(db: Db, data_dir: PathBuf) -> Self {
        TrainerServer {
            db: Arc::new(Mutex::new(db)),
            data_dir,
            tool_router: Self::tool_router(),
        }
    }

    fn with_db<T>(&self, f: impl FnOnce(&Db) -> anyhow::Result<T>) -> Result<T, String> {
        let db = self
            .db
            .lock()
            .map_err(|_| "database lock poisoned".to_string())?;
        f(&db).map_err(|e| format!("{e:#}"))
    }

    fn gen_context(&self) -> Result<GenContext, String> {
        let dir = self.data_dir.join("soundfonts");
        let installed = trainer_core::timbre::installed_soundfonts(&[dir.as_path()]);
        self.with_db(|db| {
            db.sync_library(&self.library_dir())?;
            let mut ctx = db.gen_context()?;
            ctx.soundfonts = Some(installed);
            ctx.speech = Some(trainer_core::speech::present(
                &trainer_core::speech::speech_dir(&self.data_dir),
            ));
            Ok(ctx)
        })
    }

    fn library_dir(&self) -> PathBuf {
        library::folder(&self.data_dir)
    }

    /// Reads the library folder again and reports what a newly written file produced.
    fn sync_file(&self, file: &str) -> Result<Value, String> {
        let sync = self.with_db(|db| db.sync_library(&self.library_dir()))?;
        if let Some((_, e)) = sync.errors.iter().find(|(f, _)| f == file) {
            return Err(format!("{file}: {e}"));
        }
        let ids = self.with_db(|db| {
            Ok(db
                .library()?
                .into_iter()
                .filter(|p| p.file == file)
                .map(|p| p.id)
                .collect::<Vec<_>>())
        })?;
        Ok(json!({ "file": file, "ids": ids }))
    }
}

fn compact_trial(t: &trainer_core::storage::TrialRow) -> Value {
    json!({
        "id": t.id,
        "session_id": t.session_id,
        "block": t.block_idx,
        "trial": t.trial_idx,
        "attempt": t.attempt,
        "type": t.kind,
        "started_at": t.started_at,
        "stimulus": t.stimulus,
        "answer": t.response.get("answer"),
        "correct": t.correct,
        "score": t.score,
        "rt_ms": t.rt_ms,
        "cents_error": t.metrics.get("cents_error"),
        "summary": t.summary,
        "feedback_mode": t.feedback_mode,
        "assisted": t.assisted,
        "assessment": t.assessment,
        "tags": t.tags,
    })
}

#[tool_router]
impl TrainerServer {
    #[tool(
        description = "Start here. Recent practice by exercise type (7 and 30 days), recent sessions, queued sessions, range profile, comfort reports and your latest coach notes."
    )]
    async fn get_overview(&self) -> Result<CallToolResult, ErrorData> {
        match self.with_db(|db| reports::overview(db, Some(&self.data_dir))) {
            Ok(v) => text(&v),
            Err(e) => tool_error(e),
        }
    }

    #[tool(
        description = "List sessions, newest first. Filter by status (queued, running, completed, abandoned)."
    )]
    async fn list_sessions(
        &self,
        Parameters(req): Parameters<ListSessionsRequest>,
    ) -> Result<CallToolResult, ErrorData> {
        let status = req.status.as_deref().map(SessionStatus::parse);
        let r = self.with_db(|db| {
            Ok(db
                .list_sessions(status, req.limit.unwrap_or(20).min(200))?
                .into_iter()
                .map(|s| {
                    json!({"id": s.id, "title": s.title, "status": s.status, "source": s.source, "protocol": s.protocol,
                       "queued_at": s.queued_at, "started_at": s.started_at, "completed_at": s.completed_at,
                       "note": s.queue_note, "summary": s.summary})
                })
                .collect::<Vec<_>>())
        });
        match r {
            Ok(v) => text(&v),
            Err(e) => tool_error(e),
        }
    }

    #[tool(
        description = "Per-block results of one session (accuracy, response time, cents error, pass/fail) and a one-line summary of every trial."
    )]
    async fn get_session_report(
        &self,
        Parameters(req): Parameters<SessionIdRequest>,
    ) -> Result<CallToolResult, ErrorData> {
        match self.with_db(|db| reports::session_report(db, &req.session_id)) {
            Ok(v) => text(&v),
            Err(e) => tool_error(e),
        }
    }

    #[tool(
        description = "Raw trials matching a filter, newest first. Compact rows by default; set full=true for complete metrics."
    )]
    async fn query_trials(
        &self,
        Parameters(req): Parameters<QueryTrialsRequest>,
    ) -> Result<CallToolResult, ErrorData> {
        let limit = req.limit.unwrap_or(50).min(500);
        let r = self.with_db(|db| db.query_trials(&req.filter, limit, req.offset.unwrap_or(0)));
        match r {
            Ok(rows) if req.full => text(&rows),
            Ok(rows) => text(&rows.iter().map(compact_trial).collect::<Vec<_>>()),
            Err(e) => tool_error(e),
        }
    }

    #[tool(
        description = "Aggregate statistics (trials, accuracy, median response time, median |cents|, signed bias) grouped by any fields, e.g. group_by [\"stimulus.pitch_class\"], [\"stimulus.interval\", \"stimulus.direction\"], [\"stimulus.timbre\"], [\"week\"], [\"feedback_mode\"]. This is the main tool for finding weaknesses."
    )]
    async fn get_stats(
        &self,
        Parameters(req): Parameters<StatsQuery>,
    ) -> Result<CallToolResult, ErrorData> {
        match self.with_db(|db| get_stats(db, &req)) {
            Ok(v) => text(&v),
            Err(e) => tool_error(e),
        }
    }

    #[tool(
        description = "Confusion matrix (actual vs answered) for identification trials, with the most frequent confusions. Filter e.g. {\"types\": [\"pitch_id\"]} or {\"types\": [\"interval_id\"]}."
    )]
    async fn get_confusion_matrix(
        &self,
        Parameters(req): Parameters<ConfusionRequest>,
    ) -> Result<CallToolResult, ErrorData> {
        let r = self.with_db(|db| {
            db.query_trials(&req.filter, req.max_trials.unwrap_or(2000).min(20_000), 0)
        });
        match r {
            Ok(rows) => text(&confusion(&rows)),
            Err(e) => tool_error(e),
        }
    }

    #[tool(
        description = "Everything about one trial: the exact stimulus, the response, all metrics (per-note results for sung trials) and the sung pitch trace at 20 Hz."
    )]
    async fn get_trial_detail(
        &self,
        Parameters(req): Parameters<TrialIdRequest>,
    ) -> Result<CallToolResult, ErrorData> {
        match self.with_db(|db| reports::trial_detail(db, &req.trial_id)) {
            Ok(v) => text(&v),
            Err(e) => tool_error(e),
        }
    }

    #[tool(
        description = "The absolute pitch tests (see docs/AP_TESTING.md): for the benchmark, weekly, transfer and UCSF-style tests, the latest run and the one before, whether the change is beyond run-to-run noise, the 30-day trend and every run's headline figures. Each run has accuracy with its 95% range, within-2-s accuracy, timeouts, UCSF-style points, mean absolute error, mean signed error, median correct response time, per-timbre and Shepard accuracy, sung-from-memory accuracy with a chaining check, how long after a named note it ran, and plain-language cautions. The benchmark adds its confusion matrix and signed error per note and per octave."
    )]
    async fn get_ap_profile(&self) -> Result<CallToolResult, ErrorData> {
        match self.with_db(reports::ap_profile) {
            Ok(v) => text(&v),
            Err(e) => tool_error(e),
        }
    }

    #[tool(
        description = "The aptitude screening (course ap_screening, Van Hedger's battery) over time: implicit note memory (mean absolute error in steps and cents, signed error, per run), tone or letter N-back d′ at 2 and 3 back per run, auditory span (longest length recalled) per run with the mode that ran (spoken digits or the tone pad), each with its run date and the research reference lines; the latest figures; and capacity_band (near_ceiling, mixed or lower against INM ≤ 27.5 cents and 3-back d′ ≥ 3.5), which only restates the raw numbers. Route from the raw numbers; notes say when a result is weak evidence (tone-pad span, letters missing, unmoved searches)."
    )]
    async fn get_aptitude_profile(&self) -> Result<CallToolResult, ErrorData> {
        match self.with_db(trainer_core::aptitude::aptitude_profile) {
            Ok(v) => text(&v),
            Err(e) => tool_error(e),
        }
    }

    #[tool(
        description = "Absolute pitch progress in one place: per note, naming accuracy within 2 s over the recent trials from every method (mastered at 90%+) and singing accuracy; every course with its current level, pass-rule progress (e.g. 3 of 4 checks, the next one counting from a given time) and what is due; benchmark, weekly, transfer and UCSF-style test runs (accuracy with its 95% range, within 2 s, UCSF-style points, mean absolute deviation, per timbre, cautions); singing C from memory over time; minutes this week; the daily plan."
    )]
    async fn get_ap_dashboard(&self) -> Result<CallToolResult, ErrorData> {
        match self.with_db(|db| courses::ap_dashboard(db, chrono::Utc::now())) {
            Ok(v) => text(&v),
            Err(e) => tool_error(e),
        }
    }

    #[tool(
        description = "The built-in courses: each level's title, summary and pass rule, the schedule of the recurring tests, and each course's current status."
    )]
    async fn list_courses(&self) -> Result<CallToolResult, ErrorData> {
        let statuses = match self.with_db(|db| courses::course_statuses(db, chrono::Utc::now())) {
            Ok(v) => v,
            Err(e) => return tool_error(e),
        };
        let out: Vec<Value> = courses::builtin_courses()
            .into_iter()
            .map(|c| {
                json!({
                    "id": c.id, "title": c.title, "description": c.description, "schedule": c.schedule,
                    "levels": c.levels.iter().enumerate().map(|(i, l)| json!({
                        "level": i + 1, "title": l.title, "summary": l.summary, "pass": l.pass,
                    })).collect::<Vec<_>>(),
                    "status": statuses.iter().find(|s| s.id == c.id),
                })
            })
            .collect();
        text(&out)
    }

    #[tool(
        description = "The session spec of one course level (its blocks carry the course tag), e.g. to adapt it or queue it with queue_session."
    )]
    async fn get_course_level(
        &self,
        Parameters(req): Parameters<CourseLevelRequest>,
    ) -> Result<CallToolResult, ErrorData> {
        let Some(c) = courses::find_course(&req.course) else {
            return tool_error(format!("unknown course `{}`", req.course));
        };
        match self.with_db(|db| {
            let mut spec = courses::level_session(&c, req.level)?;
            courses::apply_session_prefs(&mut spec, &courses::ap_prefs(db)?);
            Ok(spec)
        }) {
            Ok(spec) => text(&spec),
            Err(e) => tool_error(e),
        }
    }

    #[tool(
        description = "Today's set for a category (`Absolute pitch`, the default, or `Relative pitch`) as the app would compose it now: tests that are due first (absolute pitch), then each active course of that category at its current level fitted to its minutes, with the reason for each item and the full session spec."
    )]
    async fn preview_daily_set(
        &self,
        Parameters(req): Parameters<DailySetRequest>,
    ) -> Result<CallToolResult, ErrorData> {
        let category = req
            .category
            .unwrap_or_else(|| courses::ABSOLUTE_PITCH.to_string());
        match self.with_db(|db| {
            let ctx = db.gen_context()?;
            courses::compose_daily_set(db, &ctx, chrono::Utc::now(), &category)
        }) {
            Ok(v) => text(&v),
            Err(e) => tool_error(e),
        }
    }

    #[tool(
        description = "Set the daily plan: which ladder courses go into the daily set (active), minutes per course (1–90), and test_day (0 = Monday … 6 = Sunday) for the weekly and UCSF-style tests."
    )]
    async fn set_daily_plan(
        &self,
        Parameters(plan): Parameters<DailyPlan>,
    ) -> Result<CallToolResult, ErrorData> {
        match self.with_db(|db| {
            courses::save_daily_plan(db, &plan)?;
            courses::daily_plan(db)
        }) {
            Ok(v) => text(&v),
            Err(e) => tool_error(e),
        }
    }

    #[tool(
        description = "Set a course's current level by hand (correcting it; the app normally moves on when a level's checks pass)."
    )]
    async fn set_course_level(
        &self,
        Parameters(req): Parameters<CourseLevelRequest>,
    ) -> Result<CallToolResult, ErrorData> {
        match self.with_db(|db| {
            courses::set_level(db, &req.course, req.level)?;
            courses::course_statuses(db, chrono::Utc::now())
        }) {
            Ok(v) => text(&v.into_iter().find(|s| s.id == req.course)),
            Err(e) => tool_error(e),
        }
    }

    #[tool(
        description = "A workshop course's lessons by module, with each lesson's runs, best and last score and whether it passed, and the lesson the course is on."
    )]
    async fn get_workshop(
        &self,
        Parameters(req): Parameters<WorkshopRequest>,
    ) -> Result<CallToolResult, ErrorData> {
        match self.with_db(|db| {
            courses::refresh_progress(db)?;
            trainer_core::workshops::workshop_view(db, &req.course)
        }) {
            Ok(mut v) => {
                if !req.include_params {
                    for m in &mut v.modules {
                        for l in &mut m.lessons {
                            l.params = serde_json::Value::Null;
                        }
                    }
                }
                text(&v)
            }
            Err(e) => tool_error(e),
        }
    }

    #[tool(description = "The user's custom levels of a workshop course, with their scores.")]
    async fn list_custom_levels(
        &self,
        Parameters(req): Parameters<CourseRequest>,
    ) -> Result<CallToolResult, ErrorData> {
        match self.with_db(|db| trainer_core::workshops::custom_levels(db, &req.course)) {
            Ok(v) => text(&v),
            Err(e) => tool_error(e),
        }
    }

    #[tool(
        description = "Save a custom level for a workshop course: a title, a question count and the params of the course's exercise (scale_degree_id for rp_degrees, interval_id for rp_intervals …). Leave id empty for a new level; give an existing id to replace it. It is checked by generating it, and appears on the activity's Custom levels tab."
    )]
    async fn save_custom_level(
        &self,
        Parameters(level): Parameters<trainer_core::workshops::CustomLevel>,
    ) -> Result<CallToolResult, ErrorData> {
        let ctx = match self.gen_context() {
            Ok(c) => c,
            Err(e) => return tool_error(e),
        };
        match self.with_db(|db| trainer_core::workshops::save_custom_level(db, &ctx, &level)) {
            Ok(v) => text(&v),
            Err(e) => tool_error(e),
        }
    }

    #[tool(description = "Delete a custom level by id.")]
    async fn delete_custom_level(
        &self,
        Parameters(req): Parameters<DeleteCustomLevelRequest>,
    ) -> Result<CallToolResult, ErrorData> {
        match self.with_db(|db| trainer_core::workshops::delete_custom_level(db, &req.id)) {
            Ok(()) => text(&"deleted"),
            Err(e) => tool_error(e),
        }
    }

    #[tool(
        description = "The label and colour sets (note names, Taneda colours, Gervain names, the user's own colours, Eguchi chord colours), including any edits."
    )]
    async fn list_label_sets(&self) -> Result<CallToolResult, ErrorData> {
        match self.with_db(|db| trainer_core::labels::sets_from_db(db)) {
            Ok(v) => text(&v),
            Err(e) => tool_error(e),
        }
    }

    #[tool(
        description = "Replace one label set. id is note_names, taneda_colors, gervain_names, my_colors, eguchi_chords, or a new id of letters, numbers and underscores. kind is pitch_class or chord. entries map a pitch class (C, C#, …) or a chord id (red, yellow, …) to {name?, color?, word?, speech?}. color is a CSS colour such as #d64545. speech is a path inside the speech folder, such as labels/gervain_names/Sarah.wav."
    )]
    async fn set_label_set(
        &self,
        Parameters(set): Parameters<LabelSet>,
    ) -> Result<CallToolResult, ErrorData> {
        match self.with_db(|db| trainer_core::labels::save_set(db, &set)) {
            Ok(v) => text(&v),
            Err(e) => tool_error(e),
        }
    }

    #[tool(
        description = "Read or set whether early colour, chord and contrast levels use errorless feedback, and which label set the contrast course uses (contrast_labels, default gervain_names)."
    )]
    async fn set_ap_prefs(
        &self,
        Parameters(prefs): Parameters<ApPrefs>,
    ) -> Result<CallToolResult, ErrorData> {
        match self.with_db(|db| courses::save_ap_prefs(db, &prefs)) {
            Ok(v) => text(&v),
            Err(e) => tool_error(e),
        }
    }

    #[tool(
        description = "The speech folder: which spoken digits, letters, note names and label words are present, which are missing, and which speaking features are therefore off. Naming, detection and chords still run without them."
    )]
    async fn speech_folder(&self) -> Result<CallToolResult, ErrorData> {
        let dir = trainer_core::speech::speech_dir(&self.data_dir);
        text(&trainer_core::speech::inspect(&dir))
    }

    #[tool(
        description = "Every exercise type with its description, parameter JSON schema, default tags and an example block."
    )]
    async fn list_exercise_types(&self) -> Result<CallToolResult, ErrorData> {
        text(&reports::exercise_catalogue())
    }

    #[tool(
        description = "Available timbres: General MIDI programs (gm:0–127), built-in synth waves, shepard, and extra SoundFont banks, each marked training or holdout."
    )]
    async fn list_timbres(&self) -> Result<CallToolResult, ErrorData> {
        let pools: PoolConfig = self
            .with_db(|db| Ok(db.get_setting("stimulus_pools")?.unwrap_or_default()))
            .unwrap_or_default();
        let mut out = Vec::new();
        for (i, name) in GM_PROGRAM_NAMES.iter().enumerate() {
            let id = format!("gm:{i}");
            out.push(json!({"id": id, "name": name, "pool": if pools.is_holdout_timbre(&id) { "holdout" } else { "training" }}));
        }
        for w in SYNTH_WAVES {
            let id = format!("synth:{w}");
            let name = Timbre::parse(&id)
                .map(|t| t.display_name())
                .unwrap_or_default();
            out.push(json!({"id": id, "name": name, "pool": if pools.is_holdout_timbre(&id) { "holdout" } else { "training" }}));
        }
        out.push(json!({"id": "shepard", "name": "Shepard tone (pitch class only)", "pool": if pools.is_holdout_timbre("shepard") { "holdout" } else { "training" }}));
        let banks_dir = self.data_dir.join("soundfonts");
        let installed = trainer_core::timbre::installed_soundfonts(&[banks_dir.as_path()]);
        for p in &trainer_core::timbre::SOUNDFONT_PRESETS {
            if trainer_core::timbre::soundfont_installed(p.id, Some(installed.as_slice())) {
                out.push(json!({
                    "id": p.id, "name": p.name,
                    "pool": if pools.is_holdout_timbre(p.id) { "holdout" } else { "training" },
                    "usable": [trainer_core::theory::Note(p.low).name(), trainer_core::theory::Note(p.high).name()],
                }));
            }
        }
        for stem in &installed {
            let known = trainer_core::timbre::SOUNDFONT_PRESETS
                .iter()
                .any(|p| p.id.split(':').nth(1) == Some(stem.as_str()));
            if !known {
                out.push(json!({"id": format!("sf:{stem}:<program>"), "name": format!("SoundFont bank {stem} (not measured; held to C3–B5)"), "pool": "training"}));
            }
        }
        text(&json!({"timbres": out, "holdout_octaves": pools.holdout_octaves}))
    }

    #[tool(description = "Built-in presets (id, title, category, description) and saved presets.")]
    async fn list_presets(&self) -> Result<CallToolResult, ErrorData> {
        let mut out: Vec<Value> = builtin_presets()
            .into_iter()
            .map(|p| json!({"id": p.id, "title": p.title, "category": p.category, "description": p.description, "source": "builtin"}))
            .collect();
        if let Ok(saved) = self.with_db(|db| db.user_presets()) {
            for (name, spec, source) in saved {
                out.push(json!({"id": name, "title": spec.title, "category": "Saved", "description": spec.notes, "source": source}));
            }
        }
        text(&out)
    }

    #[tool(
        description = "The full session spec of a preset (built-in id or saved name), e.g. to adapt it."
    )]
    async fn get_preset(
        &self,
        Parameters(req): Parameters<PresetIdRequest>,
    ) -> Result<CallToolResult, ErrorData> {
        if let Some(spec) = find_builtin(&req.id) {
            return text(&spec);
        }
        match self.with_db(|db| db.user_presets()) {
            Ok(saved) => match saved.into_iter().find(|(n, _, _)| *n == req.id) {
                Some((_, spec, _)) => text(&spec),
                None => tool_error(format!("no preset `{}`", req.id)),
            },
            Err(e) => tool_error(e),
        }
    }

    #[tool(
        description = "Validate a session and add it to the app's \"Up next\" queue. With dry_run=true only validates: returns errors (with JSON paths), warnings, trial count, estimated minutes and a preview of the first trials of each block. Blocks: {\"type\": <exercise type>, \"count\": n, \"feedback\": continuous|after_trial|after_block|end_of_session|none, \"params\": {...}, \"grading\": {...}, \"tags\": [...], \"pass_mark\": 0.9, \"title\": \"...\", \"assessment\": false}."
    )]
    async fn queue_session(
        &self,
        Parameters(req): Parameters<QueueSessionRequest>,
    ) -> Result<CallToolResult, ErrorData> {
        let ctx = match self.gen_context() {
            Ok(c) => c,
            Err(e) => return tool_error(e),
        };
        let (spec, report) = validate_value(req.session, &ctx);
        if req.dry_run || !report.ok {
            return text(&json!({"queued": false, "validation": report}));
        }
        let spec = spec.expect("valid report has a spec");
        match self.with_db(|db| db.queue_session(&spec, "llm", req.note.as_deref())) {
            Ok(id) => text(&json!({"queued": true, "session_id": id, "validation": report})),
            Err(e) => tool_error(e),
        }
    }

    #[tool(description = "Remove a session from the queue (only while it is still queued).")]
    async fn remove_queued_session(
        &self,
        Parameters(req): Parameters<SessionIdRequest>,
    ) -> Result<CallToolResult, ErrorData> {
        match self.with_db(|db| db.delete_queued_session(&req.session_id)) {
            Ok(true) => text(&json!({"removed": true})),
            Ok(false) => tool_error("not found or no longer queued"),
            Err(e) => tool_error(e),
        }
    }

    #[tool(
        description = "Save a session spec as a reusable preset under a name (shows up in the app's library)."
    )]
    async fn save_preset(
        &self,
        Parameters(req): Parameters<SavePresetRequest>,
    ) -> Result<CallToolResult, ErrorData> {
        let ctx = match self.gen_context() {
            Ok(c) => c,
            Err(e) => return tool_error(e),
        };
        let (spec, report) = validate_value(req.session, &ctx);
        let Some(spec) = spec.filter(|_| report.ok) else {
            return text(&json!({"saved": false, "validation": report}));
        };
        match self.with_db(|db| db.save_preset(&req.name, &spec, "llm")) {
            Ok(()) => text(&json!({"saved": true, "name": req.name})),
            Err(e) => tool_error(e),
        }
    }

    #[tool(
        description = "The singer's range profile: comfortable range (used by generators) and reachable extremes."
    )]
    async fn get_range_profile(&self) -> Result<CallToolResult, ErrorData> {
        match self.with_db(|db| db.range_profile()) {
            Ok(v) => text(&v),
            Err(e) => tool_error(e),
        }
    }

    #[tool(
        description = "Update the range profile (notes like \"A2\", \"E4\"). Only change it on evidence from repeated comfortable singing, never from a single extreme note."
    )]
    async fn set_range_profile(
        &self,
        Parameters(req): Parameters<RangeProfile>,
    ) -> Result<CallToolResult, ErrorData> {
        match self.with_db(|db| {
            db.set_range_profile(&req, "llm")?;
            db.range_profile()
        }) {
            Ok(v) => text(&v),
            Err(e) => tool_error(e),
        }
    }

    #[tool(
        description = "Anchor songs the user saved: songs they know in the original key, with the first sung note and optionally the opening phrase (used by the anchor_song exercise)."
    )]
    async fn list_anchors(&self) -> Result<CallToolResult, ErrorData> {
        match self.with_db(|db| db.anchors()) {
            Ok(v) => text(&v),
            Err(e) => tool_error(e),
        }
    }

    #[tool(
        description = "Save an anchor song (or update one with the same id or title): title, first_note as recorded in the original key (e.g. \"G4\"), and optionally melody = the opening phrase in the original key as {key, time, notes: [{note, beats} | {rest}]} for key_check trials. Only save what the user confirms; never guess a recording's key."
    )]
    async fn save_anchor(
        &self,
        Parameters(req): Parameters<SaveAnchorRequest>,
    ) -> Result<CallToolResult, ErrorData> {
        match self.with_db(|db| db.save_anchor(&req.anchor, "llm")) {
            Ok(a) => text(&a),
            Err(e) => tool_error(e),
        }
    }

    #[tool(description = "Remove an anchor song by id.")]
    async fn remove_anchor(
        &self,
        Parameters(req): Parameters<RemoveAnchorRequest>,
    ) -> Result<CallToolResult, ErrorData> {
        match self.with_db(|db| db.remove_anchor(&req.id)) {
            Ok(removed) => text(&json!({ "removed": removed })),
            Err(e) => tool_error(e),
        }
    }

    #[tool(
        description = "The melody library: pieces read from JSON and MIDI files in the library folder (a subfolder is a collection). Returns the folder, one summary per piece (id, title, collection, key, time, bars, parts, range) and files that could not be read. Use pieces in sight_sing and melodic_dictation with params.melody = {\"library\": {\"collection\": …} | {\"ids\": […]} | {\"tags\": […]}, \"shuffle\"?, \"part\"?}."
    )]
    async fn list_library(
        &self,
        Parameters(req): Parameters<ListLibraryRequest>,
    ) -> Result<CallToolResult, ErrorData> {
        let dir = self.library_dir();
        let r = self.with_db(|db| {
            db.sync_library(&dir)?;
            let q = req.query.as_deref().map(str::to_lowercase);
            let pieces: Vec<Value> = db
                .library()?
                .iter()
                .filter(|p| {
                    req.collection
                        .as_deref()
                        .is_none_or(|c| library::in_collection(p, c))
                })
                .filter(|p| {
                    req.tag
                        .as_deref()
                        .is_none_or(|t| p.tags.iter().any(|x| x.eq_ignore_ascii_case(t)))
                })
                .filter(|p| {
                    q.as_deref().is_none_or(|q| {
                        p.id.to_lowercase().contains(q) || p.title.to_lowercase().contains(q)
                    })
                })
                .map(|p| p.summary())
                .collect();
            let errors: Vec<Value> = db
                .library_files()?
                .into_iter()
                .filter_map(|f| f.error.map(|e| json!({ "file": f.file, "error": e })))
                .collect();
            Ok(json!({ "folder": dir, "pieces": pieces, "files_with_errors": errors }))
        });
        match r {
            Ok(v) => text(&v),
            Err(e) => tool_error(e),
        }
    }

    #[tool(
        description = "One library piece in full, in the library JSON format (notes as {note, beats} | {rest})."
    )]
    async fn get_library_piece(
        &self,
        Parameters(req): Parameters<LibraryPieceRequest>,
    ) -> Result<CallToolResult, ErrorData> {
        let dir = self.library_dir();
        match self.with_db(|db| {
            db.sync_library(&dir)?;
            db.library_piece(&req.id)
        }) {
            Ok(Some(p)) => text(&p),
            Ok(None) => tool_error(format!("no piece `{}`; see list_library", req.id)),
            Err(e) => tool_error(e),
        }
    }

    #[tool(
        description = "Add pieces to the melody library, e.g. melodies transcribed from the user's books. They are written as one JSON file in the library folder (under collection), which the app reads. \
A piece: {title, key (\"Eb major\", \"F# minor\"), time (\"3/4\", default 4/4), tempo_bpm?, pickup? (beats before the first barline), tags?, id?, notes} for one part, or parts: [{name, clef?, notes}, …] (top part first) instead of notes. \
notes is text, \"Eb4:1 F4:.5 G4 | Ab4:2~ | Ab4:1 R:1\": pitch:beats in the time signature's unit, a duration carries over to the next note, R is a rest, ~ ties to the next note, and every | is a barline that is checked (a short first bar is a pickup). A list [{\"note\": \"Eb4\", \"beats\": 1}, {\"rest\": 1}] also works. \
Pieces without an id get collection/file (or …#n). Writing the same file_name again replaces the file. Nothing is written if a piece is invalid."
    )]
    async fn add_library_pieces(
        &self,
        Parameters(req): Parameters<AddLibraryPiecesRequest>,
    ) -> Result<CallToolResult, ErrorData> {
        if req.pieces.is_empty() {
            return tool_error("pieces is empty");
        }
        let name = req
            .file_name
            .clone()
            .or_else(|| {
                req.pieces[0]
                    .get("title")
                    .and_then(Value::as_str)
                    .map(str::to_string)
            })
            .or_else(|| {
                req.pieces[0]
                    .get("id")
                    .and_then(Value::as_str)
                    .map(str::to_string)
            })
            .unwrap_or_else(|| "pieces".into());
        let name = if name.to_lowercase().ends_with(".json") {
            name
        } else {
            format!("{name}.json")
        };
        let rel = match library::file_path(req.collection.as_deref(), &name) {
            Ok(r) => r,
            Err(e) => return tool_error(e),
        };
        let doc = library::to_json(None, &req.pieces);
        let pieces = match library::read_file(&rel, doc.as_bytes()) {
            Ok(p) => p,
            Err(e) => return tool_error(e),
        };
        let clash = self.with_db(|db| {
            for p in &pieces {
                if let Some(other) = db.library_piece(&p.id)? {
                    if other.file != rel {
                        return Ok(Some(format!(
                            "the id `{}` is already used in {}",
                            p.id, other.file
                        )));
                    }
                }
            }
            Ok(None)
        });
        match clash {
            Ok(Some(e)) | Err(e) => return tool_error(e),
            Ok(None) => {}
        }
        if let Err(e) = library::write_file(&self.library_dir(), &rel, doc.as_bytes()) {
            return tool_error(e);
        }
        match self.sync_file(&rel) {
            Ok(v) => text(&v),
            Err(e) => tool_error(e),
        }
    }

    #[tool(
        description = "Copy JSON or MIDI files into the library folder (under collection) and read them. MIDI: one part per track, top part first; chords keep their top note; times are rounded to sixteenths; the key comes from the key signature or is estimated."
    )]
    async fn import_library_files(
        &self,
        Parameters(req): Parameters<ImportLibraryFilesRequest>,
    ) -> Result<CallToolResult, ErrorData> {
        let mut results = Vec::new();
        for path in &req.paths {
            let p = std::path::Path::new(path);
            let name = p.file_name().and_then(|n| n.to_str()).unwrap_or_default();
            let r = library::file_path(req.collection.as_deref(), name).and_then(|rel| {
                let bytes = std::fs::read(p).map_err(|e| format!("reading {path}: {e}"))?;
                library::read_file(&rel, &bytes)?;
                library::write_file(&self.library_dir(), &rel, &bytes)?;
                self.sync_file(&rel)
            });
            results.push(match r {
                Ok(v) => json!({ "path": path, "imported": v }),
                Err(e) => json!({ "path": path, "error": e }),
            });
        }
        text(&results)
    }

    #[tool(description = "Delete a file from the library folder, with all its pieces.")]
    async fn remove_library_file(
        &self,
        Parameters(req): Parameters<LibraryFileRequest>,
    ) -> Result<CallToolResult, ErrorData> {
        let dir = self.library_dir();
        let r = library::remove_file(&dir, &req.file)
            .and_then(|()| self.with_db(|db| db.sync_library(&dir)).map(|_| ()));
        match r {
            Ok(()) => text(&json!({ "removed": req.file })),
            Err(e) => tool_error(e),
        }
    }

    #[tool(
        description = "Persist a coaching note (plan, observation, next steps). Kept with the training data, so it survives across conversations and agents."
    )]
    async fn save_coach_note(
        &self,
        Parameters(req): Parameters<CoachNoteRequest>,
    ) -> Result<CallToolResult, ErrorData> {
        match self.with_db(|db| db.add_coach_note(&req.text, &req.tags)) {
            Ok(n) => text(&n),
            Err(e) => tool_error(e),
        }
    }

    #[tool(
        description = "Coaching notes saved earlier, newest first, optionally filtered by tag prefix."
    )]
    async fn get_coach_notes(
        &self,
        Parameters(req): Parameters<CoachNotesRequest>,
    ) -> Result<CallToolResult, ErrorData> {
        match self
            .with_db(|db| db.coach_notes(req.tag.as_deref(), req.limit.unwrap_or(20).min(200)))
        {
            Ok(v) => text(&v),
            Err(e) => tool_error(e),
        }
    }
}

#[tool_handler(router = self.tool_router)]
impl ServerHandler for TrainerServer {
    fn get_info(&self) -> ServerConfig {
        ServerConfig::new(ServerCapabilities::builder().enable_tools().build())
            .with_server_info(Implementation::new(
                "singing-trainer",
                env!("CARGO_PKG_VERSION"),
            ))
            .with_instructions(INSTRUCTIONS)
    }
}

/// Serves MCP over stdin/stdout until the client disconnects.
pub async fn serve_stdio(data_dir: PathBuf) -> anyhow::Result<()> {
    use rmcp::ServiceExt;
    let db = Db::open(&trainer_core::storage::db_path(&data_dir))?;
    let service = TrainerServer::new(db, data_dir)
        .serve(rmcp::transport::stdio())
        .await?;
    service.waiting().await?;
    Ok(())
}

/// Blocking entry point for binaries that do not run their own async runtime.
pub fn run_stdio_blocking(data_dir: PathBuf) -> anyhow::Result<()> {
    tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()?
        .block_on(serve_stdio(data_dir))
}

#[cfg(test)]
mod tests {
    use super::*;
    use rmcp::model::{CallToolRequestParams, ClientConfig};
    use rmcp::{ClientHandler, ServiceExt};

    #[derive(Debug, Clone, Default)]
    struct Client;

    impl ClientHandler for Client {
        fn get_info(&self) -> ClientConfig {
            ClientConfig::default()
        }
    }

    fn text_of(r: &CallToolResult) -> String {
        r.content
            .first()
            .and_then(|c| c.as_text())
            .map(|t| t.text.clone())
            .unwrap_or_default()
    }

    async fn call(
        client: &rmcp::service::RunningService<rmcp::RoleClient, Client>,
        name: &'static str,
        args: Value,
    ) -> (bool, Value) {
        let mut p = CallToolRequestParams::new(name);
        if let Value::Object(m) = args {
            p = p.with_arguments(m);
        }
        let r = client.call_tool(p).await.expect("call");
        let is_error = r.is_error.unwrap_or(false);
        let t = text_of(&r);
        (
            is_error,
            serde_json::from_str(&t).unwrap_or(Value::String(t)),
        )
    }

    #[tokio::test]
    async fn tools_work_end_to_end() -> anyhow::Result<()> {
        let dir = tempfile::tempdir()?;
        let db = Db::open(&trainer_core::storage::db_path(dir.path()))?;
        let server = TrainerServer::new(db, dir.path().to_path_buf());
        let (s, c) = tokio::io::duplex(1 << 20);
        let handle = tokio::spawn(async move {
            server.serve(s).await?.waiting().await?;
            anyhow::Ok(())
        });
        let client = Client.serve(c).await?;

        let tools = client.list_all_tools().await?;
        let names: Vec<String> = tools.iter().map(|t| t.name.to_string()).collect();
        for n in [
            "get_overview",
            "get_stats",
            "queue_session",
            "list_exercise_types",
            "save_coach_note",
            "save_anchor",
            "list_anchors",
        ] {
            assert!(names.iter().any(|x| x == n), "missing tool {n}");
        }

        let (err, overview) = call(&client, "get_overview", json!({})).await;
        assert!(!err);
        assert_eq!(overview["trials_total"], json!(0));

        // Absolute-pitch courses and the daily set.
        let (err, courses) = call(&client, "list_courses", json!({})).await;
        assert!(!err, "{courses}");
        let ids: Vec<&str> = courses
            .as_array()
            .unwrap()
            .iter()
            .filter_map(|c| c["id"].as_str())
            .collect();
        assert!(
            ids.contains(&"ap_naming") && ids.contains(&"ap_weekly"),
            "{ids:?}"
        );
        let (err, level) = call(
            &client,
            "get_course_level",
            json!({"course": "ap_naming", "level": 3}),
        )
        .await;
        assert!(!err, "{level}");
        assert_eq!(level["blocks"][0]["course"]["level"], json!(3));
        let (err, set) = call(&client, "preview_daily_set", json!({})).await;
        assert!(!err, "{set}");
        assert!(!set["items"].as_array().unwrap().is_empty());
        let (err, plan) = call(
            &client,
            "set_daily_plan",
            json!({"courses": [{"course": "ap_naming", "minutes": 20, "active": true}, {"course": "ap_brady", "minutes": 5, "active": false}], "test_day": 5}),
        )
        .await;
        assert!(!err, "{plan}");
        assert_eq!(plan["test_day"], json!(5));
        let (err, _) = call(&client, "set_daily_plan", json!({"courses": [{"course": "ap_weekly", "minutes": 5, "active": true}], "test_day": 5})).await;
        assert!(err, "tests are not ladder courses");
        let (err, st) = call(
            &client,
            "set_course_level",
            json!({"course": "ap_naming", "level": 4}),
        )
        .await;
        assert!(!err, "{st}");
        assert_eq!(st["level"], json!(4));
        let (err, dash) = call(&client, "get_ap_dashboard", json!({})).await;
        assert!(!err, "{dash}");
        assert_eq!(dash["classes"].as_array().unwrap().len(), 12);

        let session = json!({"title": "Coach session", "blocks": [
            {"type": "pitch_id", "count": 20, "params": {"classes": ["C", "F#"]}},
            {"type": "pattern", "count": 3, "params": {"pattern": {"scale": {"kind": "major"}}}}
        ]});
        let (err, dry) = call(
            &client,
            "queue_session",
            json!({"session": session, "dry_run": true}),
        )
        .await;
        assert!(!err, "{dry}");
        assert_eq!(dry["queued"], json!(false));
        assert_eq!(dry["validation"]["ok"], json!(true));
        assert_eq!(dry["validation"]["trial_count"], json!(23));

        let (err, q) = call(
            &client,
            "queue_session",
            json!({"session": session, "note": "Focus on F#"}),
        )
        .await;
        assert!(!err, "{q}");
        assert_eq!(q["queued"], json!(true));

        let bad = json!({"title": "Bad", "blocks": [{"type": "pitch_id", "params": {"timbres": ["gm:500"]}}]});
        let (_, b) = call(&client, "queue_session", json!({"session": bad})).await;
        assert_eq!(b["queued"], json!(false));
        assert_eq!(
            b["validation"]["errors"][0]["path"],
            json!("blocks[0].params")
        );

        // Anchor songs: refused until one is saved, then generated from it.
        let anchors_session =
            json!({"title": "Anchors", "blocks": [{"type": "anchor_song", "count": 4}]});
        let (_, a) = call(
            &client,
            "queue_session",
            json!({"session": anchors_session, "dry_run": true}),
        )
        .await;
        assert_eq!(a["validation"]["ok"], json!(false));
        let (err, saved) = call(
            &client,
            "save_anchor",
            json!({"anchor": {"title": "Yesterday", "first_note": "G4"}}),
        )
        .await;
        assert!(!err, "{saved}");
        let (_, list) = call(&client, "list_anchors", json!({})).await;
        assert_eq!(list.as_array().unwrap().len(), 1);
        let (_, a) = call(
            &client,
            "queue_session",
            json!({"session": anchors_session, "dry_run": true}),
        )
        .await;
        assert_eq!(a["validation"]["ok"], json!(true), "{a}");

        // The melody library: pieces written as files, listed, used by a session, removed.
        let lib_session = json!({"title": "Book", "blocks": [{"type": "sight_sing", "count": 2,
            "params": {"melody": {"library": {"collection": "Book"}}}}]});
        let (_, v) = call(
            &client,
            "queue_session",
            json!({"session": lib_session, "dry_run": true}),
        )
        .await;
        assert_eq!(v["validation"]["ok"], json!(false));
        let (err, added) = call(&client, "add_library_pieces", json!({"collection": "Book", "file_name": "ch1",
            "pieces": [{"title": "No. 1", "key": "D major", "time": "3/4", "notes": "A4:1 | D5:2 C#5:1 | D5:3"}]})).await;
        assert!(!err, "{added}");
        assert_eq!(added["ids"], json!(["Book/ch1"]));
        let (err, bad) = call(
            &client,
            "add_library_pieces",
            json!({"pieces": [{"key": "C", "notes": "C4:4 | D4:3 E4:2 | C4:4"}]}),
        )
        .await;
        assert!(
            err && bad.as_str().unwrap().contains("bar 2 has 5 beats"),
            "{bad}"
        );
        let midi_dir = tempfile::tempdir()?;
        let json_path = midi_dir.path().join("extra.json");
        std::fs::write(&json_path, r#"{"key": "G", "notes": "G4:4"}"#)?;
        let (_, imported) = call(
            &client,
            "import_library_files",
            json!({"paths": [json_path.to_string_lossy()], "collection": "Book/More"}),
        )
        .await;
        assert_eq!(
            imported[0]["imported"]["ids"],
            json!(["Book/More/extra"]),
            "{imported}"
        );
        let (_, list) = call(&client, "list_library", json!({"collection": "book"})).await;
        assert_eq!(list["pieces"].as_array().unwrap().len(), 2, "{list}");
        assert_eq!(list["pieces"][0]["bars"], json!(3));
        let (_, v) = call(
            &client,
            "queue_session",
            json!({"session": lib_session, "dry_run": true}),
        )
        .await;
        assert_eq!(v["validation"]["ok"], json!(true), "{v}");
        let (err, piece) = call(&client, "get_library_piece", json!({"id": "Book/ch1"})).await;
        assert!(!err);
        assert_eq!(piece["key"], json!("D major"));
        let (err, _) = call(
            &client,
            "remove_library_file",
            json!({"file": "Book/More/extra.json"}),
        )
        .await;
        assert!(!err);
        let (_, list) = call(&client, "list_library", json!({})).await;
        assert_eq!(list["pieces"].as_array().unwrap().len(), 1);

        // Free part-writing: an SATB piece written into the session, any intervals.
        let satb = json!({"title": "Chorale", "blocks": [{"type": "harmony", "count": 2, "params": {
            "play_parts": [0, 3],
            "source": {"given": {"part": 2, "pieces": [{"title": "Phrase 1", "key": "D major", "time": "4/4", "parts": [
                {"name": "Soprano", "notes": "F#4:1 E4 D4 C#4 | D4:4"},
                {"name": "Alto", "notes": "D4:1 C#4 B3 A3 | A3:4"},
                {"name": "Tenor", "notes": "A3:1 A3 G3 G3 | F#3:4"},
                {"name": "Bass", "notes": "D3:1 A2 G2 A2 | D3:4"}]}]}}}}]});
        let (_, v) = call(
            &client,
            "queue_session",
            json!({"session": satb, "dry_run": true}),
        )
        .await;
        assert_eq!(v["validation"]["ok"], json!(true), "{v}");

        let (_, sessions) = call(&client, "list_sessions", json!({"status": "queued"})).await;
        assert_eq!(sessions.as_array().unwrap().len(), 1);

        let (err, _) = call(
            &client,
            "save_coach_note",
            json!({"text": "Start stage 2 next week", "tags": ["ap.plan"]}),
        )
        .await;
        assert!(!err);
        let (_, notes) = call(&client, "get_coach_notes", json!({"tag": "ap"})).await;
        assert_eq!(notes.as_array().unwrap().len(), 1);

        let (err, stats) = call(
            &client,
            "get_stats",
            json!({"group_by": ["stimulus.pitch_class"]}),
        )
        .await;
        assert!(!err, "{stats}");
        let (err, _) = call(&client, "get_stats", json!({"group_by": ["bogus"]})).await;
        assert!(err);

        let (err, cat) = call(&client, "list_exercise_types", json!({})).await;
        assert!(!err);
        assert!(cat.as_array().unwrap().len() >= 11);

        let (err, rp) = call(
            &client,
            "set_range_profile",
            json!({"comfortable_low": "A2", "comfortable_high": "E4"}),
        )
        .await;
        assert!(!err, "{rp}");
        assert_eq!(rp["comfortable_low"], json!("A2"));

        let (err, preset) = call(&client, "get_preset", json!({"id": "ap_benchmark"})).await;
        assert!(!err);
        assert_eq!(preset["protocol"], json!("ap_benchmark"));

        client.cancel().await?;
        handle.await??;
        Ok(())
    }
}
