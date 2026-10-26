//! The aptitude profile: the screening battery's numbers over time (implicit note memory, tone or
//! letter N-back, auditory span), each beside the research's reference lines.
//!
//! Everything is computed from stored trials, so it survives restarts and trends across runs. The
//! coach routes from the raw numbers; `capacity_band` only restates the literature's thresholds.

use std::collections::BTreeMap;

use anyhow::Result;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use ts_rs::TS;

use crate::schema::ExerciseKind;
use crate::stats::median;
use crate::storage::{Db, TrialFilter, TrialRow};

/// A research figure to read a result against.
#[derive(Clone, Debug, Serialize, Deserialize, TS)]
pub struct ReferenceLine {
    pub group: String,
    pub value: f64,
    pub se: Option<f64>,
    pub source: String,
}

fn line(group: &str, value: f64, se: Option<f64>) -> ReferenceLine {
    ReferenceLine {
        group: group.into(),
        value,
        se,
        source: "Van Hedger et al. 2015, 2019 (docs/AP_Trainer_Literature_search.md §1)".into(),
    }
}

/// Implicit note memory: mean absolute error in steps (10 cents each).
pub fn inm_reference() -> Vec<ReferenceLine> {
    vec![
        line("Adults who later reached AP thresholds", 2.75, Some(0.30)),
        line("Lifelong AP possessors", 2.44, Some(0.19)),
    ]
}

/// N-back d′ at 2 and 3 back.
pub fn n_back_reference(n: u32) -> Vec<ReferenceLine> {
    match n {
        2 => vec![
            line("Adults who later reached AP thresholds", 4.05, Some(0.17)),
            line("University undergraduates", 3.37, Some(0.16)),
        ],
        3 => vec![
            line("Adults who later reached AP thresholds", 3.52, Some(0.32)),
            line("University undergraduates", 2.27, Some(0.19)),
        ],
        _ => Vec::new(),
    }
}

/// Spoken digit span.
pub fn span_reference() -> Vec<ReferenceLine> {
    vec![
        line("Adults who later reached AP thresholds", 8.83, Some(0.65)),
        line("Non-AP musicians", 8.1, None),
        line("Lifelong AP possessors", 10.0, None),
    ]
}

/// The literature's routing thresholds (§7): near-ceiling echoic retention is at most this mean
/// note-memory error, with 3-back d′ at least `CEILING_3_BACK`.
pub const CEILING_INM_CENTS: f64 = 27.5;
pub const CEILING_3_BACK: f64 = 3.5;

#[derive(Clone, Debug, Serialize, Deserialize, TS)]
pub struct InmRun {
    pub session_id: String,
    pub at: String,
    pub trials: u32,
    pub mean_abs_error_steps: f64,
    pub mean_abs_error_cents: f64,
    /// Positive: tones were found above where they were.
    pub mean_signed_error_cents: f64,
    pub median_abs_error_steps: f64,
    /// Searches answered without moving from the start.
    pub unmoved: u32,
}

#[derive(Clone, Debug, Serialize, Deserialize, TS)]
pub struct NBackRun {
    pub session_id: String,
    pub at: String,
    pub n: u32,
    pub d_prime: f64,
    pub hits: u32,
    pub misses: u32,
    pub false_alarms: u32,
    pub correct_rejections: u32,
    /// `tones` or `letters`: what actually ran.
    pub stimuli: String,
    /// Letters were asked for but their files were missing, so tones ran.
    pub fallback: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize, TS)]
pub struct SpanRun {
    pub session_id: String,
    pub at: String,
    pub span: u32,
    /// `digits` (spoken) or `tones` (the pad).
    pub mode: String,
    pub fallback: bool,
    pub sequences: u32,
    pub sequences_correct: u32,
}

#[derive(Clone, Debug, Serialize, Deserialize, TS)]
pub struct AptitudeLatest {
    pub inm_mean_abs_error_cents: Option<f64>,
    pub inm_mean_abs_error_steps: Option<f64>,
    pub n_back_2_d_prime: Option<f64>,
    pub n_back_3_d_prime: Option<f64>,
    pub span: Option<u32>,
    pub span_mode: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize, TS)]
pub struct AptitudeProfile {
    /// Every run, oldest first.
    pub inm: Vec<InmRun>,
    pub n_back: Vec<NBackRun>,
    pub span: Vec<SpanRun>,
    pub latest: AptitudeLatest,
    pub inm_reference: Vec<ReferenceLine>,
    pub n_back_2_reference: Vec<ReferenceLine>,
    pub n_back_3_reference: Vec<ReferenceLine>,
    pub span_reference: Vec<ReferenceLine>,
    /// `near_ceiling`, `mixed` or `lower` against the §7 thresholds (INM ≤ 27.5 cents and 3-back
    /// d′ ≥ 3.5), from the latest runs; `None` until both have run. A restatement of the raw
    /// numbers, not a routing decision.
    pub capacity_band: Option<String>,
    pub notes: Vec<String>,
}

fn rows(db: &Db, kind: ExerciseKind) -> Result<Vec<TrialRow>> {
    db.query_trials(
        &TrialFilter {
            types: vec![kind.as_str().into()],
            assisted: Some(false),
            ..Default::default()
        },
        20_000,
        0,
    )
}

/// Trials grouped by session, each group oldest first, the groups in the order of their first
/// trial.
fn by_session(rows: &[TrialRow]) -> Vec<(String, Vec<&TrialRow>)> {
    let mut groups: BTreeMap<String, Vec<&TrialRow>> = BTreeMap::new();
    for r in rows {
        groups.entry(r.session_id.clone()).or_default().push(r);
    }
    let mut out: Vec<(String, Vec<&TrialRow>)> = groups
        .into_iter()
        .map(|(s, mut v)| {
            v.sort_by(|a, b| a.started_at.cmp(&b.started_at));
            (s, v)
        })
        .collect();
    out.sort_by(|a, b| a.1[0].started_at.cmp(&b.1[0].started_at));
    out
}

fn num(v: &Value, key: &str) -> Option<f64> {
    v.get(key).and_then(Value::as_f64)
}

pub fn inm_runs(rows: &[TrialRow]) -> Vec<InmRun> {
    by_session(rows)
        .into_iter()
        .filter_map(|(sid, trials)| {
            let errs: Vec<(f64, f64)> = trials
                .iter()
                .filter_map(|t| {
                    Some((
                        num(&t.metrics, "error_steps")?,
                        num(&t.metrics, "error_cents")?,
                    ))
                })
                .collect();
            if errs.is_empty() {
                return None;
            }
            let n = errs.len() as f64;
            let abs_steps: Vec<f64> = errs.iter().map(|e| e.0.abs()).collect();
            Some(InmRun {
                session_id: sid,
                at: trials[0].started_at.clone(),
                trials: errs.len() as u32,
                mean_abs_error_steps: abs_steps.iter().sum::<f64>() / n,
                mean_abs_error_cents: errs.iter().map(|e| e.1.abs()).sum::<f64>() / n,
                mean_signed_error_cents: errs.iter().map(|e| e.1).sum::<f64>() / n,
                median_abs_error_steps: median(abs_steps.clone()).unwrap_or(0.0),
                unmoved: trials
                    .iter()
                    .filter(|t| num(&t.metrics, "moves") == Some(0.0))
                    .count() as u32,
            })
        })
        .collect()
}

/// Only the screening variant (tones or letters with no key) counts; degree streams are
/// relative-pitch training.
pub fn n_back_runs(rows: &[TrialRow]) -> Vec<NBackRun> {
    let mut out: Vec<NBackRun> = rows
        .iter()
        .filter_map(|t| {
            let stimuli = t.stimulus.get("stimuli")?.as_str()?.to_string();
            Some(NBackRun {
                session_id: t.session_id.clone(),
                at: t.started_at.clone(),
                n: num(&t.stimulus, "n")? as u32,
                d_prime: num(&t.metrics, "d_prime")?,
                hits: num(&t.metrics, "hits").unwrap_or(0.0) as u32,
                misses: num(&t.metrics, "misses").unwrap_or(0.0) as u32,
                false_alarms: num(&t.metrics, "false_alarms").unwrap_or(0.0) as u32,
                correct_rejections: num(&t.metrics, "correct_rejections").unwrap_or(0.0) as u32,
                stimuli,
                fallback: t
                    .stimulus
                    .get("fallback")
                    .and_then(Value::as_bool)
                    .unwrap_or(false),
            })
        })
        .collect();
    out.sort_by(|a, b| a.at.cmp(&b.at));
    out
}

pub fn span_runs(rows: &[TrialRow]) -> Vec<SpanRun> {
    let mut out: Vec<SpanRun> = rows
        .iter()
        .filter_map(|t| {
            Some(SpanRun {
                session_id: t.session_id.clone(),
                at: t.started_at.clone(),
                span: num(&t.metrics, "span")? as u32,
                mode: t
                    .metrics
                    .get("mode")
                    .and_then(Value::as_str)
                    .unwrap_or("tones")
                    .to_string(),
                fallback: t
                    .metrics
                    .get("fallback")
                    .and_then(Value::as_bool)
                    .unwrap_or(false),
                sequences: num(&t.metrics, "sequences").unwrap_or(0.0) as u32,
                sequences_correct: num(&t.metrics, "sequences_correct").unwrap_or(0.0) as u32,
            })
        })
        .collect();
    out.sort_by(|a, b| a.at.cmp(&b.at));
    out
}

/// The band the §7 thresholds put the latest runs in.
pub fn capacity_band(inm_cents: Option<f64>, three_back: Option<f64>) -> Option<&'static str> {
    let inm_ok = inm_cents? <= CEILING_INM_CENTS;
    let nb_ok = three_back? >= CEILING_3_BACK;
    Some(match (inm_ok, nb_ok) {
        (true, true) => "near_ceiling",
        (false, false) => "lower",
        _ => "mixed",
    })
}

pub fn aptitude_profile(db: &Db) -> Result<AptitudeProfile> {
    let inm = inm_runs(&rows(db, ExerciseKind::PitchMemoryMatch)?);
    let n_back = n_back_runs(&rows(db, ExerciseKind::NBack)?);
    let span = span_runs(&rows(db, ExerciseKind::Span)?);
    let last_n = |n: u32| n_back.iter().rev().find(|r| r.n == n).map(|r| r.d_prime);
    let latest = AptitudeLatest {
        inm_mean_abs_error_cents: inm.last().map(|r| r.mean_abs_error_cents),
        inm_mean_abs_error_steps: inm.last().map(|r| r.mean_abs_error_steps),
        n_back_2_d_prime: last_n(2),
        n_back_3_d_prime: last_n(3),
        span: span.last().map(|r| r.span),
        span_mode: span.last().map(|r| r.mode.clone()),
    };
    let mut notes = Vec::new();
    if inm.is_empty() && n_back.is_empty() && span.is_empty() {
        notes.push("The screening has not run yet (course ap_screening).".into());
    }
    if span.last().map(|r| r.mode == "tones").unwrap_or(false) {
        notes.push("Span ran on the tone pad (no spoken digits in the speech folder); the digit-span reference lines are for spoken digits, so read them loosely.".into());
    }
    if n_back.iter().rev().take(2).any(|r| r.fallback) {
        notes.push(
            "Letter N-back was asked for but the letter files are missing, so tones ran.".into(),
        );
    }
    if let Some(r) = inm.last()
        && r.unmoved * 4 >= r.trials.max(1)
    {
        notes.push(format!(
            "{} of {} note-memory searches were answered without moving from the start; those errors say little about memory.",
            r.unmoved, r.trials
        ));
    }
    let band = capacity_band(latest.inm_mean_abs_error_cents, latest.n_back_3_d_prime);
    if band.is_none() && (!inm.is_empty() || !n_back.is_empty()) {
        notes.push("capacity_band needs both a note-memory run and a 3-back run.".into());
    }
    Ok(AptitudeProfile {
        inm,
        n_back,
        span,
        latest,
        inm_reference: inm_reference(),
        n_back_2_reference: n_back_reference(2),
        n_back_3_reference: n_back_reference(3),
        span_reference: span_reference(),
        capacity_band: band.map(String::from),
        notes,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::storage::tests_support::sample_trial;
    use serde_json::json;

    fn inm_trial(session: &str, i: u32, steps: i64) -> TrialRow {
        let mut t = sample_trial(session, i, false);
        t.kind = "pitch_memory_match".into();
        t.correct = None;
        t.metrics = json!({ "error_steps": steps, "error_cents": steps as f64 * 10.0, "moves": 5 });
        t
    }

    #[test]
    fn note_memory_runs_average_absolute_and_signed_error() {
        let rows = vec![
            inm_trial("a", 0, 3),
            inm_trial("a", 1, -1),
            inm_trial("a", 2, 2),
        ];
        let runs = inm_runs(&rows);
        assert_eq!(runs.len(), 1);
        assert!((runs[0].mean_abs_error_steps - 2.0).abs() < 1e-9);
        assert!((runs[0].mean_abs_error_cents - 20.0).abs() < 1e-9);
        assert!((runs[0].mean_signed_error_cents - 40.0 / 3.0).abs() < 1e-9);
    }

    #[test]
    fn the_band_restates_the_thresholds() {
        assert_eq!(capacity_band(Some(24.0), Some(3.6)), Some("near_ceiling"));
        assert_eq!(capacity_band(Some(40.0), Some(2.0)), Some("lower"));
        assert_eq!(capacity_band(Some(24.0), Some(2.0)), Some("mixed"));
        assert_eq!(capacity_band(None, Some(3.6)), None);
    }

    #[test]
    fn profile_trends_runs_from_the_database() {
        let db = Db::open_in_memory().unwrap();
        let spec = crate::schema::SessionSpec {
            title: "t".into(),
            protocol: Some("ap_screening".into()),
            planned_minutes: None,
            notes: None,
            seed: None,
            blocks: vec![],
        };
        for (day, err) in [(1, 4), (2, 2)] {
            let sid = db.start_new_session(&spec, "test").unwrap();
            for i in 0..4 {
                let mut t = inm_trial(&sid, i, if i % 2 == 0 { err } else { -err });
                t.started_at = format!("2026-10-{day:02}T10:00:0{i}Z");
                db.insert_trial(&t).unwrap();
            }
            let mut nb = sample_trial(&sid, 10, false);
            nb.kind = "n_back".into();
            nb.correct = None;
            nb.started_at = format!("2026-10-{day:02}T10:05:00Z");
            nb.stimulus = json!({ "n": 3, "stimuli": "tones", "fallback": false });
            nb.metrics = json!({ "d_prime": 2.0 + day as f64, "hits": 5, "misses": 2, "false_alarms": 1, "correct_rejections": 20 });
            db.insert_trial(&nb).unwrap();
            let mut sp = sample_trial(&sid, 11, false);
            sp.kind = "span".into();
            sp.correct = None;
            sp.started_at = format!("2026-10-{day:02}T10:09:00Z");
            sp.metrics = json!({ "span": 5 + day, "mode": "tones", "fallback": true, "sequences": 8, "sequences_correct": 6 });
            db.insert_trial(&sp).unwrap();
        }
        let p = aptitude_profile(&db).unwrap();
        assert_eq!(p.inm.len(), 2);
        assert!(p.inm[0].at < p.inm[1].at);
        assert!((p.inm[1].mean_abs_error_cents - 20.0).abs() < 1e-9);
        assert_eq!(p.latest.n_back_3_d_prime, Some(4.0));
        assert_eq!(p.latest.span, Some(7));
        assert_eq!(p.span[0].mode, "tones");
        // 20 cents and d′ 4: both past the thresholds.
        assert_eq!(p.capacity_band.as_deref(), Some("near_ceiling"));
        assert!(p.notes.iter().any(|n| n.contains("tone pad")));
        assert_eq!(p.n_back_3_reference[0].value, 3.52);
    }
}
