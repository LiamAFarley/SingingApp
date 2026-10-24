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
use trainer_core::generate::GenContext;
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
- Holdout stimuli (pool \"holdout\") are for assessment only. Put assessments at the start of a session, with no feedback.\n\
- Fade feedback as accuracy stabilises: continuous -> after_trial -> none. Test transfer (new octaves, timbres, keys) before \
calling something learned, and retest after gaps for retention.\n\
- pitch_pairing and other passive exposure are not evidence of absolute pitch.\n\
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
        self.with_db(|db| {
            let range = db.range_profile()?;
            let pools: PoolConfig = db.get_setting("stimulus_pools")?.unwrap_or_default();
            Ok(GenContext {
                comfortable: range.comfortable(),
                pools,
            })
        })
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
        description = "Results of the latest absolute pitch assessment (preset ap_assessment): accuracy and within-one-semitone accuracy per test, by timbre and octave, confusion matrix, signed error per pitch class, and the previous assessment for comparison."
    )]
    async fn get_ap_profile(&self) -> Result<CallToolResult, ErrorData> {
        match self.with_db(reports::ap_profile) {
            Ok(v) => text(&v),
            Err(e) => tool_error(e),
        }
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
        if let Ok(entries) = std::fs::read_dir(&banks_dir) {
            for e in entries.flatten() {
                let p = e.path();
                if p.extension()
                    .and_then(|x| x.to_str())
                    .map(|x| x.eq_ignore_ascii_case("sf2"))
                    .unwrap_or(false)
                {
                    if let Some(stem) = p.file_stem().and_then(|s| s.to_str()) {
                        out.push(json!({"id": format!("sf:{stem}:<program>"), "name": format!("SoundFont bank {stem}"), "pool": "training"}));
                    }
                }
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
        ] {
            assert!(names.iter().any(|x| x == n), "missing tool {n}");
        }

        let (err, overview) = call(&client, "get_overview", json!({})).await;
        assert!(!err);
        assert_eq!(overview["trials_total"], json!(0));

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

        let (err, preset) = call(&client, "get_preset", json!({"id": "ap_assessment"})).await;
        assert!(!err);
        assert_eq!(preset["protocol"], json!("ap_assessment"));

        client.cancel().await?;
        handle.await??;
        Ok(())
    }
}
