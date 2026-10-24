//! Built-in session presets. They work without the LLM and are what the app shows on first run.

use serde::{Deserialize, Serialize};
use serde_json::json;
use ts_rs::TS;

use crate::schema::{
    AllKeyword, BlockSpec, ClassSet, Exercise, HarmonyVoice, PitchIdParams, SessionSpec, SingPitchParams,
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

/// Stage `n` (1–10) trains the first `n + 2` classes of [`AP_PROGRESSIVE_ORDER`]: only those notes
/// are played and offered, with a response deadline that tightens from 4 s to 2.2 s.
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

pub const DEGREE_LEVELS: u32 = 8;

/// Level `n` (1–8) of the scale-degree course, after Functional Ear Trainer: a cadence sets the
/// key, a note plays, you name its degree, and the note then walks to the tonic. The early levels
/// keep one key and a few degrees; later ones change the key every question and add the minor
/// mode and the chromatic degrees.
pub fn scale_degree_level(n: u32) -> SessionSpec {
    let n = n.clamp(1, DEGREE_LEVELS);
    const CHROMATIC: [&str; 12] = ["1", "b2", "2", "b3", "3", "4", "#4", "5", "b6", "6", "b7", "7"];
    let (summary, params) = match n {
        1 => ("Degrees 1, 2 and 3 in C major.", json!({ "degrees": ["1", "2", "3"] })),
        2 => ("Degrees 1 to 5 in C major.", json!({ "degrees": ["1", "2", "3", "4", "5"] })),
        3 => ("All seven degrees of C major, over two octaves.", json!({ "octaves": [-1, 0] })),
        4 => (
            "All seven degrees; the major key changes every question.",
            json!({ "keys": "all", "key_change": "per_trial", "octaves": [-1, 0] }),
        ),
        5 => ("All seven degrees of C minor.", json!({ "modes": ["minor"], "octaves": [-1, 0] })),
        6 => (
            "Major and minor keys; the key changes every question.",
            json!({ "keys": "all", "modes": ["major", "minor"], "key_change": "per_trial", "octaves": [-1, 0] }),
        ),
        7 => (
            "All twelve degrees, chromatic ones included, in C major.",
            json!({ "degrees": CHROMATIC, "octaves": [-1, 0] }),
        ),
        _ => (
            "All twelve degrees; the key changes every question.",
            json!({ "degrees": CHROMATIC, "keys": "all", "key_change": "per_trial", "octaves": [-1, 0] }),
        ),
    };
    let spec = json!({
        "title": format!("Scale degrees: level {n}"),
        "protocol": "scale_degrees",
        "notes": format!("{summary} Suggested criterion for the next level: at least 90% over two sessions."),
        "blocks": [{
            "type": "scale_degree_id",
            "count": 30,
            "pass_mark": 0.9,
            "tags": ["rp.degree.course"],
            "params": params,
        }],
    });
    SessionSpec::from_value(spec).expect("scale degree level is valid")
}

/// The harmony course: one interval per level, thirds and sixths before fifths and fourths.
pub const HARMONY_LEVELS: [HarmonyVoice; 8] = [
    HarmonyVoice::ThirdBelow,
    HarmonyVoice::ThirdAbove,
    HarmonyVoice::SixthBelow,
    HarmonyVoice::SixthAbove,
    HarmonyVoice::FifthBelow,
    HarmonyVoice::FifthAbove,
    HarmonyVoice::FourthBelow,
    HarmonyVoice::FourthAbove,
];

/// Harmony level `n` (1–8): a voice that interval away from generated melodies, three times with
/// your part playing along and then six times by ear.
pub fn harmony_level(n: u32) -> SessionSpec {
    let voice = HARMONY_LEVELS[(n.clamp(1, HARMONY_LEVELS.len() as u32) - 1) as usize];
    let name = voice.describe();
    let source = json!({ "parallel": { "voice": voice, "melody": { "bars": 2, "max_leap": 2, "step_rate": 0.85 } } });
    let spec = json!({
        "title": format!("Harmony: {name}"),
        "protocol": "harmony_intervals",
        "notes": format!("Sing {name} the melody: three times hearing your part, then six times by ear."),
        "blocks": [
            { "type": "harmony", "title": "Practice: your part plays along", "count": 3,
              "params": { "source": source, "practice": true } },
            { "type": "harmony", "title": "By ear", "count": 6, "params": { "source": source } },
        ],
    });
    SessionSpec::from_value(spec).expect("harmony level is valid")
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
        "ap_incidental",
        "Absolute pitch",
        "A quick colour task in which each colour always comes with the same note. Exposure only.",
        include_str!("../presets/ap_incidental.json"),
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
        "ear_intervals_in_key",
        "Ear training",
        "Diatonic intervals in C major after a cadence, both notes from the scale.",
        include_str!("../presets/ear_intervals_in_key.json"),
    ),
    (
        "ear_intervals_changing_keys",
        "Ear training",
        "Diatonic intervals with a cadence in a new major or minor key every question.",
        include_str!("../presets/ear_intervals_changing_keys.json"),
    ),
    (
        "ear_interval_comparison",
        "Ear training",
        "Two intervals in a row: say which is larger, or that they are the same.",
        include_str!("../presets/ear_interval_comparison.json"),
    ),
    (
        "ear_degrees_up",
        "Ear training",
        "Name the degree in C major; every note walks up the scale to the tonic.",
        include_str!("../presets/ear_degrees_up.json"),
    ),
    (
        "ear_degrees_down",
        "Ear training",
        "Name the degree in C major; every note walks down the scale to the tonic.",
        include_str!("../presets/ear_degrees_down.json"),
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
    (
        "harmony_read",
        "Singing",
        "Read your harmony part from the staff while the melody plays.",
        include_str!("../presets/harmony_read.json"),
    ),
    (
        "harmony_just",
        "Singing",
        "Long notes a third below a choir voice, graded against pure (just) thirds.",
        include_str!("../presets/harmony_just.json"),
    ),
    (
        "sight_stepwise",
        "Sight singing and dictation",
        "Read four bars of mostly stepwise melody from the staff and sing them in time after the key is set.",
        include_str!("../presets/sight_stepwise.json"),
    ),
    (
        "sight_leaps",
        "Sight singing and dictation",
        "Leaps up to a fifth, eighth and dotted notes, major and minor keys, 4/4 and 3/4.",
        include_str!("../presets/sight_leaps.json"),
    ),
    (
        "sight_no_reference",
        "Sight singing and dictation",
        "Nothing is played first: start on the written pitch from memory (absolute pitch reading).",
        include_str!("../presets/sight_no_reference.json"),
    ),
    (
        "dictation_stepwise",
        "Sight singing and dictation",
        "Hear two bars of mostly stepwise melody and write them on the staff.",
        include_str!("../presets/dictation_stepwise.json"),
    ),
    (
        "dictation_leaps",
        "Sight singing and dictation",
        "Four bars with leaps up to a fifth and eighth notes, major and minor, five hearings.",
        include_str!("../presets/dictation_leaps.json"),
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
    for n in 1..=DEGREE_LEVELS {
        let spec = scale_degree_level(n);
        out.push(Preset {
            id: format!("degrees_{n:02}"),
            title: spec.title.clone(),
            category: "Ear training".into(),
            description: spec.notes.clone().unwrap_or_default(),
            spec: serde_json::to_value(&spec).unwrap(),
        });
    }
    for n in 1..=HARMONY_LEVELS.len() as u32 {
        let spec = harmony_level(n);
        out.push(Preset {
            id: format!("harmony_{n:02}"),
            title: spec.title.clone(),
            category: "Singing".into(),
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
    use crate::schema::KeyChange;
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
    fn degree_levels_grow() {
        let first = scale_degree_level(1);
        let last = scale_degree_level(DEGREE_LEVELS);
        match (&first.blocks[0].exercise, &last.blocks[0].exercise) {
            (Exercise::ScaleDegreeId(a), Exercise::ScaleDegreeId(b)) => {
                assert_eq!(a.degrees, ["1", "2", "3"]);
                assert_eq!(a.keys.classes().len(), 1);
                assert_eq!(b.degrees.len(), 12);
                assert!(b.keys.is_all());
                assert_eq!(b.key_change, KeyChange::PerTrial);
            }
            _ => panic!(),
        }
    }

    #[test]
    fn harmony_levels_practise_then_sing_by_ear() {
        for n in 1..=HARMONY_LEVELS.len() as u32 {
            let spec = harmony_level(n);
            let [practice, test] = &spec.blocks[..] else { panic!("two blocks") };
            match (&practice.exercise, &test.exercise) {
                (Exercise::Harmony(a), Exercise::Harmony(b)) => {
                    assert!(a.practice && !b.practice);
                    assert_eq!(a.source, b.source);
                }
                _ => panic!(),
            }
        }
        assert_eq!(harmony_level(5).title, "Harmony: a fifth below");
    }

    #[test]
    fn progressive_stages_grow() {
        let s1 = ap_progressive_stage(1);
        let s10 = ap_progressive_stage(10);
        match (&s1.blocks[0].exercise, &s10.blocks[0].exercise) {
            (Exercise::PitchId(a), Exercise::PitchId(b)) => {
                assert_eq!(a.classes.classes().len(), 3);
                assert!(b.classes.is_all());
                assert_eq!(a.deadline_ms, Some(4000));
                assert_eq!(b.deadline_ms, Some(2200));
            }
            _ => panic!(),
        }
    }
}
