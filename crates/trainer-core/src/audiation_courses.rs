//! The audiation side of relative pitch, after Gordon's Music Learning Theory, Kodály's inner
//! hearing and the working-memory research in aural skills (Karpinski, Klonoski, Chenette): tonal
//! context first, then a vocabulary of tonal patterns, then holding and changing pitches in the
//! head, then structure (cadences, bass lines) and single-pass error detection.
//!
//! `rp_core` is the daily core: four tiers of short mixed sessions. The other courses here are
//! workshops for each exercise. See docs/RELATIVE_PITCH.md.

use serde_json::{Value, json};

use crate::courses::{Course, Level, PassRule, Schedule, block};
use crate::rp_courses::{CATEGORY, lesson, melody, merge, module, test, workshop};

pub const RP_CORE: &str = "rp_core";
pub const RP_HOME: &str = "rp_home";
pub const RP_PATTERNS: &str = "rp_patterns";
pub const RP_MEMORY: &str = "rp_memory";
pub const RP_INNER: &str = "rp_inner";
pub const RP_HARMONY: &str = "rp_harmony";
pub const RP_ERRORS: &str = "rp_errors";
pub const RP_WALK: &str = "rp_walk";
pub const RP_MATCH: &str = "rp_match";

pub fn workshops() -> Vec<Course> {
    vec![
        home_course(),
        patterns_course(),
        memory_course(),
        inner_course(),
        harmony_course(),
        errors_course(),
        walk_course(),
        match_course(),
    ]
}

fn one_key() -> Value {
    json!({ "keys": "all", "key_change": "per_block" })
}

fn new_keys() -> Value {
    json!({ "keys": "all", "key_change": "per_trial" })
}

// ---------------------------------------------------------------------------------------------
// Find home
// ---------------------------------------------------------------------------------------------

fn home_course() -> Course {
    let sing = |t: &str, s: &str, modes: &[&str]| {
        lesson(t, s, json!({ "modes": modes, "answer": "sing" }))
    };
    let name = |t: &str, s: &str, modes: &[&str]| {
        lesson(t, s, json!({ "modes": modes, "answer": "name" })).of("resting_tone", 12)
    };
    workshop(
        RP_HOME,
        "resting_tone",
        10,
        "Find home",
        "Home",
        "A phrase that outlines a key without a cadence stops mid-way: breathe and sing its resting tone, then name it, first in major and minor, then in the modes. Tonality is heard before anything else is.",
        vec![
            module(
                "Major and minor",
                vec![
                    sing(
                        "Sing home: major",
                        "Phrases in major keys; sing do on “bum”.",
                        &["major"],
                    ),
                    sing(
                        "Sing home: minor",
                        "Phrases in minor keys; sing la.",
                        &["natural_minor"],
                    ),
                    sing(
                        "Sing home: major or minor",
                        "Either; sing the resting tone.",
                        &["major", "natural_minor"],
                    ),
                    name(
                        "Name it: major or minor",
                        "Do (major) or la (minor).",
                        &["major", "natural_minor"],
                    ),
                    name(
                        "Name it: with harmonic minor",
                        "Major, minor and harmonic minor.",
                        &["major", "natural_minor", "harmonic_minor"],
                    ),
                ],
            ),
            module(
                "The modes",
                vec![
                    name(
                        "Dorian",
                        "Major, minor and Dorian (re).",
                        &["major", "natural_minor", "dorian"],
                    ),
                    name(
                        "Mixolydian",
                        "Major, Mixolydian (sol) and Dorian.",
                        &["major", "mixolydian", "dorian"],
                    ),
                    sing(
                        "Sing home in the modes",
                        "Major, minor, Dorian, Mixolydian; sing the resting tone.",
                        &["major", "natural_minor", "dorian", "mixolydian"],
                    ),
                    name(
                        "Lydian and Phrygian",
                        "Major, Lydian (fa), minor, Phrygian (mi).",
                        &["major", "lydian", "natural_minor", "phrygian"],
                    ),
                    name(
                        "All eight",
                        "Every tonality, Locrian (ti) included.",
                        &[
                            "major",
                            "natural_minor",
                            "harmonic_minor",
                            "dorian",
                            "phrygian",
                            "lydian",
                            "mixolydian",
                            "locrian",
                        ],
                    ),
                    sing(
                        "Sing home in all eight",
                        "Every tonality; sing the resting tone.",
                        &[
                            "major",
                            "natural_minor",
                            "harmonic_minor",
                            "dorian",
                            "phrygian",
                            "lydian",
                            "mixolydian",
                            "locrian",
                        ],
                    ),
                ],
            ),
        ],
    )
}

// ---------------------------------------------------------------------------------------------
// Tonal patterns and the dialogue
// ---------------------------------------------------------------------------------------------

fn patterns_course() -> Course {
    let pat = |t: &str, s: &str, p: Value| lesson(t, s, merge(one_key(), p));
    let dia =
        |t: &str, s: &str, p: Value| lesson(t, s, merge(one_key(), p)).of("pattern_dialogue", 10);
    let td = json!(["tonic", "dominant"]);
    let tds = json!(["tonic", "dominant", "subdominant"]);
    workshop(
        RP_PATTERNS,
        "tonal_pattern",
        12,
        "Tonal patterns",
        "Patterns",
        "Gordon's tonal vocabulary: short tonic, dominant and subdominant patterns, echoed after a silent breath, each first with the tonic sounding (teaching) and then alone (evaluation). On “bum”, then in solfège, then heard on “bum” and echoed in solfège. Then the dialogue: answer a pattern instead of copying it.",
        vec![
            module(
                "Tonic and dominant in major",
                vec![
                    pat(
                        "Two notes on “bum”",
                        "Tonic and dominant patterns of two notes.",
                        json!({ "functions": td, "lengths": [2] }),
                    ),
                    pat(
                        "Three notes on “bum”",
                        "Do mi sol, sol ti re and the like.",
                        json!({ "functions": td, "lengths": [3] }),
                    ),
                    pat(
                        "Three notes in solfège",
                        "The syllables are shown; echo in solfège.",
                        json!({ "functions": td, "lengths": [3], "syllables": "solfege" }),
                    ),
                    pat(
                        "Heard on “bum”, echoed in solfège",
                        "Translate from the ear.",
                        json!({ "functions": td, "lengths": [3], "syllables": "translate" }),
                    ),
                ],
            ),
            module(
                "Tonic and dominant in minor",
                vec![
                    pat(
                        "Three notes on “bum”",
                        "Minor tonic and dominant patterns.",
                        json!({ "functions": td, "lengths": [3], "modes": ["minor"] }),
                    ),
                    pat(
                        "In solfège",
                        "Echo in solfège.",
                        json!({ "functions": td, "lengths": [3], "modes": ["minor"], "syllables": "solfege" }),
                    ),
                    pat(
                        "Heard on “bum”, echoed in solfège",
                        "Translate from the ear.",
                        json!({ "functions": td, "lengths": [3], "modes": ["minor"], "syllables": "translate" }),
                    ),
                    pat(
                        "Major and minor mixed",
                        "Either mode, on “bum”.",
                        json!({ "functions": td, "lengths": [3], "modes": ["major", "minor"] }),
                    ),
                ],
            ),
            module(
                "With the subdominant",
                vec![
                    pat(
                        "Three functions on “bum”",
                        "Tonic, dominant and subdominant.",
                        json!({ "functions": tds, "lengths": [3] }),
                    ),
                    pat(
                        "Three functions in solfège",
                        "Echo in solfège.",
                        json!({ "functions": tds, "lengths": [3], "syllables": "translate" }),
                    ),
                    pat(
                        "In minor",
                        "Minor tonic, dominant and subdominant.",
                        json!({ "functions": tds, "lengths": [3], "modes": ["minor"], "syllables": "translate" }),
                    ),
                ],
            ),
            module(
                "Longer patterns",
                vec![
                    pat(
                        "Four notes",
                        "Four-note patterns, on “bum”.",
                        json!({ "functions": tds, "lengths": [4] }),
                    ),
                    pat(
                        "Four and five notes in solfège",
                        "Heard on “bum”, echoed in solfège.",
                        json!({ "functions": tds, "lengths": [4, 5], "syllables": "translate" }),
                    ),
                    pat(
                        "Minor, four and five notes",
                        "Translate from the ear in minor.",
                        json!({ "functions": tds, "lengths": [4, 5], "modes": ["minor"], "syllables": "translate" }),
                    ),
                    pat(
                        "Evaluation only",
                        "No tonic under the echo.",
                        json!({ "functions": tds, "lengths": [3, 4, 5], "modes": ["major", "minor"], "passes": "evaluation", "syllables": "translate" }),
                    ),
                ],
            ),
            module(
                "Tendency tones",
                vec![
                    pat(
                        "In major",
                        "Ti–do, fa–mi, re–do, la–sol and the chromatic ones (fi–sol, te–la).",
                        json!({ "functions": ["tendency"], "lengths": [2], "syllables": "solfege" }),
                    ),
                    pat(
                        "In minor",
                        "The minor tendencies.",
                        json!({ "functions": ["tendency"], "lengths": [2], "modes": ["minor"], "syllables": "solfege" }),
                    ),
                    pat(
                        "By ear",
                        "Heard on “bum”, echoed in solfège.",
                        json!({ "functions": ["tendency"], "lengths": [2], "modes": ["major", "minor"], "syllables": "translate" }),
                    ),
                ],
            ),
            module(
                "Dialogue",
                vec![
                    dia(
                        "The first note",
                        "A quick pattern; sing back only its first note.",
                        json!({ "stage": "first_pitch" }),
                    ),
                    dia(
                        "The first note in minor",
                        "The same in minor.",
                        json!({ "stage": "first_pitch", "modes": ["minor"] }),
                    ),
                    dia(
                        "Another of the same",
                        "Answer with a different pattern of the same function.",
                        json!({ "stage": "same_function" }),
                    ),
                    dia(
                        "The other function",
                        "Answer tonic with dominant, dominant with tonic.",
                        json!({ "stage": "opposite_function" }),
                    ),
                    dia(
                        "The other function in minor",
                        "The same in minor.",
                        json!({ "stage": "opposite_function", "modes": ["minor"] }),
                    ),
                ],
            ),
        ],
    )
}

// ---------------------------------------------------------------------------------------------
// Working memory
// ---------------------------------------------------------------------------------------------

fn memory_course() -> Course {
    let sd =
        |t: &str, s: &str, p: Value| lesson(t, s, merge(json!({ "task": "same_different" }), p));
    let probe = |t: &str, s: &str, n: u32| lesson(t, s, json!({ "task": "probe", "notes": [n] }));
    let nb = |t: &str, s: &str, p: Value| lesson(t, s, p).of("n_back", 2);
    let canon = |t: &str, s: &str, p: Value| {
        lesson(t, s, merge(json!({ "mode": "sing" }), p)).of("n_back", 4)
    };
    let up =
        |t: &str, s: &str, p: Value| lesson(t, s, merge(one_key(), p)).of("melodic_updating", 8);
    workshop(
        RP_MEMORY,
        "pattern_memory",
        16,
        "Working memory",
        "Memory",
        "Holding pitches in mind without notation: same or different, was a note in the melody, the musical N-back (tap a match, then sing a canon), and changing a melody in your head before singing it.",
        vec![
            module(
                "Same or different",
                vec![
                    sd(
                        "Three notes",
                        "A step changed, or not.",
                        json!({ "notes": [3] }),
                    ),
                    sd(
                        "Four notes",
                        "A step changed, or not.",
                        json!({ "notes": [4] }),
                    ),
                    sd("Five notes", "Longer patterns.", json!({ "notes": [5] })),
                    sd(
                        "A semitone off",
                        "The change leaves the key by a semitone.",
                        json!({ "notes": [4, 5], "change": "chromatic" }),
                    ),
                    sd(
                        "In another key",
                        "The second in another key: only the relations count.",
                        json!({ "notes": [4], "transpose": true, "change": "any" }),
                    ),
                    sd(
                        "Six notes, another key",
                        "Long and transposed.",
                        json!({ "notes": [6], "transpose": true, "change": "any" }),
                    ),
                ],
            ),
            module(
                "Was it in the melody?",
                vec![
                    probe("Four notes", "A melody, then one note: was it there?", 4),
                    probe("Five notes", "Five-note melodies.", 5),
                    probe("Six notes", "Six-note melodies.", 6),
                    probe("Seven notes", "Seven-note melodies.", 7),
                ],
            ),
            module(
                "N-back: match",
                vec![
                    nb(
                        "1-back on do re mi",
                        "Tap when the note repeats the one before.",
                        json!({ "n": 1, "degrees": ["1", "2", "3"] }),
                    ),
                    nb("1-back on 1–5", "Five degrees.", json!({ "n": 1 })),
                    nb(
                        "2-back on 1–5",
                        "Tap when it is the note two back.",
                        json!({ "n": 2 }),
                    ),
                    nb(
                        "2-back on the whole scale",
                        "Every degree and the upper do.",
                        json!({ "n": 2, "degrees": ["1", "2", "3", "4", "5", "6", "7", "8"] }),
                    ),
                    nb(
                        "3-back on 1–5",
                        "Three back.",
                        json!({ "n": 3, "length": 30 }),
                    ),
                    nb(
                        "3-back on the whole scale",
                        "Three back, every degree and the upper do.",
                        json!({ "n": 3, "length": 30, "degrees": ["1", "2", "3", "4", "5", "6", "7", "8"] }),
                    ),
                ],
            ),
            module(
                "N-back: canon",
                vec![
                    canon(
                        "1 behind on do re mi",
                        "Sing each note as the next one sounds.",
                        json!({ "n": 1, "degrees": ["1", "2", "3"], "length": 8 }),
                    ),
                    canon(
                        "1 behind on 1–5",
                        "Five degrees.",
                        json!({ "n": 1, "length": 10 }),
                    ),
                    canon(
                        "2 behind on do re mi",
                        "Two notes behind.",
                        json!({ "n": 2, "degrees": ["1", "2", "3"], "length": 8 }),
                    ),
                    canon(
                        "2 behind on 1–5",
                        "Two behind, five degrees.",
                        json!({ "n": 2, "length": 12 }),
                    ),
                ],
            ),
            module(
                "Melodic updating",
                vec![
                    up(
                        "Swap a degree",
                        "Every mi becomes fa (and the like).",
                        json!({ "notes": [4], "operations": ["replace_degree"] }),
                    ),
                    up(
                        "Change one note",
                        "The third note becomes sol (and the like).",
                        json!({ "notes": [4], "operations": ["replace_position"] }),
                    ),
                    up(
                        "Move it a step",
                        "All of it up or down a step.",
                        json!({ "notes": [4], "operations": ["shift"] }),
                    ),
                    up(
                        "Backwards",
                        "Sing it in retrograde.",
                        json!({ "notes": [4], "operations": ["retrograde"] }),
                    ),
                    up(
                        "Leave a note out",
                        "Keep its beat silent.",
                        json!({ "notes": [5], "operations": ["omit"] }),
                    ),
                    up(
                        "Two changes",
                        "Two changes chained.",
                        json!({ "notes": [5], "changes": 2, "operations": ["replace_degree", "replace_position", "shift", "retrograde"] }),
                    ),
                    up(
                        "Three changes",
                        "Three changes on six notes.",
                        json!({ "notes": [6], "changes": 3, "operations": ["replace_degree", "replace_position", "shift", "retrograde", "omit"], "think_beats": 6 }),
                    ),
                ],
            ),
        ],
    )
}

// ---------------------------------------------------------------------------------------------
// Inner hearing
// ---------------------------------------------------------------------------------------------

fn inner_course() -> Course {
    let g = |bars: u32, degrees: u8, keys: Value, eighths: bool, meter: &str| {
        let mut rhythms = vec!["half", "quarter"];
        if eighths {
            rhythms.push("eighth");
        }
        melody(
            json!({ "bars": bars, "degrees": (1..=degrees).collect::<Vec<_>>(), "keys": keys, "rhythms": rhythms, "meters": [meter] }),
        )
    };
    let l = |t: &str, s: &str, mel: Value, extra: Value| lesson(t, s, merge(mel, extra));
    let silent = json!({ "silent": "bars" });
    workshop(
        RP_INNER,
        "sight_sing",
        4,
        "Inner hearing",
        "Inner hearing",
        "Kodály's silent singing: bars sung in the head only, then aloud again on the right note; strong or weak beats sung silently; and flash scores, read in silence and sung from memory.",
        vec![
            module(
                "Silent bars",
                vec![
                    l(
                        "Bars 2 and 3 silent, do to sol",
                        "Sing bar 1, sing 2 and 3 in your head, come back in on bar 4.",
                        g(4, 5, json!(["C"]), false, "4/4"),
                        silent.clone(),
                    ),
                    l(
                        "The whole scale",
                        "All seven degrees.",
                        g(4, 7, json!(["C"]), false, "4/4"),
                        silent.clone(),
                    ),
                    l(
                        "Other keys",
                        "Keys with up to two sharps or flats.",
                        g(4, 7, json!([]), false, "4/4"),
                        silent.clone(),
                    ),
                    l(
                        "In 3/4 with 8th notes",
                        "Three-four time, with 8ths.",
                        g(4, 7, json!([]), true, "3/4"),
                        silent,
                    ),
                ],
            ),
            module(
                "Strong and weak beats",
                vec![
                    l(
                        "Beat 1 silent",
                        "Sing every note but those on beat 1.",
                        g(4, 7, json!(["C"]), false, "4/4"),
                        json!({ "silent": "strong_beats" }),
                    ),
                    l(
                        "Only beat 1 aloud",
                        "Sing only the notes on beat 1.",
                        g(4, 7, json!(["C"]), false, "4/4"),
                        json!({ "silent": "weak_beats" }),
                    ),
                    l(
                        "Both, other keys",
                        "Beat 1 silent, in other keys.",
                        g(4, 7, json!([]), true, "4/4"),
                        json!({ "silent": "strong_beats" }),
                    ),
                ],
            ),
            module(
                "Flash scores",
                vec![
                    l(
                        "One bar, 8 s",
                        "Read it while the tonic sounds, then sing it from memory.",
                        g(1, 5, json!(["C"]), false, "4/4"),
                        json!({ "mode": "flash", "flash_s": 8 }),
                    ),
                    l(
                        "Two bars, 12 s",
                        "Two bars.",
                        g(2, 7, json!(["C"]), false, "4/4"),
                        json!({ "mode": "flash", "flash_s": 12 }),
                    ),
                    l(
                        "Two bars with 8ths, 15 s",
                        "With 8th notes, other keys.",
                        g(2, 7, json!([]), true, "4/4"),
                        json!({ "mode": "flash", "flash_s": 15 }),
                    ),
                    l(
                        "Four bars, 20 s",
                        "Four bars.",
                        g(4, 7, json!([]), false, "4/4"),
                        json!({ "mode": "flash", "flash_s": 20 }),
                    ),
                ],
            ),
        ],
    )
}

// ---------------------------------------------------------------------------------------------
// Silent pitch walk
// ---------------------------------------------------------------------------------------------

fn walk_course() -> Course {
    let w = |t: &str, s: &str, p: Value| lesson(t, s, p);
    workshop(
        RP_WALK,
        "pitch_walk",
        16,
        "Silent pitch walk",
        "Pitch walk",
        "The Pitch Imagery Arrow Task: the key and a starting note sound, arrows move the note up or down the scale, the first ones with their notes and the rest in silence, then a probe: is it where you landed? Nothing is sung, so it trains and measures hearing pitches in your head apart from the voice. The walks grow longer, then take thirds, lose the heard steps and change key.",
        vec![
            module(
                "Steps of the scale",
                vec![
                    w(
                        "Two silent steps",
                        "Two steps heard, two in your head.",
                        merge(one_key(), json!({ "steps": [2] })),
                    ),
                    w(
                        "Three silent steps",
                        "Three in your head.",
                        merge(one_key(), json!({ "steps": [3] })),
                    ),
                    w(
                        "Four silent steps",
                        "Four in your head.",
                        merge(one_key(), json!({ "steps": [4] })),
                    ),
                    w(
                        "Five or six",
                        "Five or six in your head.",
                        merge(one_key(), json!({ "steps": [5, 6] })),
                    ),
                    w(
                        "Up to eight",
                        "Six to eight in your head.",
                        merge(one_key(), json!({ "steps": [6, 7, 8] })),
                    ),
                ],
            ),
            module(
                "Harder walks",
                vec![
                    w(
                        "Thirds too",
                        "Arrows marked 2 move a third.",
                        merge(one_key(), json!({ "steps": [3, 4], "jumps": [1, 2] })),
                    ),
                    w(
                        "Nothing heard",
                        "Only the starting note sounds.",
                        merge(one_key(), json!({ "steps": [3, 4], "heard": 0 })),
                    ),
                    w(
                        "A semitone off",
                        "A wrong probe is only a semitone away.",
                        merge(
                            one_key(),
                            json!({ "steps": [3, 4], "probe_change": "chromatic" }),
                        ),
                    ),
                    w(
                        "Minor",
                        "In minor keys.",
                        merge(one_key(), json!({ "steps": [3, 4], "modes": ["minor"] })),
                    ),
                    w(
                        "A new key each time",
                        "Thirds and fourths, nothing heard, a new key every walk.",
                        merge(
                            new_keys(),
                            json!({ "steps": [4, 5, 6], "heard": 0, "jumps": [1, 2, 3], "probe_change": "any" }),
                        ),
                    ),
                ],
            ),
        ],
    )
}

// ---------------------------------------------------------------------------------------------
// Pitch matching: the first attempt, holding a note, judging it, long notes
// ---------------------------------------------------------------------------------------------

/// Timbres with strong overtones, where the octave or the fifth is easy to mistake.
const BRIGHT_TIMBRES: [&str; 3] = ["synth:saw", "synth:square", "gm:68"];

fn match_course() -> Course {
    let first = json!({ "grading": { "grade_on": "attack" } });
    let m = |t: &str, s: &str, p: Value| lesson(t, s, p);
    let held = |t: &str, s: &str, delay: u32, tones: u32| {
        lesson(
            t,
            s,
            json!({ "delay_ms": delay, "distractor_tones": tones, "timbres": ["gm:0"] }),
        )
    };
    let hold = |t: &str, s: &str, ms: u32, drone: bool| {
        lesson(t, s, json!({ "hold_ms": ms, "drone": drone })).of("sustain", 4)
    };
    workshop(
        RP_MATCH,
        "pitch_match",
        12,
        "Pitch matching",
        "Match",
        "Hear a note and sing it, measured to separate where it goes wrong: the first attempt graded on its attack (before any slide), through new and bright timbres; the note held in silence and then through other notes; calling your own note flat, in tune or sharp before the result; and long notes with and without a drone. Every note stores its attack, settled error, correction and steadiness.",
        vec![
            module(
                "First attempt",
                vec![
                    m(
                        "Piano",
                        "Graded on the attack: land on it, don't slide to it.",
                        json!({ "timbres": ["gm:0"] }),
                    )
                    .with(first.clone()),
                    m(
                        "Strings and flute",
                        "Softer attacks.",
                        json!({ "timbres": ["gm:48", "gm:73"] }),
                    )
                    .with(first.clone()),
                    m(
                        "Bright timbres",
                        "Saw, square and oboe: strong overtones.",
                        json!({ "timbres": BRIGHT_TIMBRES }),
                    )
                    .with(first.clone()),
                    m("Any timbre", "A mix of instruments.", json!({})).with(first.clone()),
                ],
            ),
            module(
                "Hold the note",
                vec![
                    held(
                        "2 seconds",
                        "Hold it in your head for 2 s, then sing.",
                        2000,
                        0,
                    ),
                    held("5 seconds", "Five seconds.", 5000, 0),
                    held("10 seconds", "Ten seconds.", 10_000, 0),
                    held("20 seconds", "Twenty seconds.", 20_000, 0),
                ],
            ),
            module(
                "Through other notes",
                vec![
                    held(
                        "Two notes",
                        "Two other notes in 4 s; then sing the first.",
                        4000,
                        2,
                    ),
                    held("Four notes", "Four other notes in 5 s.", 5000, 4),
                    held("Six notes", "Six other notes in 6 s.", 6000, 6),
                    held("Eight notes", "Eight other notes in 10 s.", 10_000, 8),
                ],
            ),
            module(
                "Judge it first",
                vec![
                    m(
                        "Flat, in tune or sharp",
                        "After singing, call it before the result.",
                        json!({ "timbres": ["gm:0"] }),
                    )
                    .with(json!({ "self_judge": true })),
                    m(
                        "On the attack",
                        "Call the start of the note, which is what is graded.",
                        json!({ "timbres": ["gm:0"] }),
                    )
                    .with(merge(first.clone(), json!({ "self_judge": true }))),
                    m(
                        "After 5 seconds",
                        "Held in silence first.",
                        json!({ "timbres": ["gm:0"], "delay_ms": 5000 }),
                    )
                    .with(json!({ "self_judge": true })),
                    m(
                        "Bright timbres",
                        "Saw, square and oboe.",
                        json!({ "timbres": BRIGHT_TIMBRES }),
                    )
                    .with(json!({ "self_judge": true })),
                ],
            ),
            module(
                "Long notes",
                vec![
                    hold(
                        "5 s over a drone",
                        "The note keeps sounding under you.",
                        5000,
                        true,
                    ),
                    hold(
                        "5 s alone",
                        "The note sounds first, then you hold it alone.",
                        5000,
                        false,
                    ),
                    hold("10 s over a drone", "Ten seconds.", 10_000, true),
                    hold("10 s alone", "Ten seconds alone.", 10_000, false),
                ],
            ),
        ],
    )
}

// ---------------------------------------------------------------------------------------------
// Cadences, bass lines and guide tones
// ---------------------------------------------------------------------------------------------

fn harmony_course() -> Course {
    let cad = |t: &str, s: &str, kinds: &[&str], modes: &[&str]| {
        lesson(t, s, json!({ "cadences": kinds, "modes": modes }))
    };
    let line = |t: &str, s: &str, p: Value| lesson(t, s, merge(one_key(), p)).of("bass_line", 6);
    let all5 = [
        "perfect_authentic",
        "imperfect_authentic",
        "plagal",
        "half",
        "deceptive",
    ];
    workshop(
        RP_HARMONY,
        "cadence_id",
        16,
        "Cadences and bass lines",
        "Harmony",
        "Harmony heard horizontally: how phrases end, in four-part writing; then singing the roots of progressions with the bass, without it and from the tonic alone; then the guide-tone line of 3rds and 7ths.",
        vec![
            module(
                "Cadences",
                vec![
                    cad(
                        "Authentic or half",
                        "Does it close on I or stop on V?",
                        &["perfect_authentic", "half"],
                        &["major"],
                    ),
                    cad(
                        "With plagal",
                        "IV–I as well.",
                        &["perfect_authentic", "half", "plagal"],
                        &["major"],
                    ),
                    cad(
                        "With deceptive",
                        "V–vi as well.",
                        &["perfect_authentic", "half", "plagal", "deceptive"],
                        &["major"],
                    ),
                    cad(
                        "Perfect or imperfect",
                        "All five, the top voice deciding perfect from imperfect.",
                        &all5,
                        &["major"],
                    ),
                    cad("In minor", "All five in minor keys.", &all5, &["minor"]),
                    cad(
                        "Major and minor",
                        "All five, either mode.",
                        &all5,
                        &["major", "minor"],
                    ),
                ],
            ),
            module(
                "Bass lines",
                vec![
                    line(
                        "Roots with the bass",
                        "I IV V I: sing the roots along with the bass.",
                        json!({ "progressions": ["i_iv_v_i"], "backing": "with_bass" }),
                    ),
                    line(
                        "Supply the bass",
                        "I IV V I without its bass: sing it.",
                        json!({ "progressions": ["i_iv_v_i"] }),
                    ),
                    line(
                        "More progressions",
                        "I vi IV V I, I ii V I, I IV I V I.",
                        json!({ "progressions": ["i_vi_iv_v_i", "i_ii_v_i", "i_iv_i_v_i"] }),
                    ),
                    line(
                        "From the tonic alone",
                        "Only the tonic sounds: audiate the chords, sing their roots.",
                        json!({ "progressions": ["i_iv_v_i", "i_vi_iv_v_i", "i_ii_v_i"], "backing": "tonic_only" }),
                    ),
                    line(
                        "In minor",
                        "Minor progressions without the bass.",
                        json!({ "progressions": ["i_iv_v_i", "i_vi_iv_v_i", "i_vi_ii_v_i"], "modes": ["minor"] }),
                    ),
                ],
            ),
            module(
                "Guide tones",
                vec![
                    line(
                        "ii–V–I with the chords",
                        "Sing 7th to 3rd to 7th through ii7 V7 Imaj7.",
                        json!({ "line": "guide_tones", "progressions": ["ii_v_i7"], "backing": "without_bass" }),
                    ),
                    line(
                        "The turnaround",
                        "I vi ii7 V7 I.",
                        json!({ "line": "guide_tones", "progressions": ["turnaround7"], "backing": "without_bass" }),
                    ),
                    line(
                        "From the tonic alone",
                        "No chords: sing the guide tones from the tonic.",
                        json!({ "line": "guide_tones", "progressions": ["ii_v_i7", "turnaround7"], "backing": "tonic_only" }),
                    ),
                    line(
                        "In minor",
                        "iiø7 V7 i.",
                        json!({ "line": "guide_tones", "progressions": ["ii_v_i7"], "modes": ["minor"], "backing": "without_bass" }),
                    ),
                ],
            ),
        ],
    )
}

// ---------------------------------------------------------------------------------------------
// Error detection and mislabeled syllables
// ---------------------------------------------------------------------------------------------

fn errors_course() -> Course {
    let err = |t: &str, s: &str, bars: u32, keys: Value, extra: Value| {
        // One bar is all quarter notes, so there is enough to read.
        let rhythms = if bars == 1 {
            json!(["quarter"])
        } else {
            json!(["half", "quarter"])
        };
        lesson(
            t,
            s,
            merge(
                json!({ "melody": { "generate": { "bars": bars, "keys": keys, "max_leap": 3, "rhythms": rhythms } } }),
                extra,
            ),
        )
    };
    let stroop =
        |t: &str, s: &str, p: Value| lesson(t, s, merge(one_key(), p)).of("syllable_check", 12);
    workshop(
        RP_ERRORS,
        "error_detection",
        6,
        "Error detection",
        "Errors",
        "Read a score in silence, hear it once, and find the note played wrong: hearing it again would overwrite what you read. Then melodies under their syllables with one syllable wrong.",
        vec![
            module(
                "Score against sound",
                vec![
                    err(
                        "One bar, 10 s",
                        "One bar in C, a step wrong.",
                        1,
                        json!(["C"]),
                        json!({ "pre_audiation_s": 10 }),
                    ),
                    err(
                        "Two bars, 15 s",
                        "Two bars in C.",
                        2,
                        json!(["C"]),
                        json!({ "pre_audiation_s": 15 }),
                    ),
                    err(
                        "Other keys",
                        "Two bars in keys with up to two sharps or flats.",
                        2,
                        json!([]),
                        json!({ "pre_audiation_s": 15 }),
                    ),
                    err(
                        "A semitone off",
                        "The wrong note leaves the key.",
                        2,
                        json!([]),
                        json!({ "pre_audiation_s": 15, "change": "chromatic" }),
                    ),
                    test(
                        "Test: four bars",
                        "Four bars, either kind of error, no results until the end.",
                        json!({ "melody": { "generate": { "bars": 4, "keys": [], "max_leap": 3 } }, "pre_audiation_s": 20, "change": "any" }),
                    ),
                ],
            ),
            module(
                "Mislabeled syllables",
                vec![
                    stroop(
                        "Five notes, do to sol",
                        "One syllable is a neighbour's.",
                        json!({ "notes": [5], "degrees": ["1", "2", "3", "4", "5"] }),
                    ),
                    stroop("The whole scale", "Six notes.", json!({ "notes": [6] })),
                    stroop(
                        "New keys",
                        "A new key every question.",
                        merge(new_keys(), json!({ "notes": [6] })),
                    ),
                    stroop(
                        "In minor",
                        "Minor keys.",
                        json!({ "notes": [6], "modes": ["minor"], "degrees": ["1", "2", "b3", "4", "5", "b6", "b7"] }),
                    ),
                ],
            ),
        ],
    )
}

// ---------------------------------------------------------------------------------------------
// The core: four tiers of short daily sessions
// ---------------------------------------------------------------------------------------------

/// One part of a core session: an exercise, its count and params; `checked` parts count towards
/// passing (practice parts, like the N-back streams, do not).
struct Part {
    kind: &'static str,
    title: &'static str,
    count: u32,
    params: Value,
    checked: bool,
}

fn part(kind: &'static str, title: &'static str, count: u32, params: Value) -> Part {
    Part {
        kind,
        title,
        count,
        params,
        checked: true,
    }
}

fn practice(kind: &'static str, title: &'static str, count: u32, params: Value) -> Part {
    Part {
        checked: false,
        ..part(kind, title, count, params)
    }
}

/// A tier: its name and its sessions (title, summary, parts).
type Tier = (&'static str, Vec<(&'static str, &'static str, Vec<Part>)>);

/// The share of a core session's checked parts each must reach.
const CORE_PASS: f32 = 0.8;

pub fn core_course() -> Course {
    let deg = |p: Value| merge(json!({ "octaves": [0] }), p);
    let tiers: Vec<Tier> = vec![
        (
            "Tonal context",
            vec![
                (
                    "Home and degrees in C",
                    "Find home in major and minor, scale degrees in C, then same or different.",
                    vec![
                        part(
                            "resting_tone",
                            "Find home",
                            6,
                            json!({ "modes": ["major", "natural_minor"] }),
                        ),
                        part(
                            "scale_degree_id",
                            "Scale degrees in C",
                            16,
                            deg(json!({ "keys": ["C"] })),
                        ),
                        part(
                            "pattern_memory",
                            "Same or different",
                            10,
                            json!({ "notes": [4] }),
                        ),
                    ],
                ),
                (
                    "Any key",
                    "Name home, scale degrees in one key for the session, then was the note in the melody.",
                    vec![
                        part(
                            "resting_tone",
                            "Name home",
                            8,
                            json!({ "modes": ["major", "natural_minor"], "answer": "name" }),
                        ),
                        part("scale_degree_id", "Scale degrees", 16, deg(one_key())),
                        part(
                            "pattern_memory",
                            "Was it in the melody?",
                            10,
                            json!({ "task": "probe", "notes": [5] }),
                        ),
                    ],
                ),
                (
                    "Changing keys",
                    "Find home in the modes, scale degrees in a new key every question, same or different across keys.",
                    vec![
                        part(
                            "resting_tone",
                            "Find home in the modes",
                            6,
                            json!({ "modes": ["major", "natural_minor", "dorian", "mixolydian"] }),
                        ),
                        part(
                            "scale_degree_id",
                            "Scale degrees, new keys",
                            16,
                            json!({ "keys": "all", "key_change": "per_trial", "cadence": "every", "octaves": [-1, 0, 1] }),
                        ),
                        part(
                            "pattern_memory",
                            "Same or different, another key",
                            10,
                            json!({ "notes": [5], "transpose": true, "change": "any" }),
                        ),
                    ],
                ),
            ],
        ),
        (
            "Patterns and working memory",
            vec![
                (
                    "Tonic and dominant",
                    "Tonal patterns on “bum”, a 1-back stream, and the 6ths sung via a 5th.",
                    vec![
                        part(
                            "tonal_pattern",
                            "Tonal patterns",
                            8,
                            merge(one_key(), json!({ "functions": ["tonic", "dominant"] })),
                        ),
                        practice("n_back", "1-back", 1, json!({ "n": 1 })),
                        part(
                            "sing_interval",
                            "6ths via a 5th",
                            6,
                            json!({ "in_key": true, "start": "tonic", "keys": "all", "intervals": ["M6"], "directions": ["up"], "scaffold": "additive" }),
                        ),
                    ],
                ),
                (
                    "Solfège and the first note",
                    "Patterns with the subdominant in solfège, the first note of a quick pattern, and a 2-back stream.",
                    vec![
                        part(
                            "tonal_pattern",
                            "Tonal patterns in solfège",
                            8,
                            merge(
                                one_key(),
                                json!({ "functions": ["tonic", "dominant", "subdominant"], "syllables": "translate" }),
                            ),
                        ),
                        part(
                            "pattern_dialogue",
                            "The first note",
                            6,
                            merge(one_key(), json!({ "stage": "first_pitch" })),
                        ),
                        practice("n_back", "2-back", 1, json!({ "n": 2 })),
                    ],
                ),
                (
                    "Dialogue and canon",
                    "Minor patterns, answering with the other function, a canon one note behind, and wide descending intervals via their inversions.",
                    vec![
                        part(
                            "tonal_pattern",
                            "Minor patterns",
                            8,
                            merge(
                                one_key(),
                                json!({ "functions": ["tonic", "dominant"], "modes": ["minor"], "syllables": "translate" }),
                            ),
                        ),
                        part(
                            "pattern_dialogue",
                            "Answer with the other function",
                            6,
                            merge(one_key(), json!({ "stage": "opposite_function" })),
                        ),
                        part(
                            "n_back",
                            "Canon, 1 behind",
                            2,
                            json!({ "n": 1, "mode": "sing", "length": 10 }),
                        ),
                        part(
                            "sing_interval",
                            "Down via the inversion",
                            6,
                            json!({ "intervals": ["m6", "M6", "m7"], "directions": ["down"], "scaffold": "complementary" }),
                        ),
                    ],
                ),
            ],
        ),
        (
            "Inner hearing and lines",
            vec![
                (
                    "Silent bars",
                    "Sight singing with bars 2 and 3 silent, changing a melody in your head, and the bass of I IV V I.",
                    vec![
                        part(
                            "sight_sing",
                            "Silent bars",
                            3,
                            merge(
                                melody(
                                    json!({ "bars": 4, "degrees": [1, 2, 3, 4, 5, 6, 7], "rhythms": ["half", "quarter"] }),
                                ),
                                json!({ "silent": "bars" }),
                            ),
                        ),
                        part(
                            "melodic_updating",
                            "Melodic updating",
                            6,
                            merge(one_key(), json!({ "notes": [4, 5] })),
                        ),
                        part(
                            "bass_line",
                            "The bass",
                            3,
                            merge(
                                one_key(),
                                json!({ "progressions": ["i_iv_v_i", "i_vi_iv_v_i"] }),
                            ),
                        ),
                    ],
                ),
                (
                    "Flash scores and guide tones",
                    "A score read in silence and sung from memory, two changes chained, the guide tones of ii–V–I.",
                    vec![
                        part(
                            "sight_sing",
                            "Flash score",
                            3,
                            merge(
                                melody(
                                    json!({ "bars": 2, "degrees": [1, 2, 3, 4, 5, 6, 7], "keys": [] }),
                                ),
                                json!({ "mode": "flash", "flash_s": 12 }),
                            ),
                        ),
                        part(
                            "melodic_updating",
                            "Two changes",
                            6,
                            merge(one_key(), json!({ "notes": [5], "changes": 2 })),
                        ),
                        part(
                            "bass_line",
                            "Guide tones",
                            3,
                            merge(
                                one_key(),
                                json!({ "line": "guide_tones", "progressions": ["ii_v_i7", "turnaround7"] }),
                            ),
                        ),
                    ],
                ),
                (
                    "From the tonic alone",
                    "Beat 1 sung silently, three changes, and the roots of progressions from the tonic alone.",
                    vec![
                        part(
                            "sight_sing",
                            "Beat 1 silent",
                            3,
                            merge(
                                melody(
                                    json!({ "bars": 4, "degrees": [1, 2, 3, 4, 5, 6, 7], "keys": [] }),
                                ),
                                json!({ "silent": "strong_beats" }),
                            ),
                        ),
                        part(
                            "melodic_updating",
                            "Three changes",
                            4,
                            merge(
                                one_key(),
                                json!({ "notes": [6], "changes": 3, "think_beats": 6 }),
                            ),
                        ),
                        part(
                            "bass_line",
                            "Roots from the tonic",
                            3,
                            merge(
                                one_key(),
                                json!({ "progressions": ["i_iv_v_i", "i_vi_iv_v_i", "i_ii_v_i"], "backing": "tonic_only" }),
                            ),
                        ),
                    ],
                ),
            ],
        ),
        (
            "Structure and error detection",
            vec![
                (
                    "Errors and cadences",
                    "Score against sound, heard once; one wrong syllable; how phrases end.",
                    vec![
                        part(
                            "error_detection",
                            "Error detection",
                            4,
                            json!({ "melody": { "generate": { "bars": 2, "keys": ["C"], "max_leap": 3 } }, "pre_audiation_s": 15 }),
                        ),
                        part(
                            "syllable_check",
                            "Mislabeled syllables",
                            8,
                            merge(one_key(), json!({ "notes": [6] })),
                        ),
                        part(
                            "cadence_id",
                            "Cadences",
                            10,
                            json!({ "cadences": ["perfect_authentic", "half", "deceptive"] }),
                        ),
                    ],
                ),
                (
                    "Singback and four bars",
                    "Longer error detection, melody singback, all five cadences.",
                    vec![
                        part(
                            "error_detection",
                            "Four bars",
                            4,
                            json!({ "melody": { "generate": { "bars": 4, "keys": [], "max_leap": 3 } }, "pre_audiation_s": 20 }),
                        ),
                        part(
                            "sight_sing",
                            "Singback",
                            4,
                            merge(
                                melody(
                                    json!({ "bars": 2, "degrees": [1, 2, 3, 4, 5, 6, 7], "keys": [], "rhythms": ["half", "quarter", "eighth"] }),
                                ),
                                json!({ "mode": "singback", "start": "key_only" }),
                            ),
                        ),
                        part(
                            "cadence_id",
                            "All five cadences",
                            10,
                            json!({ "cadences": ["perfect_authentic", "imperfect_authentic", "plagal", "half", "deceptive"] }),
                        ),
                    ],
                ),
                (
                    "Everything, in minor too",
                    "Semitone errors, syllables in minor, cadences in both modes, guide tones from the tonic.",
                    vec![
                        part(
                            "error_detection",
                            "A semitone off",
                            4,
                            json!({ "melody": { "generate": { "bars": 2, "keys": [], "modes": ["major", "minor"], "max_leap": 3 } }, "pre_audiation_s": 15, "change": "any" }),
                        ),
                        part(
                            "syllable_check",
                            "Syllables in minor",
                            8,
                            merge(
                                one_key(),
                                json!({ "notes": [6], "modes": ["minor"], "degrees": ["1", "2", "b3", "4", "5", "b6", "b7"] }),
                            ),
                        ),
                        part(
                            "cadence_id",
                            "Cadences, major and minor",
                            10,
                            json!({ "cadences": ["perfect_authentic", "imperfect_authentic", "plagal", "half", "deceptive"], "modes": ["major", "minor"] }),
                        ),
                        part(
                            "bass_line",
                            "Guide tones from the tonic",
                            3,
                            merge(
                                one_key(),
                                json!({ "line": "guide_tones", "progressions": ["ii_v_i7"], "backing": "tonic_only" }),
                            ),
                        ),
                    ],
                ),
            ],
        ),
    ];
    let mut levels = Vec::new();
    for (tier, sessions) in tiers {
        for (title, summary, parts) in sessions {
            let n = levels.len() as u32 + 1;
            let checked = parts.iter().filter(|p| p.checked).count() as u32;
            let blocks = parts
                .into_iter()
                .map(|p| {
                    block(
                        RP_CORE,
                        n,
                        p.checked,
                        json!({
                            "type": p.kind, "title": p.title, "count": p.count, "feedback": "after_trial",
                            "pass_mark": CORE_PASS, "tags": [RP_CORE], "params": p.params,
                        }),
                    )
                })
                .collect();
            levels.push(Level {
                module: Some(tier.into()),
                title: title.into(),
                summary: format!(
                    "{summary} Each part at {:.0}% or more in one session moves on.",
                    CORE_PASS * 100.0
                ),
                pass: PassRule::Accuracy {
                    min: CORE_PASS,
                    blocks: checked,
                },
                blocks,
            });
        }
    }
    Course {
        id: RP_CORE.into(),
        title: "Core relative pitch".into(),
        short: "Core".into(),
        category: CATEGORY.into(),
        description: "The daily core, built on research into how relative pitch is learnt: tonality first (find home, scale degrees), then a vocabulary of tonal patterns and working memory (echo, dialogue, N-back), then inner hearing and lines (silent bars, flash scores, melodic updating, bass and guide tones), then structure and single-pass error detection. Each session mixes three or four short parts.".into(),
        schedule: Schedule::Ladder,
        levels,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::courses::level_session;
    use crate::generate::{GenContext, generate_block};
    use crate::validate::validate_session;

    #[test]
    fn every_session_and_lesson_generates() {
        let ctx = GenContext::default();
        for c in std::iter::once(core_course()).chain(workshops()) {
            for (i, l) in c.levels.iter().enumerate() {
                let spec = level_session(&c, i as u32 + 1).unwrap();
                let r = validate_session(&spec, &ctx);
                assert!(r.ok, "{} {}: {:?}", c.id, l.title, r.errors);
                for b in &spec.blocks {
                    for seed in 0..6 {
                        if let Err(e) = generate_block(b, seed, &ctx) {
                            panic!(
                                "{} {} ({}) seed {seed}: {}",
                                c.id,
                                l.title,
                                l.module.as_deref().unwrap_or(""),
                                e.0
                            );
                        }
                    }
                }
            }
        }
    }

    #[test]
    fn the_core_has_four_tiers_of_mixed_sessions() {
        let c = core_course();
        let tiers: std::collections::BTreeSet<_> =
            c.levels.iter().map(|l| l.module.clone().unwrap()).collect();
        assert_eq!(tiers.len(), 4);
        for l in &c.levels {
            assert!(l.blocks.len() >= 3, "{}", l.title);
            let checked = l
                .blocks
                .iter()
                .filter(|b| b.course.as_ref().unwrap().verify)
                .count() as u32;
            assert_eq!(
                l.pass,
                PassRule::Accuracy {
                    min: CORE_PASS,
                    blocks: checked
                }
            );
        }
    }

    #[test]
    fn n_back_lessons_pass_on_d_prime() {
        let m = memory_course();
        let l = m
            .levels
            .iter()
            .find(|l| l.title == "2-back on 1–5")
            .unwrap();
        assert!(matches!(l.pass, PassRule::Detection { .. }));
        let canon = m
            .levels
            .iter()
            .find(|l| l.title == "2 behind on 1–5")
            .unwrap();
        assert!(matches!(canon.pass, PassRule::Accuracy { .. }));
    }
}
