//! Scoring of the absolute-pitch tests (benchmark, weekly, transfer, UCSF-style), as the
//! dashboard and `get_ap_profile` report them. Everything is computed from the stored trials, so
//! a threshold can change without re-running anything.

use std::collections::BTreeMap;

use anyhow::Result;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use ts_rs::TS;

use crate::courses::tagged_trials;
use crate::stats::median;
use crate::storage::{Db, TrialRow};

/// Answers count as automatic when correct within this time of the tone's onset (the tone itself
/// lasts 1 s, so this is 1 s after it ends).
pub const IN_TIME_MS: f64 = 2000.0;
/// Twelve names: the chance level of a naming test.
pub const CHANCE: f64 = 1.0 / 12.0;
/// Named notes heard less than this long before a test can lift it.
const CONTAMINATION_MINUTES: f64 = 360.0;

/// One run of a test (its trials in one session).
#[derive(Clone, Debug, Serialize, Deserialize, TS)]
pub struct TestRun {
    pub session_id: String,
    pub at: String,
    /// Naming trials, Shepard tones apart (they are in `by_timbre` and `shepard`).
    pub trials: u32,
    /// Correct within the answer limit (5 s); timeouts count as wrong.
    pub accuracy: f64,
    /// 95% interval of `accuracy` (Wilson).
    pub accuracy_low: f64,
    pub accuracy_high: f64,
    /// Correct within 2 s of the tone's onset.
    pub in_time: f64,
    pub timeouts: u32,
    /// UCSF-style points as a share of the maximum (1 exact, 0.75 a semitone off, 0 timeouts).
    pub ucsf_points: f64,
    /// Mean absolute error in semitones over answered trials, folded to the nearest octave
    /// (0 perfect, 3 chance).
    pub mad: Option<f64>,
    /// Mean signed error in semitones (answer minus note); far from 0 on every note suggests the
    /// tuning or sample rate, not the ear.
    pub mean_signed_error: Option<f64>,
    /// Median response time of correct answers only, so fast guesses do not flatter it.
    pub median_correct_rt_ms: Option<f64>,
    /// Accuracy per timbre.
    pub by_timbre: Vec<TimbreAccuracy>,
    /// Shepard-tone accuracy (note name without height), when the run had them.
    pub shepard: Option<f64>,
    pub sung: Option<SungSummary>,
    /// Minutes between the last named note heard and the start of this run.
    pub named_note_minutes_before: Option<f64>,
    /// Plain-language cautions about this run.
    pub notes: Vec<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize, TS)]
pub struct TimbreAccuracy {
    pub timbre: String,
    pub trials: u32,
    pub accuracy: f64,
}

/// Notes sung from memory.
#[derive(Clone, Debug, Serialize, Deserialize, TS)]
pub struct SungSummary {
    pub trials: u32,
    /// Sung within the category tolerance of the asked note, in any octave.
    pub accuracy: f64,
    pub median_abs_cents: Option<f64>,
    /// Median error of the interval between consecutive sung notes, in cents.
    pub median_interval_abs_cents: Option<f64>,
    /// `median_interval_abs_cents / median_abs_cents`. About 1.4 when each note is sung on its
    /// own; well below 1 when notes are worked out from the one before.
    pub chaining_ratio: Option<f64>,
    pub chaining_likely: bool,
}

/// 95% Wilson interval of a proportion.
pub fn wilson(k: f64, n: f64) -> (f64, f64) {
    if n <= 0.0 {
        return (0.0, 1.0);
    }
    let z = 1.96f64;
    let p = k / n;
    let d = 1.0 + z * z / n;
    let c = (p + z * z / (2.0 * n)) / d;
    let h = z * (p * (1.0 - p) / n + z * z / (4.0 * n * n)).sqrt() / d;
    ((c - h).max(0.0), (c + h).min(1.0))
}

fn is_shepard(t: &TrialRow) -> bool {
    t.stimulus.get("timbre").and_then(Value::as_str) == Some("shepard")
}

pub(crate) fn is_out_of_set(t: &TrialRow) -> bool {
    t.stimulus
        .get("out_of_set")
        .and_then(Value::as_bool)
        .unwrap_or(false)
}

fn metric(t: &TrialRow, k: &str) -> Option<f64> {
    t.metrics.get(k).and_then(Value::as_f64)
}

fn accuracy(rows: &[&TrialRow]) -> f64 {
    rows.iter().filter(|t| t.correct == Some(true)).count() as f64 / rows.len().max(1) as f64
}

/// The summary of one run's trials (any mix of naming and sung trials).
pub fn summarise(session_id: &str, rows: &[&TrialRow]) -> TestRun {
    let named: Vec<&TrialRow> = rows
        .iter()
        .copied()
        .filter(|t| t.kind == "pitch_id" && t.correct.is_some() && !is_out_of_set(t))
        .collect();
    let main: Vec<&TrialRow> = named.iter().copied().filter(|t| !is_shepard(t)).collect();
    let shepard: Vec<&TrialRow> = named.iter().copied().filter(|t| is_shepard(t)).collect();
    let n = main.len() as f64;
    let correct = main.iter().filter(|t| t.correct == Some(true)).count() as f64;
    let (lo, hi) = wilson(correct, n);
    let in_time = main
        .iter()
        .filter(|t| t.correct == Some(true) && t.rt_ms.map(|r| r <= IN_TIME_MS).unwrap_or(false))
        .count() as f64;
    let timeouts = main
        .iter()
        .filter(|t| t.metrics.get("timeout").and_then(Value::as_bool) == Some(true))
        .count() as u32;
    let points: f64 = main.iter().filter_map(|t| metric(t, "ucsf_points")).sum();
    let errs: Vec<f64> = main
        .iter()
        .filter_map(|t| metric(t, "class_error"))
        .collect();
    let mean = |v: &[f64]| (!v.is_empty()).then(|| v.iter().sum::<f64>() / v.len() as f64);
    let mad = mean(&errs.iter().map(|e| e.abs()).collect::<Vec<_>>());
    let signed = mean(&errs);
    let mut timbres: BTreeMap<String, (u32, u32)> = BTreeMap::new();
    for t in &named {
        let k = t
            .stimulus
            .get("timbre")
            .and_then(Value::as_str)
            .unwrap_or("?")
            .to_string();
        let e = timbres.entry(k).or_default();
        e.0 += 1;
        e.1 += (t.correct == Some(true)) as u32;
    }
    let sung_rows: Vec<&TrialRow> = rows
        .iter()
        .copied()
        .filter(|t| t.kind == "sing_pitch" && t.correct.is_some())
        .collect();
    let sung = (!sung_rows.is_empty()).then(|| sung_summary(&sung_rows));
    let first = rows.iter().min_by(|a, b| a.started_at.cmp(&b.started_at));
    let named_before = first
        .and_then(|t| t.secs_since_labeled_reference)
        .map(|s| s / 60.0);

    let mut notes = Vec::new();
    if n >= 12.0 && lo <= CHANCE {
        notes.push(format!(
            "Not clearly above chance yet: {:.0}% (95% range {:.0}–{:.0}%) against 8% by guessing.",
            correct / n * 100.0,
            lo * 100.0,
            hi * 100.0
        ));
    }
    if let Some(m) = signed {
        let lean = errs.iter().filter(|e| **e != 0.0).collect::<Vec<_>>();
        let same_side = lean.iter().filter(|e| e.signum() == m.signum()).count() as f64
            / lean.len().max(1) as f64;
        if errs.len() >= 24 && m.abs() >= 0.75 && same_side >= 0.8 {
            notes.push(format!(
                "Answers lean {:.1} semitones {} on almost every note. Check the audio setup first (a 44.1 vs 48 kHz sample-rate mismatch shifts everything by about 1.5 semitones) before reading this as the ear.",
                m.abs(),
                if m > 0.0 { "high" } else { "low" }
            ));
        }
    }
    if let Some(m) = named_before.filter(|m| *m < CONTAMINATION_MINUTES) {
        notes.push(format!(
            "Named notes were heard {} before this run, which can lift it. Tests are meant to come first in the day.",
            if m < 90.0 { format!("{m:.0} min") } else { format!("{:.1} h", m / 60.0) }
        ));
    }
    if sung.as_ref().map(|s| s.chaining_likely).unwrap_or(false) {
        notes.push("The sung notes look worked out from one another: the steps between them are much more accurate than the notes themselves.".into());
    }
    TestRun {
        session_id: session_id.to_string(),
        at: first.map(|t| t.started_at.clone()).unwrap_or_default(),
        trials: n as u32,
        accuracy: correct / n.max(1.0),
        accuracy_low: lo,
        accuracy_high: hi,
        in_time: in_time / n.max(1.0),
        timeouts,
        ucsf_points: points / n.max(1.0),
        mad,
        mean_signed_error: signed,
        median_correct_rt_ms: median(
            main.iter()
                .filter(|t| t.correct == Some(true))
                .filter_map(|t| t.rt_ms)
                .collect(),
        ),
        by_timbre: timbres
            .into_iter()
            .map(|(timbre, (k, c))| TimbreAccuracy {
                timbre,
                trials: k,
                accuracy: c as f64 / k as f64,
            })
            .collect(),
        shepard: (!shepard.is_empty()).then(|| accuracy(&shepard)),
        sung,
        named_note_minutes_before: named_before,
        notes,
    }
}

/// Wraps cents to −600..600 (the nearest octave).
fn fold_cents(c: f64) -> f64 {
    (c + 600.0).rem_euclid(1200.0) - 600.0
}

fn sung_summary(rows: &[&TrialRow]) -> SungSummary {
    let mut ordered: Vec<&TrialRow> = rows.to_vec();
    ordered.sort_by_key(|t| (t.block_idx, t.trial_idx, t.attempt));
    let errs: Vec<(u32, u32, f64)> = ordered
        .iter()
        .filter_map(|t| metric(t, "cents_error").map(|e| (t.block_idx, t.trial_idx, e)))
        .collect();
    let abs: Vec<f64> = errs.iter().map(|e| e.2.abs()).collect();
    // Steps between notes sung one after the other.
    let steps: Vec<f64> = errs
        .windows(2)
        .filter(|w| w[0].0 == w[1].0 && w[1].1 == w[0].1 + 1)
        .map(|w| fold_cents(w[1].2 - w[0].2).abs())
        .collect();
    let med_abs = median(abs);
    let med_step = median(steps.clone());
    let ratio = match (med_step, med_abs) {
        (Some(s), Some(a)) if a > 0.0 => Some(s / a),
        _ => None,
    };
    SungSummary {
        trials: rows.len() as u32,
        accuracy: accuracy(rows),
        median_abs_cents: med_abs,
        median_interval_abs_cents: med_step,
        chaining_ratio: ratio,
        chaining_likely: steps.len() >= 6
            && med_abs.map(|a| a >= 100.0).unwrap_or(false)
            && ratio.map(|r| r < 0.6).unwrap_or(false),
    }
}

/// Every run of a test course, oldest first.
pub fn test_runs(db: &Db, course: &str) -> Result<Vec<TestRun>> {
    let trials = tagged_trials(db, &format!("course.{course}"), 20_000)?;
    let mut by_session: BTreeMap<String, Vec<&TrialRow>> = BTreeMap::new();
    for t in &trials {
        by_session.entry(t.session_id.clone()).or_default().push(t);
    }
    let mut out: Vec<TestRun> = by_session
        .into_iter()
        .map(|(sid, rows)| summarise(&sid, &rows))
        .filter(|r| r.trials > 0 || r.sung.is_some() || r.shepard.is_some())
        .collect();
    out.sort_by(|a, b| a.at.cmp(&b.at));
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::storage::tests_support::sample_trial;

    #[test]
    fn wilson_interval_brackets_the_rate() {
        let (lo, hi) = wilson(15.0, 60.0);
        assert!(lo < 0.25 && hi > 0.25);
        assert!(
            (lo - 0.158).abs() < 0.01 && (hi - 0.372).abs() < 0.01,
            "{lo} {hi}"
        );
        assert_eq!(wilson(0.0, 0.0), (0.0, 1.0));
    }

    fn naming(i: u32, correct: bool, rt: Option<f64>, err: i64, timbre: &str) -> TrialRow {
        let mut t = sample_trial("s", i, correct);
        t.rt_ms = rt;
        t.stimulus = serde_json::json!({ "pitch_class": "C", "timbre": timbre });
        t.metrics = if rt.is_none() {
            serde_json::json!({ "timeout": true })
        } else {
            let pts = match err.abs() {
                0 => 1.0,
                1 => 0.75,
                _ => 0.0,
            };
            serde_json::json!({ "timeout": false, "class_error": err, "ucsf_points": pts })
        };
        t
    }

    fn sung(i: u32, cents: f64) -> TrialRow {
        let mut t = sample_trial("s", 100 + i, cents.abs() <= 50.0);
        t.kind = "sing_pitch".into();
        t.block_idx = 1;
        t.trial_idx = i;
        t.metrics = serde_json::json!({ "cents_error": cents });
        t
    }

    #[test]
    fn a_run_scores_time_timeouts_and_shepard_apart() {
        let mut rows = vec![
            naming(0, true, Some(1200.0), 0, "gm:0"),
            naming(1, true, Some(3500.0), 0, "gm:0"),
            naming(2, false, Some(1800.0), 1, "gm:0"),
            naming(3, false, None, 0, "gm:0"),
            naming(4, true, Some(1500.0), 0, "shepard"),
            naming(5, false, Some(1500.0), 4, "shepard"),
        ];
        rows[0].secs_since_labeled_reference = Some(600.0);
        let refs: Vec<&TrialRow> = rows.iter().collect();
        let r = summarise("s", &refs);
        assert_eq!(r.trials, 4);
        assert_eq!(r.accuracy, 0.5);
        assert_eq!(r.in_time, 0.25);
        assert_eq!(r.timeouts, 1);
        // 1 + 1 + 0.75 + 0 (the timeout) over four.
        assert!((r.ucsf_points - 2.75 / 4.0).abs() < 1e-9);
        // The timeout has no error to average.
        assert!((r.mad.unwrap() - 1.0 / 3.0).abs() < 1e-9);
        assert_eq!(r.median_correct_rt_ms, Some(2350.0));
        assert_eq!(r.shepard, Some(0.5));
        assert!(
            r.notes
                .iter()
                .any(|n| n.contains("Named notes were heard 10 min"))
        );
    }

    #[test]
    fn chained_singing_is_flagged() {
        // Every note 250 cents off the same way: the steps between them are right.
        let chained: Vec<TrialRow> = (0..12)
            .map(|i| sung(i, 250.0 + (i % 3) as f64 * 10.0))
            .collect();
        let s = sung_summary(&chained.iter().collect::<Vec<_>>());
        assert!(s.chaining_likely, "{s:?}");
        // Errors that come and go note by note are not chaining.
        let own: Vec<TrialRow> = (0..12)
            .map(|i| sung(i, if i % 2 == 0 { 220.0 } else { -260.0 }))
            .collect();
        let s = sung_summary(&own.iter().collect::<Vec<_>>());
        assert!(!s.chaining_likely, "{s:?}");
        assert!(s.chaining_ratio.unwrap() > 1.0);
    }

    #[test]
    fn folding_keeps_the_short_way_round() {
        assert_eq!(fold_cents(700.0), -500.0);
        assert_eq!(fold_cents(-650.0), 550.0);
        assert_eq!(fold_cents(30.0), 30.0);
    }
}
