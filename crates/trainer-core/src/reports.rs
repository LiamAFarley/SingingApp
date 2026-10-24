//! Reports built from the trial log, shared by the app's results views and the MCP server.

use std::path::Path;

use anyhow::{Result, anyhow};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use ts_rs::TS;

use crate::schema::{
    AnchorSongParams, ChordIdParams, DictationParams, ExerciseKind, IncidentalPairingParams, IntervalIdParams, PatternParams, PitchIdParams, PitchMatchParams,
    PitchPairingParams, ScaleIdParams, SessionSpec, SightSingParams, SingIntervalParams, SingPitchParams,
    SustainParams, TonalCenterIdParams,
};
use crate::stats::{StatsQuery, aggregate, confusion, get_stats};
use crate::storage::{Db, SessionRow, SessionStatus, TrialFilter, TrialRow};

#[derive(Clone, Debug, Serialize, Deserialize, TS)]
pub struct BlockReport {
    pub index: u32,
    pub title: String,
    pub kind: String,
    pub trials: usize,
    pub graded: usize,
    pub correct: usize,
    pub accuracy: Option<f64>,
    pub mean_score: Option<f64>,
    pub median_rt_ms: Option<f64>,
    pub median_abs_cents: Option<f64>,
    pub pass_mark: Option<f32>,
    pub passed: Option<bool>,
    pub feedback: String,
    pub assessment: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize, TS)]
pub struct TrialBrief {
    pub id: String,
    pub block_idx: u32,
    pub trial_idx: u32,
    pub attempt: u32,
    #[serde(rename = "type")]
    pub kind: String,
    pub correct: Option<bool>,
    pub score: Option<f32>,
    pub rt_ms: Option<f64>,
    pub summary: String,
    #[ts(type = "Record<string, unknown>")]
    pub stimulus: Value,
    pub assisted: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize, TS)]
pub struct SessionReport {
    pub session: SessionRow,
    pub blocks: Vec<BlockReport>,
    pub trials: Vec<TrialBrief>,
    /// Accuracy over graded, unassisted trials.
    pub accuracy: Option<f64>,
    pub duration_s: Option<f64>,
}

fn brief(t: &TrialRow) -> TrialBrief {
    TrialBrief {
        id: t.id.clone(),
        block_idx: t.block_idx,
        trial_idx: t.trial_idx,
        attempt: t.attempt,
        kind: t.kind.clone(),
        correct: t.correct,
        score: t.score,
        rt_ms: t.rt_ms,
        summary: t.summary.clone(),
        stimulus: t.stimulus.clone(),
        assisted: t.assisted,
    }
}

pub fn session_report(db: &Db, session_id: &str) -> Result<SessionReport> {
    let session = db
        .get_session(session_id)?
        .ok_or_else(|| anyhow!("no session {session_id}"))?;
    let spec: Option<SessionSpec> = session.spec().ok();
    let trials = db.trials_for_session(session_id)?;
    let mut blocks = Vec::new();
    let n_blocks = spec.as_ref().map(|s| s.blocks.len()).unwrap_or(0).max(
        trials
            .iter()
            .map(|t| t.block_idx as usize + 1)
            .max()
            .unwrap_or(0),
    );
    for b in 0..n_blocks {
        let rows: Vec<TrialRow> = trials
            .iter()
            .filter(|t| t.block_idx as usize == b && !t.assisted)
            .cloned()
            .collect();
        let spec_block = spec.as_ref().and_then(|s| s.blocks.get(b));
        let stats = aggregate(&rows, &[]);
        let st = stats.first();
        let accuracy = st.and_then(|s| s.accuracy);
        let pass_mark = spec_block.and_then(|s| s.pass_mark);
        blocks.push(BlockReport {
            index: b as u32,
            title: spec_block
                .map(|s| s.display_title())
                .unwrap_or_else(|| format!("Block {}", b + 1)),
            kind: spec_block
                .map(|s| s.exercise.kind().as_str().to_string())
                .or_else(|| rows.first().map(|r| r.kind.clone()))
                .unwrap_or_default(),
            trials: rows.len(),
            graded: st.map(|s| s.graded).unwrap_or(0),
            correct: st.map(|s| s.correct).unwrap_or(0),
            accuracy,
            mean_score: st.and_then(|s| s.mean_score),
            median_rt_ms: st.and_then(|s| s.median_rt_ms),
            median_abs_cents: st.and_then(|s| s.median_abs_cents),
            pass_mark,
            passed: match (pass_mark, accuracy) {
                (Some(p), Some(a)) => Some(a >= p as f64),
                _ => None,
            },
            feedback: spec_block
                .map(|s| s.effective_feedback().as_str().to_string())
                .unwrap_or_default(),
            assessment: spec_block.map(|s| s.assessment).unwrap_or(false),
        });
    }
    let unassisted: Vec<TrialRow> = trials.iter().filter(|t| !t.assisted).cloned().collect();
    let accuracy = aggregate(&unassisted, &[]).first().and_then(|s| s.accuracy);
    let duration_s = match (&session.started_at, &session.completed_at) {
        (Some(a), Some(b)) => {
            let a = chrono::DateTime::parse_from_rfc3339(a).ok();
            let b = chrono::DateTime::parse_from_rfc3339(b).ok();
            a.zip(b)
                .map(|(a, b)| (b - a).num_milliseconds() as f64 / 1000.0)
        }
        _ => None,
    };
    Ok(SessionReport {
        session,
        blocks,
        trials: trials.iter().map(brief).collect(),
        accuracy,
        duration_s,
    })
}

/// Compact summary stored on the session when it finishes.
pub fn session_summary(report: &SessionReport) -> Value {
    json!({
        "accuracy": report.accuracy,
        "trials": report.trials.len(),
        "blocks": report.blocks.iter().map(|b| json!({
            "title": b.title,
            "type": b.kind,
            "accuracy": b.accuracy,
            "median_abs_cents": b.median_abs_cents,
            "passed": b.passed,
        })).collect::<Vec<_>>(),
    })
}

fn since_days(days: i64) -> String {
    (chrono::Utc::now() - chrono::Duration::days(days)).to_rfc3339()
}

pub fn overview(db: &Db, data_dir: Option<&Path>) -> Result<Value> {
    let by_type = |days: i64| -> Result<Value> {
        let q = StatsQuery {
            filter: TrialFilter {
                since: Some(since_days(days)),
                assisted: Some(false),
                ..Default::default()
            },
            group_by: vec!["type".into()],
            max_trials: Some(20_000),
        };
        Ok(serde_json::to_value(get_stats(db, &q)?)?)
    };
    let recent: Vec<Value> = db
        .list_sessions(None, 8)?
        .into_iter()
        .filter(|s| s.status != SessionStatus::Queued)
        .map(|s| {
            json!({
                "id": s.id, "title": s.title, "status": s.status, "source": s.source,
                "started_at": s.started_at, "completed_at": s.completed_at, "summary": s.summary,
            })
        })
        .collect();
    let queued: Vec<Value> = db
        .queued_sessions()?
        .into_iter()
        .map(|s| json!({ "id": s.id, "title": s.title, "queued_at": s.queued_at, "note": s.queue_note }))
        .collect();
    Ok(json!({
        "now": chrono::Utc::now().to_rfc3339(),
        "data_dir": data_dir.map(|d| d.display().to_string()),
        "trials_total": db.count_trials()?,
        "last_7_days_by_type": by_type(7)?,
        "last_30_days_by_type": by_type(30)?,
        "recent_sessions": recent,
        "queued_sessions": queued,
        "range_profile": db.range_profile()?,
        "recent_comfort_reports": db.comfort_reports(5)?,
        "coach_notes": db.coach_notes(None, 5)?,
        "last_labeled_reference_at": db.last_labeled_reference_at()?.map(|d| d.to_rfc3339()),
    }))
}

/// Full detail of one trial, with the pitch trace decimated to 20 Hz.
pub fn trial_detail(db: &Db, trial_id: &str) -> Result<Value> {
    let t = db
        .get_trial(trial_id)?
        .ok_or_else(|| anyhow!("no trial {trial_id}"))?;
    let trace = db.get_trace(trial_id)?.map(|tr| {
        let step = ((0.05 / tr.hop_s).round() as usize).max(1);
        tr.frames
            .iter()
            .step_by(step)
            .map(|f| {
                json!([
                    (f.t * 100.0).round() / 100.0,
                    f.midi.map(|m| (m * 100.0).round() / 100.0)
                ])
            })
            .collect::<Vec<_>>()
    });
    let mut v = serde_json::to_value(&t)?;
    if let Some(trace) = trace {
        v["trace_20hz"] = json!({ "columns": ["t_s", "midi"], "rows": trace });
    }
    v["has_recording"] = json!(db.recording_path(trial_id)?.is_some());
    Ok(v)
}

/// Results of the most recent absolute-pitch assessment, compared with the previous one.
pub fn ap_profile(db: &Db) -> Result<Value> {
    let sessions: Vec<SessionRow> = db
        .list_sessions(Some(SessionStatus::Completed), 200)?
        .into_iter()
        .filter(|s| s.protocol.as_deref() == Some("ap_assessment"))
        .collect();
    let Some(latest) = sessions.first() else {
        return Ok(
            json!({ "assessment": null, "message": "No completed absolute pitch assessment yet (preset ap_assessment)." }),
        );
    };
    let profile = |s: &SessionRow| -> Result<Value> {
        let trials = db.trials_for_session(&s.id)?;
        let spec = s.spec().ok();
        let mut blocks = Vec::new();
        for b in 0..spec.as_ref().map(|s| s.blocks.len()).unwrap_or(0) {
            let rows: Vec<TrialRow> = trials
                .iter()
                .filter(|t| t.block_idx as usize == b)
                .cloned()
                .collect();
            if rows.is_empty() {
                continue;
            }
            let st = aggregate(&rows, &[]).into_iter().next();
            let within_one = rows
                .iter()
                .filter(|t| {
                    t.metrics
                        .get("signed_error")
                        .and_then(Value::as_i64)
                        .map(|e| e.abs() <= 1)
                        .unwrap_or(false)
                })
                .count();
            let title = spec
                .as_ref()
                .map(|s| s.blocks[b].display_title())
                .unwrap_or_default();
            let mut entry = json!({
                "title": title,
                "type": rows[0].kind,
                "trials": rows.len(),
                "accuracy": st.as_ref().and_then(|s| s.accuracy),
                "within_one_semitone": within_one as f64 / rows.len() as f64,
                "median_rt_ms": st.as_ref().and_then(|s| s.median_rt_ms),
                "median_abs_cents": st.as_ref().and_then(|s| s.median_abs_cents),
            });
            if rows[0].kind == ExerciseKind::PitchId.as_str() {
                entry["by_timbre"] =
                    serde_json::to_value(aggregate(&rows, &["stimulus.timbre".into()]))?;
                entry["by_octave"] =
                    serde_json::to_value(aggregate(&rows, &["stimulus.octave".into()]))?;
            }
            blocks.push(entry);
        }
        let id_rows: Vec<TrialRow> = trials
            .iter()
            .filter(|t| t.kind == ExerciseKind::PitchId.as_str())
            .cloned()
            .collect();
        Ok(json!({
            "session_id": s.id,
            "completed_at": s.completed_at,
            "blocks": blocks,
            "identification_confusion": confusion(&id_rows),
            "signed_error_by_pitch_class": aggregate(&id_rows, &["stimulus.pitch_class".into()]),
        }))
    };
    let current = profile(latest)?;
    let previous = match sessions.get(1) {
        Some(p) => Some(profile(p)?),
        None => None,
    };
    Ok(
        json!({ "assessment": current, "previous": previous, "assessments_completed": sessions.len() }),
    )
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ExerciseTypeInfo {
    #[serde(rename = "type")]
    pub kind: String,
    pub title: String,
    pub description: String,
    pub default_tags: Vec<String>,
    pub params_schema: Value,
    pub example: Value,
}

fn schema_of<T: schemars::JsonSchema>() -> Value {
    serde_json::to_value(schemars::schema_for!(T)).unwrap_or(Value::Null)
}

pub fn exercise_catalogue() -> Vec<ExerciseTypeInfo> {
    let entry =
        |kind: ExerciseKind, description: &str, schema: Value, example: Value| ExerciseTypeInfo {
            kind: kind.as_str().into(),
            title: kind.title().into(),
            description: description.into(),
            default_tags: kind.default_tags().iter().map(|s| s.to_string()).collect(),
            params_schema: schema,
            example,
        };
    vec![
        entry(
            ExerciseKind::PitchId,
            "A single tone, no reference; name it. Absolute pitch identification. Use classes + out_of_set_rate for progressive category training, octaves/timbres/pool for transfer tests, deadline_ms for speed. Keep a mask between trials when feedback reveals answers.",
            schema_of::<PitchIdParams>(),
            json!({"type": "pitch_id", "count": 40, "feedback": "after_trial", "params": {"classes": ["C", "E", "G#"], "out_of_set_rate": 0.15, "timbres": ["gm:0", "gm:48"], "mask": "random_tones"}}),
        ),
        entry(
            ExerciseKind::SingPitch,
            "Prompt 'Sing E♭' with no reference; graded on pitch class (octave ignored) unless octave is set. Reports initial error, settled error, settling time, latency.",
            schema_of::<SingPitchParams>(),
            json!({"type": "sing_pitch", "count": 12, "params": {"classes": "all", "pre_mask": "random_tones"}}),
        ),
        entry(
            ExerciseKind::PitchPairing,
            "Passive exposure: tone then its name on screen. No response; never counts as evidence of absolute pitch.",
            schema_of::<PitchPairingParams>(),
            json!({"type": "pitch_pairing", "count": 36, "params": {}}),
        ),
        entry(
            ExerciseKind::TonalCenterId,
            "A I–IV–V–I cadence in a random key; name the tonic. Without reference = absolute pitch in context; reference=true plays a labelled A4 first (relative).",
            schema_of::<TonalCenterIdParams>(),
            json!({"type": "tonal_center_id", "count": 12, "params": {"modes": ["major", "minor"]}}),
        ),
        entry(
            ExerciseKind::IntervalId,
            "Two notes (ascending, descending or together); name the interval.",
            schema_of::<IntervalIdParams>(),
            json!({"type": "interval_id", "count": 30, "params": {"intervals": ["m6", "M6", "P5"], "directions": ["descending"]}}),
        ),
        entry(
            ExerciseKind::ChordId,
            "A chord (block or arpeggio); name the quality, optionally the inversion.",
            schema_of::<ChordIdParams>(),
            json!({"type": "chord_id", "count": 20, "params": {"qualities": ["major", "minor", "diminished", "augmented"]}}),
        ),
        entry(
            ExerciseKind::ScaleId,
            "A scale or mode; name it.",
            schema_of::<ScaleIdParams>(),
            json!({"type": "scale_id", "count": 20, "params": {"scales": ["major", "dorian", "mixolydian"]}}),
        ),
        entry(
            ExerciseKind::PitchMatch,
            "Hear a note and sing it back (or sing along with it). Range defaults to the comfortable range.",
            schema_of::<PitchMatchParams>(),
            json!({"type": "pitch_match", "count": 15, "feedback": "continuous", "params": {}}),
        ),
        entry(
            ExerciseKind::Sustain,
            "Hold a note; reports centre error, drift, stability and vibrato (vibrato never counts as out of tune).",
            schema_of::<SustainParams>(),
            json!({"type": "sustain", "count": 4, "params": {"hold_ms": 5000, "drone": true}}),
        ),
        entry(
            ExerciseKind::SingInterval,
            "The first note is given; sing both notes of the interval in time with the metronome. Reports interval error.",
            schema_of::<SingIntervalParams>(),
            json!({"type": "sing_interval", "count": 12, "params": {"intervals": ["M3", "P5"], "directions": ["up"], "tempo_bpm": 80}}),
        ),
        entry(
            ExerciseKind::Pattern,
            "A sung pattern (scale, arpeggio, scale degrees or exact notes) on a metronome timeline, transposed per trial. reference: echo (hear first), guided (sing along), first_note, none. backing: none, click, drone, chords, melody, melody_and_chords. Graded note by note with alignment; relative_drift_tolerant grading is used when nothing is played.",
            schema_of::<PatternParams>(),
            json!({"type": "pattern", "count": 6, "feedback": "continuous", "params": {"pattern": {"degrees": {"degrees": [1, 3, 5, 3, 1]}}, "keys": "ascending", "tempo_bpm": 90, "reference": "echo", "backing": "click"}}),
        ),
        entry(
            ExerciseKind::SightSing,
            "Sight singing from staff notation (no piano roll, so durations are read, not seen). The score is shown, the key is established (cadence and/or first note, per `start`), one bar is counted in and the melody is sung with the click; graded note by note for pitch and timing. melody: {\"generate\": {keys, modes, meters, bars, rhythms, max_leap, step_rate, rest_rate, start_degrees, range, clef}} or {\"given\": [melodies you write]} where a melody is {key: \"Eb major\", time: \"4/4\", notes: [{note: \"Eb4\", beats: 1}, {rest: 0.5}, ...]} (spelling is kept). start: key_and_first_note (default), key_only, first_note, none (graded relative to the first sung note unless grading.mode is set). After each attempt the user can listen, sing along with the melody or retry.",
            schema_of::<SightSingParams>(),
            json!({"type": "sight_sing", "count": 6, "feedback": "after_trial", "params": {"melody": {"generate": {"keys": ["C", "G", "F"], "bars": 4, "rhythms": ["half", "quarter"], "max_leap": 2}}, "tempo_bpm": 72}}),
        ),
        entry(
            ExerciseKind::MelodicDictation,
            "Melodic dictation: the key is played (cadence), a bar is counted in and the melody plays; the user writes it on a staff in the score editor (durations, dots, rests, accidentals) and checks it. Graded note by note after aligning the written notes to the melody, so one rhythm slip does not cost later pitches; grade_rhythm=false grades pitches only. melody works as in sight_sing (generate or given). max_plays limits hearings (0 = no limit); give_first_note writes the first note in.",
            schema_of::<DictationParams>(),
            json!({"type": "melodic_dictation", "count": 6, "feedback": "after_trial", "params": {"melody": {"generate": {"keys": ["C", "F", "G"], "bars": 2, "rhythms": ["half", "quarter"], "max_leap": 2}}, "max_plays": 4}}),
        ),
        entry(
            ExerciseKind::AnchorSong,
            "Anchor songs (songs the user knows in their original key, saved in the app or with save_anchor). mode sing_first_note: 'Sing the first note of …', graded by pitch class with the octave reported. mode key_check: the saved opening phrase plays in the original key or shifted by one of `shifts`; answer original or shifted (needs anchors with a melody). Fails validation when no matching anchors are saved; check list_anchors first.",
            schema_of::<AnchorSongParams>(),
            json!({"type": "anchor_song", "count": 8, "params": {"mode": "sing_first_note"}}),
        ),
        entry(
            ExerciseKind::IncidentalPairing,
            "Incidental pairing: a colour task in which each colour always comes with the same pitch class (fixed colours: C red, C# orange, D yellow, D# olive, E green, F teal, F# sky, G blue, G# indigo, A purple, A# pink, B brown). The user presses the colour; the tone is never mentioned. Exposure only, never evidence of absolute pitch; transfer is measured with pitch_id. mismatch_rate trials pair a colour with another class: slower responses on those (get_stats grouped by stimulus.matched, field rt) suggest the pairing is being learnt.",
            schema_of::<IncidentalPairingParams>(),
            json!({"type": "incidental_pairing", "count": 60, "params": {"classes": ["C", "E", "G#", "D"]}}),
        ),
    ]
}

pub fn session_schema() -> Value {
    schema_of::<SessionSpec>()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::storage::tests_support::sample_trial;

    #[test]
    fn reports_on_a_session() {
        let db = Db::open_in_memory().unwrap();
        let spec = SessionSpec::from_json(
            r#"{"title": "T", "blocks": [{"type": "pitch_id", "count": 4, "pass_mark": 0.7}]}"#,
        )
        .unwrap();
        let id = db.start_new_session(&spec, "preset").unwrap();
        for i in 0..4 {
            db.insert_trial(&sample_trial(&id, i, i != 3)).unwrap();
        }
        let r = session_report(&db, &id).unwrap();
        assert_eq!(r.blocks.len(), 1);
        assert_eq!(r.blocks[0].correct, 3);
        assert_eq!(r.blocks[0].passed, Some(true));
        assert_eq!(r.trials.len(), 4);
        let o = overview(&db, None).unwrap();
        assert_eq!(o["trials_total"], json!(4));
        let d = trial_detail(&db, &r.trials[0].id).unwrap();
        assert_eq!(d["type"], json!("pitch_id"));
        assert!(ap_profile(&db).unwrap()["assessment"].is_null());
    }

    #[test]
    fn catalogue_covers_every_exercise() {
        let cat = exercise_catalogue();
        assert_eq!(cat.len(), ExerciseKind::ALL.len());
        for e in &cat {
            assert!(e.params_schema.is_object(), "{}", e.kind);
            let mut ex = e.example.clone();
            ex.as_object_mut()
                .unwrap()
                .entry("params")
                .or_insert(json!({}));
            let spec = SessionSpec::from_value(json!({"title": "x", "blocks": [ex]}));
            assert!(spec.is_ok(), "{}: {:?}", e.kind, spec.err());
        }
        assert!(session_schema().is_object());
    }
}
