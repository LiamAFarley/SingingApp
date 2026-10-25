//! Session validation: parse errors with paths, semantic checks, duration estimate and a preview.

use serde::{Deserialize, Serialize};
use serde_json::Value;
use ts_rs::TS;

use crate::generate::{GenContext, block_seed, generate_block};
use crate::schema::{Exercise, Mask, Pacing, Pool, SessionSpec, TrialSpec};

#[derive(Clone, Debug, Serialize, Deserialize, TS)]
pub struct Issue {
    pub path: String,
    pub message: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, TS)]
pub struct TrialPreview {
    pub block: usize,
    pub index: usize,
    pub prompt: String,
    #[ts(type = "Record<string, unknown>")]
    pub stimulus: Value,
    pub correct: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize, TS)]
pub struct ValidationReport {
    pub ok: bool,
    pub errors: Vec<Issue>,
    pub warnings: Vec<Issue>,
    pub trial_count: u32,
    pub estimated_minutes: f64,
    pub preview: Vec<TrialPreview>,
}

fn issue(path: impl Into<String>, message: impl Into<String>) -> Issue {
    Issue {
        path: path.into(),
        message: message.into(),
    }
}

/// Rough wall-clock seconds for one trial, including answering and feedback.
pub fn estimate_trial_seconds(t: &TrialSpec) -> f32 {
    let mask_s = |m: Mask| m.seconds();
    match t {
        TrialSpec::Identify(t) if t.pace_ms.is_some() => t.pace_ms.unwrap_or(0) as f32 / 1000.0,
        TrialSpec::Identify(t) => {
            let span = t
                .events
                .iter()
                .map(|e| e.start_ms + e.dur_ms)
                .fold(0.0, f64::max) as f32
                / 1000.0;
            span + mask_s(t.mask_before) + 2.5 + 1.2
        }
        TrialSpec::SingPitch(t) => {
            (t.delay_ms + t.response_ms) as f32 / 1000.0 + mask_s(t.pre_mask) + 2.5
        }
        TrialSpec::Pairing(t) => {
            let span = t
                .events
                .iter()
                .map(|e| e.start_ms + e.dur_ms)
                .fold(0.0, f64::max) as f32
                / 1000.0;
            span + t.gap_ms as f32 / 1000.0
        }
        TrialSpec::Sung(t) => t.total_ms() as f32 / 1000.0 + 2.5,
        // About three hearings plus writing time.
        TrialSpec::Dictation(t) => {
            t.playback.total_ms() as f32 / 1000.0 * 3.0 + 12.0 * t.score.bars as f32
        }
        TrialSpec::Detect(t) => {
            t.prompt_ms as f32 / 1000.0 + t.tones.len() as f32 * t.window_ms as f32 / 1000.0 + 1.5
        }
    }
}

pub fn validate_value(value: Value, ctx: &GenContext) -> (Option<SessionSpec>, ValidationReport) {
    match SessionSpec::from_value(value) {
        Ok(spec) => {
            let report = validate_session(&spec, ctx);
            (Some(spec), report)
        }
        Err(e) => (
            None,
            ValidationReport {
                ok: false,
                errors: vec![issue(e.path, e.message)],
                warnings: vec![],
                trial_count: 0,
                estimated_minutes: 0.0,
                preview: vec![],
            },
        ),
    }
}

pub fn validate_session(spec: &SessionSpec, ctx: &GenContext) -> ValidationReport {
    let mut errors = Vec::new();
    let mut warnings = Vec::new();
    let mut preview = Vec::new();
    let mut seconds = 0.0f32;
    if spec.title.trim().is_empty() {
        errors.push(issue("title", "title must not be empty"));
    }
    if spec.blocks.is_empty() {
        errors.push(issue("blocks", "a session needs at least one block"));
    }
    if spec.blocks.len() > 40 {
        errors.push(issue("blocks", "at most 40 blocks"));
    }
    if spec.total_trials() > 2000 {
        errors.push(issue("blocks", "at most 2000 trials per session"));
    }
    let base_seed = spec.seed.unwrap_or(0);
    for (i, b) in spec.blocks.iter().enumerate() {
        let path = format!("blocks[{i}]");
        if let Some(pm) = b.pass_mark {
            if !(0.0..=1.0).contains(&pm) {
                errors.push(issue(
                    format!("{path}.pass_mark"),
                    "pass_mark is a fraction between 0 and 1",
                ));
            }
        }
        for t in &b.tags {
            if t.is_empty() || t.contains(char::is_whitespace) {
                errors.push(issue(format!("{path}.tags"), format!("invalid tag `{t}`")));
            }
        }
        match b.exercise.pool() {
            Some(Pool::Training) if b.assessment => warnings.push(issue(
                format!("{path}.params.pool"),
                "assessment blocks should use the holdout pool so results are not inflated by training stimuli",
            )),
            Some(Pool::Holdout) if !b.assessment => warnings.push(issue(
                format!("{path}.params.pool"),
                "holdout stimuli are reserved for assessment; using them for training contaminates later assessments",
            )),
            _ => {}
        }
        if let Exercise::PitchId(p) = &b.exercise {
            // A stream is too fast to chain intervals, and a 13-semitone jump rule defeats it.
            // Under three octaves there is no room for one, and nothing else to suggest.
            let span =
                p.octaves.iter().max().unwrap_or(&0) - p.octaves.iter().min().unwrap_or(&0) + 1;
            let chaining_closed = p.pacing != Pacing::Respond
                || p.min_jump_semitones >= 13
                || p.min_octave_step >= 2
                || span < 3;
            if p.mask == Mask::None
                && b.effective_feedback().shows_trial_result()
                && !chaining_closed
            {
                warnings.push(issue(
                    format!("{path}.params.min_jump_semitones"),
                    "each answer reveals a labelled pitch that can be used as a relative reference for the next trial; spread notes over three octaves with min_jump_semitones 13",
                ));
            }
        }
        if let Exercise::PitchId(p) = &b.exercise {
            let span =
                p.octaves.iter().max().unwrap_or(&0) - p.octaves.iter().min().unwrap_or(&0) + 1;
            if p.min_jump_semitones >= 12 && span < 3 {
                warnings.push(issue(
                    format!("{path}.params.min_jump_semitones"),
                    "a jump rule of an octave or more needs at least three octaves, or tones become predictable",
                ));
            }
            // Every octave needs somewhere to step to, or the step rule pins the middle octaves.
            let step = p.min_octave_step as i32;
            if step > 0
                && p.octaves
                    .iter()
                    .any(|o| !p.octaves.iter().any(|q| (q - o).abs() >= step))
            {
                warnings.push(issue(
                    format!("{path}.params.min_octave_step"),
                    format!("a step of {step} octaves cannot be made from every listed octave; use more octaves (four for a step of 2)"),
                ));
            }
        }
        match generate_block(b, block_seed(base_seed, i, b), ctx) {
            Ok(trials) => {
                for t in &trials {
                    seconds += estimate_trial_seconds(t);
                }
                if let Some(m) = b.pre_mask {
                    seconds += m.seconds();
                }
                if let Some(TrialSpec::Sung(_)) = trials.first() {
                    let out_of_range = trials
                        .iter()
                        .filter_map(|t| match t {
                            TrialSpec::Sung(s) => Some(s),
                            _ => None,
                        })
                        .flat_map(|s| s.notes.iter())
                        .any(|n| !ctx.comfortable.contains(n.midi));
                    if out_of_range {
                        warnings.push(issue(
                            format!("{path}.params.range"),
                            format!(
                                "some notes fall outside the comfortable range {}–{}",
                                ctx.comfortable.low, ctx.comfortable.high
                            ),
                        ));
                    }
                }
                for (k, t) in trials.iter().take(3).enumerate() {
                    preview.push(TrialPreview {
                        block: i,
                        index: k,
                        prompt: t.prompt().to_string(),
                        stimulus: t.stimulus().clone(),
                        correct: match t {
                            TrialSpec::Identify(t) => Some(t.correct.clone()),
                            _ => None,
                        },
                    });
                }
            }
            Err(e) => errors.push(issue(format!("{path}.params"), e.0)),
        }
    }
    ValidationReport {
        ok: errors.is_empty(),
        errors,
        warnings,
        trial_count: spec.total_trials(),
        estimated_minutes: (f64::from(seconds) / 60.0 * 10.0).round() / 10.0,
        preview,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn reports_errors_with_paths() {
        let ctx = GenContext::default();
        let (_, r) = validate_value(
            json!({"title": "x", "blocks": [{"type": "pitch_id", "params": {"timbres": ["gm:999"]}}]}),
            &ctx,
        );
        assert!(!r.ok);
        assert_eq!(r.errors[0].path, "blocks[0].params");
        let (_, r) = validate_value(
            json!({"title": "x", "blocks": [{"type": "pitch_id", "params": {"octaves": "four"}}]}),
            &ctx,
        );
        assert!(!r.ok);
        assert_eq!(r.errors[0].path, "blocks[0]");
    }

    #[test]
    fn warns_about_pool_misuse_and_estimates_time() {
        let ctx = GenContext::default();
        let (_, r) = validate_value(
            json!({"title": "x", "blocks": [
                {"type": "pitch_id", "count": 40, "assessment": true, "params": {}},
                {"type": "pitch_id", "count": 10, "params": {"pool": "holdout"}}
            ]}),
            &ctx,
        );
        assert!(r.ok, "{:?}", r.errors);
        // The pool misuse twice, and the holdout training block shows answers with no jump rule.
        assert_eq!(r.warnings.len(), 3, "{:?}", r.warnings);
        assert!(
            r.warnings
                .iter()
                .any(|w| w.path == "blocks[1].params.min_jump_semitones")
        );
        assert!(
            r.estimated_minutes > 2.0 && r.estimated_minutes < 6.0,
            "{}",
            r.estimated_minutes
        );
        assert_eq!(r.preview.len(), 6);
    }
}
