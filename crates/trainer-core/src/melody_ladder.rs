//! The shared melodic ladder for sight singing, dictation and singback.
//!
//! One list of steps supplies the bars, keys, rhythm vocabulary and leap ceiling. The three
//! courses differ only in what the learner does with the melody.

use serde_json::{Value, json};

use crate::courses::{Course, PassRule};
use crate::rp_courses::{Module, lesson, module, workshop};

pub const RP_SIGHT_V2: &str = "rp_sight_v2";
pub const RP_DICTATION_V2: &str = "rp_dictation_v2";
pub const RP_SINGBACK_V2: &str = "rp_singback_v2";

/// Lessons per generated ladder, not bars.
pub const TRIALS: u32 = 8;
/// Seven of eight is a pass; requiring eight of eight from a generated melody would be brittle.
pub const PASS: f32 = 0.85;

const ALL_MAJOR: &[&str] = &[
    "C", "G", "D", "A", "E", "B", "F#", "F", "Bb", "Eb", "Ab", "Db", "Gb",
];
const ALL_MINOR: &[&str] = &[
    "A", "E", "B", "F#", "C#", "G#", "D#", "D", "G", "C", "F", "Bb",
];

const RHY_BASIC: &[&str] = &["half", "quarter"];
const RHY_EIGHTH: &[&str] = &["half", "quarter", "eighth"];
const RHY_DOTTED: &[&str] = &["half", "quarter", "eighth", "dotted_quarter"];
const RHY_DOTTED8: &[&str] = &[
    "half",
    "quarter",
    "eighth",
    "dotted_quarter",
    "dotted_eighth",
    "sixteenth",
];
const RHY_SIXTEENTH: &[&str] = &[
    "half",
    "quarter",
    "eighth",
    "dotted_quarter",
    "dotted_eighth",
    "sixteenth",
];
const RHY_MIXED: &[&str] = &[
    "whole",
    "dotted_half",
    "half",
    "dotted_quarter",
    "quarter",
    "dotted_eighth",
    "eighth",
    "sixteenth",
];

const ALL_DEGREES: &[u8] = &[1, 2, 3, 4, 5, 6, 7, 8];
const DO_SOL: &[u8] = &[1, 2, 3, 4, 5];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LadderMode {
    Sight,
    Dictation,
    Singback,
}

impl LadderMode {
    pub fn id(self) -> &'static str {
        match self {
            LadderMode::Sight => RP_SIGHT_V2,
            LadderMode::Dictation => RP_DICTATION_V2,
            LadderMode::Singback => RP_SINGBACK_V2,
        }
    }

    pub fn title(self) -> &'static str {
        match self {
            LadderMode::Sight => "Melodic sight singing",
            LadderMode::Dictation => "Melodic dictation",
            LadderMode::Singback => "Melody singback",
        }
    }

    pub fn short(self) -> &'static str {
        match self {
            LadderMode::Sight => "Sight singing",
            LadderMode::Dictation => "Dictation",
            LadderMode::Singback => "Singback",
        }
    }

    pub fn kind(self) -> &'static str {
        match self {
            LadderMode::Dictation => "melodic_dictation",
            LadderMode::Sight | LadderMode::Singback => "sight_sing",
        }
    }

    pub fn description(self) -> &'static str {
        match self {
            LadderMode::Sight => {
                "Read a generated melody from the staff. Two bars of do–sol first, then the full scale; rhythm and leaps grow before the keys widen, and every lesson is eight melodies."
            }
            LadderMode::Dictation => {
                "Hear the same generated ladder and write it on the staff. The key and first note are given; pitch and rhythm are graded."
            }
            LadderMode::Singback => {
                "Hear the same generated ladder after its key, with the score hidden, and sing it back. Pitch and timing are graded."
            }
        }
    }

    fn extra(self) -> Value {
        match self {
            LadderMode::Sight => json!({ "mode": "read" }),
            LadderMode::Dictation => json!({
                "max_plays": 2,
                "grade_rhythm": true,
                "give_first_note": true,
                "play_key": true,
            }),
            LadderMode::Singback => json!({ "mode": "singback", "start": "key_only" }),
        }
    }
}

/// One rung of the shared ladder.
pub struct LadderStep {
    pub module: &'static str,
    pub title: &'static str,
    pub summary: &'static str,
    pub bars: u32,
    pub keys: &'static [&'static str],
    pub rhythms: &'static [&'static str],
    pub max_leap: u8,
    pub degrees: &'static [u8],
    pub syncopation: f32,
    pub run_rate: f32,
    pub minor: bool,
}

impl LadderStep {
    pub fn params(&self, mode: LadderMode) -> Value {
        let modes = if self.minor {
            json!(["minor"])
        } else if self.title == "Major and minor" {
            json!(["major", "minor"])
        } else {
            json!(["major"])
        };
        let generated = json!({
            "bars": self.bars,
            "keys": self.keys,
            "modes": modes,
            "meters": ["4/4"],
            "rhythms": self.rhythms,
            "max_leap": self.max_leap,
            "step_rate": if self.max_leap <= 2 { 0.85 } else { 0.72 },
            "syncopation": self.syncopation,
            "run_rate": self.run_rate,
            "degrees": self.degrees,
        });
        crate::rp_courses::merge(crate::rp_courses::melody(generated), mode.extra())
    }
}

macro_rules! step {
    ($module:literal, $title:literal, $summary:literal, $bars:literal, $keys:expr, $rhythms:expr,
     $leap:literal, $degrees:expr, $sync:literal, $minor:literal) => {
        LadderStep {
            module: $module,
            title: $title,
            summary: $summary,
            bars: $bars,
            keys: $keys,
            rhythms: $rhythms,
            max_leap: $leap,
            degrees: $degrees,
            syncopation: $sync,
            run_rate: 0.0,
            minor: $minor,
        }
    };
}

/// The whole ladder, in order. Steps 1–22 are major; 23–26 are minor and mixed review.
pub fn steps() -> Vec<LadderStep> {
    let mut steps = vec![
        step!(
            "Start",
            "Do to sol",
            "Two bars, C major, half and quarter notes, steps only, using only do–sol.",
            2,
            &["C"],
            RHY_BASIC,
            1,
            DO_SOL,
            0.0,
            false
        ),
        step!(
            "Start",
            "The whole scale",
            "Two bars, C major, half and quarter notes, steps only, the full scale.",
            2,
            &["C"],
            RHY_BASIC,
            1,
            ALL_DEGREES,
            0.0,
            false
        ),
        step!(
            "C major, 4 bars",
            "Four bars",
            "Four bars in C major, still steps and half/quarter notes.",
            4,
            &["C"],
            RHY_BASIC,
            1,
            ALL_DEGREES,
            0.0,
            false
        ),
        step!(
            "C major, 4 bars",
            "Eighth notes",
            "The first eighth notes, in C major, steps only.",
            4,
            &["C"],
            RHY_EIGHTH,
            1,
            ALL_DEGREES,
            0.0,
            false
        ),
        step!(
            "C major, 4 bars",
            "Thirds",
            "Steps and thirds, with eighth notes, in C major.",
            4,
            &["C"],
            RHY_EIGHTH,
            2,
            ALL_DEGREES,
            0.0,
            false
        ),
        step!(
            "One accidental",
            "F and G major",
            "The same melodic demand in F and G major; both keys appear in the eight trials.",
            4,
            &["F", "G"],
            RHY_EIGHTH,
            2,
            ALL_DEGREES,
            0.0,
            false
        ),
        step!(
            "One accidental",
            "Dotted quarter",
            "Dotted quarter/eighth pairs, steps and thirds, in F and G major.",
            4,
            &["F", "G"],
            RHY_DOTTED,
            2,
            ALL_DEGREES,
            0.0,
            false
        ),
        step!(
            "Two accidentals",
            "B-flat and D major",
            "Two-accidental keys, with dotted quarter/eighth rhythms.",
            4,
            &["Bb", "D"],
            RHY_DOTTED,
            2,
            ALL_DEGREES,
            0.0,
            false
        ),
        step!(
            "Two accidentals",
            "Dotted eighth and sixteenth",
            "Dotted-eighth/sixteenth pairs arrive in B-flat and D major.",
            4,
            &["Bb", "D"],
            RHY_DOTTED8,
            2,
            ALL_DEGREES,
            0.0,
            false
        ),
        step!(
            "Two accidentals",
            "Fourths",
            "Leaps widen to fourths in B-flat and D major.",
            4,
            &["Bb", "D"],
            RHY_DOTTED8,
            3,
            ALL_DEGREES,
            0.0,
            false
        ),
        step!(
            "Three accidentals",
            "E-flat and A major",
            "Three-accidental keys, fourths, dotted eighths and sixteenths.",
            4,
            &["Eb", "A"],
            RHY_DOTTED8,
            3,
            ALL_DEGREES,
            0.0,
            false
        ),
        step!(
            "Three accidentals",
            "Sixteenth runs",
            "Sixteenth-note runs in E-flat and A major.",
            4,
            &["Eb", "A"],
            RHY_SIXTEENTH,
            3,
            ALL_DEGREES,
            0.0,
            false
        ),
        step!(
            "Four accidentals",
            "A-flat and E major",
            "Four-accidental keys with sixteenth runs and fourths.",
            4,
            &["Ab", "E"],
            RHY_SIXTEENTH,
            3,
            ALL_DEGREES,
            0.0,
            false
        ),
        step!(
            "Four accidentals",
            "Fifths",
            "Leaps widen to fifths in A-flat and E major.",
            4,
            &["Ab", "E"],
            RHY_SIXTEENTH,
            4,
            ALL_DEGREES,
            0.0,
            false
        ),
        step!(
            "Five accidentals",
            "D-flat and B major",
            "Five-accidental keys, fifths and sixteenth runs.",
            4,
            &["Db", "B"],
            RHY_SIXTEENTH,
            4,
            ALL_DEGREES,
            0.0,
            false
        ),
        step!(
            "Five accidentals",
            "Syncopation",
            "Offbeat attacks begin: an eighth on the beat followed by a quarter.",
            4,
            &["Db", "B"],
            RHY_SIXTEENTH,
            4,
            ALL_DEGREES,
            0.45,
            false
        ),
        step!(
            "Six accidentals",
            "G-flat and F-sharp major",
            "The last two major spellings, with syncopation and fifths.",
            4,
            &["Gb", "F#"],
            RHY_SIXTEENTH,
            4,
            ALL_DEGREES,
            0.45,
            false
        ),
        step!(
            "Six accidentals",
            "Sixths",
            "Leaps widen to sixths in G-flat and F-sharp major.",
            4,
            &["Gb", "F#"],
            RHY_SIXTEENTH,
            5,
            ALL_DEGREES,
            0.45,
            false
        ),
        step!(
            "All major, 8 bars",
            "Eight bars",
            "Length grows to eight bars; all major keys, syncopation and mixed eighths.",
            8,
            ALL_MAJOR,
            RHY_MIXED,
            5,
            ALL_DEGREES,
            0.4,
            false
        ),
        step!(
            "All major, 8 bars",
            "Octave leap",
            "The octave leap arrives in all major keys.",
            8,
            ALL_MAJOR,
            RHY_MIXED,
            7,
            ALL_DEGREES,
            0.4,
            false
        ),
        step!(
            "All major, 8 bars",
            "Consolidation",
            "All major keys, mixed rhythm, octaves: consolidation, not a new key.",
            8,
            ALL_MAJOR,
            RHY_MIXED,
            7,
            ALL_DEGREES,
            0.4,
            false
        ),
        step!(
            "All major, 8 bars",
            "Major review",
            "A review of the major ladder with fresh generated melodies each session.",
            8,
            ALL_MAJOR,
            RHY_MIXED,
            7,
            ALL_DEGREES,
            0.4,
            false
        ),
        step!(
            "Minor",
            "A minor",
            "Minor mode in A minor, eight bars and mixed rhythm; leaps to sixths.",
            8,
            &["A"],
            RHY_MIXED,
            5,
            ALL_DEGREES,
            0.4,
            true
        ),
        step!(
            "Minor",
            "E and D minor",
            "More minor keys, still sixths and mixed rhythm.",
            8,
            &["E", "D"],
            RHY_MIXED,
            5,
            ALL_DEGREES,
            0.4,
            true
        ),
        step!(
            "Minor",
            "All minor keys",
            "All minor keys, octave leaps and mixed rhythm.",
            8,
            ALL_MINOR,
            RHY_MIXED,
            7,
            ALL_DEGREES,
            0.4,
            true
        ),
        step!(
            "Mixed review",
            "Major and minor",
            "Major and minor together: switch mode and key without a reset.",
            8,
            ALL_MAJOR,
            RHY_MIXED,
            7,
            ALL_DEGREES,
            0.4,
            false
        ),
    ];
    // Sixteenth runs arrive at step 12 and continue; the mixed eight-bar lessons keep some.
    for (i, s) in steps.iter_mut().enumerate() {
        s.run_rate = match i {
            11..=17 => 0.55,
            18..=25 => 0.35,
            _ => 0.0,
        };
    }
    steps
}

/// The steps grouped into their modules, with the activity-specific lesson settings.
pub(crate) fn modules(mode: LadderMode) -> Vec<Module> {
    let mut out: Vec<Module> = Vec::new();
    for (i, step) in steps().into_iter().enumerate() {
        let n = i + 1;
        let title = format!("{n}. {}", step.title);
        let params = step.params(mode);
        let lesson = lesson(title, step.summary, params).of(mode.kind(), TRIALS);
        match out.last_mut() {
            Some(m) if m.title == step.module => m.lessons.push(lesson),
            _ => out.push(module(step.module, vec![lesson])),
        }
    }
    out
}

/// A complete ladder course. Its lessons use eight trials and a 7/8 pass mark.
pub fn course(mode: LadderMode) -> Course {
    let mut c = workshop(
        mode.id(),
        mode.kind(),
        TRIALS,
        mode.title(),
        mode.short(),
        mode.description(),
        modules(mode),
    );
    for level in &mut c.levels {
        level.pass = PassRule::Accuracy {
            min: PASS,
            blocks: 1,
        };
    }
    c
}

/// Every ladder course, for the built-in course list.
pub fn courses() -> Vec<Course> {
    vec![
        course(LadderMode::Sight),
        course(LadderMode::Dictation),
        course(LadderMode::Singback),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::courses::level_session;
    use crate::generate::{GenContext, generate_block};
    use crate::schema::TrialSpec;

    fn all_steps() -> Vec<LadderStep> {
        steps()
    }

    #[test]
    fn the_ladder_has_the_planned_shape() {
        let steps = all_steps();
        assert_eq!(steps.len(), 26);
        assert_eq!(steps[0].bars, 2);
        assert_eq!(steps[1].bars, 2);
        for s in &steps[2..18] {
            assert_eq!(s.bars, 4, "{}", s.title);
        }
        for s in &steps[18..] {
            assert_eq!(s.bars, 8, "{}", s.title);
        }
        assert_eq!(steps[0].degrees, DO_SOL);
        for s in &steps[1..] {
            assert_eq!(s.degrees, ALL_DEGREES, "{}", s.title);
        }
        assert!(
            steps
                .iter()
                .any(|s| s.rhythms.contains(&"eighth") && s.bars == 4)
        );
        assert!(steps.iter().any(|s| s.rhythms.contains(&"dotted_eighth")));
        assert!(steps.iter().any(|s| s.rhythms.contains(&"sixteenth")));
        assert!(steps.iter().any(|s| s.syncopation > 0.0));
        assert_eq!(steps[8].run_rate, 0.0);
        assert!(steps[11].run_rate > 0.0);
        assert!(steps[0].max_leap <= steps[5].max_leap);
        assert!(steps[..22].iter().map(|s| s.max_leap).is_sorted());
    }

    #[test]
    fn every_lesson_has_eight_trials_and_a_85_pass() {
        for mode in [
            LadderMode::Sight,
            LadderMode::Dictation,
            LadderMode::Singback,
        ] {
            let c = course(mode);
            assert_eq!(c.levels.len(), 26);
            for l in &c.levels {
                assert_eq!(l.blocks[0].count, TRIALS, "{}", l.title);
                assert!(matches!(
                    l.pass,
                    PassRule::Accuracy { min, blocks: 1 } if (min - PASS).abs() < 1e-6
                ));
            }
        }
    }

    #[test]
    fn all_three_modes_use_the_same_steps() {
        let sight = course(LadderMode::Sight);
        let dictation = course(LadderMode::Dictation);
        let singback = course(LadderMode::Singback);
        assert_eq!(sight.levels.len(), dictation.levels.len());
        assert_eq!(sight.levels.len(), singback.levels.len());
        for (a, b) in sight.levels.iter().zip(&dictation.levels) {
            assert_eq!(a.title, b.title);
            assert_eq!(a.module, b.module);
            assert_eq!(a.summary, b.summary);
        }
    }

    #[test]
    fn every_major_spelling_appears_before_the_eight_bar_lessons() {
        let mut seen = std::collections::BTreeSet::new();
        for s in &steps()[..18] {
            seen.extend(s.keys.iter().copied());
        }
        for key in [
            "C", "G", "D", "A", "E", "B", "F#", "F", "Bb", "Eb", "Ab", "Db", "Gb",
        ] {
            assert!(seen.contains(key), "missing major spelling {key}");
        }
    }

    #[test]
    fn the_two_key_lessons_hear_both_keys() {
        let ctx = GenContext::default();
        let c = course(LadderMode::Sight);
        // Lesson 6 is F and G major.
        let spec = level_session(&c, 6).unwrap();
        for seed in 0..20 {
            let mut keys = std::collections::BTreeSet::new();
            for t in generate_block(&spec.blocks[0], seed, &ctx).unwrap() {
                let TrialSpec::Sung(t) = t else { panic!() };
                let score = t.score.unwrap();
                keys.insert(score.key.to_string());
            }
            assert!(keys.len() == 2, "seed {seed}: {keys:?}");
        }
    }

    #[test]
    fn dotted_eighths_and_syncopation_render() {
        let ctx = GenContext::default();
        let c = course(LadderMode::Sight);
        let dotted = level_session(&c, 9).unwrap();
        let syncopated = level_session(&c, 16).unwrap();
        let mut saw_dotted = false;
        let mut saw_offbeat = false;
        for seed in 0..40 {
            for t in generate_block(&dotted.blocks[0], seed, &ctx).unwrap() {
                let TrialSpec::Sung(t) = t else { panic!() };
                let score = t.score.unwrap();
                saw_dotted |= score.notes.iter().any(|n| n.dur == 3);
            }
            for t in generate_block(&syncopated.blocks[0], seed, &ctx).unwrap() {
                let TrialSpec::Sung(t) = t else { panic!() };
                let score = t.score.unwrap();
                let beat = score.time.beat_ticks();
                saw_offbeat |= score
                    .notes
                    .iter()
                    .filter(|n| n.pitch.is_some())
                    .any(|n| n.start % beat != 0);
            }
        }
        assert!(saw_dotted, "no dotted eighth (3 ticks) was generated");
        assert!(saw_offbeat, "no offbeat attack was generated");
    }

    #[test]
    #[ignore]
    fn write_ladder_gallery() {
        // One score from lessons 1, 4, 9, 16, 19 and 22, for `?scenario=notation`.
        let ctx = GenContext::default();
        let c = course(LadderMode::Sight);
        let mut scores = Vec::new();
        for lesson in [1u32, 4, 9, 16, 19, 22] {
            let spec = level_session(&c, lesson).unwrap();
            let trials = generate_block(&spec.blocks[0], 1, &ctx).unwrap();
            for t in trials {
                let TrialSpec::Sung(t) = t else { panic!() };
                scores.push(t.score.expect("sight singing has a score"));
                break;
            }
        }
        assert_eq!(scores.len(), 6);
        let path = concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../ui/src/notation/gallery.json"
        );
        std::fs::write(path, serde_json::to_string_pretty(&scores).unwrap()).unwrap();
    }

    #[test]
    fn every_step_generates_in_all_three_modes() {
        let ctx = GenContext::default();
        for mode in [
            LadderMode::Sight,
            LadderMode::Dictation,
            LadderMode::Singback,
        ] {
            let c = course(mode);
            for (i, _) in c.levels.iter().enumerate() {
                let spec = level_session(&c, i as u32 + 1).unwrap();
                for seed in 0..20 {
                    let trials = generate_block(&spec.blocks[0], seed, &ctx).unwrap_or_else(|e| {
                        panic!("{} lesson {} seed {seed}: {}", mode.id(), i + 1, e.0)
                    });
                    assert_eq!(trials.len(), TRIALS as usize);
                }
            }
        }
    }

    #[test]
    fn the_first_lesson_is_do_to_sol_and_later_lessons_use_the_scale() {
        let ctx = GenContext::default();
        let c = course(LadderMode::Sight);
        let spec = level_session(&c, 1).unwrap();
        let allowed = [0, 2, 4, 5, 7]; // do re mi fa sol
        for seed in 0..50 {
            for t in generate_block(&spec.blocks[0], seed, &ctx).unwrap() {
                let TrialSpec::Sung(t) = t else { panic!() };
                let score = t.score.unwrap();
                for (_, p) in score.pitched() {
                    assert!(
                        allowed.contains(&p.midi().rem_euclid(12)),
                        "lesson 1 used {}",
                        p
                    );
                }
            }
        }
    }

    #[test]
    fn every_trial_ends_on_the_tonic_and_fills_its_bars() {
        let ctx = GenContext::default();
        for mode in [LadderMode::Sight, LadderMode::Dictation] {
            let c = course(mode);
            for (i, _) in c.levels.iter().enumerate() {
                let spec = level_session(&c, i as u32 + 1).unwrap();
                for seed in 0..5 {
                    for t in generate_block(&spec.blocks[0], seed, &ctx).unwrap() {
                        let score = match &t {
                            TrialSpec::Sung(t) => t.score.as_ref().unwrap(),
                            TrialSpec::Dictation(t) => &t.score,
                            _ => panic!("unexpected trial"),
                        };
                        assert_eq!(
                            score.notes.last().map(|n| n.start + n.dur),
                            Some(score.total_ticks()),
                            "{} lesson {} seed {seed}",
                            mode.id(),
                            i + 1
                        );
                        let last = score.notes.last().unwrap().pitch.unwrap();
                        assert_eq!(
                            last.midi().rem_euclid(12),
                            score.key.tonic_pc(),
                            "{} lesson {} seed {seed}",
                            mode.id(),
                            i + 1
                        );
                    }
                }
            }
        }
    }

    #[test]
    fn a_generated_lesson_has_no_duplicate_fingerprints() {
        let ctx = GenContext::default();
        let c = course(LadderMode::Sight);
        for seed in 0..100 {
            let spec = level_session(&c, 19).unwrap();
            let trials = generate_block(&spec.blocks[0], seed, &ctx).unwrap();
            let mut seen = std::collections::BTreeSet::new();
            for t in trials {
                let TrialSpec::Sung(t) = t else { panic!() };
                let score = t.score.unwrap();
                let finger = crate::generate::melody_fingerprint(&score);
                assert!(seen.insert(finger), "seed {seed} repeated a melody");
            }
        }
    }
}
