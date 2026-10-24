//! Deterministic grading. Every function here is pure: same inputs, same grade.

use serde::{Deserialize, Serialize};
use serde_json::json;
use ts_rs::TS;

use crate::metrics::{
    NoteMetricOptions, NoteMetrics, NoteSegment, SegmentOptions, note_metrics, segment_notes,
};
use crate::pitch::PitchTrack;
use crate::schema::{
    DictationTrial, ExerciseKind, GradingOverrides, IdentifyTrial, PitchRefMode, SingPitchTrial,
    SungTrial, Tuning,
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
    let summary = match answer {
        None => format!("No answer. It was {}.", label_of(&trial.correct)),
        Some(_) if correct => format!("Correct: {}.", label_of(&trial.correct)),
        Some(a) => format!(
            "You answered {}. It was {}.",
            label_of(a),
            label_of(&trial.correct)
        ),
    };
    TrialOutcome {
        correct: Some(correct),
        score: Some(if correct { 1.0 } else { 0.0 }),
        rt_ms,
        response: json!({ "answer": answer }),
        metrics,
        summary,
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

/// Grades a sung timeline. `sing_start_s` is the capture time at which the trial's
/// `sing_start_beat` falls (already latency-compensated).
pub fn grade_sung(
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
        PitchRefMode::Absolute => abs_errors.clone(),
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
            target_midi: x.midi.round() as i32,
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
        let pitch = if tp.midi() == wp.midi() { 2.0 } else if tp.step == wp.step { 0.5 } else { 0.0 };
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
            parts.push(format!("{} with the right pitch but a different rhythm", pitch_right - right));
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
        summary.push_str(&format!(
            " {respelled} spelled differently from the score."
        ));
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
    use crate::schema::{Backing, Choice, LaneTargets, Mask, TargetNote};

    const SR: f32 = 48_000.0;

    fn id_trial(correct: &str) -> IdentifyTrial {
        IdentifyTrial {
            answer_after_ms: None,
            after_answer: None,
            cue: None,
            choice_sounds: Vec::new(),
            exercise: ExerciseKind::PitchId,
            prompt: "Name the note".into(),
            events: vec![],
            mask_before: Mask::None,
            choices: PitchClass::ALL
                .iter()
                .map(|p| Choice {
                    id: p.name().into(),
                    label: p.display_name().into(),
                })
                .collect(),
            correct: correct.into(),
            deadline_ms: None,
            reference: None,
            stimulus: json!({}),
        }
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
    fn just_tuning_moves_the_targets_by_the_trial_offsets() {
        // E4 and G4 over a C: the pure third is 13.7 cents flat, the fifth 2 cents sharp.
        let mut trial = sung_trial(&[(64, 0.0, 2.0), (67, 2.0, 2.0)], 120.0, ExerciseKind::Harmony);
        trial.just_cents = vec![-13.7, 2.0];
        let sung = |t: f32| if t < 1.0 { 63.863 } else { 67.02 };
        let mut sig = silence(SR, 1.0);
        sig.extend(harmonic_tone(SR, 2.0, 440.0, sung));
        sig.extend(silence(SR, 0.5));
        let tr = track(&sig, &TrackOptions::new(SR));
        let errors = |tuning: Tuning| -> Vec<f64> {
            let overrides = GradingOverrides { tuning: Some(tuning), ..GradingOverrides::default() };
            let o = grade_sung(&trial, &tr, &GradingConfig::resolve(&overrides, PitchRefMode::Absolute), 1.0);
            let notes: Vec<NoteResult> = serde_json::from_value(o.metrics["notes"].clone()).unwrap();
            assert_eq!(notes[0].target_midi, 64);
            notes.iter().map(|n| n.cents_error.unwrap() as f64).collect()
        };
        let equal = errors(Tuning::Equal);
        assert!((equal[0] + 13.7).abs() < 2.0 && (equal[1] - 2.0).abs() < 2.0, "{equal:?}");
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
            .map(|(p, s, d)| ScoreNote { pitch: Some(p.parse().unwrap()), start: *s, dur: *d })
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
            .map(|(p, s, d)| ScoreNote { pitch: Some(p.parse().unwrap()), start: *s, dur: *d })
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
        assert!(o.summary.contains("right pitch but a different rhythm"), "{}", o.summary);
        // Without rhythm grading the same answer is right.
        let t2 = dictation(&melody, false);
        assert_eq!(grade_dictation(&t2, &written(&answer), 1).correct, Some(true));
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
        assert_eq!(o.metrics["answer_marks"], json!(["correct", "correct", "wrong", "extra"]));
        assert!(o.summary.contains("spelled differently"), "{}", o.summary);
        let empty = grade_dictation(&dictation(&melody, true), &[], 1);
        assert_eq!(empty.score, Some(0.0));
        assert!(empty.summary.contains("nothing written"));
    }
}
