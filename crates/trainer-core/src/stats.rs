//! Aggregate statistics over the trial log, grouped by any stored field. This is what the LLM
//! reads instead of thousands of raw trials.

use std::collections::BTreeMap;

use anyhow::{Result, anyhow};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use ts_rs::TS;

use crate::storage::{Db, TrialFilter, TrialRow};

#[derive(Clone, Debug, Default, Serialize, Deserialize, schemars::JsonSchema, TS)]
pub struct StatsQuery {
    #[serde(default)]
    pub filter: TrialFilter,
    /// Fields to group by: `type`, `protocol`, `pool`, `feedback_mode`, `assisted`, `assessment`,
    /// `session_id`, `day`, `week`, `block_idx`, `correct`, or a dotted path into the stored JSON:
    /// `stimulus.pitch_class`, `stimulus.timbre`, `stimulus.interval`, `metrics.sung_octave`,
    /// `response.answer`, …
    #[serde(default)]
    pub group_by: Vec<String>,
    /// Maximum number of trials to aggregate (newest first). Default 5000.
    #[serde(default)]
    pub max_trials: Option<u32>,
}

#[derive(Clone, Debug, Serialize, Deserialize, TS)]
pub struct StatsRow {
    #[ts(type = "Record<string, unknown>")]
    pub group: BTreeMap<String, Value>,
    pub trials: usize,
    pub graded: usize,
    pub correct: usize,
    pub accuracy: Option<f64>,
    pub mean_score: Option<f64>,
    pub median_rt_ms: Option<f64>,
    /// Median |cents error| of sung responses (single-note `cents_error` or per-melody mean).
    pub median_abs_cents: Option<f64>,
    /// Mean signed cents error (positive = sharp).
    pub mean_signed_cents: Option<f64>,
    /// Mean signed category error for identification (answer − actual, semitones).
    pub mean_signed_error: Option<f64>,
}

pub fn field_value(t: &TrialRow, field: &str) -> Value {
    let simple = match field {
        "type" => Some(Value::from(t.kind.clone())),
        "protocol" => Some(t.protocol.clone().map(Value::from).unwrap_or(Value::Null)),
        "pool" => Some(t.pool.clone().map(Value::from).unwrap_or(Value::Null)),
        "feedback_mode" => Some(Value::from(t.feedback_mode.clone())),
        "assisted" => Some(Value::from(t.assisted)),
        "assessment" => Some(Value::from(t.assessment)),
        "session_id" => Some(Value::from(t.session_id.clone())),
        "block_idx" => Some(Value::from(t.block_idx)),
        "correct" => Some(t.correct.map(Value::from).unwrap_or(Value::Null)),
        "day" => Some(Value::from(
            t.started_at.get(..10).unwrap_or("").to_string(),
        )),
        "week" => Some(Value::from(iso_week(&t.started_at))),
        _ => None,
    };
    if let Some(v) = simple {
        return v;
    }
    let (root, rest) = field.split_once('.').unwrap_or((field, ""));
    let base = match root {
        "stimulus" => &t.stimulus,
        "metrics" => &t.metrics,
        "response" => &t.response,
        "spec" => &t.spec,
        _ => return Value::Null,
    };
    let mut cur = base;
    for part in rest.split('.').filter(|p| !p.is_empty()) {
        cur = match cur {
            Value::Object(m) => m.get(part).unwrap_or(&Value::Null),
            Value::Array(a) => part
                .parse::<usize>()
                .ok()
                .and_then(|i| a.get(i))
                .unwrap_or(&Value::Null),
            _ => &Value::Null,
        };
    }
    cur.clone()
}

fn iso_week(ts: &str) -> String {
    use chrono::Datelike;
    chrono::DateTime::parse_from_rfc3339(ts)
        .map(|d| {
            let w = d.iso_week();
            format!("{}-W{:02}", w.year(), w.week())
        })
        .unwrap_or_default()
}

fn is_valid_field(f: &str) -> bool {
    matches!(
        f,
        "type"
            | "protocol"
            | "pool"
            | "feedback_mode"
            | "assisted"
            | "assessment"
            | "session_id"
            | "block_idx"
            | "correct"
            | "day"
            | "week"
    ) || ["stimulus.", "metrics.", "response.", "spec."]
        .iter()
        .any(|p| f.starts_with(p) && f.len() > p.len())
}

pub(crate) fn median(mut v: Vec<f64>) -> Option<f64> {
    if v.is_empty() {
        return None;
    }
    v.sort_by(|a, b| a.total_cmp(b));
    let n = v.len();
    Some(if n % 2 == 1 {
        v[n / 2]
    } else {
        0.5 * (v[n / 2 - 1] + v[n / 2])
    })
}

fn mean(v: &[f64]) -> Option<f64> {
    if v.is_empty() {
        None
    } else {
        Some(v.iter().sum::<f64>() / v.len() as f64)
    }
}

fn cents_of(t: &TrialRow) -> Option<f64> {
    t.metrics.get("cents_error").and_then(Value::as_f64)
}

fn abs_cents_of(t: &TrialRow) -> Option<f64> {
    cents_of(t)
        .map(f64::abs)
        .or_else(|| t.metrics.get("mean_abs_cents").and_then(Value::as_f64))
}

pub fn aggregate(trials: &[TrialRow], group_by: &[String]) -> Vec<StatsRow> {
    let mut groups: BTreeMap<String, (BTreeMap<String, Value>, Vec<&TrialRow>)> = BTreeMap::new();
    for t in trials {
        let mut key = BTreeMap::new();
        for g in group_by {
            key.insert(g.clone(), field_value(t, g));
        }
        let k = serde_json::to_string(&key).unwrap_or_default();
        groups
            .entry(k)
            .or_insert_with(|| (key, Vec::new()))
            .1
            .push(t);
    }
    groups
        .into_values()
        .map(|(group, rows)| {
            let graded: Vec<&&TrialRow> = rows.iter().filter(|t| t.correct.is_some()).collect();
            let correct = graded.iter().filter(|t| t.correct == Some(true)).count();
            let scores: Vec<f64> = rows
                .iter()
                .filter_map(|t| t.score.map(|s| s as f64))
                .collect();
            StatsRow {
                trials: rows.len(),
                graded: graded.len(),
                correct,
                accuracy: if graded.is_empty() {
                    None
                } else {
                    Some(correct as f64 / graded.len() as f64)
                },
                mean_score: mean(&scores),
                median_rt_ms: median(rows.iter().filter_map(|t| t.rt_ms).collect()),
                median_abs_cents: median(rows.iter().filter_map(|t| abs_cents_of(t)).collect()),
                mean_signed_cents: mean(
                    &rows.iter().filter_map(|t| cents_of(t)).collect::<Vec<_>>(),
                ),
                mean_signed_error: mean(
                    &rows
                        .iter()
                        .filter_map(|t| t.metrics.get("signed_error").and_then(Value::as_f64))
                        .collect::<Vec<_>>(),
                ),
                group,
            }
        })
        .collect()
}

pub fn get_stats(db: &Db, q: &StatsQuery) -> Result<Vec<StatsRow>> {
    for g in &q.group_by {
        if !is_valid_field(g) {
            return Err(anyhow!("unknown group_by field `{g}`"));
        }
    }
    let trials = db.query_trials(&q.filter, q.max_trials.unwrap_or(5000).min(50_000), 0)?;
    Ok(aggregate(&trials, &q.group_by))
}

#[derive(Clone, Debug, Serialize, Deserialize, TS)]
pub struct Confusion {
    /// Row/column labels in order.
    pub labels: Vec<String>,
    /// `matrix[actual][answered]` counts.
    pub matrix: Vec<Vec<u32>>,
    pub total: u32,
    /// The most frequent confusions, as (actual, answered, count).
    pub top_confusions: Vec<(String, String, u32)>,
}

/// Confusions between the expected answer and the given answer for identification trials.
pub fn confusion(trials: &[TrialRow]) -> Confusion {
    let mut labels: Vec<String> = Vec::new();
    let add = |l: &str, labels: &mut Vec<String>| {
        if !labels.iter().any(|x| x == l) {
            labels.push(l.to_string());
        }
    };
    let mut pairs = Vec::new();
    for t in trials {
        let Some(expected) = t.metrics.get("expected").and_then(Value::as_str) else {
            continue;
        };
        let answered = t
            .response
            .get("answer")
            .and_then(Value::as_str)
            .unwrap_or("(none)");
        add(expected, &mut labels);
        add(answered, &mut labels);
        pairs.push((expected.to_string(), answered.to_string()));
    }
    labels.sort_by_key(|l| label_order(l));
    let idx = |l: &str| labels.iter().position(|x| x == l).unwrap();
    let mut matrix = vec![vec![0u32; labels.len()]; labels.len()];
    for (e, a) in &pairs {
        matrix[idx(e)][idx(a)] += 1;
    }
    let mut top = Vec::new();
    for (i, row) in matrix.iter().enumerate() {
        for (j, &c) in row.iter().enumerate() {
            if i != j && c > 0 {
                top.push((labels[i].clone(), labels[j].clone(), c));
            }
        }
    }
    top.sort_by(|a, b| b.2.cmp(&a.2));
    top.truncate(10);
    Confusion {
        total: pairs.len() as u32,
        labels,
        matrix,
        top_confusions: top,
    }
}

/// Pitch classes, intervals and notes sort musically; anything else alphabetically after them.
fn label_order(l: &str) -> (u8, i32, String) {
    if let Ok(pc) = l.parse::<crate::theory::PitchClass>() {
        return (0, pc.index() as i32, String::new());
    }
    if let Ok(iv) = l.parse::<crate::theory::Interval>() {
        return (1, iv.semitones(), String::new());
    }
    if let Ok(n) = l.parse::<crate::theory::Note>() {
        return (2, n.0, String::new());
    }
    (3, 0, l.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::storage::{TrialRow, new_id, now};
    use serde_json::json;

    fn t(pc: &str, answer: &str, timbre: &str, rt: f64) -> TrialRow {
        TrialRow {
            id: new_id(),
            session_id: "s".into(),
            block_idx: 0,
            trial_idx: 0,
            attempt: 0,
            kind: "pitch_id".into(),
            protocol: None,
            tags: vec![],
            stimulus: json!({"pitch_class": pc, "timbre": timbre}),
            spec: json!({}),
            pool: Some("training".into()),
            feedback_mode: "after_trial".into(),
            response: json!({"answer": answer}),
            metrics: json!({"expected": pc, "signed_error": answer.parse::<crate::theory::PitchClass>().unwrap().signed_distance(pc.parse().unwrap())}),
            summary: String::new(),
            correct: Some(pc == answer),
            score: Some((pc == answer) as i32 as f32),
            rt_ms: Some(rt),
            assisted: false,
            assessment: false,
            labeled_reference: false,
            started_at: now().to_rfc3339(),
            start_frame: None,
            device_profile: None,
            calibrated: false,
            secs_since_labeled_reference: None,
            interrupted: false,
        }
    }

    #[test]
    fn groups_by_stimulus_fields() {
        let rows = vec![
            t("C", "C", "gm:0", 700.0),
            t("C", "C#", "gm:48", 900.0),
            t("E", "E", "gm:0", 800.0),
            t("E", "F", "gm:0", 1000.0),
            t("E", "E", "gm:48", 600.0),
        ];
        let stats = aggregate(&rows, &["stimulus.pitch_class".to_string()]);
        assert_eq!(stats.len(), 2);
        let e = stats
            .iter()
            .find(|s| s.group["stimulus.pitch_class"] == json!("E"))
            .unwrap();
        assert_eq!(e.trials, 3);
        assert!((e.accuracy.unwrap() - 2.0 / 3.0).abs() < 1e-9);
        assert_eq!(e.median_rt_ms, Some(800.0));
        assert!((e.mean_signed_error.unwrap() - 1.0 / 3.0).abs() < 1e-9);
        let by_timbre = aggregate(&rows, &["stimulus.timbre".to_string(), "type".to_string()]);
        assert_eq!(by_timbre.len(), 2);
    }

    #[test]
    fn confusion_matrix_orders_pitch_classes() {
        let rows = vec![
            t("E", "F", "gm:0", 1.0),
            t("E", "F", "gm:0", 1.0),
            t("C", "C", "gm:0", 1.0),
            t("D#", "E", "gm:0", 1.0),
        ];
        let c = confusion(&rows);
        assert_eq!(c.labels, vec!["C", "D#", "E", "F"]);
        assert_eq!(c.total, 4);
        assert_eq!(c.top_confusions[0], ("E".to_string(), "F".to_string(), 2));
        assert_eq!(c.matrix[2][3], 2);
    }

    #[test]
    fn rejects_unknown_fields() {
        assert!(is_valid_field("stimulus.pitch_class"));
        assert!(is_valid_field("week"));
        assert!(!is_valid_field("stimulus."));
        assert!(!is_valid_field("password"));
    }
}
