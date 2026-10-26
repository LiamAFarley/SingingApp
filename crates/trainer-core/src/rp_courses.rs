//! Relative-pitch workshops, after EarMaster's general workshops and Functional Ear Trainer's
//! levels: one course per activity, its lessons grouped into modules. A module keeps one pool of
//! material (two intervals, three chords, a set of degrees) and its lessons change only the drill
//! (ascending, descending, harmonic, all three), or grow the pool a step at a time. Each lesson is
//! a single run of questions with feedback after each; 90% passes it, and the course moves on to
//! the next lesson. "Test" lessons give no feedback until the end. Only the pattern is EarMaster's
//! and Functional Ear Trainer's; the material is this app's own.

use serde_json::{Value, json};

use crate::courses::{Course, Level, PassRule, Schedule, block};

pub const RP_DEGREES: &str = "rp_degrees";
pub const RP_INTERVALS: &str = "rp_intervals";
pub const RP_INTERVALS_KEY: &str = "rp_intervals_key";
pub const RP_COMPARE: &str = "rp_compare";
pub const RP_SING: &str = "rp_sing";
pub const RP_CHORDS: &str = "rp_chords";
pub const RP_INVERSIONS: &str = "rp_inversions";
pub const RP_SCALES: &str = "rp_scales";
pub const RP_DICTATION: &str = "rp_dictation";
pub const RP_SIGHT: &str = "rp_sight";

pub const CATEGORY: &str = "Relative pitch";

/// The score a lesson needs.
pub const PASS: f32 = 0.9;

pub fn courses() -> Vec<Course> {
    vec![
        crate::audiation_courses::core_course(),
        degrees_course(),
        intervals_course(),
        intervals_key_course(),
        compare_course(),
        sing_course(),
        chords_course(),
        inversions_course(),
        scales_course(),
        dictation_course(),
        sight_course(),
    ]
    .into_iter()
    .chain(crate::audiation_courses::workshops())
    .collect()
}

// ---------------------------------------------------------------------------------------------
// Building blocks
// ---------------------------------------------------------------------------------------------

pub(crate) struct Lesson {
    pub(crate) title: String,
    pub(crate) summary: String,
    pub(crate) params: Value,
    /// No feedback until the end.
    pub(crate) test: bool,
    /// Another exercise than the workshop's (a workshop mixing exercises).
    pub(crate) kind: Option<&'static str>,
    /// Another question count than the workshop's.
    pub(crate) count: Option<u32>,
    /// Block fields beside the params, e.g. `grading` or `self_judge`.
    pub(crate) block: Value,
}

pub(crate) fn lesson(
    title: impl Into<String>,
    summary: impl Into<String>,
    params: Value,
) -> Lesson {
    Lesson {
        title: title.into(),
        summary: summary.into(),
        params,
        test: false,
        kind: None,
        count: None,
        block: json!({}),
    }
}

impl Lesson {
    /// The same lesson with another exercise and question count.
    pub(crate) fn of(mut self, kind: &'static str, count: u32) -> Lesson {
        self.kind = Some(kind);
        self.count = Some(count);
        self
    }

    pub(crate) fn with(mut self, block: Value) -> Lesson {
        self.block = merge(self.block, block);
        self
    }
}

pub(crate) fn test(title: impl Into<String>, summary: impl Into<String>, params: Value) -> Lesson {
    Lesson {
        test: true,
        ..lesson(title, summary, params)
    }
}

pub(crate) struct Module {
    pub(crate) title: String,
    pub(crate) lessons: Vec<Lesson>,
}

pub(crate) fn module(title: impl Into<String>, lessons: Vec<Lesson>) -> Module {
    Module {
        title: title.into(),
        lessons,
    }
}

pub(crate) fn merge(mut a: Value, b: Value) -> Value {
    if let (Some(a), Value::Object(b)) = (a.as_object_mut(), b) {
        a.extend(b);
    }
    a
}

/// A workshop course: its modules' lessons in order, numbered from 1.
pub(crate) fn workshop(
    id: &str,
    kind: &str,
    count: u32,
    title: &str,
    short: &str,
    description: &str,
    modules: Vec<Module>,
) -> Course {
    let mut levels = Vec::new();
    for m in modules {
        for l in m.lessons {
            let n = levels.len() as u32 + 1;
            let kind = l.kind.unwrap_or(kind);
            let mut b = json!({
                "type": kind, "title": l.title, "count": l.count.unwrap_or(count), "pass_mark": PASS,
                "feedback": if l.test { "none" } else { "after_trial" },
                "tags": [id], "params": l.params,
            });
            if l.test {
                b["instructions"] = json!("A test: no results until the end.");
            }
            b = merge(b, l.block);
            // Detection streams (N-back) are scored as d′, not as answers right.
            let pass = if kind == "n_back"
                && l.params.get("mode").and_then(|m| m.as_str()) != Some("sing")
            {
                PassRule::Detection {
                    min_d_prime: 2.0,
                    streams: 1,
                }
            } else {
                PassRule::Accuracy {
                    min: PASS,
                    blocks: 1,
                }
            };
            levels.push(Level {
                module: Some(m.title.clone()),
                title: l.title,
                summary: l.summary,
                pass,
                blocks: vec![block(id, n, true, b)],
            });
        }
    }
    Course {
        id: id.into(),
        title: title.into(),
        short: short.into(),
        category: CATEGORY.into(),
        description: description.into(),
        schedule: Schedule::Ladder,
        levels,
    }
}

/// The four drills of an EarMaster module, in its order: up, down, together, all three.
const DRILLS: [(&str, &[&str]); 4] = [
    ("Ascending", &["ascending"]),
    ("Descending", &["descending"]),
    ("Harmonic", &["harmonic"]),
    (
        "Ascending, descending and harmonic",
        &["ascending", "descending", "harmonic"],
    ),
];

/// The same drills for chords: arpeggiated up, down, as a block, or any of those.
fn chord_styles(drill: usize) -> Value {
    json!(match drill {
        0 => vec!["arpeggio"],
        1 => vec!["arpeggio_down"],
        2 => vec!["block"],
        _ => vec!["arpeggio", "arpeggio_down", "block"],
    })
}

fn interval_name(id: &str) -> &'static str {
    id.parse::<crate::theory::Interval>()
        .map(|i| i.long_name())
        .unwrap_or("?")
}

/// "Minor 2nd, major 2nd and minor 3rd".
fn list_names(names: &[&str]) -> String {
    let lower: Vec<String> = names
        .iter()
        .enumerate()
        .map(|(i, n)| {
            if i == 0 {
                n.to_string()
            } else {
                n.to_lowercase()
            }
        })
        .collect();
    match lower.as_slice() {
        [] => String::new(),
        [one] => one.clone(),
        [init @ .., last] => format!("{} and {last}", init.join(", ")),
    }
}

fn interval_list(ids: &[&str]) -> String {
    list_names(&ids.iter().map(|i| interval_name(i)).collect::<Vec<_>>())
}

fn chord_list(ids: &[&str]) -> String {
    list_names(
        &ids.iter()
            .map(|q| {
                crate::theory::ChordQuality::from_id(q)
                    .map(|c| c.long_name())
                    .unwrap_or("?")
            })
            .collect::<Vec<_>>(),
    )
}

/// One random key for the session.
fn one_key() -> Value {
    json!({ "keys": "all", "key_change": "per_block" })
}

/// A new key every question.
fn changing_keys() -> Value {
    json!({ "keys": "all", "key_change": "per_trial" })
}

const SIMPLE: [&str; 12] = [
    "m2", "M2", "m3", "M3", "P4", "TT", "P5", "m6", "M6", "m7", "M7", "P8",
];

// ---------------------------------------------------------------------------------------------
// Scale degrees (Functional Ear Trainer's basic levels)
// ---------------------------------------------------------------------------------------------

fn degrees_course() -> Course {
    const NOT_C: [&str; 11] = ["C#", "D", "D#", "E", "F", "F#", "G", "G#", "A", "A#", "B"];
    const CHROMATIC_LOW: [&str; 6] = ["1", "b2", "2", "b3", "3", "4"];
    const CHROMATIC_HIGH: [&str; 7] = ["#4", "5", "b6", "6", "b7", "7", "8"];
    let one_octave = json!({ "octaves": [0] });
    let many_octaves = json!({ "octaves": [-1, 0, 1] });
    // The eight levels of each scale: halves of one octave in C, all of it, more octaves, then
    // one other key for the session, then a new key (and cadence) every question.
    let ladder = |mode: &str, low: &[&str], high: &[&str], all: Option<&[&str]>| -> Vec<Lesson> {
        let tonic = if mode == "minor" {
            "C minor"
        } else {
            "C major"
        };
        let base = |degrees: Option<&[&str]>| {
            let mut v = json!({ "modes": [mode], "keys": ["C"] });
            if let Some(d) = degrees {
                v["degrees"] = json!(d);
            }
            v
        };
        let all_label = if all.is_some() { "all twelve" } else { "all" };
        let keyed = |keys: Value, per_trial: bool, octaves: &Value| {
            let mut v = merge(base(all), octaves.clone());
            v["keys"] = keys;
            if per_trial {
                v["key_change"] = json!("per_trial");
                v["cadence"] = json!("every");
            }
            v
        };
        let (other, random) = if mode == "minor" {
            ("Not C minor", "Random minor")
        } else {
            ("Not C major", "Random major")
        };
        vec![
            lesson(
                format!("{tonic} · bottom half"),
                format!("{} in {tonic}, one octave.", low.join(" ")),
                merge(
                    merge(base(Some(low)), one_octave.clone()),
                    json!({ "upper_tonic": false }),
                ),
            ),
            lesson(
                format!("{tonic} · top half"),
                format!(
                    "{} in {tonic}, one octave (8 is the upper tonic).",
                    high.join(" ")
                ),
                merge(base(Some(high)), one_octave.clone()),
            ),
            lesson(
                format!("{tonic} · one octave"),
                format!("The {all_label} degrees in {tonic}, 1 to 8."),
                merge(base(all), one_octave.clone()),
            ),
            lesson(
                format!("{tonic} · many octaves"),
                format!("The {all_label} degrees in {tonic}, over three octaves."),
                merge(base(all), many_octaves.clone()),
            ),
            lesson(
                format!("{other} · one octave"),
                "One key other than C for the session, one octave.",
                keyed(json!(NOT_C), false, &one_octave),
            ),
            lesson(
                format!("{other} · many octaves"),
                "One key other than C for the session, over three octaves.",
                keyed(json!(NOT_C), false, &many_octaves),
            ),
            lesson(
                format!("{random} · one octave"),
                "A new key every question, with its cadence each time.",
                keyed(json!("all"), true, &one_octave),
            ),
            lesson(
                format!("{random} · many octaves"),
                "A new key every question, over three octaves, cadence each time.",
                keyed(json!("all"), true, &many_octaves),
            ),
        ]
    };
    let chromatic: Vec<&str> = CHROMATIC_LOW
        .iter()
        .chain(CHROMATIC_HIGH.iter())
        .copied()
        .collect();
    workshop(
        RP_DEGREES,
        "scale_degree_id",
        20,
        "Scale degrees",
        "Degrees",
        "A I–IV–V–I cadence sets the key, a note plays, you name its degree; the note then walks to the tonic. Each scale climbs from half an octave in C to every degree in a new key each question.",
        vec![
            module(
                "Major scale",
                ladder("major", &["1", "2", "3", "4"], &["5", "6", "7", "8"], None),
            ),
            module(
                "Chromatics in major",
                ladder("major", &CHROMATIC_LOW, &CHROMATIC_HIGH, Some(&chromatic)),
            ),
            module(
                "Minor scale",
                ladder(
                    "minor",
                    &["1", "2", "b3", "4"],
                    &["5", "b6", "b7", "8"],
                    None,
                ),
            ),
            module(
                "Chromatics in minor",
                ladder("minor", &CHROMATIC_LOW, &CHROMATIC_HIGH, Some(&chromatic)),
            ),
        ],
    )
}

// ---------------------------------------------------------------------------------------------
// Interval identification (EarMaster's ladder: pairs, then sets, each in four drills)
// ---------------------------------------------------------------------------------------------

/// The four drills of one interval pool.
fn interval_drills(ivs: &[&str]) -> Vec<Lesson> {
    DRILLS
        .iter()
        .map(|(name, dirs)| {
            lesson(
                *name,
                format!("{}, {}.", interval_list(ivs), name.to_lowercase()),
                json!({ "intervals": ivs, "directions": dirs }),
            )
        })
        .collect()
}

/// Practice lessons: each pool heard together, then mixed.
fn harmonic_then_mixed(pools: &[&[&str]]) -> Vec<Lesson> {
    let mut out = Vec::new();
    for pool in pools {
        for (name, dirs) in [DRILLS[2], DRILLS[3]] {
            out.push(lesson(
                format!("{}: {}", interval_list(pool), name.to_lowercase()),
                format!("{}, {}.", interval_list(pool), name.to_lowercase()),
                json!({ "intervals": pool, "directions": dirs }),
            ));
        }
    }
    out
}

/// A set's four drills, titled with the set ("All four: ascending").
fn named_drills(name: &str, set: &[&str]) -> Vec<Lesson> {
    interval_drills(set)
        .into_iter()
        .map(|mut l| {
            l.title = format!("{name}: {}", l.title.to_lowercase());
            l
        })
        .collect()
}

fn intervals_course() -> Course {
    let all: Vec<&str> = std::iter::once("P1").chain(SIMPLE).collect();
    let pools: Vec<(&str, Vec<&str>)> = vec![
        ("Minor and major 2nds", vec!["m2", "M2"]),
        ("Minor and major 3rds", vec!["m3", "M3"]),
        ("2nds and 3rds", vec!["m2", "M2", "m3", "M3"]),
        ("Perfect 4th and tritone", vec!["P4", "TT"]),
        ("Tritone and perfect 5th", vec!["TT", "P5"]),
        ("4th, tritone and 5th", vec!["P4", "TT", "P5"]),
        ("Minor 2nd to perfect 5th", SIMPLE[..7].to_vec()),
        ("Minor and major 6ths", vec!["m6", "M6"]),
        ("Minor and major 7ths", vec!["m7", "M7"]),
        ("6ths and 7ths", vec!["m6", "M6", "m7", "M7"]),
        ("Perfect 4th to major 7th", SIMPLE[4..11].to_vec()),
        ("Minor 2nd to major 7th", SIMPLE[..11].to_vec()),
    ];
    let mut modules: Vec<Module> = pools
        .into_iter()
        .map(|(t, ivs)| module(t, interval_drills(&ivs)))
        .collect();
    // Practice modules: two pairs heard together then mixed, then the set's four drills.
    let mut perfect = harmonic_then_mixed(&[&["P1", "P8"], &["P4", "P5"]]);
    perfect.extend(named_drills("All four", &["P1", "P4", "P5", "P8"]));
    modules.push(module("Practice: perfect intervals", perfect));
    let mut consonant = harmonic_then_mixed(&[&["m3", "M6"], &["M3", "m6"]]);
    consonant.extend(named_drills("All four", &["m3", "M3", "m6", "M6"]));
    modules.push(module("Practice: 3rds and 6ths", consonant));
    // Seconds, sevenths and the tritone: two pairs, the four, then each pair with the tritone,
    // then all five.
    let mut dissonant = harmonic_then_mixed(&[&["m2", "M7"], &["M2", "m7"]]);
    dissonant.extend(named_drills("All four", &["m2", "M2", "m7", "M7"]));
    dissonant.extend(harmonic_then_mixed(&[
        &["m2", "TT", "M7"],
        &["M2", "TT", "m7"],
    ]));
    dissonant.extend(named_drills("All five", &["m2", "M2", "TT", "m7", "M7"]));
    modules.push(module("Practice: 2nds, 7ths and the tritone", dissonant));
    modules.push(module(
        "All intervals within the octave",
        interval_drills(&all),
    ));
    for m in &mut modules {
        for l in &mut m.lessons {
            l.params = merge(
                json!({ "root_range": { "low": "C3", "high": "C5" } }),
                l.params.take(),
            );
        }
    }
    workshop(
        RP_INTERVALS,
        "interval_id",
        20,
        "Interval identification",
        "Intervals",
        "Intervals on random notes, no key. Each module is one set of intervals heard ascending, descending, harmonic, then all three mixed; pairs first, then larger sets, then practice modules for perfect, consonant and dissonant intervals.",
        modules,
    )
}

// ---------------------------------------------------------------------------------------------
// Intervals in a key (this app's own: diatonic and chromatic intervals against a cadence)
// ---------------------------------------------------------------------------------------------

fn intervals_key_course() -> Course {
    let from_do = json!({ "in_key": true, "start": "tonic", "start_octaves": [-1, 0] });
    let with = |keys: Value, ivs: &[&str], dirs: &[&str], extra: Value| {
        merge(
            merge(merge(from_do.clone(), keys), extra),
            json!({ "intervals": ivs, "directions": dirs }),
        )
    };
    let up = |t: &str, s: &str, ivs: &[&str]| {
        lesson(
            t,
            format!("{s} One key for the session."),
            with(one_key(), ivs, &["ascending"], json!({})),
        )
    };
    let down = |t: &str, s: &str, ivs: &[&str]| {
        lesson(
            t,
            format!("{s} One key for the session."),
            with(one_key(), ivs, &["descending"], json!({})),
        )
    };
    let diatonic_up = ["M2", "M3", "P4", "P5", "M6", "M7", "P8"];
    let diatonic_down = ["m2", "m3", "P4", "P5", "m6", "m7", "P8"];
    let both: Vec<&str> = SIMPLE.iter().copied().filter(|i| *i != "TT").collect();
    let anywhere = |t: &str, s: &str, dirs: &[&str], keys: Value| {
        lesson(
            t,
            s,
            with(keys, &SIMPLE, dirs, json!({ "start": "random" })),
        )
    };
    let chrom = |t: &str, s: &str, ivs: &[&str], dirs: &[&str], keys: Value| {
        lesson(t, s, with(keys, ivs, dirs, json!({ "chromatic": true })))
    };
    let modules = vec![
        module(
            "Up from the tonic",
            vec![
                up(
                    "Sol and the upper do",
                    "Up from do to sol or do.",
                    &["P5", "P8"],
                ),
                up("Mi and sol", "Up from do to mi or sol.", &["M3", "P5"]),
                up("Fa and sol", "Up from do to fa or sol.", &["P4", "P5"]),
                up("Re and mi", "Up from do to re or mi.", &["M2", "M3"]),
                up("La and ti", "Up from do to la or ti.", &["M6", "M7"]),
                up(
                    "The whole scale up",
                    "Every degree up from do.",
                    &diatonic_up,
                ),
            ],
        ),
        module(
            "Down from the tonic",
            vec![
                down("Ti and la", "Down from do to ti or la.", &["m2", "m3"]),
                down("Sol and fa", "Down from do to sol or fa.", &["P4", "P5"]),
                down("Mi and re", "Down from do to mi or re.", &["m6", "m7"]),
                down(
                    "The whole scale down",
                    "Every degree down from do.",
                    &diatonic_down,
                ),
            ],
        ),
        module(
            "From the tonic, mixed",
            vec![
                lesson(
                    "Up and down",
                    "Every degree up or down from do, one key for the session.",
                    with(one_key(), &both, &["ascending", "descending"], json!({})),
                ),
                lesson(
                    "Harmonic",
                    "Do and another degree together.",
                    with(one_key(), &diatonic_up, &["harmonic"], json!({})),
                ),
                lesson(
                    "Up and down, new keys",
                    "The key changes every question.",
                    with(
                        changing_keys(),
                        &both,
                        &["ascending", "descending"],
                        json!({}),
                    ),
                ),
            ],
        ),
        module(
            "Between any two notes of the key",
            vec![
                anywhere(
                    "Ascending",
                    "Diatonic intervals from any note of the key, upward.",
                    &["ascending"],
                    one_key(),
                ),
                anywhere(
                    "Descending",
                    "The same, downward.",
                    &["descending"],
                    one_key(),
                ),
                anywhere("Harmonic", "Both notes together.", &["harmonic"], one_key()),
                anywhere(
                    "Ascending, descending and harmonic",
                    "All three, one key for the session.",
                    &["ascending", "descending", "harmonic"],
                    one_key(),
                ),
                anywhere(
                    "All three, new keys",
                    "All three; the key changes every question.",
                    &["ascending", "descending", "harmonic"],
                    changing_keys(),
                ),
            ],
        ),
        module(
            "Chromatic, from the tonic",
            vec![
                chrom(
                    "Me and mi",
                    "Up from do to the minor or major third.",
                    &["m3", "M3"],
                    &["ascending"],
                    one_key(),
                ),
                chrom(
                    "Le and la",
                    "Up from do to the minor or major sixth.",
                    &["m6", "M6"],
                    &["ascending"],
                    one_key(),
                ),
                chrom(
                    "Te and ti",
                    "Up from do to the minor or major seventh.",
                    &["m7", "M7"],
                    &["ascending"],
                    one_key(),
                ),
                chrom(
                    "Ra and re",
                    "Up from do to the minor or major second.",
                    &["m2", "M2"],
                    &["ascending"],
                    one_key(),
                ),
                chrom(
                    "Fa, fi and sol",
                    "Up from do to the fourth, the tritone or the fifth.",
                    &["P4", "TT", "P5"],
                    &["ascending"],
                    one_key(),
                ),
                chrom(
                    "All twelve up",
                    "Every interval up from do, in and out of the key.",
                    &SIMPLE,
                    &["ascending"],
                    one_key(),
                ),
                chrom(
                    "All twelve down",
                    "Every interval down from do.",
                    &SIMPLE,
                    &["descending"],
                    one_key(),
                ),
                chrom(
                    "All twelve, new keys",
                    "Up or down from do; the key changes every question.",
                    &SIMPLE,
                    &["ascending", "descending"],
                    changing_keys(),
                ),
            ],
        ),
    ];
    workshop(
        RP_INTERVALS_KEY,
        "interval_id",
        20,
        "Intervals in a key",
        "In a key",
        "Intervals heard against a cadence: up and down from do, two at a time, then between any two notes of the key, then intervals that leave the scale. Notes are named in fixed do after each answer.",
        modules,
    )
}

// ---------------------------------------------------------------------------------------------
// Interval comparison (EarMaster's five pools × four ways the two intervals are related)
// ---------------------------------------------------------------------------------------------

/// A comparison pool: its name and its lessons' interval sets.
type Pool<'a> = (&'a str, Vec<(&'a str, Vec<&'a str>)>);

fn compare_course() -> Course {
    let pools: Vec<Pool> = vec![
        (
            "perfect intervals",
            vec![
                ("4th and 5th", vec!["P4", "P5"]),
                ("5th and octave", vec!["P5", "P8"]),
                ("4th, 5th and octave", vec!["P4", "P5", "P8"]),
            ],
        ),
        (
            "3rds and 6ths",
            vec![
                ("Minor 3rd and minor 6th", vec!["m3", "m6"]),
                ("Major 3rd and minor 6th", vec!["M3", "m6"]),
                ("Minor and major 3rd", vec!["m3", "M3"]),
                ("Minor and major 6th", vec!["m6", "M6"]),
                ("All four", vec!["m3", "M3", "m6", "M6"]),
            ],
        ),
        (
            "2nds, 7ths and the tritone",
            vec![
                ("Minor 2nd and major 7th", vec!["m2", "M7"]),
                ("Major 2nd and minor 7th", vec!["M2", "m7"]),
                ("2nds and 7ths", vec!["m2", "M2", "m7", "M7"]),
                ("With the tritone", vec!["m2", "M2", "TT", "m7", "M7"]),
            ],
        ),
        (
            "all simple intervals",
            vec![
                ("6ths and 7ths", vec!["m6", "M6", "m7", "M7"]),
                ("5th to octave", vec!["P5", "m6", "M6", "m7", "M7", "P8"]),
                ("2nds and 3rds", vec!["m2", "M2", "m3", "M3"]),
                ("2nds to tritone", vec!["m2", "M2", "m3", "M3", "P4", "TT"]),
                ("All twelve", SIMPLE.to_vec()),
            ],
        ),
        (
            "compound intervals",
            vec![
                ("11th, 12th and two octaves", vec!["P11", "P12", "P15"]),
                ("10ths and 13ths", vec!["m10", "M10", "m13", "M13"]),
                (
                    "9ths, 12th and 14ths",
                    vec!["m9", "M9", "P12", "m14", "M14"],
                ),
                (
                    "All twelve",
                    vec![
                        "m9", "M9", "m10", "M10", "P11", "A11", "P12", "m13", "M13", "m14", "M14",
                        "P15",
                    ],
                ),
            ],
        ),
    ];
    let relations: [(&str, &[&str]); 4] = [
        ("Same first note", &["first"]),
        ("A shared note", &["first", "second"]),
        ("Nearby first notes", &["near"]),
        ("No shared note", &["none"]),
    ];
    let mut modules = Vec::new();
    for (rel, shared) in relations {
        for (pool, steps) in &pools {
            let range = if pool.starts_with("compound") {
                json!({ "low": "C3", "high": "C4" })
            } else {
                json!({ "low": "C3", "high": "C5" })
            };
            let base = json!({ "pairs_from_pool": true, "shared": shared, "root_range": range });
            let mut lessons: Vec<Lesson> = steps
                .iter()
                .map(|(t, ivs)| {
                    lesson(
                        *t,
                        format!("Which is larger: {}, ascending.", interval_list(ivs)),
                        merge(
                            base.clone(),
                            json!({ "intervals": ivs, "directions": ["ascending"] }),
                        ),
                    )
                })
                .collect();
            let widest = &steps.last().unwrap().1;
            lessons.push(test(
                "Test: all, up and down",
                format!(
                    "{}, ascending or descending, no feedback until the end.",
                    interval_list(widest)
                ),
                merge(
                    base,
                    json!({ "intervals": widest, "directions": ["ascending", "descending"] }),
                ),
            ));
            modules.push(module(format!("{rel}: {pool}"), lessons));
        }
    }
    workshop(
        RP_COMPARE,
        "interval_comparison",
        20,
        "Interval comparison",
        "Compare",
        "Two different intervals from a pool: which is larger? Five pools (perfect, 3rds and 6ths, 2nds and 7ths, all simple, compound), each first with the two starting on the same note, then sharing either note, then starting near each other, then unrelated.",
        modules,
    )
}

// ---------------------------------------------------------------------------------------------
// Interval singing (from do: up, down, then complementary pairs)
// ---------------------------------------------------------------------------------------------

fn sing_course() -> Course {
    let key = json!({ "in_key": true, "start": "tonic", "keys": "all", "key_change": "per_block" });
    // (degrees reached, intervals): up from do, then the same degree counts down.
    let up: [(&str, &[&str]); 12] = [
        ("2", &["M2"]),
        ("3", &["M3"]),
        ("2 and 3", &["M2", "M3"]),
        ("4", &["P4"]),
        ("5", &["P5"]),
        ("4 and 5", &["P4", "P5"]),
        ("2, 3, 4 and 5", &["M2", "M3", "P4", "P5"]),
        ("6", &["M6"]),
        ("7", &["M7"]),
        ("8", &["P8"]),
        ("6, 7 and 8", &["M6", "M7", "P8"]),
        ("every degree", &["M2", "M3", "P4", "P5", "M6", "M7", "P8"]),
    ];
    let down: [(&str, &[&str]); 12] = [
        ("7", &["m2"]),
        ("6", &["m3"]),
        ("7 and 6", &["m2", "m3"]),
        ("5", &["P4"]),
        ("4", &["P5"]),
        ("5 and 4", &["P4", "P5"]),
        ("7, 6, 5 and 4", &["m2", "m3", "P4", "P5"]),
        ("3", &["m6"]),
        ("2", &["m7"]),
        ("1", &["P8"]),
        ("3, 2 and 1", &["m6", "m7", "P8"]),
        ("every degree", &["m2", "m3", "P4", "P5", "m6", "m7", "P8"]),
    ];
    let make = |sets: &[(&str, &[&str])], dirs: &[&str], where_: &str| -> Vec<Lesson> {
        sets.iter()
            .map(|(t, ivs)| {
                lesson(
                    format!("1 → {t}{where_} ({})", interval_list(ivs).to_lowercase()),
                    format!(
                        "Hear the key and do, then sing {}{where_}.",
                        interval_list(ivs).to_lowercase()
                    ),
                    merge(key.clone(), json!({ "intervals": ivs, "directions": dirs })),
                )
            })
            .collect()
    };
    // Both ways: each interval up, or its complement down, reaching the same degree.
    let both: Vec<Lesson> = up
        .iter()
        .zip(down.iter())
        .map(|((t, ups), (_, downs))| {
            let mut ivs: Vec<&str> = Vec::new();
            for i in ups.iter().chain(downs.iter()) {
                if !ivs.contains(i) {
                    ivs.push(i);
                }
            }
            lesson(
                format!("1 → {t}, above or below"),
                format!(
                    "From do, sing {t} above (a {}) or below (a {}).",
                    interval_list(ups).to_lowercase(),
                    interval_list(downs).to_lowercase()
                ),
                merge(
                    key.clone(),
                    json!({ "intervals": ivs, "directions": ["up", "down"] }),
                ),
            )
        })
        .collect();
    workshop(
        RP_SING,
        "sing_interval",
        8,
        "Interval singing",
        "Sing",
        "Hear the key and do, then sing the other note: up from do one degree at a time, then down, then each degree above or below (complementary intervals). One key for the session, in your own range.",
        vec![
            module("Up from do", make(&up, &["up"], "")),
            module("Down from do", make(&down, &["down"], " below")),
            module("Above or below do", both),
        ],
    )
}

// ---------------------------------------------------------------------------------------------
// Chords
// ---------------------------------------------------------------------------------------------

fn chord_drills(qualities: &[&str], extra: Value) -> Vec<Lesson> {
    DRILLS
        .iter()
        .enumerate()
        .map(|(i, (name, _))| {
            lesson(
                *name,
                format!("{}, {}.", chord_list(qualities), name.to_lowercase()),
                merge(
                    json!({ "qualities": qualities, "styles": chord_styles(i) }),
                    extra.clone(),
                ),
            )
        })
        .collect()
}

const SEVENTHS: [&str; 17] = [
    "dominant7",
    "minor7",
    "major7",
    "minor_major7",
    "diminished7",
    "dominant7_b5",
    "half_diminished7",
    "dominant7_s5",
    "dominant7_sus4",
    "major6",
    "minor6",
    "minor_major7_b5",
    "major7_b5",
    "major7_s5",
    "dominant7_sus2",
    "major7_sus4",
    "major7_sus2",
];

fn chords_course() -> Course {
    let pools: Vec<(&str, Vec<&str>)> = vec![
        ("Major and minor", vec!["major", "minor"]),
        ("Major, minor and sus4", vec!["major", "minor", "sus4"]),
        (
            "Major with an altered 5th",
            vec!["major", "major_b5", "augmented"],
        ),
        ("Minor and diminished", vec!["minor", "diminished"]),
        ("Sus chords", vec!["sus2", "sus4"]),
        (
            "Triads with an altered 5th",
            vec!["diminished", "major_b5", "augmented"],
        ),
        (
            "All triads",
            vec![
                "major",
                "minor",
                "diminished",
                "major_b5",
                "augmented",
                "sus2",
                "sus4",
            ],
        ),
        (
            "Major triad plus a tone",
            vec!["dominant7", "major7", "major6"],
        ),
        (
            "Minor triad plus a tone",
            vec!["minor7", "minor_major7", "minor6"],
        ),
        (
            "Diminished triad plus a tone",
            vec!["diminished7", "half_diminished7", "minor_major7_b5"],
        ),
        ("Major ♭5 plus a tone", vec!["dominant7_b5", "major7_b5"]),
        ("Augmented plus a tone", vec!["dominant7_s5", "major7_s5"]),
        (
            "Sus chords with a minor 7th",
            vec!["dominant7_sus4", "dominant7_sus2"],
        ),
        (
            "Sus chords with a major 7th",
            vec!["major7_sus4", "major7_sus2"],
        ),
        (
            "Sus 7th chords",
            vec![
                "dominant7_sus4",
                "dominant7_sus2",
                "major7_sus4",
                "major7_sus2",
            ],
        ),
        (
            "Major 7ths with a perfect or altered 5th",
            vec!["major7", "major7_b5", "major7_s5"],
        ),
        (
            "7ths with a perfect or altered 5th",
            vec!["dominant7", "dominant7_b5", "dominant7_s5"],
        ),
        (
            "Minor and diminished 7ths",
            vec![
                "minor7",
                "minor_major7",
                "diminished7",
                "half_diminished7",
                "minor_major7_b5",
            ],
        ),
        (
            "All major 7ths",
            vec![
                "major7",
                "minor_major7",
                "minor_major7_b5",
                "major7_b5",
                "major7_s5",
                "major7_sus4",
                "major7_sus2",
            ],
        ),
        (
            "All chords with a minor 7th",
            vec![
                "dominant7",
                "minor7",
                "dominant7_b5",
                "half_diminished7",
                "dominant7_s5",
                "dominant7_sus4",
                "dominant7_sus2",
            ],
        ),
        (
            "6ths and the diminished 7th",
            vec!["diminished7", "major6", "minor6"],
        ),
        ("All 7th chords", SEVENTHS.to_vec()),
    ];
    workshop(
        RP_CHORDS,
        "chord_id",
        20,
        "Chord identification",
        "Chords",
        "Name the chord. Triads in contrasting pairs, then all triads, then sevenths by which tone is added to which triad, then the large families. Each set is heard arpeggiated up, down, as a block, then all three mixed.",
        pools
            .into_iter()
            .map(|(t, q)| module(t, chord_drills(&q, json!({}))))
            .collect(),
    )
}

fn inversions_course() -> Course {
    let pools: Vec<(&str, Vec<&str>)> = vec![
        ("Major", vec!["major"]),
        ("Minor", vec!["minor"]),
        ("Sus4", vec!["sus4"]),
        ("Major ♭5", vec!["major_b5"]),
        ("Diminished", vec!["diminished"]),
        (
            "Major with a perfect or altered 5th",
            vec!["major", "augmented", "major_b5"],
        ),
        (
            "Minor with a perfect or altered 5th",
            vec!["minor", "diminished"],
        ),
        ("Altered 5ths", vec!["diminished", "augmented", "major_b5"]),
        (
            "All the triads",
            vec![
                "major",
                "minor",
                "diminished",
                "augmented",
                "sus4",
                "major_b5",
            ],
        ),
        ("Major 7th and 7th", vec!["major7", "dominant7"]),
        (
            "Minor 7th and minor-major 7th",
            vec!["minor7", "minor_major7"],
        ),
        (
            "Half-diminished, diminished and minor-major ♭5",
            vec!["half_diminished7", "diminished7", "minor_major7_b5"],
        ),
        ("7th ♭5 and major 7th ♭5", vec!["dominant7_b5", "major7_b5"]),
        ("7th ♯5 and major 7th ♯5", vec!["dominant7_s5", "major7_s5"]),
        (
            "The three dominant shapes",
            vec!["dominant7", "dominant7_b5", "dominant7_s5"],
        ),
        (
            "The three major 7th shapes",
            vec!["major7", "major7_b5", "major7_s5"],
        ),
        (
            "The minor-family 7ths",
            vec![
                "minor7",
                "half_diminished7",
                "diminished7",
                "minor_major7_b5",
                "minor_major7",
            ],
        ),
        (
            "All the 7th chords",
            vec![
                "major7",
                "dominant7",
                "minor7",
                "half_diminished7",
                "diminished7",
                "minor_major7_b5",
                "dominant7_b5",
                "major7_b5",
                "dominant7_s5",
                "major7_s5",
                "minor_major7",
            ],
        ),
    ];
    workshop(
        RP_INVERSIONS,
        "chord_id",
        20,
        "Chord inversions",
        "Inversions",
        "Name the chord and its inversion: each triad alone, then triads with altered fifths, then sevenths in pairs and families. The augmented triad and the diminished seventh have no inversions to name. Sus2 is left out: its inversions are sus4 chords on another root.",
        pools
            .into_iter()
            .map(|(t, q)| {
                module(
                    t,
                    chord_drills(
                        &q,
                        json!({ "answer_inversion": true, "inversions": [0, 1, 2, 3] }),
                    ),
                )
            })
            .collect(),
    )
}

// ---------------------------------------------------------------------------------------------
// Scales
// ---------------------------------------------------------------------------------------------

fn scales_course() -> Course {
    let modes = [
        "major",
        "dorian",
        "phrygian",
        "lydian",
        "mixolydian",
        "natural_minor",
        "locrian",
        "harmonic_minor",
        "melodic_minor",
    ];
    let melodic_family = [
        "melodic_minor",
        "whole_tone",
        "dorian_b2",
        "lydian_augmented",
        "lydian_dominant",
        "mixolydian_b6",
        "locrian_s2",
        "altered",
        "diminished_whole_half",
        "diminished_half_whole",
    ];
    let harmonic_family = [
        "harmonic_minor",
        "locrian_s6",
        "ionian_s5",
        "dorian_s4",
        "phrygian_dominant",
        "lydian_s2",
        "ultra_locrian",
    ];
    let all: Vec<&str> = modes
        .iter()
        .chain(melodic_family.iter().skip(1))
        .copied()
        .collect();
    let name = |id: &str| {
        crate::theory::ScaleKind::from_id(id)
            .map(|s| s.long_name())
            .unwrap_or("?")
    };
    let l = |t: &str, ids: &[&str], dir: &str| {
        lesson(
            t,
            format!(
                "{}, {dir}.",
                list_names(&ids.iter().map(|i| name(i)).collect::<Vec<_>>())
            ),
            json!({ "scales": ids, "direction": dir }),
        )
    };
    workshop(
        RP_SCALES,
        "scale_id",
        15,
        "Scale identification",
        "Scales",
        "Name the scale or mode: the modes grouped by how they begin, then all of them, then the families of melodic and harmonic minor, the pentatonics, and finally descending.",
        vec![
            module(
                "Modes",
                vec![
                    l(
                        "Starting with two whole tones",
                        &["major", "lydian", "mixolydian"],
                        "ascending",
                    ),
                    l(
                        "Minor-sounding scales",
                        &["dorian", "natural_minor", "harmonic_minor", "melodic_minor"],
                        "ascending",
                    ),
                    l(
                        "Starting with a semitone",
                        &["phrygian", "locrian"],
                        "ascending",
                    ),
                    l("All modes and the minor scales", &modes, "ascending"),
                ],
            ),
            module(
                "Melodic minor family",
                vec![
                    l(
                        "Whole tone and Lydian variants",
                        &[
                            "whole_tone",
                            "lydian_augmented",
                            "lydian_dominant",
                            "mixolydian_b6",
                        ],
                        "ascending",
                    ),
                    l(
                        "Melodic minor, Locrian ♯2, whole–half diminished",
                        &["melodic_minor", "locrian_s2", "diminished_whole_half"],
                        "ascending",
                    ),
                    l(
                        "Phrygian ♯6, altered, half–whole diminished",
                        &["dorian_b2", "altered", "diminished_half_whole"],
                        "ascending",
                    ),
                    l("The whole family", &melodic_family, "ascending"),
                    l("All scales so far", &all, "ascending"),
                ],
            ),
            module(
                "Harmonic minor family",
                vec![l("The harmonic minor modes", &harmonic_family, "ascending")],
            ),
            module(
                "Pentatonic",
                vec![l(
                    "The two pentatonic scales",
                    &["major_pentatonic", "minor_pentatonic"],
                    "ascending",
                )],
            ),
            module(
                "Descending",
                vec![
                    l("All modes, descending", &modes, "descending"),
                    l(
                        "The harmonic minor modes, descending",
                        &harmonic_family,
                        "descending",
                    ),
                ],
            ),
        ],
    )
}

// ---------------------------------------------------------------------------------------------
// Melodies (EarMaster's ladder: degrees grow from do and re, then rhythm, keys, minor)
// ---------------------------------------------------------------------------------------------

/// "do re mi" for the first `n` degrees, in major or (do-based) minor.
fn degree_title(n: usize, minor: bool) -> String {
    let names = if minor {
        ["do", "re", "me", "fa", "sol", "le", "te"]
    } else {
        ["do", "re", "mi", "fa", "sol", "la", "ti"]
    };
    names[..n].join(" ")
}

pub(crate) fn melody(g: Value) -> Value {
    let mut g = merge(
        json!({ "max_leap": 2, "step_rate": 0.8, "start_degrees": [1, 3, 5] }),
        g,
    );
    if g.get("keys").is_none() {
        g["keys"] = json!(["C"]);
    }
    json!({ "melody": { "generate": g } })
}

/// The melody ladder shared by dictation and sight singing. `bar_steps` are the lengths of the
/// rhythm modules (1 and 2 bars for dictation, 1, 2 and 4 for sight singing).
pub(crate) fn melody_modules(bar_steps: &[u32]) -> Vec<Module> {
    let mut modules = Vec::new();
    // Notes only, in C major: the degrees grow by one each lesson, at two lengths.
    for (title, lengths, from) in [
        ("3 to 4 notes in C major", [3, 4], 2),
        ("5 to 6 notes in C major", [5, 6], 2),
        ("7 to 8 notes in C major", [7, 8], 3),
    ] {
        let mut lessons = Vec::new();
        for notes in lengths {
            for d in from..=7 {
                lessons.push(lesson(
                    format!("{notes} notes: {}", degree_title(d, false)),
                    format!("{notes}-note melodies on {}.", degree_title(d, false)),
                    melody(json!({ "degrees": (1..=d as u8).collect::<Vec<_>>(), "notes": notes })),
                ));
            }
        }
        modules.push(module(title, lessons));
    }
    let rhythm_module = |title: String, meter: &str, eighths: bool, keys: Value, minor: bool| {
        let mut rhythms = if meter == "3/4" {
            vec!["dotted_half", "half", "quarter"]
        } else {
            vec!["whole", "half", "quarter"]
        };
        if eighths {
            rhythms.push("eighth");
        }
        let mut lessons = Vec::new();
        for (k, bars) in bar_steps.iter().enumerate() {
            let from = if k == 0 { 2 } else { 3 };
            for d in from..=7 {
                let mut g = json!({
                    "degrees": (1..=d as u8).collect::<Vec<_>>(), "bars": bars,
                    "meters": [meter], "rhythms": rhythms, "keys": keys,
                });
                if minor {
                    g["modes"] = json!(["minor"]);
                }
                lessons.push(lesson(
                    format!(
                        "{bars} bar{}: {}",
                        if *bars == 1 { "" } else { "s" },
                        degree_title(d, minor)
                    ),
                    format!(
                        "{bars}-bar melodies in {meter} on {}.",
                        degree_title(d, minor)
                    ),
                    melody(g),
                ));
            }
        }
        module(title, lessons)
    };
    let meters = [("4/4", false), ("3/4", false), ("4/4", true), ("3/4", true)];
    let with_eighths = |e: bool| if e { ", with 8th notes" } else { "" };
    for (meter, e) in meters {
        modules.push(rhythm_module(
            format!("C major{}, {meter}", with_eighths(e)),
            meter,
            e,
            json!(["C"]),
            false,
        ));
    }
    // Empty keys: any key with up to two sharps or flats.
    for (meter, e) in meters {
        modules.push(rhythm_module(
            format!("Other major keys{}, {meter}", with_eighths(e)),
            meter,
            e,
            json!([]),
            false,
        ));
    }
    for (meter, e) in meters {
        modules.push(rhythm_module(
            format!("A minor{}, {meter}", with_eighths(e)),
            meter,
            e,
            json!(["A"]),
            true,
        ));
    }
    modules.push(rhythm_module(
        "All minor keys, with 8th notes, 4/4".into(),
        "4/4",
        true,
        json!([]),
        true,
    ));
    modules
}

fn dictation_course() -> Course {
    workshop(
        RP_DICTATION,
        "melodic_dictation",
        4,
        "Melodic dictation",
        "Dictation",
        "Hear a melody after its key and write it on the staff. Do and re first, a degree more each lesson; then rhythm in 4/4 and 3/4, 8th notes, other keys and minor keys.",
        melody_modules(&[1, 2]),
    )
}

fn sight_course() -> Course {
    workshop(
        RP_SIGHT,
        "sight_sing",
        4,
        "Melodic sight singing",
        "Sight singing",
        "Sing a melody from the staff after its key. The same ladder as dictation, with 1, 2 and 4-bar melodies.",
        melody_modules(&[1, 2, 4]),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::courses::{compose_daily_set, level_session};
    use crate::generate::{GenContext, generate_block};
    use crate::schema::Mask;
    use crate::storage::Db;
    use crate::validate::validate_session;

    #[test]
    fn every_lesson_generates_in_any_key() {
        let ctx = GenContext::default();
        for c in courses() {
            for (i, l) in c.levels.iter().enumerate() {
                for b in level_session(&c, i as u32 + 1).unwrap().blocks {
                    for seed in 0..8 {
                        if let Err(e) = generate_block(&b, seed, &ctx) {
                            panic!(
                                "{} lesson {} ({} / {}) seed {seed}: {}",
                                c.id,
                                i + 1,
                                l.module.as_deref().unwrap_or(""),
                                l.title,
                                e.0
                            );
                        }
                    }
                }
            }
        }
    }

    #[test]
    fn lessons_are_one_block_in_a_module() {
        // The core's sessions mix several parts; every other course is one exercise a lesson.
        for c in courses()
            .into_iter()
            .filter(|c| c.id != crate::audiation_courses::RP_CORE)
        {
            assert_eq!(c.category, CATEGORY);
            for l in &c.levels {
                assert!(l.module.is_some(), "{}", c.id);
                assert_eq!(l.blocks.len(), 1, "{}", c.id);
                assert!(l.blocks[0].course.as_ref().unwrap().verify);
            }
        }
    }

    #[test]
    fn workshops_follow_the_earmaster_shape() {
        let find = |id: &str| courses().into_iter().find(|c| c.id == id).unwrap();
        let modules = |c: &Course| {
            let mut v: Vec<String> = Vec::new();
            for l in &c.levels {
                let m = l.module.clone().unwrap();
                if v.last() != Some(&m) {
                    v.push(m);
                }
            }
            v.len()
        };
        assert_eq!(modules(&find(RP_INTERVALS)), 16);
        assert_eq!(modules(&find(RP_CHORDS)), 22);
        assert_eq!(find(RP_CHORDS).levels.len(), 88);
        assert_eq!(modules(&find(RP_INVERSIONS)), 18);
        assert_eq!(modules(&find(RP_COMPARE)), 20);
        assert_eq!(find(RP_SING).levels.len(), 36);
        assert_eq!(find(RP_DEGREES).levels.len(), 32);
        assert_eq!(find(RP_SCALES).levels.len(), 13);
    }

    #[test]
    fn the_relative_pitch_set_has_its_own_courses() {
        let db = Db::open_in_memory().unwrap();
        let ctx = GenContext::default();
        let set = compose_daily_set(&db, &ctx, chrono::Utc::now(), CATEGORY).unwrap();
        let ids: Vec<&str> = set.items.iter().map(|i| i.course.as_str()).collect();
        assert!(
            ids.contains(&RP_DEGREES) && ids.contains(&crate::audiation_courses::RP_CORE),
            "{ids:?}"
        );
        assert!(ids.iter().all(|i| i.starts_with("rp_")));
        // No clearing glissando: that is for absolute pitch.
        assert!(
            set.spec
                .blocks
                .iter()
                .all(|b| b.pre_mask != Some(Mask::ShepardGlissando))
        );
        assert!(set.spec.title.starts_with("Relative pitch"));
        assert!(validate_session(&set.spec, &ctx).ok);
    }
}
