//! Deterministic grading. Every function here is pure: same inputs, same grade.

use serde::{Deserialize, Serialize};
use serde_json::json;
use ts_rs::TS;

use crate::metrics::{
    NoteMetricOptions, NoteMetrics, NoteSegment, SegmentOptions, note_metrics, segment_notes,
};
use crate::pitch::PitchTrack;
use crate::schema::{
    DetectTrial, DictationTrial, ExerciseKind, FeedbackMode, GradingOverrides, IdentifyTrial,
    MatchTrial, PitchRefMode, SingPitchTrial, SpanTrial, SungTrial, Tuning,
};
use crate::score::{Pitch, ScoreNote};
use crate::theory::{Note, PitchClass};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, TS)]
pub struct GradingConfig {
    pub mode: PitchRefMode,
    pub category_tol_cents: f32,
    pub in_tune_tol_cents: f32,
    pub stable_tol_cents: f32,
    pub hold_ms: f32,
    pub onset_tol_ms: f32,
    pub tuning: Tuning,
}

impl GradingConfig {
    pub fn resolve(overrides: &GradingOverrides, default_mode: PitchRefMode) -> Self {
        GradingConfig {
            mode: overrides.mode.unwrap_or(default_mode),
            category_tol_cents: overrides.category_tol_cents.unwrap_or(50.0),
            in_tune_tol_cents: overrides.in_tune_tol_cents.unwrap_or(20.0),
            stable_tol_cents: overrides.stable_tol_cents.unwrap_or(25.0),
            hold_ms: overrides.hold_ms.map(|v| v as f32).unwrap_or(400.0),
            onset_tol_ms: overrides.onset_tol_ms.map(|v| v as f32).unwrap_or(120.0),
            tuning: overrides.tuning.unwrap_or_default(),
        }
    }

    fn note_opts(&self) -> NoteMetricOptions {
        NoteMetricOptions {
            stable_tol_cents: self.stable_tol_cents,
            hold_ms: self.hold_ms,
        }
    }
}

/// The graded result of one trial, as stored.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, TS)]
pub struct TrialOutcome {
    /// `None` for passive trials.
    pub correct: Option<bool>,
    /// 0..1: 1/0 for choices, fraction of notes right for melodies.
    pub score: Option<f32>,
    pub rt_ms: Option<f64>,
    #[ts(type = "Record<string, unknown>")]
    pub response: serde_json::Value,
    #[ts(type = "Record<string, unknown>")]
    pub metrics: serde_json::Value,
    /// Short deterministic text, e.g. "12 cents sharp, settled in 180 ms".
    pub summary: String,
}

impl TrialOutcome {
    pub fn passive() -> Self {
        TrialOutcome {
            correct: None,
            score: None,
            rt_ms: None,
            response: json!({}),
            metrics: json!({}),
            summary: String::new(),
        }
    }
}

pub fn describe_cents(c: f32) -> String {
    let r = c.round() as i32;
    match r {
        0 => "in tune".to_string(),
        1 => "1 cent sharp".to_string(),
        -1 => "1 cent flat".to_string(),
        r if r > 0 => format!("{r} cents sharp"),
        r => format!("{} cents flat", -r),
    }
}

// ---------------------------------------------------------------------------------------------
// Identification
// ---------------------------------------------------------------------------------------------

pub fn grade_choice(
    trial: &IdentifyTrial,
    answer: Option<&str>,
    rt_ms: Option<f64>,
) -> TrialOutcome {
    let correct = answer == Some(trial.correct.as_str());
    let label_of = |id: &str| {
        trial
            .choices
            .iter()
            .find(|c| c.id == id)
            .map(|c| c.label.clone())
            .unwrap_or(id.to_string())
    };
    let mut metrics = json!({ "expected": trial.correct, "timeout": answer.is_none() });
    // Signed category error for pitch-class answers (answer − actual, wrapped to −6..5).
    if trial.exercise == ExerciseKind::PitchId || trial.exercise == ExerciseKind::TonalCenterId {
        if let (Some(a), Ok(exp)) = (answer, trial.correct.parse::<PitchClass>()) {
            if let Ok(ans) = a.parse::<PitchClass>() {
                metrics["signed_error"] = json!(ans.signed_distance(exp));
            } else if let (Ok(ans), Ok(expn)) = (a.parse::<Note>(), trial.correct.parse::<Note>()) {
                metrics["signed_error"] = json!(ans.0 - expn.0);
            }
        } else if let (Some(a), Ok(exp)) = (answer, trial.correct.parse::<Note>()) {
            if let Ok(ans) = a.parse::<Note>() {
                metrics["signed_error"] = json!(ans.0 - exp.0);
                metrics["pitch_class_correct"] = json!(ans.pitch_class() == exp.pitch_class());
            }
        }
    }
    // Error in pitch classes, folded to −6..5: UCSF-style points (1 exact, 0.75 a semitone off)
    // and the absolute error that mean absolute deviation is built from.
    let class_error = match (
        trial.exercise,
        metrics
            .get("signed_error")
            .and_then(serde_json::Value::as_i64),
    ) {
        (ExerciseKind::PitchId, Some(e)) => Some(((e + 6).rem_euclid(12)) - 6),
        _ => None,
    };
    if let Some(e) = class_error {
        metrics["class_error"] = json!(e);
        metrics["ucsf_points"] = json!(match e.abs() {
            0 => 1.0,
            1 => 0.75,
            _ => 0.0,
        });
    }
    let mut summary = match answer {
        None => format!("No answer. It was {}.", label_of(&trial.correct)),
        Some(_) if correct => format!("Correct: {}.", label_of(&trial.correct)),
        Some(a) => format!(
            "You answered {}. It was {}.",
            label_of(a),
            label_of(&trial.correct)
        ),
    };
    if let Some(x) = &trial.explain {
        summary = format!("{} ({x}).", summary.trim_end_matches('.'));
    }
    TrialOutcome {
        correct: Some(correct),
        score: Some(if correct { 1.0 } else { 0.0 }),
        rt_ms,
        response: json!({ "answer": answer }),
        metrics,
        summary,
    }
}

/// What the feedback screen shows. Errorless mode keeps the stored result and hides the error.
#[derive(Clone, Debug, PartialEq)]
pub struct ShownFeedback {
    pub correct: Option<bool>,
    pub summary: String,
    pub answered: Option<String>,
    pub errorless: bool,
}

pub fn present_feedback(
    mode: FeedbackMode,
    stored_correct: Option<bool>,
    summary: &str,
    answered: Option<&str>,
    correct_label: &str,
) -> ShownFeedback {
    let errorless = mode.is_errorless();
    if !mode.shows_trial_result() {
        return ShownFeedback {
            correct: None,
            summary: String::new(),
            answered: None,
            errorless: false,
        };
    }
    if errorless && stored_correct == Some(false) {
        return ShownFeedback {
            correct: None,
            summary: format!("It was {correct_label}."),
            answered: None,
            errorless: true,
        };
    }
    ShownFeedback {
        correct: stored_correct,
        summary: summary.to_string(),
        answered: answered.map(str::to_string),
        errorless,
    }
}

/// Log-linear correction (Hautus 1995): add ½ to each count and 1 to each total, so d′ stays
/// finite when every target is caught or there are no false alarms.
pub fn d_prime(hits: u32, misses: u32, false_alarms: u32, correct_rejections: u32) -> f64 {
    let n_signal = hits + misses;
    let n_noise = false_alarms + correct_rejections;
    if n_signal == 0 || n_noise == 0 {
        return 0.0;
    }
    let hit_rate = (hits as f64 + 0.5) / (n_signal as f64 + 1.0);
    let fa_rate = (false_alarms as f64 + 0.5) / (n_noise as f64 + 1.0);
    inv_norm(hit_rate) - inv_norm(fa_rate)
}

/// Inverse of the standard normal CDF (Acklam's approximation).
fn inv_norm(p: f64) -> f64 {
    let p = p.clamp(1e-12, 1.0 - 1e-12);
    const A: [f64; 6] = [
        -3.969683028665376e+01,
        2.209460984245205e+02,
        -2.759285104469687e+02,
        1.383577518672690e+02,
        -3.066479806614716e+01,
        2.506628277459239e+00,
    ];
    const B: [f64; 5] = [
        -5.447609879822406e+01,
        1.615858368580409e+02,
        -1.556989798598866e+02,
        6.680131188771972e+01,
        -1.328068155288572e+01,
    ];
    const C: [f64; 6] = [
        -7.784894002430293e-03,
        -3.223964580411365e-01,
        -2.400758277161838e+00,
        -2.549732539343734e+00,
        4.374664141464968e+00,
        2.938163982698783e+00,
    ];
    const D: [f64; 4] = [
        7.784695709041462e-03,
        3.224671290700398e-01,
        2.445134137142996e+00,
        3.754408661907416e+00,
    ];
    let plow = 0.02425;
    if p < plow {
        let q = (-2.0 * p.ln()).sqrt();
        (((((C[0] * q + C[1]) * q + C[2]) * q + C[3]) * q + C[4]) * q + C[5])
            / ((((D[0] * q + D[1]) * q + D[2]) * q + D[3]) * q + 1.0)
    } else if p > 1.0 - plow {
        let q = (-2.0 * (1.0 - p).ln()).sqrt();
        -(((((C[0] * q + C[1]) * q + C[2]) * q + C[3]) * q + C[4]) * q + C[5])
            / ((((D[0] * q + D[1]) * q + D[2]) * q + D[3]) * q + 1.0)
    } else {
        let q = p - 0.5;
        let r = q * q;
        (((((A[0] * r + A[1]) * r + A[2]) * r + A[3]) * r + A[4]) * r + A[5]) * q
            / (((((B[0] * r + B[1]) * r + B[2]) * r + B[3]) * r + B[4]) * r + 1.0)
    }
}

/// Score one detection stream. `presses_ms` is the response time from each tone's onset, or none.
pub fn grade_detect(trial: &DetectTrial, presses_ms: &[Option<f64>]) -> TrialOutcome {
    let mut hits = 0u32;
    let mut misses = 0u32;
    let mut false_alarms = 0u32;
    let mut correct_rejections = 0u32;
    let mut far_targets = 0u32;
    let mut far_hits = 0u32;
    let mut hit_latencies = Vec::new();
    for (tone, press) in trial.tones.iter().zip(presses_ms.iter()) {
        let pressed = press.is_some();
        if tone.far && tone.target {
            far_targets += 1;
        }
        match (tone.target, pressed) {
            (true, true) => {
                hits += 1;
                if let Some(ms) = press {
                    hit_latencies.push(*ms);
                }
                if tone.far {
                    far_hits += 1;
                }
            }
            (true, false) => misses += 1,
            (false, true) => false_alarms += 1,
            (false, false) => correct_rejections += 1,
        }
    }
    let d = d_prime(hits, misses, false_alarms, correct_rejections);
    hit_latencies.sort_by(|a, b| a.total_cmp(b));
    let median = (!hit_latencies.is_empty()).then(|| hit_latencies[hit_latencies.len() / 2]);
    let far_note = if far_targets > 0 {
        format!(" {far_hits} of {far_targets} targets were more than an octave from the prompt.")
    } else {
        String::new()
    };
    let latency = median
        .map(|m| format!("{m:.0} ms"))
        .unwrap_or_else(|| "—".into());
    TrialOutcome {
        // A stream is not a single right or wrong answer. d′ is the score.
        correct: None,
        score: Some(d as f32),
        rt_ms: median,
        response: json!({ "presses_ms": presses_ms }),
        metrics: json!({
            "hits": hits,
            "misses": misses,
            "false_alarms": false_alarms,
            "correct_rejections": correct_rejections,
            "d_prime": d,
            "median_hit_ms": median,
            "far_targets": far_targets,
            "far_hits": far_hits,
        }),
        summary: format!(
            "d′ {d:.2}. {hits} hits, {misses} misses, {false_alarms} false alarms. Median hit {latency}.{far_note}"
        ),
    }
}

// ---------------------------------------------------------------------------------------------
// Screening: implicit note memory and span
// ---------------------------------------------------------------------------------------------

/// Implicit note memory: the error of the chosen step, signed (positive is above the target) and
/// absolute, in steps and cents. There is no right or wrong; the error is the result.
pub fn grade_match(trial: &MatchTrial, answer_step: u32, moves: u32, rt_ms: f64) -> TrialOutcome {
    let answer = answer_step.clamp(1, trial.steps);
    let error_steps = answer as i32 - trial.target_step as i32;
    let error_cents = error_steps as f32 * trial.step_cents;
    let direction = match error_steps.signum() {
        0 => "exactly".to_string(),
        1 => "high".to_string(),
        _ => "low".to_string(),
    };
    TrialOutcome {
        correct: None,
        score: None,
        rt_ms: Some(rt_ms),
        response: json!({ "answer_step": answer, "moves": moves }),
        metrics: json!({
            "target_step": trial.target_step,
            "start_step": trial.start_step,
            "answer_step": answer,
            "error_steps": error_steps,
            "abs_error_steps": error_steps.abs(),
            "error_cents": error_cents,
            "abs_error_cents": error_cents.abs(),
            "moves": moves,
        }),
        summary: if error_steps == 0 {
            "Found it exactly.".into()
        } else {
            format!(
                "{} {} ({:.0} cents) {direction}.",
                error_steps.abs(),
                if error_steps.abs() == 1 {
                    "step"
                } else {
                    "steps"
                },
                error_cents.abs()
            )
        },
    }
}

/// A whole span ladder. `answers` are what was entered for each sequence presented, in order.
/// The span is the longest length with at least one sequence right.
pub fn grade_span(trial: &SpanTrial, answers: &[String]) -> TrialOutcome {
    let mut ladder = Vec::with_capacity(answers.len());
    let mut span = 0u32;
    for (item, answer) in trial.items.iter().zip(answers) {
        let given: Vec<u8> = answer
            .chars()
            .filter_map(|c| c.to_digit(10).map(|d| d as u8))
            .collect();
        let ok = given == item.sequence;
        if ok {
            span = span.max(item.length);
        }
        ladder.push(json!({
            "length": item.length,
            "sequence": item.sequence.iter().map(|d| d.to_string()).collect::<String>(),
            "answer": answer,
            "correct": ok,
        }));
    }
    let right = ladder.iter().filter(|l| l["correct"] == true).count();
    TrialOutcome {
        correct: None,
        score: None,
        rt_ms: None,
        response: json!({ "answers": answers }),
        metrics: json!({
            "span": span,
            "mode": trial.mode,
            "fallback": trial.stimulus.get("fallback").cloned().unwrap_or(json!(false)),
            "sequences": answers.len(),
            "sequences_correct": right,
            "ladder": ladder,
        }),
        summary: if span == 0 {
            "No sequence recalled in full.".into()
        } else {
            format!(
                "Span {span} ({right} of {} sequences right).",
                answers.len()
            )
        },
    }
}

// ---------------------------------------------------------------------------------------------
// Single sung notes
// ---------------------------------------------------------------------------------------------

/// Picks the note a single-note response is judged on: the first segment of at least 200 ms
/// after `from_s`, else the longest one.
fn pick_response_note(segs: &[NoteSegment], from_s: f32) -> Option<NoteSegment> {
    let candidates: Vec<NoteSegment> = segs
        .iter()
        .copied()
        .filter(|s| s.offset_s > from_s)
        .collect();
    candidates
        .iter()
        .copied()
        .find(|s| s.duration_s() >= 0.2)
        .or_else(|| {
            candidates
                .iter()
                .copied()
                .max_by(|a, b| a.duration_s().total_cmp(&b.duration_s()))
        })
}

fn nearest_octave_target(pc: PitchClass, sung_midi: f32) -> f32 {
    let base = pc.index() as f32;
    let k = ((sung_midi - base) / 12.0).round();
    base + 12.0 * k
}

fn note_json(m: &NoteMetrics) -> serde_json::Value {
    serde_json::to_value(m).unwrap_or_default()
}

fn single_note_summary(m: &NoteMetrics, category_ok: bool, sung_name: &str) -> String {
    let mut s = if category_ok {
        describe_cents(m.centre_cents)
    } else {
        format!("Sang {sung_name} ({})", describe_cents(m.centre_cents))
    };
    if let Some(ms) = m.settling_ms {
        if ms >= 30.0 {
            s.push_str(&format!(", settled in {} ms", ms.round() as i32));
        }
    } else {
        s.push_str(", did not settle");
    }
    if m.drift_cents_per_s.abs() >= 8.0 {
        s.push_str(&format!(", drifting {:+.0} cents/s", m.drift_cents_per_s));
    }
    s
}

/// "Sing E-flat" with no reference. `prompt_end_s` is when the prompt finished, in capture time.
pub fn grade_sing_pitch(
    trial: &SingPitchTrial,
    track: &PitchTrack,
    cfg: &GradingConfig,
    prompt_end_s: f32,
) -> TrialOutcome {
    let segs = segment_notes(track, &SegmentOptions::default());
    let Some(seg) = pick_response_note(&segs, prompt_end_s) else {
        return no_voice_outcome();
    };
    let absolute = cfg.mode == PitchRefMode::Absolute && trial.target_midi.is_some();
    let target = if absolute {
        trial.target_midi.unwrap() as f32
    } else {
        nearest_octave_target(trial.target_class, seg.median_midi)
    };
    let Some(m) = note_metrics(
        &track.frames[seg.start..seg.end],
        track.hop_s,
        target,
        &cfg.note_opts(),
    ) else {
        return no_voice_outcome();
    };
    let category_ok = m.centre_cents.abs() <= cfg.category_tol_cents;
    let in_tune = m.centre_cents.abs() <= cfg.in_tune_tol_cents;
    let sung = Note((target + m.centre_cents / 100.0).round() as i32);
    let latency_ms = ((m.onset_s - prompt_end_s) * 1000.0).max(0.0);
    let mut metrics = note_json(&m);
    metrics["sung_note"] = json!(sung.name());
    metrics["sung_octave"] = json!(sung.octave());
    metrics["category_correct"] = json!(category_ok);
    metrics["in_tune"] = json!(in_tune);
    metrics["cents_error"] = json!(m.centre_cents);
    metrics["response_latency_ms"] = json!(latency_ms);
    TrialOutcome {
        correct: Some(category_ok),
        score: Some(if category_ok { 1.0 } else { 0.0 }),
        rt_ms: Some(latency_ms as f64),
        response: json!({ "voiced": true }),
        summary: single_note_summary(&m, category_ok, &sung.display_name()),
        metrics,
    }
}

fn no_voice_outcome() -> TrialOutcome {
    TrialOutcome {
        correct: Some(false),
        score: Some(0.0),
        rt_ms: None,
        response: json!({ "voiced": false }),
        metrics: json!({ "no_voice": true }),
        summary: "No singing detected.".to_string(),
    }
}

// ---------------------------------------------------------------------------------------------
// Sung timelines (pitch matching, sustain, intervals, patterns)
// ---------------------------------------------------------------------------------------------

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, TS)]
pub struct NoteResult {
    pub target_midi: i32,
    pub expected_onset_s: f32,
    pub matched: bool,
    pub sung_midi: Option<f32>,
    pub cents_error: Option<f32>,
    pub onset_error_ms: Option<f32>,
    pub correct: bool,
    pub in_tune: bool,
    pub on_time: Option<bool>,
    pub onset_s: Option<f32>,
    pub offset_s: Option<f32>,
    /// Sung silently by design (inner hearing): not graded.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub silent: bool,
}

struct Expected {
    midi: f32,
    onset: f32,
    dur: f32,
}

/// Needleman–Wunsch alignment of expected notes to performed segments.
fn align(
    expected: &[Expected],
    performed: &[NoteSegment],
    offset_semis: f32,
    beat_s: f32,
    timed: bool,
) -> Vec<Option<usize>> {
    const DEL: f32 = 2.0;
    let n = expected.len();
    let m = performed.len();
    let ins_cost = |j: usize| {
        if performed[j].duration_s() < 0.15 {
            0.5
        } else {
            1.0
        }
    };
    let match_cost = |i: usize, j: usize| -> f32 {
        let d = (performed[j].median_midi - expected[i].midi - offset_semis)
            .abs()
            .min(3.0);
        let dt = (performed[j].onset_s - expected[i].onset).abs() / beat_s;
        let time = if timed {
            dt.min(2.0)
        } else {
            0.25 * dt.min(2.0)
        };
        d + time
    };
    let mut dp = vec![vec![0.0f32; m + 1]; n + 1];
    for i in 1..=n {
        dp[i][0] = dp[i - 1][0] + DEL;
    }
    for j in 1..=m {
        dp[0][j] = dp[0][j - 1] + ins_cost(j - 1);
    }
    for i in 1..=n {
        for j in 1..=m {
            let a = dp[i - 1][j - 1] + match_cost(i - 1, j - 1);
            let b = dp[i - 1][j] + DEL;
            let c = dp[i][j - 1] + ins_cost(j - 1);
            dp[i][j] = a.min(b).min(c);
        }
    }
    let mut out = vec![None; n];
    let (mut i, mut j) = (n, m);
    while i > 0 && j > 0 {
        let here = dp[i][j];
        if (here - (dp[i - 1][j - 1] + match_cost(i - 1, j - 1))).abs() < 1e-4 {
            out[i - 1] = Some(j - 1);
            i -= 1;
            j -= 1;
        } else if (here - (dp[i - 1][j] + DEL)).abs() < 1e-4 {
            i -= 1;
        } else {
            j -= 1;
        }
    }
    out
}

fn median_f32(mut v: Vec<f32>) -> Option<f32> {
    if v.is_empty() {
        return None;
    }
    v.sort_by(|a, b| a.total_cmp(b));
    Some(v[v.len() / 2])
}

/// "an octave lower", "two octaves higher", or nothing for no move.
fn octave_words(shift_semis: f32) -> Option<String> {
    let k = (shift_semis / 12.0).round() as i32;
    let dir = if k < 0 { "lower" } else { "higher" };
    match k.abs() {
        0 => None,
        1 => Some(format!("an octave {dir}")),
        2 => Some(format!("two octaves {dir}")),
        n => Some(format!("{n} octaves {dir}")),
    }
}

/// How far the singer is from the targets in semitones: the median difference between each
/// target and the segment that overlaps it most in time (or, with no overlaps at all, between
/// the middle of what was sung and the middle of the targets). Used to find the octave.
fn register_offset(expected: &[Expected], segs: &[NoteSegment]) -> f32 {
    let diffs: Vec<f32> = expected
        .iter()
        .filter_map(|e| {
            let overlap = |s: &NoteSegment| {
                (s.offset_s.min(e.onset + e.dur) - s.onset_s.max(e.onset)).max(0.0)
            };
            segs.iter()
                .filter(|s| overlap(s) > 0.0)
                .max_by(|a, b| overlap(a).total_cmp(&overlap(b)))
                .map(|s| s.median_midi - e.midi)
        })
        .collect();
    median_f32(diffs).unwrap_or_else(|| {
        let sung = median_f32(segs.iter().map(|s| s.median_midi).collect()).unwrap_or(0.0);
        let target = median_f32(expected.iter().map(|e| e.midi).collect()).unwrap_or(0.0);
        sung - target
    })
}

/// Grades a sung timeline. `sing_start_s` is the capture time at which the trial's
/// `sing_start_beat` falls (already latency-compensated).
pub fn grade_sung(
    trial: &SungTrial,
    track: &PitchTrack,
    cfg: &GradingConfig,
    sing_start_s: f32,
) -> TrialOutcome {
    let mut out = if let Some(f) = &trial.free {
        grade_free(trial, f, track, sing_start_s)
    } else if !trial.silent.is_empty() {
        grade_with_silent(trial, track, cfg, sing_start_s)
    } else {
        grade_targets(trial, track, cfg, sing_start_s)
    };
    if let Some(r) = &trial.reveal {
        out.summary = format!(
            "{} {r}",
            out.summary.trim_end_matches('.').to_string() + "."
        );
    }
    out
}

/// Notes sung in the head are left out of grading and come back as silent placeholders; the
/// first note after a silent stretch (the re-entry) is reported on its own.
fn grade_with_silent(
    trial: &SungTrial,
    track: &PitchTrack,
    cfg: &GradingConfig,
    sing_start_s: f32,
) -> TrialOutcome {
    let mut aloud = trial.clone();
    aloud.silent.clear();
    aloud.notes = trial
        .notes
        .iter()
        .enumerate()
        .filter(|(i, _)| !trial.silent.contains(i))
        .map(|(_, n)| n.clone())
        .collect();
    let mut out = grade_targets(&aloud, track, cfg, sing_start_s);
    let graded: Vec<NoteResult> = out
        .metrics
        .get("notes")
        .and_then(|v| serde_json::from_value(v.clone()).ok())
        .unwrap_or_default();
    let beat_s = 60.0 / trial.tempo_bpm;
    let mut all = Vec::with_capacity(trial.notes.len());
    let mut k = 0;
    let mut reentry: Option<bool> = None;
    for (i, n) in trial.notes.iter().enumerate() {
        if trial.silent.contains(&i) {
            all.push(NoteResult {
                target_midi: n.midi,
                expected_onset_s: sing_start_s + n.start_beat * beat_s,
                matched: false,
                sung_midi: None,
                cents_error: None,
                onset_error_ms: None,
                correct: false,
                in_tune: false,
                on_time: None,
                onset_s: None,
                offset_s: None,
                silent: true,
            });
        } else if let Some(r) = graded.get(k) {
            if i > 0 && trial.silent.contains(&(i - 1)) && reentry.is_none() {
                reentry = Some(r.correct);
            }
            all.push(r.clone());
            k += 1;
        }
    }
    out.metrics["notes"] = json!(all);
    out.metrics["silent_notes"] = json!(trial.silent.len());
    if let Some(ok) = reentry {
        out.metrics["reentry_correct"] = json!(ok);
        out.summary.push_str(if ok {
            "; came back in on the right note"
        } else {
            "; the re-entry after singing silently was off"
        });
    }
    out
}

/// A free answer: notes sung after singing starts must all be from the allowed set, between
/// `min_notes` and `max_notes` of them, and not the prompt again.
fn grade_free(
    trial: &SungTrial,
    f: &crate::schema::FreeResponse,
    track: &PitchTrack,
    sing_start_s: f32,
) -> TrialOutcome {
    let segs: Vec<NoteSegment> = segment_notes(track, &SegmentOptions::default())
        .into_iter()
        .filter(|s| s.offset_s > sing_start_s - 0.25 && s.duration_s() >= 0.12)
        .collect();
    if segs.is_empty() {
        return no_voice_outcome();
    }
    let name = |pc: i32| {
        f.names
            .iter()
            .find(|(p, _)| *p == pc)
            .map(|(_, n)| n.clone())
            .unwrap_or_else(|| {
                PitchClass::new(pc.rem_euclid(12))
                    .display_name()
                    .to_string()
            })
    };
    let mut results = Vec::new();
    let mut pcs = Vec::new();
    for sg in &segs {
        let nearest = sg.median_midi.round() as i32;
        let pc = nearest.rem_euclid(12);
        let ok = f.allowed.contains(&pc);
        pcs.push(pc);
        results.push(NoteResult {
            target_midi: nearest,
            expected_onset_s: sg.onset_s,
            matched: true,
            sung_midi: Some(sg.median_midi),
            cents_error: Some((sg.median_midi - nearest as f32) * 100.0),
            onset_error_ms: None,
            correct: ok,
            in_tune: ok && (sg.median_midi - nearest as f32).abs() <= 0.25,
            on_time: None,
            onset_s: Some(sg.onset_s),
            offset_s: Some(sg.offset_s),
            silent: false,
        });
    }
    let count_ok = (f.min_notes as usize..=f.max_notes as usize).contains(&pcs.len());
    let all_ok = results.iter().all(|r| r.correct);
    let copied = !f.differ_from.is_empty() && pcs == f.differ_from;
    let correct = count_ok && all_ok && !copied;
    let sung = pcs.iter().map(|p| name(*p)).collect::<Vec<_>>().join(" ");
    let summary = if copied {
        format!("You sang {sung}: the same pattern back. Answer with a different one.")
    } else if !all_ok {
        let wrong: Vec<String> = results
            .iter()
            .filter(|r| !r.correct)
            .map(|r| name(r.target_midi.rem_euclid(12)))
            .collect();
        format!(
            "You sang {sung}; {} is not part of {}.",
            wrong.join(", "),
            f.what
        )
    } else if !count_ok {
        format!(
            "You sang {sung}: {} notes, {}–{} were asked for.",
            pcs.len(),
            f.min_notes,
            f.max_notes
        )
    } else {
        format!("You sang {sung}: {}.", f.what)
    };
    let _ = trial;
    TrialOutcome {
        correct: Some(correct),
        score: Some(results.iter().filter(|r| r.correct).count() as f32 / results.len() as f32),
        rt_ms: None,
        response: json!({ "voiced": true, "segments": segs.len() }),
        metrics: json!({ "notes": results, "sung_pitch_classes": pcs, "copied": copied, "free": true }),
        summary,
    }
}

fn grade_targets(
    trial: &SungTrial,
    track: &PitchTrack,
    cfg: &GradingConfig,
    sing_start_s: f32,
) -> TrialOutcome {
    let beat_s = 60.0 / trial.tempo_bpm;
    // In just intonation each note sits where it is pure against the other voice.
    let just = |i: usize| match cfg.tuning {
        Tuning::Just => trial.just_cents.get(i).copied().unwrap_or(0.0) / 100.0,
        Tuning::Equal => 0.0,
    };
    let expected: Vec<Expected> = trial
        .notes
        .iter()
        .enumerate()
        .map(|(i, n)| Expected {
            midi: n.midi as f32 + just(i),
            onset: sing_start_s + n.start_beat * beat_s,
            dur: n.beats * beat_s,
        })
        .collect();
    let all_segs = segment_notes(track, &SegmentOptions::default());
    // Ignore anything sung well before singing was due (e.g. humming along with the reference).
    let segs: Vec<NoteSegment> = all_segs
        .into_iter()
        .filter(|s| s.offset_s > sing_start_s - 0.25)
        .collect();
    if segs.is_empty() || expected.is_empty() {
        return no_voice_outcome();
    }
    // Any octave: the targets move by the whole octaves the singer is away from them.
    let octave_shift = if cfg.mode == PitchRefMode::AnyOctave {
        (register_offset(&expected, &segs) / 12.0).round() * 12.0
    } else {
        0.0
    };
    let expected: Vec<Expected> = expected
        .into_iter()
        .map(|e| Expected {
            midi: e.midi + octave_shift,
            ..e
        })
        .collect();

    let single = expected.len() == 1;
    let alignment: Vec<Option<usize>> = if single {
        // Untimed single note: the longest segment that overlaps the response window.
        let e = &expected[0];
        let pick = segs
            .iter()
            .enumerate()
            .filter(|(_, s)| s.offset_s > e.onset - 0.3)
            .max_by(|a, b| a.1.duration_s().total_cmp(&b.1.duration_s()))
            .map(|(i, _)| i);
        vec![pick]
    } else {
        let relative = matches!(
            cfg.mode,
            PitchRefMode::RelativeToFirst | PitchRefMode::RelativeDriftTolerant
        );
        let offset = if relative {
            // First pass on timing alone to estimate the singer's transposition.
            let first = align(&expected, &segs, 0.0, beat_s, true);
            median_f32(
                first
                    .iter()
                    .enumerate()
                    .filter_map(|(i, j)| j.map(|j| segs[j].median_midi - expected[i].midi))
                    .collect(),
            )
            .unwrap_or(0.0)
        } else {
            0.0
        };
        align(&expected, &segs, offset, beat_s, trial.timed)
    };

    // Per-note metrics against the absolute target first.
    let mut raw: Vec<Option<NoteMetrics>> = Vec::new();
    for (i, j) in alignment.iter().enumerate() {
        raw.push(j.and_then(|j| {
            let s = segs[j];
            note_metrics(
                &track.frames[s.start..s.end],
                track.hop_s,
                expected[i].midi,
                &cfg.note_opts(),
            )
        }));
    }

    // Reference-mode adjustment of the errors.
    let abs_errors: Vec<Option<f32>> = raw
        .iter()
        .map(|m| m.as_ref().map(|m| m.centre_cents))
        .collect();
    let mut offset_cents = 0.0f32;
    let mut drift_cents_per_s = 0.0f32;
    let adjusted: Vec<Option<f32>> = match cfg.mode {
        PitchRefMode::Absolute | PitchRefMode::AnyOctave => abs_errors.clone(),
        PitchRefMode::PitchClass => abs_errors
            .iter()
            .map(|e| e.map(|c| crate::theory::wrap_semitones((c / 100.0) as f64) as f32 * 100.0))
            .collect(),
        PitchRefMode::RelativeToFirst => {
            offset_cents = abs_errors.iter().flatten().next().copied().unwrap_or(0.0);
            abs_errors
                .iter()
                .map(|e| e.map(|c| c - offset_cents))
                .collect()
        }
        PitchRefMode::RelativeDriftTolerant => {
            let pts: Vec<(f32, f32)> = abs_errors
                .iter()
                .zip(&expected)
                .filter_map(|(e, x)| e.map(|c| (x.onset - sing_start_s, c)))
                .collect();
            let (slope, intercept) = crate::metrics::linear_fit(
                &pts.iter().map(|p| p.0).collect::<Vec<_>>(),
                &pts.iter().map(|p| p.1).collect::<Vec<_>>(),
            );
            offset_cents = intercept;
            drift_cents_per_s = slope;
            abs_errors
                .iter()
                .zip(&expected)
                .map(|(e, x)| e.map(|c| c - (intercept + slope * (x.onset - sing_start_s))))
                .collect()
        }
    };

    let mut results = Vec::new();
    for (i, x) in expected.iter().enumerate() {
        let m = raw[i].as_ref();
        let err = adjusted[i];
        let onset_err = m.map(|m| (m.onset_s - x.onset) * 1000.0);
        let on_time = if trial.timed && !single {
            onset_err.map(|e| e.abs() <= cfg.onset_tol_ms)
        } else {
            None
        };
        let correct = err
            .map(|e| e.abs() <= cfg.category_tol_cents)
            .unwrap_or(false);
        results.push(NoteResult {
            // The written note, before any octave move.
            target_midi: (x.midi - octave_shift).round() as i32,
            expected_onset_s: x.onset,
            matched: m.is_some(),
            sung_midi: m.map(|m| m.target_midi + m.centre_cents / 100.0),
            cents_error: err,
            onset_error_ms: onset_err,
            correct,
            in_tune: err
                .map(|e| e.abs() <= cfg.in_tune_tol_cents)
                .unwrap_or(false),
            on_time,
            onset_s: m.map(|m| m.onset_s),
            offset_s: m.map(|m| m.offset_s),
            silent: false,
        });
        let _ = x.dur;
    }

    let n = results.len() as f32;
    let n_correct = results.iter().filter(|r| r.correct).count();
    let n_missed = results.iter().filter(|r| !r.matched).count();
    let errs: Vec<f32> = results.iter().filter_map(|r| r.cents_error).collect();
    let mean_abs = if errs.is_empty() {
        None
    } else {
        Some(errs.iter().map(|e| e.abs()).sum::<f32>() / errs.len() as f32)
    };
    let score = n_correct as f32 / n;

    let mut metrics = json!({
        "notes": results,
        "notes_correct": n_correct,
        "notes_total": results.len(),
        "notes_missed": n_missed,
        "extra_notes": segs.len().saturating_sub(alignment.iter().flatten().count()),
        "mean_abs_cents": mean_abs,
        "mode": cfg.mode,
    });
    if cfg.mode == PitchRefMode::AnyOctave {
        metrics["octave_offset"] = json!((octave_shift / 12.0) as i32);
    }
    if matches!(
        cfg.mode,
        PitchRefMode::RelativeToFirst | PitchRefMode::RelativeDriftTolerant
    ) {
        metrics["offset_cents"] = json!(offset_cents);
        metrics["drift_cents_per_bar"] =
            json!(drift_cents_per_s * beat_s * trial.beats_per_bar as f32);
    }
    if trial.timed && !single {
        let on_time = results.iter().filter(|r| r.on_time == Some(true)).count();
        metrics["notes_on_time"] = json!(on_time);
        let onset_errs: Vec<f32> = results.iter().filter_map(|r| r.onset_error_ms).collect();
        if let Some(med) = median_f32(onset_errs) {
            metrics["median_onset_error_ms"] = json!(med);
        }
    }
    // Interval accuracy between the first two sung notes (interval singing).
    if trial.exercise == ExerciseKind::SingInterval && results.len() == 2 {
        if let (Some(a), Some(b)) = (results[0].sung_midi, results[1].sung_midi) {
            let target = (trial.notes[1].midi - trial.notes[0].midi) as f32;
            let interval_err = ((b - a) - target) * 100.0;
            metrics["interval_cents_error"] = json!(interval_err);
            metrics["interval_correct"] = json!(interval_err.abs() <= cfg.category_tol_cents);
        }
    }

    let summary = if single {
        match raw[0].as_ref() {
            Some(m) => {
                let sung = Note((m.target_midi + m.centre_cents / 100.0).round() as i32);
                let mut s = single_note_summary(m, results[0].correct, &sung.display_name());
                if let Some(v) = &m.vibrato {
                    s.push_str(&format!(
                        ", vibrato {:.1} Hz ±{:.0} cents",
                        v.rate_hz, v.extent_cents
                    ));
                }
                if trial.exercise == ExerciseKind::Sustain {
                    s.push_str(&format!(", held {:.1} s", m.duration_ms / 1000.0));
                }
                s
            }
            None => "No singing detected.".to_string(),
        }
    } else {
        let mut s = format!("{n_correct} of {} notes right", results.len());
        if let Some(e) = mean_abs {
            s.push_str(&format!(", average error {} cents", e.round() as i32));
        }
        if n_missed > 0 {
            s.push_str(&format!(", {n_missed} missed"));
        }
        if let Some(e) = metrics.get("interval_cents_error").and_then(|v| v.as_f64()) {
            s.push_str(&format!("; interval {}", describe_cents(e as f32)));
        }
        if cfg.mode == PitchRefMode::RelativeDriftTolerant
            && drift_cents_per_s.abs() * beat_s * trial.beats_per_bar as f32 >= 5.0
        {
            s.push_str(&format!(
                "; drifted {:+.0} cents per bar",
                drift_cents_per_s * beat_s * trial.beats_per_bar as f32
            ));
        }
        if let Some(octaves) = octave_words(octave_shift) {
            s.push_str(&format!(" (sung {octaves})"));
        }
        s
    };
    let correct = if single {
        results[0].correct
    } else {
        n_correct == results.len()
    };
    TrialOutcome {
        correct: Some(correct),
        score: Some(score),
        rt_ms: None,
        response: json!({ "voiced": true, "segments": segs.len() }),
        metrics,
        summary,
    }
}

// ---------------------------------------------------------------------------------------------
// Melodic dictation
// ---------------------------------------------------------------------------------------------

/// One note of the melody and what was written for it.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, TS)]
pub struct DictationNote {
    pub target: Pitch,
    /// Ticks from the start of bar 1.
    pub start: u32,
    pub dur: u32,
    /// Index (among the written notes) of the note aligned with this one.
    pub answer_index: Option<u32>,
    pub written: Option<Pitch>,
    pub pitch_ok: bool,
    /// Written at the same position with the same length.
    pub rhythm_ok: bool,
    /// The right pitch with another spelling (D# for Eb).
    pub respelled: bool,
    pub correct: bool,
}

/// Grades a written melody against the played one. Notes are aligned (so one rhythm slip does
/// not cost every later pitch); each melody note is right when its pitch matches and, with
/// `grade_rhythm`, its position and length too. Written notes aligned to nothing are extra.
pub fn grade_dictation(trial: &DictationTrial, answer: &[ScoreNote], plays: u32) -> TrialOutcome {
    let target: Vec<(Pitch, u32, u32)> = trial
        .score
        .notes
        .iter()
        .filter_map(|n| n.pitch.map(|p| (p, n.start, n.dur)))
        .collect();
    let written: Vec<(Pitch, u32, u32)> = answer
        .iter()
        .filter_map(|n| n.pitch.map(|p| (p, n.start, n.dur)))
        .collect();
    let (n, m) = (target.len(), written.len());
    let sim = |i: usize, j: usize| -> f32 {
        let (tp, ts, td) = target[i];
        let (wp, ws, wd) = written[j];
        let pitch = if tp.midi() == wp.midi() {
            2.0
        } else if tp.step == wp.step {
            0.5
        } else {
            0.0
        };
        let onset = if ts == ws { 1.0 } else { 0.0 };
        let len = if td == wd { 0.5 } else { 0.0 };
        pitch + onset + len - 1.2
    };
    const GAP: f32 = -0.6;
    let mut dp = vec![vec![0.0f32; m + 1]; n + 1];
    for i in 1..=n {
        dp[i][0] = GAP * i as f32;
    }
    for j in 1..=m {
        dp[0][j] = GAP * j as f32;
    }
    for i in 1..=n {
        for j in 1..=m {
            dp[i][j] = (dp[i - 1][j - 1] + sim(i - 1, j - 1))
                .max(dp[i - 1][j] + GAP)
                .max(dp[i][j - 1] + GAP);
        }
    }
    let mut pairs: Vec<(usize, usize)> = Vec::new();
    let (mut i, mut j) = (n, m);
    while i > 0 && j > 0 {
        if (dp[i][j] - (dp[i - 1][j - 1] + sim(i - 1, j - 1))).abs() < 1e-4 {
            pairs.push((i - 1, j - 1));
            i -= 1;
            j -= 1;
        } else if (dp[i][j] - (dp[i - 1][j] + GAP)).abs() < 1e-4 {
            i -= 1;
        } else {
            j -= 1;
        }
    }
    pairs.reverse();

    let mut notes: Vec<DictationNote> = target
        .iter()
        .map(|(p, s, d)| DictationNote {
            target: *p,
            start: *s,
            dur: *d,
            answer_index: None,
            written: None,
            pitch_ok: false,
            rhythm_ok: false,
            respelled: false,
            correct: false,
        })
        .collect();
    let mut answer_marks = vec!["extra"; m];
    for (ti, wi) in &pairs {
        let (tp, ts, td) = target[*ti];
        let (wp, ws, wd) = written[*wi];
        let note = &mut notes[*ti];
        note.answer_index = Some(*wi as u32);
        note.written = Some(wp);
        note.pitch_ok = tp.midi() == wp.midi();
        note.respelled = note.pitch_ok && (tp.step != wp.step || tp.alter != wp.alter);
        note.rhythm_ok = ts == ws && td == wd;
        note.correct = note.pitch_ok && (note.rhythm_ok || !trial.grade_rhythm);
        answer_marks[*wi] = if note.correct { "correct" } else { "wrong" };
    }
    let right = notes.iter().filter(|x| x.correct).count();
    let extra = answer_marks.iter().filter(|x| **x == "extra").count();
    let pitch_right = notes.iter().filter(|x| x.pitch_ok).count();
    let rhythm_right = notes.iter().filter(|x| x.rhythm_ok).count();
    let respelled = notes.iter().filter(|x| x.respelled).count();
    let denom = n.max(m).max(1) as f32;
    let all = right == n && extra == 0;
    let mut summary = if all {
        format!("All {n} notes right.")
    } else {
        format!("{right} of {n} notes right")
    };
    if !all {
        let mut parts = Vec::new();
        if trial.grade_rhythm && pitch_right > right {
            parts.push(format!(
                "{} with the right pitch but a different rhythm",
                pitch_right - right
            ));
        }
        if extra > 0 {
            parts.push(format!("{extra} extra"));
        }
        if m == 0 {
            parts.push("nothing written".into());
        }
        if !parts.is_empty() {
            summary.push_str(&format!(" ({})", parts.join(", ")));
        }
        summary.push('.');
    }
    if respelled > 0 {
        summary.push_str(&format!(" {respelled} spelled differently from the score."));
    }
    TrialOutcome {
        correct: Some(all),
        score: Some(right as f32 / denom),
        rt_ms: None,
        response: json!({ "answer": answer, "plays": plays }),
        metrics: json!({
            "notes": notes,
            "answer_marks": answer_marks,
            "pitch_accuracy": pitch_right as f32 / n.max(1) as f32,
            "rhythm_accuracy": rhythm_right as f32 / n.max(1) as f32,
            "extra_notes": extra,
            "respelled": respelled,
            "plays": plays,
        }),
        summary,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pitch::synth::*;
    use crate::pitch::{TrackOptions, track};
    use crate::schema::{
        Backing, Choice, DetectMarks, DetectTone, DetectTrial, LaneTargets, Mask, TargetNote,
    };

    const SR: f32 = 48_000.0;

    fn id_trial(correct: &str) -> IdentifyTrial {
        IdentifyTrial {
            answer_after_ms: None,
            after_answer: None,
            cue: None,
            pace_ms: None,
            choice_sounds: Vec::new(),
            exercise: ExerciseKind::PitchId,
            prompt: "Name the note".into(),
            events: vec![],
            mask_before: Mask::None,
            choices: PitchClass::ALL
                .iter()
                .map(|p| Choice::new(p.name(), p.display_name()))
                .collect(),
            correct: correct.into(),
            deadline_ms: None,
            reference: None,
            stimulus: json!({}),
            ..IdentifyTrial::skeleton()
        }
    }

    #[test]
    fn naming_scores_ucsf_points_on_the_pitch_class_circle() {
        let points = |correct: &str, answer: Option<&str>| {
            let o = grade_choice(&id_trial(correct), answer, Some(900.0));
            (
                o.metrics.get("class_error").and_then(|v| v.as_i64()),
                o.metrics.get("ucsf_points").and_then(|v| v.as_f64()),
            )
        };
        assert_eq!(points("C", Some("C")), (Some(0), Some(1.0)));
        assert_eq!(points("C", Some("C#")), (Some(1), Some(0.75)));
        // B is a semitone below C around the circle.
        assert_eq!(points("C", Some("B")), (Some(-1), Some(0.75)));
        assert_eq!(points("C", Some("F#")).1, Some(0.0));
        assert_eq!(points("C", None), (None, None));
        // "Other" on a tone from inside the set has no class error.
        assert_eq!(points("C", Some("other")), (None, None));
    }

    #[test]
    fn choice_grading_reports_signed_error() {
        let t = id_trial("E");
        let o = grade_choice(&t, Some("F"), Some(820.0));
        assert_eq!(o.correct, Some(false));
        assert_eq!(o.metrics["signed_error"], json!(1));
        assert!(o.summary.contains("It was E"));
        let ok = grade_choice(&t, Some("E"), Some(700.0));
        assert_eq!(ok.correct, Some(true));
        let none = grade_choice(&t, None, None);
        assert_eq!(none.correct, Some(false));
        assert_eq!(none.metrics["timeout"], json!(true));
    }

    #[test]
    fn sing_pitch_ignores_octave_in_pitch_class_mode() {
        let mut sig = silence(SR, 0.5);
        // Target E♭ (class 3); sung E♭3 15 cents sharp.
        sig.extend(harmonic_tone(SR, 1.2, 440.0, |_| 51.15));
        sig.extend(silence(SR, 0.3));
        let tr = track(&sig, &TrackOptions::new(SR));
        let trial = SingPitchTrial {
            exercise: ExerciseKind::SingPitch,
            answer_color: None,
            prompt_color: None,
            speech: None,
            compare: None,
            compare_ms: 0,
            prompt: "Sing E♭".into(),
            target_class: PitchClass::new(3),
            target_midi: None,
            response_ms: 4000,
            delay_ms: 0,
            pre_mask: Mask::None,
            reveal: None,
            stimulus: json!({}),
        };
        let cfg = GradingConfig::resolve(&GradingOverrides::default(), PitchRefMode::PitchClass);
        let o = grade_sing_pitch(&trial, &tr, &cfg, 0.2);
        assert_eq!(o.correct, Some(true), "{o:?}");
        let c = o.metrics["cents_error"].as_f64().unwrap();
        assert!((c - 15.0).abs() < 1.5, "{c}");
        assert_eq!(o.metrics["sung_octave"], json!(3));
        let latency = o.rt_ms.unwrap();
        assert!((latency - 300.0).abs() < 40.0, "{latency}");
    }

    fn sung_trial(notes: &[(i32, f32, f32)], tempo: f32, exercise: ExerciseKind) -> SungTrial {
        SungTrial {
            exercise,
            prompt: "Sing".into(),
            notes: notes
                .iter()
                .map(|&(m, s, b)| TargetNote {
                    midi: m,
                    start_beat: s,
                    beats: b,
                })
                .collect(),
            tempo_bpm: tempo,
            beats_per_bar: 4,
            sing_start_beat: 4.0,
            total_beats: 4.0 + notes.iter().map(|n| n.1 + n.2).fold(0.0, f32::max) + 1.0,
            reference_beats: vec![],
            reference_timbre: "gm:0".into(),
            backing: Backing::Click,
            backing_volume: 0.7,
            guide: false,
            drone_midi: None,
            lane_targets: LaneTargets::Shown,
            timed: true,
            score: None,
            count_in_beat: None,
            sections: Vec::new(),
            follow_cursor: false,
            accompaniment: Vec::new(),
            just_cents: Vec::new(),
            silent: Vec::new(),
            reveal: None,
            free: None,
            score_until_beat: None,
            stimulus: json!({}),
        }
    }

    #[test]
    fn grades_scale_with_one_wrong_and_one_missed_note() {
        // 120 bpm: 0.5 s per beat; singing starts at 1.0 s in the capture.
        let targets = [
            (60, 0.0, 1.0),
            (62, 1.0, 1.0),
            (64, 2.0, 1.0),
            (65, 3.0, 1.0),
            (67, 4.0, 1.0),
        ];
        let trial = sung_trial(&targets, 120.0, ExerciseKind::Pattern);
        let sung = |t: f32| -> f32 {
            match t {
                t if t < 0.5 => 60.1, // 10 cents sharp
                t if t < 1.0 => 62.0,
                t if t < 1.5 => 63.0, // a semitone flat
                _ => 65.0,            // holds F through the last beat (G missed)
            }
        };
        let mut sig = silence(SR, 1.0);
        sig.extend(harmonic_tone(SR, 2.0, 440.0, sung));
        sig.extend(silence(SR, 1.0));
        let tr = track(&sig, &TrackOptions::new(SR));
        let cfg = GradingConfig::resolve(&GradingOverrides::default(), PitchRefMode::Absolute);
        let o = grade_sung(&trial, &tr, &cfg, 1.0);
        let notes: Vec<NoteResult> = serde_json::from_value(o.metrics["notes"].clone()).unwrap();
        assert_eq!(notes.len(), 5);
        assert!(
            notes[0].correct && notes[1].correct && !notes[2].correct && notes[3].correct,
            "{notes:#?}"
        );
        assert!(!notes[4].correct, "{notes:#?}");
        let e0 = notes[0].cents_error.unwrap();
        assert!((e0 - 10.0).abs() < 2.0, "{e0}");
        assert_eq!(o.correct, Some(false));
        assert!((o.score.unwrap() - 0.6).abs() < 1e-6);
    }

    #[test]
    fn free_answers_are_graded_on_their_pitch_classes() {
        let sing = |notes: &[f32]| {
            let mut sig = silence(SR, 1.0);
            for m in notes {
                let m = *m;
                sig.extend(harmonic_tone(SR, 0.4, 440.0, move |_| m));
                sig.extend(silence(SR, 0.15));
            }
            sig.extend(silence(SR, 0.5));
            track(&sig, &TrackOptions::new(SR))
        };
        let mut trial = sung_trial(&[], 120.0, ExerciseKind::PatternDialogue);
        trial.free = Some(crate::schema::FreeResponse {
            allowed: vec![2, 5, 7, 11],
            names: vec![
                (2, "re".into()),
                (5, "fa".into()),
                (7, "sol".into()),
                (11, "ti".into()),
            ],
            min_notes: 3,
            max_notes: 5,
            differ_from: vec![7, 11, 2],
            what: "a different dominant pattern".into(),
        });
        let cfg = GradingConfig::resolve(&GradingOverrides::default(), PitchRefMode::AnyOctave);
        let ok = grade_sung(&trial, &sing(&[67.0, 65.0, 62.0, 59.0]), &cfg, 1.0);
        assert_eq!(ok.correct, Some(true), "{}", ok.summary);
        assert!(ok.summary.contains("sol fa re ti"), "{}", ok.summary);
        let copied = grade_sung(&trial, &sing(&[67.0, 71.0, 74.0]), &cfg, 1.0);
        assert_eq!(copied.correct, Some(false));
        assert_eq!(copied.metrics["copied"], json!(true));
        let wrong = grade_sung(&trial, &sing(&[67.0, 64.0, 62.0]), &cfg, 1.0);
        assert_eq!(wrong.correct, Some(false), "mi is not a dominant tone");
    }

    #[test]
    fn silent_notes_are_not_graded_and_the_reentry_is() {
        let mut trial = sung_trial(
            &[
                (60, 0.0, 1.0),
                (62, 1.0, 1.0),
                (64, 2.0, 1.0),
                (65, 3.0, 1.0),
            ],
            120.0,
            ExerciseKind::SightSing,
        );
        trial.silent = vec![1, 2];
        trial.reveal = Some("Inner hearing.".into());
        let mut sig = silence(SR, 1.0);
        sig.extend(harmonic_tone(SR, 0.45, 440.0, |_| 60.0));
        sig.extend(silence(SR, 1.05));
        sig.extend(harmonic_tone(SR, 0.45, 440.0, |_| 65.0));
        sig.extend(silence(SR, 0.5));
        let tr = track(&sig, &TrackOptions::new(SR));
        let cfg = GradingConfig::resolve(&GradingOverrides::default(), PitchRefMode::Absolute);
        let o = grade_sung(&trial, &tr, &cfg, 1.0);
        assert_eq!(o.correct, Some(true), "{}", o.summary);
        let notes: Vec<NoteResult> = serde_json::from_value(o.metrics["notes"].clone()).unwrap();
        assert_eq!(notes.len(), 4);
        assert!(notes[1].silent && notes[2].silent && !notes[3].silent);
        assert_eq!(o.metrics["reentry_correct"], json!(true));
        assert!(o.summary.ends_with("Inner hearing."), "{}", o.summary);
    }

    #[test]
    fn relative_mode_forgives_transposition_but_not_intervals() {
        let targets = [(60, 0.0, 1.0), (64, 1.0, 1.0), (67, 2.0, 1.0)];
        let trial = sung_trial(&targets, 120.0, ExerciseKind::Pattern);
        // Sung a whole tone + 30 cents high throughout.
        let sung = |t: f32| -> f32 {
            let base = if t < 0.5 {
                60.0
            } else if t < 1.0 {
                64.0
            } else {
                67.0
            };
            base + 2.3
        };
        let mut sig = silence(SR, 1.0);
        sig.extend(harmonic_tone(SR, 1.5, 440.0, sung));
        sig.extend(silence(SR, 0.5));
        let tr = track(&sig, &TrackOptions::new(SR));
        let abs = grade_sung(
            &trial,
            &tr,
            &GradingConfig::resolve(&GradingOverrides::default(), PitchRefMode::Absolute),
            1.0,
        );
        assert_eq!(abs.score, Some(0.0));
        let rel = grade_sung(
            &trial,
            &tr,
            &GradingConfig::resolve(&GradingOverrides::default(), PitchRefMode::RelativeToFirst),
            1.0,
        );
        assert_eq!(rel.correct, Some(true), "{:?}", rel.metrics);
        let off = rel.metrics["offset_cents"].as_f64().unwrap();
        assert!((off - 230.0).abs() < 3.0, "{off}");
    }

    #[test]
    fn interval_singing_reports_interval_error() {
        let targets = [(57, 0.0, 2.0), (64, 2.0, 2.0)];
        let trial = sung_trial(&targets, 120.0, ExerciseKind::SingInterval);
        // Fifth sung 20 cents narrow.
        let sung = |t: f32| if t < 1.0 { 57.0 } else { 63.8 };
        let mut sig = silence(SR, 1.0);
        sig.extend(harmonic_tone(SR, 2.0, 440.0, sung));
        sig.extend(silence(SR, 0.5));
        let tr = track(&sig, &TrackOptions::new(SR));
        let o = grade_sung(
            &trial,
            &tr,
            &GradingConfig::resolve(&GradingOverrides::default(), PitchRefMode::Absolute),
            1.0,
        );
        let ie = o.metrics["interval_cents_error"].as_f64().unwrap();
        assert!((ie - -20.0).abs() < 3.0, "{ie}");
        assert_eq!(o.metrics["interval_correct"], json!(true));
    }

    #[test]
    fn any_octave_forgives_a_melody_sung_an_octave_down() {
        // C5 D5 E5 sung as C4 D4 E4, the middle note 30 cents sharp.
        let trial = sung_trial(
            &[(72, 0.0, 1.0), (74, 1.0, 1.0), (76, 2.0, 1.0)],
            120.0,
            ExerciseKind::SightSing,
        );
        let sung = |t: f32| {
            if t < 0.5 {
                60.0
            } else if t < 1.0 {
                62.3
            } else {
                64.0
            }
        };
        let mut sig = silence(SR, 1.0);
        sig.extend(harmonic_tone(SR, 1.5, 440.0, sung));
        sig.extend(silence(SR, 0.5));
        let tr = track(&sig, &TrackOptions::new(SR));
        let grade = |mode| {
            grade_sung(
                &trial,
                &tr,
                &GradingConfig::resolve(&GradingOverrides::default(), mode),
                1.0,
            )
        };

        let o = grade(PitchRefMode::AnyOctave);
        assert_eq!(o.correct, Some(true), "{:?}", o.metrics);
        assert_eq!(o.metrics["octave_offset"], json!(-1));
        assert!(
            o.summary.ends_with("(sung an octave lower)"),
            "{}",
            o.summary
        );
        let notes: Vec<NoteResult> = serde_json::from_value(o.metrics["notes"].clone()).unwrap();
        assert_eq!(
            notes.iter().map(|n| n.target_midi).collect::<Vec<_>>(),
            [72, 74, 76]
        );
        let c = notes[1].cents_error.unwrap();
        assert!((c - 30.0).abs() < 3.0, "{c}");

        // Exact pitch still counts the octave as wrong.
        assert_eq!(grade(PitchRefMode::Absolute).score, Some(0.0));
    }

    #[test]
    fn just_tuning_moves_the_targets_by_the_trial_offsets() {
        // E4 and G4 over a C: the pure third is 13.7 cents flat, the fifth 2 cents sharp.
        let mut trial = sung_trial(
            &[(64, 0.0, 2.0), (67, 2.0, 2.0)],
            120.0,
            ExerciseKind::Harmony,
        );
        trial.just_cents = vec![-13.7, 2.0];
        let sung = |t: f32| if t < 1.0 { 63.863 } else { 67.02 };
        let mut sig = silence(SR, 1.0);
        sig.extend(harmonic_tone(SR, 2.0, 440.0, sung));
        sig.extend(silence(SR, 0.5));
        let tr = track(&sig, &TrackOptions::new(SR));
        let errors = |tuning: Tuning| -> Vec<f64> {
            let overrides = GradingOverrides {
                tuning: Some(tuning),
                ..GradingOverrides::default()
            };
            let o = grade_sung(
                &trial,
                &tr,
                &GradingConfig::resolve(&overrides, PitchRefMode::Absolute),
                1.0,
            );
            let notes: Vec<NoteResult> =
                serde_json::from_value(o.metrics["notes"].clone()).unwrap();
            assert_eq!(notes[0].target_midi, 64);
            notes
                .iter()
                .map(|n| n.cents_error.unwrap() as f64)
                .collect()
        };
        let equal = errors(Tuning::Equal);
        assert!(
            (equal[0] + 13.7).abs() < 2.0 && (equal[1] - 2.0).abs() < 2.0,
            "{equal:?}"
        );
        let just = errors(Tuning::Just);
        assert!(just.iter().all(|e| e.abs() < 2.0), "{just:?}");
    }

    fn dictation(notes: &[(&str, u32, u32)], grade_rhythm: bool) -> DictationTrial {
        let m: crate::score::CompactMelody = serde_json::from_value(json!({
            "key": "C major",
            "notes": [{"note": "C4", "beats": 1}]
        }))
        .unwrap();
        let mut score = crate::score::Score::from_compact(&m, 80.0).unwrap();
        score.notes = notes
            .iter()
            .map(|(p, s, d)| ScoreNote {
                pitch: Some(p.parse().unwrap()),
                start: *s,
                dur: *d,
            })
            .collect();
        score.bars = 2;
        DictationTrial {
            prompt: String::new(),
            given: score.clone(),
            playback: sung_trial(&[(60, 0.0, 1.0)], 80.0, ExerciseKind::MelodicDictation),
            score,
            max_plays: 0,
            grade_rhythm,
            stimulus: json!({}),
        }
    }

    fn written(notes: &[(&str, u32, u32)]) -> Vec<ScoreNote> {
        notes
            .iter()
            .map(|(p, s, d)| ScoreNote {
                pitch: Some(p.parse().unwrap()),
                start: *s,
                dur: *d,
            })
            .collect()
    }

    #[test]
    fn dictation_exact_answer_is_all_right() {
        let melody = [("C4", 0, 4), ("D4", 4, 4), ("E4", 8, 8), ("G4", 16, 16)];
        let o = grade_dictation(&dictation(&melody, true), &written(&melody), 2);
        assert_eq!(o.correct, Some(true));
        assert_eq!(o.score, Some(1.0));
        assert_eq!(o.summary, "All 4 notes right.");
    }

    #[test]
    fn dictation_rhythm_slip_keeps_later_pitches_aligned() {
        let melody = [("C4", 0, 4), ("D4", 4, 4), ("E4", 8, 8), ("G4", 16, 16)];
        // D written as an eighth, so E and G are early, but every pitch is right.
        let answer = [("C4", 0, 4), ("D4", 4, 2), ("E4", 6, 8), ("G4", 14, 16)];
        let t = dictation(&melody, true);
        let o = grade_dictation(&t, &written(&answer), 1);
        assert_eq!(o.correct, Some(false));
        let notes: Vec<DictationNote> = serde_json::from_value(o.metrics["notes"].clone()).unwrap();
        assert!(notes.iter().all(|n| n.pitch_ok));
        assert_eq!(notes.iter().filter(|n| n.correct).count(), 1);
        assert!(
            o.summary.contains("right pitch but a different rhythm"),
            "{}",
            o.summary
        );
        // Without rhythm grading the same answer is right.
        let t2 = dictation(&melody, false);
        assert_eq!(
            grade_dictation(&t2, &written(&answer), 1).correct,
            Some(true)
        );
    }

    #[test]
    fn dictation_wrong_extra_and_respelled_notes() {
        let melody = [("C4", 0, 4), ("Eb4", 4, 4), ("F4", 8, 8)];
        let answer = [("C4", 0, 4), ("D#4", 4, 4), ("G4", 8, 4), ("A4", 12, 4)];
        let o = grade_dictation(&dictation(&melody, true), &written(&answer), 3);
        let notes: Vec<DictationNote> = serde_json::from_value(o.metrics["notes"].clone()).unwrap();
        assert!(notes[1].correct && notes[1].respelled);
        assert!(!notes[2].correct);
        assert_eq!(o.metrics["extra_notes"], 1);
        assert_eq!(
            o.metrics["answer_marks"],
            json!(["correct", "correct", "wrong", "extra"])
        );
        assert!(o.summary.contains("spelled differently"), "{}", o.summary);
        let empty = grade_dictation(&dictation(&melody, true), &[], 1);
        assert_eq!(empty.score, Some(0.0));
        assert!(empty.summary.contains("nothing written"));
    }

    #[test]
    fn errorless_hides_the_error_and_keeps_the_true_result() {
        let outcome = grade_choice(&id_trial("E"), Some("C"), Some(800.0));
        assert_eq!(outcome.correct, Some(false));
        let shown = present_feedback(
            FeedbackMode::Errorless,
            outcome.correct,
            &outcome.summary,
            Some("C"),
            "E",
        );
        assert_ne!(shown.correct, Some(false));
        assert!(shown.answered.is_none());
        assert!(shown.errorless);
        assert!(
            !shown.summary.to_ascii_lowercase().contains("wrong"),
            "{}",
            shown.summary
        );
        assert!(shown.summary.contains('E'));
        let honest = present_feedback(
            FeedbackMode::AfterTrial,
            outcome.correct,
            &outcome.summary,
            Some("C"),
            "E",
        );
        assert_eq!(honest.correct, Some(false));
        assert_eq!(honest.answered.as_deref(), Some("C"));
    }

    #[test]
    fn detection_d_prime_matches_the_log_linear_correction() {
        // 4 hits, 0 misses, 0 false alarms, 12 correct rejections:
        // h = 4.5/5, f = 0.5/13, d′ = z(h) − z(f).
        let d = d_prime(4, 0, 0, 12);
        assert!((d - 3.050376604).abs() < 1e-4, "{d}");
        assert!(d.is_finite(), "all hits and no false alarms stay finite");
        let miss = d_prime(0, 4, 12, 0);
        assert!((miss + d).abs() < 1e-6, "{miss}");
        let eight = d_prime(8, 0, 2, 6);
        assert!((eight - 2.182674616).abs() < 1e-4, "{eight}");

        let tone = |target, far| DetectTone {
            events: vec![],
            speech: None,
            target,
            far,
            midi: if far { 48 } else { 60 },
        };
        let trial = DetectTrial {
            exercise: ExerciseKind::PitchDetect,
            prompt: "Listen for C".into(),
            target_class: PitchClass::new(0),
            target_label: "C".into(),
            target_color: None,
            prompt_events: vec![],
            prompt_ms: 1500,
            speech: None,
            tones: vec![
                tone(true, false),
                tone(false, false),
                tone(true, true),
                tone(false, false),
            ],
            window_ms: 1750,
            marks: DetectMarks::Summary,
            stimulus: json!({}),
        };
        let o = grade_detect(&trial, &[Some(400.0), Some(200.0), None, None]);
        assert!(o.correct.is_none());
        assert_eq!(o.metrics["hits"], 1);
        assert_eq!(o.metrics["misses"], 1);
        assert_eq!(o.metrics["false_alarms"], 1);
        assert_eq!(o.metrics["far_targets"], 1);
        assert_eq!(o.metrics["far_hits"], 0);
        assert_eq!(o.rt_ms, Some(400.0));
        assert!(o.summary.contains("d′"), "{}", o.summary);
    }

    #[test]
    fn d_prime_matches_a_hand_computed_fixture() {
        // Log-linear correction: hit rate 8.5/11, false alarms 3.5/21; z(0.7727) − z(0.1667).
        assert!((d_prime(8, 2, 3, 17) - 1.71528).abs() < 1e-3);
        assert!((d_prime(10, 0, 0, 20) - 3.67137).abs() < 1e-3);
    }

    fn match_trial(target: u32, start: u32) -> MatchTrial {
        MatchTrial {
            prompt: String::new(),
            target_step: target,
            start_step: start,
            steps: 27,
            step_cents: 10.0,
            base_midi: 70,
            base_cents: 20.0,
            tone_ms: 1000,
            mask_ms: 2000,
            step_ms: 700,
            timbre: "synth:sine".into(),
            stimulus: json!({}),
        }
    }

    #[test]
    fn note_memory_error_is_signed_in_steps_and_cents() {
        let high = grade_match(&match_trial(11, 1), 14, 13, 6000.0);
        assert_eq!(high.metrics["error_steps"], 3);
        assert_eq!(high.metrics["error_cents"], 30.0);
        assert_eq!(high.metrics["abs_error_cents"], 30.0);
        assert!(high.summary.contains("high"));
        let low = grade_match(&match_trial(11, 27), 9, 9, 6000.0);
        assert_eq!(low.metrics["error_steps"], -2);
        assert_eq!(low.metrics["error_cents"], -20.0);
        assert_eq!(low.metrics["abs_error_steps"], 2);
        assert!(low.summary.contains("low"));
        assert_eq!(
            grade_match(&match_trial(5, 1), 5, 4, 1.0).metrics["error_steps"],
            0
        );
        // Off the ladder is the ladder's end.
        assert_eq!(
            grade_match(&match_trial(23, 27), 40, 0, 1.0).metrics["answer_step"],
            27
        );
    }

    #[test]
    fn span_is_the_longest_length_recalled() {
        let item = |length: u32, seq: &[u8]| crate::schema::SpanItem {
            length,
            sequence: seq.to_vec(),
        };
        let t = SpanTrial {
            prompt: String::new(),
            mode: "tones".into(),
            items: vec![
                item(3, &[1, 2, 3]),
                item(3, &[4, 5, 6]),
                item(4, &[1, 2, 3, 4]),
                item(4, &[5, 6, 7, 8]),
            ],
            per_length: 2,
            ioi_ms: 1000,
            pad: vec![],
            timbre: "gm:0".into(),
            stimulus: json!({ "fallback": true }),
        };
        let out = grade_span(
            &t,
            &["123".into(), "456".into(), "1243".into(), "5 6 7 8".into()],
        );
        assert_eq!(out.metrics["span"], 4);
        assert_eq!(out.metrics["sequences_correct"], 3);
        assert_eq!(out.metrics["fallback"], true);
        let out = grade_span(&t, &["12".into(), "465".into()]);
        assert_eq!(out.metrics["span"], 0);
    }
}
