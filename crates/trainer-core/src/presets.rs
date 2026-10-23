//! Built-in session presets. They work without the LLM and are what the app shows on first run.

use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::schema::{
    AllKeyword, BlockSpec, ClassSet, Exercise, PitchIdParams, SessionSpec, SingPitchParams,
};
use crate::theory::PitchClass;

#[derive(Clone, Debug, Serialize, Deserialize, TS)]
pub struct Preset {
    pub id: String,
    pub title: String,
    /// `Absolute pitch`, `Ear training` or `Singing`.
    pub category: String,
    pub description: String,
    #[ts(type = "Record<string, unknown>")]
    pub spec: serde_json::Value,
}

/// Order in which pitch classes are added in progressive category training: maximally spread
/// classes first, adjacent semitones last.
pub const AP_PROGRESSIVE_ORDER: [&str; 12] = [
    "C", "E", "G#", "D", "F#", "A#", "C#", "F", "A", "D#", "G", "B",
];
pub const AP_PROGRESSIVE_STAGES: u32 = 10;

/// Stage `n` (1–10) trains the first `n + 2` classes of [`AP_PROGRESSIVE_ORDER`], with
/// out-of-set distractors and a response deadline that tightens from 4 s to 2.2 s.
pub fn ap_progressive_stage(n: u32) -> SessionSpec {
    let n = n.clamp(1, AP_PROGRESSIVE_STAGES);
    let classes: Vec<PitchClass> = AP_PROGRESSIVE_ORDER[..(n as usize + 2)]
        .iter()
        .map(|s| s.parse().unwrap())
        .collect();
    let all = classes.len() == 12;
    let set = if all {
        ClassSet::Keyword(AllKeyword::All)
    } else {
        ClassSet::List(classes.clone())
    };
    let mut id = PitchIdParams {
        classes: set.clone(),
        ..PitchIdParams::default()
    };
    id.out_of_set_rate = if all { 0.0 } else { 0.15 };
    id.deadline_ms = Some(4000 - (n - 1) * 200);
    let mut id_block = BlockSpec::new(Exercise::PitchId(id), 60);
    id_block.title = Some("Name the note".into());
    id_block.pass_mark = Some(0.9);
    id_block.tags = vec!["ap.progressive".into()];
    let sing = SingPitchParams {
        classes: set,
        ..SingPitchParams::default()
    };
    let mut sing_block = BlockSpec::new(Exercise::SingPitch(sing), 12);
    sing_block.title = Some("Sing the note".into());
    sing_block.tags = vec!["ap.progressive".into()];
    SessionSpec {
        title: format!("Absolute pitch: stage {n}"),
        protocol: Some("ap_progressive".into()),
        planned_minutes: None,
        notes: Some(format!(
            "Classes: {}. Suggested criterion for the next stage: at least 90% over two sessions.",
            classes
                .iter()
                .map(|c| c.display_name())
                .collect::<Vec<_>>()
                .join(", ")
        )),
        seed: None,
        blocks: vec![id_block, sing_block],
    }
}

const FILES: &[(&str, &str, &str, &str)] = &[
    (
        "ap_assessment",
        "Absolute pitch",
        "Holdout tones, no feedback: identification across octaves and timbres, speed, singing and key. Run first in a session.",
        include_str!("../presets/ap_assessment.json"),
    ),
    (
        "ap_daily",
        "Absolute pitch",
        "Name 36 notes and sing 12, all twelve pitch classes.",
        include_str!("../presets/ap_daily.json"),
    ),
    (
        "ap_pairing",
        "Absolute pitch",
        "Listen: each tone followed by its name. Passive exposure.",
        include_str!("../presets/ap_pairing.json"),
    ),
    (
        "ap_key_finding",
        "Absolute pitch",
        "Name the key of a cadence with no reference tone.",
        include_str!("../presets/ap_key_finding.json"),
    ),
    (
        "ear_intervals_melodic",
        "Ear training",
        "Melodic intervals up and down, minor 2nd to octave.",
        include_str!("../presets/ear_intervals_melodic.json"),
    ),
    (
        "ear_intervals_harmonic",
        "Ear training",
        "Harmonic intervals, minor 2nd to octave.",
        include_str!("../presets/ear_intervals_harmonic.json"),
    ),
    (
        "ear_chords",
        "Ear training",
        "Triads, inversions and seventh chords.",
        include_str!("../presets/ear_chords.json"),
    ),
    (
        "ear_scales",
        "Ear training",
        "Major and minor scales, then the modes.",
        include_str!("../presets/ear_scales.json"),
    ),
    (
        "ear_key_finding",
        "Ear training",
        "Find the key of a cadence after a labelled A.",
        include_str!("../presets/ear_key_finding.json"),
    ),
    (
        "sing_pitch_matching",
        "Singing",
        "Hear a note and sing it back, then hold notes steadily.",
        include_str!("../presets/sing_pitch_matching.json"),
    ),
    (
        "sing_warmup",
        "Singing",
        "Five-note scales and arpeggios, moving up a semitone at a time.",
        include_str!("../presets/sing_warmup.json"),
    ),
    (
        "sing_scales",
        "Singing",
        "Listen to a scale, then sing it back in time.",
        include_str!("../presets/sing_scales.json"),
    ),
    (
        "sing_intervals",
        "Singing",
        "The first note is given; sing the interval in time.",
        include_str!("../presets/sing_intervals.json"),
    ),
];

pub fn builtin_presets() -> Vec<Preset> {
    let mut out = Vec::new();
    for n in 1..=AP_PROGRESSIVE_STAGES {
        let spec = ap_progressive_stage(n);
        out.push(Preset {
            id: format!("ap_progressive_{n:02}"),
            title: spec.title.clone(),
            category: "Absolute pitch".into(),
            description: spec.notes.clone().unwrap_or_default(),
            spec: serde_json::to_value(&spec).unwrap(),
        });
    }
    for (id, category, description, json) in FILES {
        let spec =
            SessionSpec::from_json(json).unwrap_or_else(|e| panic!("preset {id} is invalid: {e}"));
        out.push(Preset {
            id: id.to_string(),
            title: spec.title.clone(),
            category: category.to_string(),
            description: description.to_string(),
            spec: serde_json::to_value(&spec).unwrap(),
        });
    }
    out
}

pub fn find_builtin(id: &str) -> Option<SessionSpec> {
    builtin_presets()
        .into_iter()
        .find(|p| p.id == id)
        .and_then(|p| SessionSpec::from_value(p.spec).ok())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::generate::GenContext;
    use crate::validate::validate_session;

    #[test]
    fn all_presets_validate() {
        let ctx = GenContext::default();
        for p in builtin_presets() {
            let spec = SessionSpec::from_value(p.spec.clone()).unwrap();
            let r = validate_session(&spec, &ctx);
            assert!(r.ok, "preset {} has errors: {:?}", p.id, r.errors);
            // The assessment's own pool choices must not trigger the pool warnings.
            if p.id == "ap_assessment" {
                assert!(r.warnings.is_empty(), "{:?}", r.warnings);
            }
        }
    }

    #[test]
    fn progressive_stages_grow() {
        let s1 = ap_progressive_stage(1);
        let s10 = ap_progressive_stage(10);
        match (&s1.blocks[0].exercise, &s10.blocks[0].exercise) {
            (Exercise::PitchId(a), Exercise::PitchId(b)) => {
                assert_eq!(a.classes.classes().len(), 3);
                assert!(b.classes.is_all());
                assert_eq!(b.out_of_set_rate, 0.0);
                assert_eq!(a.deadline_ms, Some(4000));
                assert_eq!(b.deadline_ms, Some(2200));
            }
            _ => panic!(),
        }
    }
}
