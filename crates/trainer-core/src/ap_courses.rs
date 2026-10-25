//! Absolute-pitch courses added in AP-2: detection, note colours, chord colours and contrast.

use serde_json::{Value, json};

use crate::courses::{
    AP_CHORD_COLORS, AP_CONTRAST, AP_NOTE_COLORS, AP_SPEED, Course, ERRORLESS_DEFAULT_TAG, Level,
    PassRule, SPREAD, STEP, Schedule, block,
};
use crate::labels::{self, GERVAIN_NAMES, TANEDA_COLORS};

fn tags(base: &str, errorless: bool) -> Vec<&str> {
    if errorless {
        vec![base, ERRORLESS_DEFAULT_TAG]
    } else {
        vec![base]
    }
}

pub fn courses() -> Vec<Course> {
    vec![
        speed_course(),
        note_colors_course(),
        chord_colors_course(),
        contrast_course(),
    ]
}

fn speed_course() -> Course {
    let windows = [2500u32, 2100, 1750];
    let levels = windows
        .into_iter()
        .enumerate()
        .map(|(i, window)| {
            let n = i as u32 + 1;
            Level {
                module: None,
                title: format!("Every {:.1} s", window as f32 / 1000.0),
                summary: format!(
                    "The target note is named and sung, then sixteen tones, one every {:.1} s. Press when you hear the target, in any octave.",
                    window as f32 / 1000.0
                ),
                pass: PassRule::Detection { min_d_prime: 1.5, streams: 3 },
                blocks: vec![block(AP_SPEED, n, true, json!({
                    "type": "pitch_detect",
                    "title": "Find the note",
                    "count": 6,
                    "feedback": "after_trial",
                    "tags": ["ap.speed"],
                    "instructions": "The target note is named and sung. Then sixteen tones. Press Space whenever you hear that note, in any octave.",
                    "params": {
                        "classes": "all",
                        "octaves": SPREAD,
                        "min_jump_semitones": 13,
                        "window_ms": window,
                        "tones": 16,
                        "target_rate": 0.25,
                    },
                }))],
            }
        })
        .collect();
    Course {
        id: AP_SPEED.into(),
        title: "Simple Speed".into(),
        short: "Speed".into(),
        category: "Absolute pitch".into(),
        description: "Catch one note in a stream. The target is sung, then sixteen tones go by; press whenever that note appears, in any octave. The gap shrinks from 2.5 s to 1.75 s.".into(),
        schedule: Schedule::Ladder,
        levels,
    }
}

/// C E G, then the other white keys, then the black keys, then four octaves.
const COLOR_ORDER: [&str; 12] = [
    "C", "E", "G", "D", "F", "A", "B", "F#", "C#", "G#", "D#", "A#",
];

fn note_colors_course() -> Course {
    let mut levels = Vec::new();
    for n in 1..=10u32 {
        let classes = &COLOR_ORDER[..(n as usize + 2)];
        let errorless = n < 10;
        let params = color_params(classes, &[4], errorless, 0, None);
        levels.push(color_level(
            n,
            classes,
            errorless,
            params.clone(),
            params,
            "one octave",
        ));
    }
    for (n, deadline) in [(11u32, 3000u32), (12, 2000)] {
        let practice = color_params(&COLOR_ORDER, &SPREAD, false, 13, Some(deadline));
        let mut check = practice.clone();
        check["mask"] = json!("none");
        levels.push(Level {
            module: None,
            title: if n == 12 { "All twelve, within 2 s".into() } else { "All twelve, four octaves".into() },
            summary: format!(
                "All twelve colours over four octaves, each two octaves from the last, answered within {:.1} s. The check has no feedback.",
                deadline as f32 / 1000.0
            ),
            pass: PassRule::Accuracy { min: 0.9, blocks: 2 },
            blocks: vec![
                color_block(n, false, "Name the colour", 24, "after_trial", practice, "Name each note by its colour."),
                color_block(n, true, "Check: no feedback", 16, "none", check, "No results until the end. Answer from the sound alone."),
            ],
        });
    }
    Course {
        id: AP_NOTE_COLORS.into(),
        title: "Note colours (Taneda)".into(),
        short: "Colours".into(),
        category: "Absolute pitch".into(),
        description: "Taneda's colours, starting from C, E and G in one octave and adding a key at each level. Early levels are errorless: a miss is not marked, the right colour lights up, and that note comes back. Later levels spread over four octaves and show ordinary feedback.".into(),
        schedule: Schedule::Ladder,
        levels,
    }
}

fn color_params(
    classes: &[&str],
    octaves: &[i32],
    errorless: bool,
    jump: u32,
    deadline: Option<u32>,
) -> Value {
    let mut params = json!({
        "classes": classes,
        "octaves": octaves,
        "labels": TANEDA_COLORS,
        "color_flash": true,
        "hide_note_names": errorless,
        "requeue_misses": errorless,
    });
    if classes.len() < 12 {
        params["out_of_set_rate"] = json!(0.15);
    }
    if jump > 0 {
        params["min_jump_semitones"] = json!(jump);
        if octaves.len() >= 4 {
            params["min_octave_step"] = json!(STEP);
        }
    }
    if let Some(ms) = deadline {
        params["deadline_ms"] = json!(ms);
    }
    params
}

fn color_level(
    n: u32,
    classes: &[&str],
    errorless: bool,
    train: Value,
    check: Value,
    where_: &str,
) -> Level {
    let names = classes.join(" ");
    Level {
        module: None,
        title: if classes.len() == 12 {
            "All twelve colours".into()
        } else {
            format!("{} colours", classes.len())
        },
        summary: format!(
            "{names}, in {where_}.{}",
            if errorless {
                " A miss is not marked; the right colour lights up and the note returns."
            } else {
                ""
            }
        ),
        pass: PassRule::Accuracy {
            min: 0.9,
            blocks: 2,
        },
        blocks: vec![
            color_block(
                n,
                false,
                "Name the colour",
                24,
                if errorless {
                    "errorless"
                } else {
                    "after_trial"
                },
                train,
                "Name each note by its colour.",
            ),
            color_block(
                n,
                true,
                "Check: no feedback",
                16,
                "none",
                quiet_check(check),
                "No results until the end. Answer from the sound alone.",
            ),
        ],
    }
}

/// A level's check: nothing is shown, so nothing needs masking, and a missed note does not
/// come back (its return would say it was missed).
fn quiet_check(mut params: Value) -> Value {
    params["mask"] = json!("none");
    params["requeue_misses"] = json!(false);
    params
}

fn color_block(
    level: u32,
    verify: bool,
    title: &str,
    count: u32,
    feedback: &str,
    params: Value,
    instructions: &str,
) -> crate::schema::BlockSpec {
    let errorless = feedback == "errorless";
    block(
        AP_NOTE_COLORS,
        level,
        verify,
        json!({
            "type": "pitch_id",
            "title": title,
            "count": count,
            "feedback": feedback,
            "pass_mark": 0.9,
            "tags": tags("ap.note_colors", errorless),
            "instructions": instructions,
            "params": params,
        }),
    )
}

fn chord_colors_course() -> Course {
    let mut levels = Vec::new();
    // The first level already has two chords to tell apart (Red and Yellow): one alone would be
    // a question with a single answer.
    for i in 1..labels::EGUCHI.len() {
        let active = &labels::EGUCHI[..=i];
        let ids: Vec<&str> = active.iter().map(|c| c.id).collect();
        let names = active.iter().map(|c| c.word).collect::<Vec<_>>().join(", ");
        let errorless = i < 4;
        let n = i as u32;
        let chord = &labels::EGUCHI[i];
        levels.push(Level {
            module: None,
            title: if i == 1 {
                format!("{} and {}", labels::EGUCHI[0].word, chord.word)
            } else {
                chord.word.into()
            },
            summary: format!("Active colours: {names}. The next chord is added after 100% and 14 days."),
            pass: PassRule::Spacing { min: 1.0, days: 14.0 },
            blocks: vec![block(AP_CHORD_COLORS, n, true, json!({
                "type": "chord_color_id",
                "title": "Name the colour",
                "count": 20,
                "feedback": if errorless { "errorless" } else { "after_trial" },
                "tags": tags("ap.chord_colors", errorless),
                "instructions": "Name the chord's colour. Inversions of the same chord have different colours.",
                "params": { "chords": ids, "requeue_misses": errorless },
            }))],
        });
    }
    let all: Vec<&str> = labels::EGUCHI.iter().map(|c| c.id).collect();
    let params = json!({ "chords": all, "single_note_after": true, "note_labels": TANEDA_COLORS });
    levels.push(Level {
        module: None,
        title: "Notes from the chord".into(),
        summary: "After each chord, one note plays alone and is named by its colour.".into(),
        pass: PassRule::Accuracy { min: 0.9, blocks: 2 },
        blocks: vec![
            block(AP_CHORD_COLORS, labels::EGUCHI.len() as u32, false, json!({
                "type": "chord_color_id",
                "title": "Chord, then a note",
                "count": 12,
                "feedback": "after_trial",
                "tags": ["ap.chord_colors"],
                "instructions": "Name the chord colour, then the note that follows.",
                "params": params,
            })),
            block(AP_CHORD_COLORS, labels::EGUCHI.len() as u32, true, json!({
                "type": "chord_color_id",
                "title": "Check: no feedback",
                "count": 10,
                "feedback": "none",
                "pass_mark": 0.9,
                "tags": ["ap.chord_colors", "ap.check"],
                "instructions": "No results until the end.",
                "params": { "chords": all, "single_note_after": true, "note_labels": TANEDA_COLORS },
            })),
        ],
    });
    Course {
        id: AP_CHORD_COLORS.into(),
        title: "Chord colours (Eguchi)".into(),
        short: "Chords".into(),
        category: "Absolute pitch".into(),
        description: "Eguchi's fourteen coloured chords, white chords first, then black. A chord is added only after 100% and 14 days. The last level asks for a single note from the chord.".into(),
        schedule: Schedule::Ladder,
        levels,
    }
}

const BLACK: [&str; 5] = ["C#", "D#", "F#", "G#", "A#"];

fn contrast_course() -> Course {
    let mut levels = Vec::new();
    let steps: [(&[&str], &str); 3] = [
        (&["D#", "F#"], "E♭ and F♯"),
        (&["D#", "F#", "A"], "E♭, F♯ and A"),
        (&BLACK, "the five black keys"),
    ];
    for (i, (classes, title)) in steps.into_iter().enumerate() {
        let n = i as u32 + 1;
        let params = contrast_params(classes, &[3], true, 0, None);
        levels.push(contrast_level(
            n,
            title,
            true,
            params.clone(),
            params,
            "in one octave",
        ));
    }
    let spread = contrast_params(&BLACK, &SPREAD, false, 13, None);
    let mut spread_check = spread.clone();
    spread_check["mask"] = json!("none");
    levels.push(Level {
        module: None,
        title: "Black keys, four octaves".into(),
        summary: "The five black keys over four octaves, each two octaves from the last. The check has no feedback.".into(),
        pass: PassRule::Accuracy { min: 0.9, blocks: 2 },
        blocks: vec![
            contrast_block(4, false, "Name the word", 24, "after_trial", spread, "Do not hum. Answer with the word."),
            contrast_block(4, true, "Check: no feedback", 16, "none", spread_check, "No results until the end. Do not hum."),
        ],
    });
    let all = contrast_params(
        &[
            "C", "C#", "D", "D#", "E", "F", "F#", "G", "G#", "A", "A#", "B",
        ],
        &SPREAD,
        false,
        13,
        Some(2000),
    );
    let mut all_check = all.clone();
    all_check["mask"] = json!("none");
    levels.push(Level {
        module: None,
        title: "All twelve words".into(),
        summary: "All twelve names over four octaves, within 2 s. The check has no feedback."
            .into(),
        pass: PassRule::Accuracy {
            min: 0.9,
            blocks: 2,
        },
        blocks: vec![
            contrast_block(
                5,
                false,
                "Name the word",
                24,
                "after_trial",
                all,
                "Do not hum. Answer with the word.",
            ),
            contrast_block(
                5,
                true,
                "Check: no feedback",
                16,
                "none",
                all_check,
                "No results until the end. Do not hum.",
            ),
        ],
    });
    Course {
        id: AP_CONTRAST.into(),
        title: "Contrast (Burge / Gervain)".into(),
        short: "Contrast".into(),
        category: "Absolute pitch".into(),
        description: "Starts with E♭3 against F♯3, then A, then the black keys, then all twelve spread out. Answers are Gervain's names unless you choose your own colours in Settings. Do not hum.".into(),
        schedule: Schedule::Ladder,
        levels,
    }
}

fn contrast_params(
    classes: &[&str],
    octaves: &[i32],
    errorless: bool,
    jump: u32,
    deadline: Option<u32>,
) -> Value {
    let mut params = json!({
        "classes": classes,
        "octaves": octaves,
        "labels": GERVAIN_NAMES,
        "color_flash": true,
        "hide_note_names": true,
        "requeue_misses": errorless,
    });
    if classes.len() < 12 {
        params["out_of_set_rate"] = json!(0.15);
    }
    if jump > 0 {
        params["min_jump_semitones"] = json!(jump);
        if octaves.len() >= 4 {
            params["min_octave_step"] = json!(STEP);
        }
    }
    if let Some(ms) = deadline {
        params["deadline_ms"] = json!(ms);
    }
    params
}

fn contrast_level(
    n: u32,
    title: &str,
    errorless: bool,
    train: Value,
    check: Value,
    where_: &str,
) -> Level {
    Level {
        module: None,
        title: title.into(),
        summary: format!(
            "{title}, {where_}. Do not hum.{}",
            if errorless {
                " A miss is not marked, and the note comes back."
            } else {
                ""
            }
        ),
        pass: PassRule::Accuracy {
            min: 0.9,
            blocks: 2,
        },
        blocks: vec![
            contrast_block(
                n,
                false,
                "Name the word",
                20,
                "errorless",
                train,
                "Do not hum. Answer with the word.",
            ),
            contrast_block(
                n,
                true,
                "Check: no feedback",
                12,
                "none",
                quiet_check(check),
                "No results until the end. Answer from the sound alone. Do not hum.",
            ),
        ],
    }
}

fn contrast_block(
    level: u32,
    verify: bool,
    title: &str,
    count: u32,
    feedback: &str,
    params: Value,
    instructions: &str,
) -> crate::schema::BlockSpec {
    block(
        AP_CONTRAST,
        level,
        verify,
        json!({
            "type": "pitch_id",
            "title": title,
            "count": count,
            "feedback": feedback,
            "pass_mark": 0.9,
            "tags": tags("ap.contrast", feedback == "errorless"),
            "instructions": instructions,
            "params": params,
        }),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::courses::level_session;
    use crate::generate::{GenContext, generate_block};
    use crate::schema::{FeedbackMode, TrialSpec};

    #[test]
    fn checks_show_nothing() {
        for c in courses() {
            for n in 1..=c.levels.len() as u32 {
                for b in level_session(&c, n).unwrap().blocks {
                    if b.title.as_deref().unwrap_or("").starts_with("Check") {
                        assert_eq!(b.feedback, FeedbackMode::None, "{} level {n}", c.id);
                    }
                }
            }
        }
    }

    #[test]
    fn a_repeated_chord_moves_an_octave() {
        let c = chord_colors_course();
        let spec = level_session(&c, 1).unwrap();
        let mut b = spec.blocks[0].clone();
        b.count = 200;
        let trials = generate_block(&b, 4, &GenContext::default()).unwrap();
        let played: Vec<(String, i64)> = trials
            .iter()
            .map(|t| {
                let TrialSpec::Identify(t) = t else { panic!() };
                (t.correct.clone(), t.stimulus["midi"][0].as_i64().unwrap())
            })
            .collect();
        let repeats: Vec<_> = played.windows(2).filter(|w| w[0].0 == w[1].0).collect();
        assert!(repeats.len() > 40, "two chords repeat about half the time");
        assert!(repeats.iter().all(|w| (w[0].1 - w[1].1).abs() == 12));
    }
}
