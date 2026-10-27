//! Reports built from the trial log, shared by the app's results views and the MCP server.

use std::path::Path;

use anyhow::{Result, anyhow};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use ts_rs::TS;

use crate::ap_tests::{CHANCE, IN_TIME_MS, TestRun, test_runs};
use crate::schema::{
    AnchorSongParams, BassLineParams, CadenceIdParams, ChordColorParams, ChordIdParams,
    DictationParams, ErrorDetectionParams, ExerciseKind, HarmonyParams, IncidentalPairingParams,
    IntervalComparisonParams, IntervalIdParams, MelodicUpdatingParams, NBackParams,
    PatternDialogueParams, PatternMemoryParams, PatternParams, PitchDetectParams, PitchIdParams,
    PitchMatchParams, PitchMemoryMatchParams, PitchPairingParams, PitchWalkParams,
    RestingToneParams, ScaleDegreeParams, ScaleIdParams, SessionSpec, SightSingParams,
    SingIntervalParams, SingPitchParams, SpanParams, SustainParams, SyllableCheckParams,
    TonalCenterIdParams, TonalPatternParams,
};
use crate::stats::{StatsQuery, aggregate, confusion, get_stats};
use crate::storage::{Db, SessionRow, SessionStatus, TrialFilter, TrialRow};

#[derive(Clone, Debug, Serialize, Deserialize, TS)]
pub struct BlockReport {
    pub index: u32,
    pub title: String,
    pub kind: String,
    /// Unassisted trials.
    pub trials: usize,
    /// Attempts sung with help (practice, sing-along retries), not counted in accuracy.
    pub assisted: usize,
    pub graded: usize,
    pub correct: usize,
    pub accuracy: Option<f64>,
    pub mean_score: Option<f64>,
    pub median_rt_ms: Option<f64>,
    pub median_abs_cents: Option<f64>,
    /// Sung notes: median time to settle within tolerance after the onset.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub median_settling_ms: Option<f64>,
    /// Single sung notes: median size of the first attempt's error (its attack), and how far the
    /// note then moved toward the target.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub median_abs_attack_cents: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub median_correction_cents: Option<f64>,
    /// Self-judged trials: the fraction judged right (flat, in tune or sharp).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub judgement_accuracy: Option<f64>,
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
            assisted: trials
                .iter()
                .filter(|t| t.block_idx as usize == b && t.assisted)
                .count(),
            graded: st.map(|s| s.graded).unwrap_or(0),
            correct: st.map(|s| s.correct).unwrap_or(0),
            accuracy,
            mean_score: st.and_then(|s| s.mean_score),
            median_rt_ms: st.and_then(|s| s.median_rt_ms),
            median_abs_cents: st.and_then(|s| s.median_abs_cents),
            median_settling_ms: crate::stats::median(
                rows.iter()
                    .filter_map(|t| t.metrics.get("settling_ms").and_then(|v| v.as_f64()))
                    .collect(),
            ),
            median_abs_attack_cents: crate::stats::median(
                rows.iter()
                    .filter_map(|t| t.metrics.get("attack_cents").and_then(|v| v.as_f64()))
                    .map(f64::abs)
                    .collect(),
            ),
            median_correction_cents: crate::stats::median(
                rows.iter()
                    .filter_map(|t| t.metrics.get("correction_cents").and_then(|v| v.as_f64()))
                    .collect(),
            ),
            judgement_accuracy: {
                let judged: Vec<bool> = rows
                    .iter()
                    .filter_map(|t| t.metrics.get("judgement_correct").and_then(|v| v.as_bool()))
                    .collect();
                (!judged.is_empty())
                    .then(|| judged.iter().filter(|j| **j).count() as f64 / judged.len() as f64)
            },
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
            "median_settling_ms": b.median_settling_ms,
            "median_abs_attack_cents": b.median_abs_attack_cents,
            "median_correction_cents": b.median_correction_cents,
            "judgement_accuracy": b.judgement_accuracy,
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

/// One test's history: the latest run, the one before, whether the change between them is bigger
/// than run-to-run noise, and the trend over every run.
fn test_history(runs: &[TestRun]) -> Value {
    let Some(latest) = runs.last() else {
        return Value::Null;
    };
    let previous = runs.len().checked_sub(2).map(|i| &runs[i]);
    let change = previous.map(|p| {
        let d = latest.accuracy - p.accuracy;
        let se = (latest.accuracy * (1.0 - latest.accuracy) / latest.trials.max(1) as f64
            + p.accuracy * (1.0 - p.accuracy) / p.trials.max(1) as f64)
            .sqrt();
        json!({
            "accuracy": d,
            "accuracy_noise_95": 1.96 * se,
            "beyond_noise": d.abs() > 1.96 * se,
            "ucsf_points": latest.ucsf_points - p.ucsf_points,
            "mad": match (latest.mad, p.mad) { (Some(a), Some(b)) => json!(a - b), _ => Value::Null },
            "in_time": latest.in_time - p.in_time,
        })
    });
    json!({
        "runs": runs.len(),
        "latest": latest,
        "previous": previous,
        "change_from_previous": change,
        "trend_per_30_days": trend_per_30_days(runs),
        "history": runs.iter().map(|r| json!({
            "at": r.at, "trials": r.trials, "accuracy": r.accuracy, "in_time": r.in_time,
            "ucsf_points": r.ucsf_points, "mad": r.mad, "shepard": r.shepard,
            "sung_accuracy": r.sung.as_ref().map(|s| s.accuracy),
            "sung_median_abs_cents": r.sung.as_ref().and_then(|s| s.median_abs_cents),
        })).collect::<Vec<_>>(),
    })
}

/// Least-squares slope of accuracy and UCSF points per 30 days, from three runs on.
fn trend_per_30_days(runs: &[TestRun]) -> Value {
    let pts: Vec<(f64, &TestRun)> = runs
        .iter()
        .filter(|r| r.trials > 0)
        .filter_map(|r| {
            chrono::DateTime::parse_from_rfc3339(&r.at)
                .ok()
                .map(|t| (t.timestamp() as f64 / 86_400.0, r))
        })
        .collect();
    if pts.len() < 3 {
        return Value::Null;
    }
    let slope = |f: &dyn Fn(&TestRun) -> f64| {
        let n = pts.len() as f64;
        let mx = pts.iter().map(|p| p.0).sum::<f64>() / n;
        let my = pts.iter().map(|p| f(p.1)).sum::<f64>() / n;
        let sxx: f64 = pts.iter().map(|p| (p.0 - mx).powi(2)).sum();
        let sxy: f64 = pts.iter().map(|p| (p.0 - mx) * (f(p.1) - my)).sum();
        if sxx > 0.0 { sxy / sxx * 30.0 } else { 0.0 }
    };
    json!({ "accuracy": slope(&|r| r.accuracy), "ucsf_points": slope(&|r| r.ucsf_points) })
}

/// The absolute-pitch tests: the benchmark in detail, and the weekly, transfer and UCSF-style
/// tests. Runs are found by their course tags, whether started on their own or in today's set.
pub fn ap_profile(db: &Db) -> Result<Value> {
    use crate::courses::{AP_BENCHMARK, AP_TRANSFER, AP_UCSF_STYLE, AP_WEEKLY};
    let benchmark = test_runs(db, AP_BENCHMARK)?;
    let mut bench = test_history(&benchmark);
    if let Some(latest) = benchmark.last() {
        let tag = format!("course.{AP_BENCHMARK}.");
        let rows: Vec<TrialRow> = db
            .trials_for_session(&latest.session_id)?
            .into_iter()
            .filter(|t| t.kind == ExerciseKind::PitchId.as_str() && !t.assisted)
            .filter(|t| t.tags.iter().any(|g| g.starts_with(&tag)))
            .collect();
        bench["identification_confusion"] = serde_json::to_value(confusion(&rows))?;
        bench["signed_error_by_pitch_class"] =
            serde_json::to_value(aggregate(&rows, &["stimulus.pitch_class".into()]))?;
        bench["by_octave"] = serde_json::to_value(aggregate(&rows, &["stimulus.octave".into()]))?;
    }
    let legacy = db
        .list_sessions(Some(SessionStatus::Completed), 500)?
        .into_iter()
        .filter(|s| s.protocol.as_deref() == Some("ap_assessment"))
        .count();
    let aptitude = crate::aptitude::aptitude_profile(db)?;
    Ok(json!({
        "scoring": {
            "answer_limit_ms": crate::courses::TEST_DEADLINE_MS,
            "in_time_ms": IN_TIME_MS,
            "times_measured_from": "tone onset (tones last 1 s)",
            "chance": CHANCE,
            "accuracy": "correct within the answer limit; timeouts count as wrong",
            "ucsf_points": "1 exact, 0.75 a semitone off, 0 otherwise, as a share of the maximum",
            "mad": "mean absolute error in semitones over answered notes, 0 perfect, 3 chance",
        },
        "benchmark": bench,
        "weekly": test_history(&test_runs(db, AP_WEEKLY)?),
        "transfer": test_history(&test_runs(db, AP_TRANSFER)?),
        "ucsf_style": test_history(&test_runs(db, AP_UCSF_STYLE)?),
        "aptitude": {
            "latest": aptitude.latest,
            "capacity_band": aptitude.capacity_band,
            "runs": { "inm": aptitude.inm.len(), "n_back": aptitude.n_back.len(), "span": aptitude.span.len() },
            "detail": "get_aptitude_profile",
        },
        "legacy_assessments": legacy,
        "legacy_note": (legacy > 0).then_some("Runs of the old ap_assessment preset (holdout piano, random-tone masks) are not comparable with these tests and are left out."),
        "guide": "docs/AP_TESTING.md",
    }))
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
            "A single tone, no reference; name it. Absolute pitch identification. `classes` is all twelve or a subset for progressive category training; with out_of_set_rate (0.15–0.2) that share of tones comes from outside the subset, at the edges of the range, answered Other. Anti-cheating options: octaves spanning at least three octaves with min_jump_semitones 13 (no tone within 13 semitones of the last, so it cannot be named by its interval from the previous labelled tone), deadline_ms 2000, and a block pre_mask \"shepard_glissando\" (20 s, clears pitch memory) before unassisted checks. class_weights (e.g. {\"C\": 0.4}) weights an anchor class (Brady); pacing {\"stream\": {\"ioi_ms\": 1400}} plays a tone on a fixed schedule whether or not the user answered. octaves/timbres/pool for transfer tests. After answering, the user can click any answer to hear it. Keep a mask between trials when feedback reveals answers.",
            schema_of::<PitchIdParams>(),
            json!({"type": "pitch_id", "count": 40, "feedback": "after_trial", "params": {"classes": ["C", "E", "G#", "D"], "out_of_set_rate": 0.15, "octaves": [3, 4, 5], "min_jump_semitones": 13, "deadline_ms": 3000}}),
        ),
        entry(
            ExerciseKind::SingPitch,
            "Prompt 'Sing E♭' with no reference; graded on pitch class (octave ignored) unless octave is set. Reports initial error, settled error, settling time, latency.",
            schema_of::<SingPitchParams>(),
            json!({"type": "sing_pitch", "count": 12, "params": {"classes": "all"}}),
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
            "Two notes (ascending, descending or together); name the interval. Free-standing on random roots by default (root_range). in_key=true plays a cadence (cadence: start | key_change | every, cadence_bpm) and places the notes in the key: start random (any scale note), tonic (from do; also start_note without a key) or fixed (start_note), moved by start_octaves; chromatic=true lets the second note leave the scale (non-diatonic intervals). other_rate (0-0.5) adds intervals from outside the set answered Other, so one interval alone can be drilled. After answering, an in-key interval's notes are named with labels (fixed_do default, solfege, numbers). keys/modes/key_change (per_block = one random key with keys \"all\", per_trial = new key each question).",
            schema_of::<IntervalIdParams>(),
            json!({"type": "interval_id", "count": 30, "params": {"intervals": ["m6", "M6", "P5"], "directions": ["descending"]}}),
        ),
        entry(
            ExerciseKind::ChordId,
            "A chord; name the quality, optionally the inversion (answer_inversion with inversions; the augmented triad and diminished seventh have none to name). style, or styles mixed per question: block (harmonic), arpeggio (ascending), arpeggio_down (descending). 24 qualities, among them major_b5, dominant7_b5, dominant7_s5, dominant7_sus4, dominant7_sus2, minor_major7_b5, major7_b5, major7_s5, major7_sus4, major7_sus2.",
            schema_of::<ChordIdParams>(),
            json!({"type": "chord_id", "count": 20, "params": {"qualities": ["major", "minor", "diminished", "augmented"]}}),
        ),
        entry(
            ExerciseKind::ScaleId,
            "A scale or mode; name it. 28 scales: the church modes, harmonic and melodic minor and their modes (dorian_b2, lydian_augmented, lydian_dominant, mixolydian_b6, locrian_s2, altered; locrian_s6, ionian_s5, dorian_s4, phrygian_dominant, lydian_s2, ultra_locrian), whole tone, both diminished scales, pentatonics, blues, chromatic.",
            schema_of::<ScaleIdParams>(),
            json!({"type": "scale_id", "count": 20, "params": {"scales": ["major", "dorian", "mixolydian"]}}),
        ),
        entry(
            ExerciseKind::PitchMatch,
            "Hear a note and sing it back (or sing along with it). Range defaults to the comfortable range. delay_ms holds the note in memory through silence before singing (the lane hides the target), and distractor_tones play random notes within a fifth during the delay to interfere with it. Every single sung note stores attack_cents (the first reliable pitch, 40–140 ms in), centre_cents (settled), correction_cents (|attack| − |centre|: how far it moved toward the target), settling_ms, drift_cents_per_s, stability_cents and error_semitones; block grading grade_on \"attack\" marks it on the attack instead of where it settled. A block with self_judge asks flat, in tune or sharp before the result (judgement_correct, judged_actual).",
            schema_of::<PitchMatchParams>(),
            json!({"type": "pitch_match", "count": 15, "feedback": "continuous", "params": {}}),
        ),
        entry(
            ExerciseKind::Sustain,
            "Hold a note; reports centre error, drift, stability (stability_cents, the wavering left after the drift is removed) and vibrato (vibrato never counts as out of tune). drone true keeps the note sounding under the voice; comparing drone on and off separates losing the pitch from uneven production.",
            schema_of::<SustainParams>(),
            json!({"type": "sustain", "count": 4, "params": {"hold_ms": 5000, "drone": true}}),
        ),
        entry(
            ExerciseKind::SingInterval,
            "The first note is given; sing both notes of the interval in time with the metronome. Reports interval error. in_key=true plays a cadence first (cadence plan as for intervals) and starts from the tonic (start tonic) or any scale note (start random), staying in the scale unless chromatic; the prompt names the notes with labels (fixed do by default), e.g. From sol, sing ti.",
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
            "Sight singing from staff notation (no piano roll, so durations are read, not seen). The score is shown, the key is established (cadence and/or first note, per `start`), one bar is counted in and the melody is sung with the click; graded note by note for pitch and timing, in any octave (a line sung an octave down is graded on its notes; grading.mode absolute requires the written octave). melody: {\"generate\": {keys, modes, meters, bars, rhythms, max_leap, step_rate, rest_rate, start_degrees, range, clef, degrees (the notes used, 1–8 from the tonic), notes (a fixed count of quarter notes, no rhythm)}} or {\"given\": [melodies you write]} where a melody is {key: \"Eb major\", time: \"4/4\", notes: [{note: \"Eb4\", beats: 1}, {rest: 0.5}, ...]} (spelling is kept). start: key_and_first_note (default), key_only, first_note, none (graded relative to the first sung note unless grading.mode is set). After each attempt the user can listen, sing along with the melody or retry.",
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
        entry(
            ExerciseKind::ScaleDegreeId,
            "Scale degrees, as in Functional Ear Trainer: a cadence sets the key, one note plays, the user names its degree; after answering the note walks diatonically to the tonic, each answer lighting up as it sounds (resolution: nearest = 2-4 down and 5-7 up, ascending, descending, none). labels: fixed_do (default; C is do, so buttons name the key's notes, e.g. sol la ti do re mi fi sol in G, with di ri fi si li for sharps and ra me se le te for flats), solfege (movable do) or numbers. upper_tonic (default true) adds 8, the tonic above the cadence's; 1 and 8 both count as right for any tonic, whatever its octave. Build levels with `degrees` (e.g. [\"1\",\"2\",\"3\"], then add degrees; chromatic ones like b2 or #4 later), a fixed key (keys [\"C\"]), one random key for the block (keys \"all\", key_change per_block) or a new key every question (key_change per_trial). cadence: start (default in one key), key_change (default when the key changes) or every; cadence_bpm sets its tempo. resolve_before_answer is a learning aid for new degrees; octaves [-1,0,1] for range.",
            schema_of::<ScaleDegreeParams>(),
            json!({"type": "scale_degree_id", "count": 30, "feedback": "after_trial", "params": {"keys": ["C"], "degrees": ["1", "2", "3"]}}),
        ),
        entry(
            ExerciseKind::IntervalComparison,
            "Two intervals play one after the other (melodic or harmonic); answer which is larger, or that they are the same. `differences` are the semitone gaps used (0 = same; smaller gaps are harder). `shared` picks per question which note they share: first (same starting note, easiest), second (same last note), near (starts a semitone to a minor third apart) or none (hardest). pairs_from_pool: both intervals come from `intervals`, always two different ones, and there is no Same answer.",
            schema_of::<IntervalComparisonParams>(),
            json!({"type": "interval_comparison", "count": 24, "params": {"differences": [0, 1, 2], "directions": ["ascending"]}}),
        ),
        entry(
            ExerciseKind::RestingTone,
            "Find home (Gordon's resting tone): a phrase that outlines a tonality without a cadence stops mid-way; after a breath, sing the resting tone (answer: sing, any octave) or name it (answer: name: do major, la minor, re Dorian, mi Phrygian, fa Lydian, sol Mixolydian, ti Locrian). modes are scale ids (major, natural_minor, harmonic_minor, dorian, phrygian, lydian, mixolydian, locrian).",
            schema_of::<RestingToneParams>(),
            json!({"type": "resting_tone", "count": 10, "params": {"modes": ["major", "natural_minor"]}}),
        ),
        entry(
            ExerciseKind::TonalPattern,
            "Tonal pattern echo (Music Learning Theory): after the tonic chord a 2–5 note pattern outlining a function (tonic, dominant, subdominant, or a two-note tendency resolution) plays on an even pulse; a silent breath (breath_beats), then echo it. passes: teaching_then_evaluation (each pattern twice, first with the tonic sounding under the echo, then alone), teaching or evaluation. syllables: neutral (on \"bum\"), solfege (syllables shown) or translate (heard on bum, echoed in solfège; shown afterwards).",
            schema_of::<TonalPatternParams>(),
            json!({"type": "tonal_pattern", "count": 12, "params": {"functions": ["tonic", "dominant"], "lengths": [3]}}),
        ),
        entry(
            ExerciseKind::PatternDialogue,
            "The I–V dialogue: the app sings a tonic or dominant pattern. stage first_pitch: sing back only its first note (the starting pitch of a dictation). same_function: answer with a different pattern of the same function. opposite_function: answer tonic with dominant, dominant with tonic. Free answers are graded on the notes' pitch classes (3–5 notes from the asked function).",
            schema_of::<PatternDialogueParams>(),
            json!({"type": "pattern_dialogue", "count": 10, "params": {"stage": "opposite_function"}}),
        ),
        entry(
            ExerciseKind::NBack,
            "Musical N-back for tonal working memory: a slow stream of scale degrees (degrees, one every ioi_ms) over a quiet tonic. mode recognize: tap Match when the note is the same as n back (scored as d′ like a detection stream; a lesson passes on d′). mode sing: a canon, singing each note n behind the app while the next sounds (headphones needed). stimuli tones is the aptitude screening: tones from `classes` over `octaves` (three or more) with no key, drone or cadence, a match being the same tone; stimuli letters uses spoken letters from the speech folder and falls back to tones when they are missing (the stored stimulus says which ran, with `fallback`).",
            schema_of::<NBackParams>(),
            json!({"type": "n_back", "count": 2, "params": {"n": 2, "length": 24}}),
        ),
        entry(
            ExerciseKind::PitchMemoryMatch,
            "Implicit note memory (Van Hedger): a sine tone from a 27-step ladder (471.58–547.99 Hz, 10 cents a step; targets the odd steps 5–23), a 2 s mask, then the learner finds it again from step 1, 3, 25 or 27 with the arrow keys (Shift for two steps), hearing each step. Metrics: error_steps and error_cents (signed, positive is high) and their absolute values. No right or wrong; run as an assessment. Reference: later-successful adult learners 2.75 steps (27.5 cents), lifelong AP possessors 2.44.",
            schema_of::<PitchMemoryMatchParams>(),
            json!({"type": "pitch_memory_match", "count": 20, "assessment": true}),
        ),
        entry(
            ExerciseKind::Span,
            "Auditory span: a sequence plays (spoken digits from the speech folder, or tones on a nine-key pad when the digits are missing or stimuli is tones), then the learner types the digits or plays the tones back. per_length sequences at each length from start_length; the ladder stops when every sequence at one length is wrong. One trial is one ladder; metrics.span is the longest length recalled, metrics.ladder every sequence, metrics.mode what ran. Reference (spoken digits): successful adult learners 8.83, non-AP musicians 8.1, AP possessors 10.0.",
            schema_of::<SpanParams>(),
            json!({"type": "span", "count": 1, "assessment": true}),
        ),
        entry(
            ExerciseKind::PitchWalk,
            "Silent pitch walk (the Pitch Imagery Arrow Task, Gelding et al. 2015): the tonic chord, a starting note on do, mi, sol or the upper do, then arrows one per beat moving up or down the scale (jumps 1 a step, 2 a third, 3 a fourth). The first `heard` arrows sound their notes, the next `steps` (one drawn per trial) are moved in silence; then a probe tone: is it where the walk ended? Wrong probes are a scale step off (probe_change chromatic: a semitone). Nothing is sung, so it measures pitch imagery without the voice. Stimulus: silent_steps, heard_steps, arrows, start, end, match; group get_stats by stimulus.silent_steps for accuracy against walk length.",
            schema_of::<PitchWalkParams>(),
            json!({"type": "pitch_walk", "count": 20, "params": {"steps": [2, 3], "heard": 2}}),
        ),
        entry(
            ExerciseKind::MelodicUpdating,
            "Melodic updating (Chenette): a 3–8 note melody plays without notation; an instruction then asks for a change done in the head (operations: replace_degree \"every mi becomes fa\", replace_position, shift up or down a step, retrograde, omit), chained `changes` times, with think_beats of silence to rehearse; then sing the changed melody.",
            schema_of::<MelodicUpdatingParams>(),
            json!({"type": "melodic_updating", "count": 8, "params": {"notes": [4, 5], "changes": 1}}),
        ),
        entry(
            ExerciseKind::PatternMemory,
            "Tonal short-term memory without notation. task same_different: a pattern, a gap, then the same pattern or one with a note changed (change: step, chromatic or any; transpose plays the second in another key). task probe (Sternberg): a melody, then one note; was it in the melody?",
            schema_of::<PatternMemoryParams>(),
            json!({"type": "pattern_memory", "count": 16, "params": {"task": "same_different", "notes": [4]}}),
        ),
        entry(
            ExerciseKind::ErrorDetection,
            "Single-pass error detection: the score shows and the tonic sounds; after pre_audiation_s of silent reading the melody plays once (no replay) with one note a step (or a semitone) off; tap that note. melody works as in sight_sing.",
            schema_of::<ErrorDetectionParams>(),
            json!({"type": "error_detection", "count": 6, "params": {"pre_audiation_s": 15}}),
        ),
        entry(
            ExerciseKind::SyllableCheck,
            "Mislabeled syllables (a solfège Stroop): a melody plays under its syllables, one of them a neighbour's name; tap the wrong one.",
            schema_of::<SyllableCheckParams>(),
            json!({"type": "syllable_check", "count": 12, "params": {"notes": [5]}}),
        ),
        entry(
            ExerciseKind::CadenceId,
            "Cadences in context: a four-part phrase with lead_in chords before the cadence; name the ending: perfect_authentic (V–I, tonic on top), imperfect_authentic (V–I, third or fifth on top), plagal (IV–I), half (on V) or deceptive (V–vi). The result names the chords in Roman numerals.",
            schema_of::<CadenceIdParams>(),
            json!({"type": "cadence_id", "count": 16, "params": {"cadences": ["perfect_authentic", "half", "deceptive"]}}),
        ),
        entry(
            ExerciseKind::BassLine,
            "Bass and guide-tone singing: sing the roots (line: roots) or the 3rd–7th guide-tone line (line: guide_tones) of a progression (i_iv_v_i, i_vi_iv_v_i, i_iv_i_v_i, i_ii_v_i, i_vi_ii_v_i, ii_v_i7, turnaround7) shown in Roman numerals, with the chords and their bass (backing: with_bass), without the bass (without_bass), or with only the tonic given (tonic_only).",
            schema_of::<BassLineParams>(),
            json!({"type": "bass_line", "count": 6, "params": {"line": "roots", "backing": "without_bass"}}),
        ),
        entry(
            ExerciseKind::Harmony,
            "Sing one voice while the app plays the others, graded note by note like sight singing. Two kinds. (1) Fixed intervals, `source.parallel`: a melody (generated by default, or `given` by you, or from the `library`) is played and the singer adds a voice a diatonic third, fourth, fifth or sixth below or above it (or sings the melody over that voice), by ear unless `notation` is true; a tritone there becomes a perfect fourth (`avoid_tritone`, on by default). (2) Free part-writing, `source.given` (pieces you write into the session) or `source.library` (pieces from the melody library): two to four parts, e.g. a two-voice duet or SATB, sung exactly as written with any intervals, tritones and dissonances included. The singer sings `part` (0 = top, 3 = bass in SATB) from the staff; `play_parts` picks the voices the app plays (default all the others), e.g. only the bass first, then everything. A written piece: {title, key, time, tempo_bpm, parts: [{name: \"Soprano\", notes: \"G4:1 A4 B4 G4 | …\"}, …]}, the same JSON as add_library_pieces; use the library for material worth reusing. `practice: true` plays the singer's own part along and shows its notes; those attempts are stored as assisted (not in accuracy), so follow a practice block with one without. The key and the singer's first note are played first (`start`). Grade with {\"grading\": {\"tuning\": \"just\"}} to score each note against the pure interval over the lowest other voice.",
            schema_of::<HarmonyParams>(),
            json!({"type": "harmony", "count": 6, "params": {"source": {"parallel": {"voice": "fifth_below", "melody": {"generate": {"bars": 2, "max_leap": 2}}}}}}),
        ),
        entry(
            ExerciseKind::PitchDetect,
            "Fast note detection (Simple Speed). One trial is one stream: the target class is shown and sung (GM choir), then `tones` (default 16) follow, about `target_rate` (default 0.25) of them the target in varying octaves. The user presses on the target. Each tone's window is `window_ms` (default 1750) and the next tone follows whether or not they pressed. min_jump_semitones keeps successive tones apart. Scored by d′ (log-linear corrected) and median hit latency; targets more than an octave from the prompt are counted separately in metrics.far_hits. marks: summary (default), each, or none. Optional labels and spoken name (speech folder). Not a single right/wrong trial: metrics.d_prime is the score.",
            schema_of::<PitchDetectParams>(),
            json!({"type": "pitch_detect", "count": 4, "feedback": "after_trial", "params": {"window_ms": 1750, "min_jump_semitones": 13, "octaves": [3, 4, 5]}}),
        ),
        entry(
            ExerciseKind::ChordColorId,
            "Eguchi chord colours. Each trial plays one fixed voicing; the answer is its colour (red C E G, yellow C F A, blue B D G, black A C F, green D G B, orange E G C, purple F A C, pink G B D, brown G C E, then gray A C# E, tan D F# A, light_green E G# B, light_purple Bb D F, sky_blue Eb G Bb). `chords` lists the ids in use. Inversions are different colours on purpose. register_shift moves the voicing by octaves. single_note_after plays one note of the chord (or a white-key note) and stores that answer as a pitch_id trial with stimulus.context chord_extraction, so it counts toward note mastery. feedback errorless marks no error and still stores the true result. Pass rule for the course is spacing: 100% and 14 days before the next chord.",
            schema_of::<ChordColorParams>(),
            json!({"type": "chord_color_id", "count": 20, "feedback": "errorless", "params": {"chords": ["red", "yellow"]}}),
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
        let profile = ap_profile(&db).unwrap();
        assert!(profile["benchmark"].is_null());
        assert_eq!(profile["scoring"]["answer_limit_ms"], json!(5000));
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
