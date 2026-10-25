//! Relative-pitch courses: ladders in the manner of EarMaster's workshops, built from the
//! existing exercises. Each starts from a contrasting pair, adds material a level at a time,
//! then moves from one key to many and from the tonic to anywhere. Every level is practice with
//! feedback, then a check without it; two checks in a row at 90% move the course on.

use serde_json::{Value, json};

use crate::courses::{Course, Level, PassRule, Schedule, block};

pub const RP_DEGREES: &str = "rp_degrees";
pub const RP_INTERVALS_KEY: &str = "rp_intervals_key";
pub const RP_INTERVALS_CHROMATIC: &str = "rp_intervals_chromatic";
pub const RP_INTERVALS: &str = "rp_intervals";
pub const RP_COMPARE: &str = "rp_compare";
pub const RP_SING: &str = "rp_sing";

pub const CATEGORY: &str = "Relative pitch";

pub fn courses() -> Vec<Course> {
    vec![
        degrees_course(),
        intervals_key_course(),
        chromatic_course(),
        intervals_course(),
        compare_course(),
        sing_course(),
    ]
}

/// One key for the session, chosen at random.
fn one_key() -> Value {
    json!({ "keys": "all", "key_change": "per_block" })
}

/// A new major key every question.
fn changing_keys() -> Value {
    json!({ "keys": "all", "key_change": "per_trial" })
}

fn merge(mut a: Value, b: Value) -> Value {
    if let (Some(a), Value::Object(b)) = (a.as_object_mut(), b) {
        a.extend(b);
    }
    a
}

/// A level: `practice` questions with feedback, then a `check` without.
#[allow(clippy::too_many_arguments)]
fn level(
    course: &str,
    n: u32,
    kind: &str,
    title: &str,
    summary: &str,
    params: Value,
    practice: u32,
    check: u32,
) -> Level {
    Level {
        title: title.into(),
        summary: summary.into(),
        pass: PassRule::Accuracy {
            min: 0.9,
            blocks: 2,
        },
        blocks: vec![
            block(
                course,
                n,
                false,
                json!({
                    "type": kind, "title": "Practice", "count": practice,
                    "feedback": "after_trial", "tags": [course], "params": params.clone(),
                }),
            ),
            block(
                course,
                n,
                true,
                json!({
                    "type": kind, "title": "Check: no feedback", "count": check,
                    "feedback": "none", "pass_mark": 0.9, "tags": [course, "rp.check"], "params": params,
                    "instructions": "No results until the end.",
                }),
            ),
        ],
    }
}

fn course(id: &str, title: &str, short: &str, description: &str, levels: Vec<Level>) -> Course {
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

fn degrees_course() -> Course {
    const CHROMATIC: [&str; 12] = [
        "1", "b2", "2", "b3", "3", "4", "#4", "5", "b6", "6", "b7", "7",
    ];
    let steps: Vec<(&str, &str, Value)> = vec![
        (
            "1 2 3",
            "Do, re and mi, and the upper do, in one key for the session.",
            merge(json!({ "degrees": ["1", "2", "3"] }), one_key()),
        ),
        (
            "1 to 5",
            "Degrees 1 to 5 and the upper do, in one key.",
            merge(json!({ "degrees": ["1", "2", "3", "4", "5"] }), one_key()),
        ),
        (
            "All seven",
            "The whole major scale, over two octaves, in one key.",
            merge(json!({ "octaves": [-1, 0] }), one_key()),
        ),
        (
            "All seven, new keys",
            "The whole major scale; the key changes every question.",
            merge(json!({ "octaves": [-1, 0] }), changing_keys()),
        ),
        (
            "Minor",
            "The natural minor scale, in one key.",
            merge(json!({ "modes": ["minor"], "octaves": [-1, 0] }), one_key()),
        ),
        (
            "Major and minor",
            "Major and minor keys; the key changes every question.",
            merge(
                json!({ "modes": ["major", "minor"], "octaves": [-1, 0] }),
                changing_keys(),
            ),
        ),
        (
            "Chromatic",
            "All twelve degrees in one major key.",
            merge(
                json!({ "degrees": CHROMATIC, "octaves": [-1, 0] }),
                one_key(),
            ),
        ),
        (
            "Chromatic, new keys",
            "All twelve degrees; the key changes every question.",
            merge(
                json!({ "degrees": CHROMATIC, "octaves": [-1, 0] }),
                changing_keys(),
            ),
        ),
    ];
    let levels = steps
        .into_iter()
        .enumerate()
        .map(|(i, (t, s, p))| level(RP_DEGREES, i as u32 + 1, "scale_degree_id", t, s, p, 24, 16))
        .collect();
    course(
        RP_DEGREES,
        "Scale degrees",
        "Degrees",
        "A cadence sets the key, a note plays, you name it; it then walks to the tonic. From three degrees in one key to all twelve in changing keys.",
        levels,
    )
}

/// The interval ladder from the tonic: pairs first, then the set, then down, then keys.
fn intervals_key_course() -> Course {
    let up = |ivs: &[&str]| json!({ "intervals": ivs, "directions": ["ascending"] });
    let from_do = json!({ "in_key": true, "start": "tonic", "start_octaves": [-1, 0] });
    let all_up = ["M2", "M3", "P4", "P5", "M6", "M7", "P8"];
    let all_down = ["m2", "m3", "P4", "P5", "m6", "m7", "P8"];
    let steps: Vec<(&str, &str, Value)> = vec![
        (
            "P5 and P8 up",
            "Up from do to sol or the upper do.",
            up(&["P5", "P8"]),
        ),
        (
            "M3 and P5 up",
            "Up from do to mi or sol.",
            up(&["M3", "P5"]),
        ),
        (
            "P4 and P5 up",
            "Up from do to fa or sol.",
            up(&["P4", "P5"]),
        ),
        ("M2 and M3 up", "Up from do to re or mi.", up(&["M2", "M3"])),
        ("M6 and M7 up", "Up from do to la or ti.", up(&["M6", "M7"])),
        ("All up", "Every degree up from do.", up(&all_up)),
        (
            "All down",
            "Every degree down from do: ti, la, sol … to the lower do.",
            json!({ "intervals": all_down, "directions": ["descending"] }),
        ),
    ];
    let mut levels: Vec<Level> = steps
        .into_iter()
        .enumerate()
        .map(|(i, (t, s, p))| {
            let params = merge(merge(from_do.clone(), p), one_key());
            level(
                RP_INTERVALS_KEY,
                i as u32 + 1,
                "interval_id",
                t,
                &format!("{s} One key for the session."),
                params,
                24,
                16,
            )
        })
        .collect();
    let later: Vec<(&str, &str, Value)> = vec![
        (
            "Up and down, new keys",
            "Every degree up or down from do; the key changes every question.",
            merge(
                json!({ "intervals": ["M2", "M3", "P4", "P5", "M6", "M7", "P8", "m2", "m3", "m6", "m7"], "directions": ["ascending", "descending"] }),
                changing_keys(),
            ),
        ),
        (
            "From any note",
            "Diatonic intervals between any two notes of the key, up or down.",
            merge(
                json!({ "start": "random", "intervals": ["m2", "M2", "m3", "M3", "P4", "TT", "P5", "m6", "M6", "m7", "M7", "P8"], "directions": ["ascending", "descending"] }),
                one_key(),
            ),
        ),
        (
            "From any note, new keys",
            "The same, with the key changing every question.",
            merge(
                json!({ "start": "random", "intervals": ["m2", "M2", "m3", "M3", "P4", "TT", "P5", "m6", "M6", "m7", "M7", "P8"], "directions": ["ascending", "descending"] }),
                changing_keys(),
            ),
        ),
        (
            "Together",
            "Both notes at once, from any note of the key.",
            merge(
                json!({ "start": "random", "intervals": ["m2", "M2", "m3", "M3", "P4", "TT", "P5", "m6", "M6", "m7", "M7", "P8"], "directions": ["harmonic"] }),
                one_key(),
            ),
        ),
    ];
    for (t, s, p) in later {
        let n = levels.len() as u32 + 1;
        let params = merge(from_do.clone(), p);
        levels.push(level(
            RP_INTERVALS_KEY,
            n,
            "interval_id",
            t,
            s,
            params,
            24,
            16,
        ));
    }
    course(
        RP_INTERVALS_KEY,
        "Intervals in a key",
        "In a key",
        "Intervals heard against a key: first up from do, two at a time, then all of them, then down, then in changing keys and from any note of the scale. Notes are named in fixed do after each answer.",
        levels,
    )
}

/// Intervals that leave the scale, from the tonic: the minor and altered degrees of a major key.
fn chromatic_course() -> Course {
    let base =
        json!({ "in_key": true, "start": "tonic", "chromatic": true, "start_octaves": [-1, 0] });
    let steps: Vec<(&str, &str, Value)> = vec![
        (
            "m3 and M3",
            "Up from do to me or mi.",
            json!({ "intervals": ["m3", "M3"], "directions": ["ascending"] }),
        ),
        (
            "m6 and M6",
            "Up from do to le or la.",
            json!({ "intervals": ["m6", "M6"], "directions": ["ascending"] }),
        ),
        (
            "m7 and M7",
            "Up from do to te or ti.",
            json!({ "intervals": ["m7", "M7"], "directions": ["ascending"] }),
        ),
        (
            "m2 and M2",
            "Up from do to ra or re.",
            json!({ "intervals": ["m2", "M2"], "directions": ["ascending"] }),
        ),
        (
            "TT, P4 and P5",
            "Up from do to fa, fi or sol.",
            json!({ "intervals": ["P4", "TT", "P5"], "directions": ["ascending"] }),
        ),
        (
            "All twelve up",
            "Every interval up from do, in and out of the key.",
            json!({ "intervals": ["m2", "M2", "m3", "M3", "P4", "TT", "P5", "m6", "M6", "m7", "M7", "P8"], "directions": ["ascending"] }),
        ),
        (
            "All twelve down",
            "Every interval down from do.",
            json!({ "intervals": ["m2", "M2", "m3", "M3", "P4", "TT", "P5", "m6", "M6", "m7", "M7", "P8"], "directions": ["descending"] }),
        ),
    ];
    let mut levels: Vec<Level> = steps
        .into_iter()
        .enumerate()
        .map(|(i, (t, s, p))| {
            let params = merge(merge(base.clone(), p), one_key());
            level(
                RP_INTERVALS_CHROMATIC,
                i as u32 + 1,
                "interval_id",
                t,
                &format!("{s} One key for the session."),
                params,
                24,
                16,
            )
        })
        .collect();
    let n = levels.len() as u32 + 1;
    levels.push(level(
        RP_INTERVALS_CHROMATIC,
        n,
        "interval_id",
        "All twelve, new keys",
        "Every interval up or down from do; the key changes every question.",
        merge(merge(base, json!({ "intervals": ["m2", "M2", "m3", "M3", "P4", "TT", "P5", "m6", "M6", "m7", "M7", "P8"], "directions": ["ascending", "descending"] })), changing_keys()),
        24,
        16,
    ));
    course(
        RP_INTERVALS_CHROMATIC,
        "Chromatic intervals in a key",
        "Chromatic",
        "Intervals from the tonic that leave the major scale, each set against the diatonic one beside it (minor against major third), then all twelve, then changing keys.",
        levels,
    )
}

/// Intervals with no key: random roots over three octaves, pairs, then sets, then directions,
/// then together, then beyond the octave.
fn intervals_course() -> Course {
    let range = json!({ "root_range": { "low": "C3", "high": "C5" } });
    let up = |ivs: &[&str]| json!({ "intervals": ivs, "directions": ["ascending"] });
    let all = [
        "m2", "M2", "m3", "M3", "P4", "TT", "P5", "m6", "M6", "m7", "M7", "P8",
    ];
    let steps: Vec<(&str, &str, Value)> = vec![
        (
            "P4 and P5 up",
            "Perfect fourth or fifth, upward, from any note.",
            up(&["P4", "P5"]),
        ),
        (
            "m3 and M3 up",
            "Minor or major third, upward.",
            up(&["m3", "M3"]),
        ),
        (
            "m2 and M2 up",
            "Minor or major second, upward.",
            up(&["m2", "M2"]),
        ),
        (
            "Thirds, fourths, fifths",
            "m3, M3, P4 and P5, upward.",
            up(&["m3", "M3", "P4", "P5"]),
        ),
        (
            "m6 and M6 up",
            "Minor or major sixth, upward.",
            up(&["m6", "M6"]),
        ),
        (
            "m7 and M7 up",
            "Minor or major seventh, upward.",
            up(&["m7", "M7"]),
        ),
        (
            "Tritone and octave",
            "TT, P4, P5 and P8, upward.",
            up(&["P4", "TT", "P5", "P8"]),
        ),
        ("All up", "Every interval to the octave, upward.", up(&all)),
        (
            "All down",
            "Every interval to the octave, downward.",
            json!({ "intervals": all, "directions": ["descending"] }),
        ),
        (
            "All, either way",
            "Every interval to the octave, up or down.",
            json!({ "intervals": all, "directions": ["ascending", "descending"] }),
        ),
        (
            "All together",
            "Both notes at once.",
            json!({ "intervals": all, "directions": ["harmonic"] }),
        ),
        (
            "Beyond the octave",
            "Ninths, tenths, the eleventh and the twelfth, up or down.",
            json!({ "intervals": ["m9", "M9", "m10", "M10", "P11", "P12"], "directions": ["ascending", "descending"], "root_range": { "low": "C3", "high": "C4" } }),
        ),
    ];
    let levels = steps
        .into_iter()
        .enumerate()
        .map(|(i, (t, s, p))| {
            level(
                RP_INTERVALS,
                i as u32 + 1,
                "interval_id",
                t,
                s,
                merge(range.clone(), p),
                24,
                16,
            )
        })
        .collect();
    course(
        RP_INTERVALS,
        "Intervals without a key",
        "Intervals",
        "Intervals on random notes over two octaves, no key: two at a time, then sets, then down, either way, together and beyond the octave.",
        levels,
    )
}

/// Comparing two intervals: big differences with a shared first note, down to small ones with
/// nothing shared.
fn compare_course() -> Course {
    let steps: Vec<(&str, &str, Value)> = vec![
        (
            "Far apart, same start",
            "The two differ by 3 or 4 semitones and start on the same note.",
            json!({ "differences": [3, 4], "shared": ["first"] }),
        ),
        (
            "Closer, same start",
            "They differ by 2 or 3 semitones, same start.",
            json!({ "differences": [2, 3], "shared": ["first"] }),
        ),
        (
            "A semitone, same start",
            "1 or 2 semitones apart, or the same, from the same note.",
            json!({ "differences": [0, 1, 2], "shared": ["first"] }),
        ),
        (
            "Same end",
            "1 or 2 semitones apart or the same; they end on the same note.",
            json!({ "differences": [0, 1, 2], "shared": ["second"] }),
        ),
        (
            "Nearby starts",
            "They start a semitone to a minor third apart.",
            json!({ "differences": [0, 1, 2], "shared": ["near"] }),
        ),
        (
            "Unrelated",
            "Nothing shared: two intervals anywhere.",
            json!({ "differences": [0, 1, 2], "shared": ["none"] }),
        ),
        (
            "Either way",
            "Unrelated, up or down.",
            json!({ "differences": [0, 1, 2], "shared": ["none", "near"], "directions": ["ascending", "descending"] }),
        ),
    ];
    let levels = steps
        .into_iter()
        .enumerate()
        .map(|(i, (t, s, p))| {
            let params = merge(json!({ "directions": ["ascending"] }), p);
            level(
                RP_COMPARE,
                i as u32 + 1,
                "interval_comparison",
                t,
                s,
                params,
                20,
                12,
            )
        })
        .collect();
    course(
        RP_COMPARE,
        "Interval comparison",
        "Compare",
        "Two intervals: which is larger, or are they the same? From big differences sharing a note to a semitone with nothing shared.",
        levels,
    )
}

/// Singing intervals: up from do in a key, then down, then from any scale note, then free.
fn sing_course() -> Course {
    let key = json!({ "in_key": true, "start": "tonic", "keys": "all", "key_change": "per_block" });
    let steps: Vec<(&str, &str, Value)> = vec![
        (
            "Do to sol and do",
            "From do, sing sol or the upper do.",
            merge(
                key.clone(),
                json!({ "intervals": ["P5", "P8"], "directions": ["up"] }),
            ),
        ),
        (
            "Do to mi and sol",
            "From do, sing mi or sol.",
            merge(
                key.clone(),
                json!({ "intervals": ["M3", "P5"], "directions": ["up"] }),
            ),
        ),
        (
            "Do up the scale",
            "From do, any degree up to the octave.",
            merge(
                key.clone(),
                json!({ "intervals": ["M2", "M3", "P4", "P5", "M6", "M7", "P8"], "directions": ["up"] }),
            ),
        ),
        (
            "Do down the scale",
            "From do, any degree down to the octave.",
            merge(
                key.clone(),
                json!({ "intervals": ["m2", "m3", "P4", "P5", "m6", "m7", "P8"], "directions": ["down"] }),
            ),
        ),
        (
            "From any note",
            "From any note of the key, up or down, staying in the key.",
            merge(
                key.clone(),
                json!({ "start": "random", "intervals": ["m2", "M2", "m3", "M3", "P4", "P5", "m6", "M6"], "directions": ["up", "down"] }),
            ),
        ),
        (
            "Without a key",
            "From a given note, any interval to the octave, up or down.",
            json!({ "intervals": ["m2", "M2", "m3", "M3", "P4", "TT", "P5", "m6", "M6", "m7", "M7", "P8"], "directions": ["up", "down"] }),
        ),
    ];
    let levels = steps
        .into_iter()
        .enumerate()
        .map(|(i, (t, s, p))| level(RP_SING, i as u32 + 1, "sing_interval", t, s, p, 8, 6))
        .collect();
    course(
        RP_SING,
        "Singing intervals",
        "Sing",
        "Hear the key and the first note, then sing the second: from do up and down, then from any note of the key, then without a key. Sung in your own range.",
        levels,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::courses::{compose_daily_set, level_session};
    use crate::generate::{GenContext, generate_block};
    use crate::schema::{FeedbackMode, Mask};
    use crate::storage::Db;
    use crate::validate::validate_session;

    #[test]
    fn every_level_generates_in_any_key() {
        let ctx = GenContext::default();
        for c in courses() {
            for n in 1..=c.levels.len() as u32 {
                for b in level_session(&c, n).unwrap().blocks {
                    for seed in 0..25 {
                        if let Err(e) = generate_block(&b, seed, &ctx) {
                            panic!("{} level {n} seed {seed}: {}", c.id, e.0);
                        }
                    }
                }
            }
        }
    }

    #[test]
    fn levels_practise_then_check_without_feedback() {
        for c in courses() {
            assert_eq!(c.category, CATEGORY);
            for l in &c.levels {
                assert_eq!(l.blocks.len(), 2, "{}", c.id);
                assert_eq!(l.blocks[1].feedback, FeedbackMode::None);
                assert!(l.blocks[1].course.as_ref().unwrap().verify);
            }
        }
    }

    #[test]
    fn the_relative_pitch_set_has_its_own_courses() {
        let db = Db::open_in_memory().unwrap();
        let ctx = GenContext::default();
        let set = compose_daily_set(&db, &ctx, chrono::Utc::now(), CATEGORY).unwrap();
        let ids: Vec<&str> = set.items.iter().map(|i| i.course.as_str()).collect();
        assert!(
            ids.contains(&RP_DEGREES) && ids.contains(&RP_INTERVALS_KEY),
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
