//! Courses, the daily set built from them, and the absolute-pitch dashboard.
//!
//! A course is a ladder of levels or a recurring test. A level is a set of blocks tagged with
//! the course and level, plus a pass rule over its check blocks. The app keeps each course's
//! current level, moves it on when the rule is met, says what is due, and composes a daily set
//! from the active courses. Rules are shown, not enforced: any level can be started at any time,
//! and the coach can set a course's level.

use std::collections::BTreeMap;

use anyhow::{Result, anyhow};
use chrono::{DateTime, Datelike, Duration, Local, Utc};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use ts_rs::TS;

use crate::generate::{GenContext, block_seed, generate_block};
use crate::presets::AP_PROGRESSIVE_ORDER;
use crate::schema::{BlockSpec, CourseRef, Mask, SessionSpec};
use crate::storage::{Db, SessionStatus, TrialFilter, TrialRow};
use crate::theory::PitchClass;
use crate::validate::estimate_trial_seconds;

/// When a level counts as passed.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, TS)]
#[serde(tag = "rule", rename_all = "snake_case")]
pub enum PassRule {
    /// Each of the last `blocks` check blocks at or above `min`.
    Accuracy { min: f32, blocks: u32 },
    /// `passes` check blocks at or above `min`, the last at least `gap_hours` after the one
    /// before it, so a night's sleep separates them (Wong et al.).
    Verification {
        min: f32,
        passes: u32,
        gap_hours: f32,
    },
    /// A test with no pass mark.
    None,
    /// d′ at least `min_d_prime` on each of the last `streams` detection streams.
    Detection { min_d_prime: f32, streams: u32 },
    /// Every active item correct, and at least `days` since this level's item was added (Eguchi).
    Spacing { min: f32, days: f32 },
}

/// How a course comes up in the daily set.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, TS)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Schedule {
    /// Work up through the levels, a little every day.
    Ladder,
    /// A test repeated every `days` days on the plan's test day (`days` ≤ 7 means weekly). With
    /// `after`, only once that course is complete.
    Recurring {
        days: f32,
        on_test_day: bool,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        after: Option<String>,
    },
}

#[derive(Clone, Debug, Serialize, Deserialize, TS)]
pub struct Level {
    pub title: String,
    pub summary: String,
    pub pass: PassRule,
    #[ts(type = "Array<Record<string, unknown>>")]
    pub blocks: Vec<BlockSpec>,
}

#[derive(Clone, Debug, Serialize, Deserialize, TS)]
pub struct Course {
    pub id: String,
    pub title: String,
    /// Short name used in the daily set, e.g. `Naming`.
    pub short: String,
    pub category: String,
    pub description: String,
    pub schedule: Schedule,
    pub levels: Vec<Level>,
}

// ---------------------------------------------------------------------------------------------
// Built-in absolute-pitch courses
// ---------------------------------------------------------------------------------------------

pub const AP_NAMING: &str = "ap_naming";
pub const AP_BRADY: &str = "ap_brady";
pub const AP_WEEKLY: &str = "ap_weekly";
pub const AP_UCSF_STYLE: &str = "ap_ucsf_style";
pub const AP_MAINTENANCE: &str = "ap_maintenance";
pub const AP_SPEED: &str = "ap_speed";
pub const AP_CHORD_COLORS: &str = "ap_chord_colors";
pub const AP_NOTE_COLORS: &str = "ap_note_colors";
pub const AP_CONTRAST: &str = "ap_contrast";

/// On a course block whose errorless feedback is the default, so a setting can turn it off.
pub const ERRORLESS_DEFAULT_TAG: &str = "ap.errorless_default";

/// The tag on anchor-recall trials ("sing your C from memory").
pub const ANCHOR_CHECK_TAG: &str = "ap.anchor_check";

/// A block from JSON, tagged with its course level.
pub(crate) fn block(course: &str, level: u32, verify: bool, mut v: Value) -> BlockSpec {
    if let Some(o) = v.as_object_mut() {
        o.entry("params").or_insert_with(|| json!({}));
    }
    let mut b: BlockSpec = serde_json::from_value(v).expect("built-in course block is valid");
    b.course = Some(CourseRef {
        course: course.into(),
        level,
        verify,
    });
    b
}

const NAMING_LEVELS: u32 = 10;
/// Training octaves C3–B5: three octaves, so tones can always jump 13 semitones.
pub(crate) const SPREAD: [i32; 3] = [3, 4, 5];

fn naming_course() -> Course {
    let levels = (1..=NAMING_LEVELS)
        .map(|n| {
            let classes: Vec<&str> = AP_PROGRESSIVE_ORDER[..(n as usize + 2)].to_vec();
            let all = classes.len() == 12;
            let class_json = if all { json!("all") } else { json!(classes) };
            // 4 s at the first level down to 2 s at the last.
            let deadline = 4000 - (n - 1) * 2000 / (NAMING_LEVELS - 1);
            let other = if all { 0.0 } else { 0.15 };
            let names = classes
                .iter()
                .map(|c| c.parse::<PitchClass>().unwrap().display_name())
                .collect::<Vec<_>>()
                .join(" ");
            let params = json!({
                "classes": class_json, "octaves": SPREAD, "min_jump_semitones": 13,
                "out_of_set_rate": other, "deadline_ms": deadline,
            });
            let mut train = params.clone();
            train["mask"] = json!("random_tones");
            let mut check = params;
            check["mask"] = json!("none");
            Level {
                title: if all { "All twelve notes".into() } else { format!("{} notes", classes.len()) },
                summary: format!(
                    "{names}{}, over three octaves, answered within {:.1} s.",
                    if all { "" } else { ", plus Other for notes outside the set" },
                    deadline as f32 / 1000.0
                ),
                pass: if all {
                    PassRule::Verification { min: 0.9, passes: 4, gap_hours: 12.0 }
                } else {
                    PassRule::Accuracy { min: 0.9, blocks: 2 }
                },
                blocks: vec![
                    block(AP_NAMING, n, false, json!({
                        "type": "pitch_id", "title": "Name the note", "count": 40, "pass_mark": 0.9,
                        "tags": ["ap.naming"], "params": train,
                    })),
                    block(AP_NAMING, n, true, json!({
                        "type": "pitch_id", "title": "Check: no feedback", "count": if all { 36 } else { 24 },
                        "feedback": "none", "pass_mark": 0.9, "tags": ["ap.naming", "ap.check"], "params": check,
                        "instructions": "No results until the end. Answer from the sound alone.",
                    })),
                ],
            }
        })
        .collect();
    Course {
        id: AP_NAMING.into(),
        title: "Note naming".into(),
        short: "Naming".into(),
        category: "Absolute pitch".into(),
        description: "Name a growing set of notes spread over three octaves, never two tones close together, with Other for notes outside the set. Each level ends with a check without feedback; the last level needs four passes, the last after a night's sleep.".into(),
        schedule: Schedule::Ladder,
        levels,
    }
}

/// Brady's fixed-scale steps: the weight of C and the time between tones.
const BRADY_STEPS: [(f32, u32); 5] = [
    (0.40, 5000),
    (0.30, 4000),
    (0.20, 3000),
    (0.12, 2000),
    (1.0 / 12.0, 1400),
];

fn brady_course() -> Course {
    let levels = BRADY_STEPS
        .iter()
        .enumerate()
        .map(|(i, &(c, ioi))| {
            let n = i as u32 + 1;
            let uniform = (c - 1.0 / 12.0).abs() < 1e-3;
            let mut params = json!({
                "classes": "all", "octaves": SPREAD, "duration_ms": 1000,
                "pacing": { "stream": { "ioi_ms": ioi } },
            });
            if !uniform {
                params["class_weights"] = json!({ "C": c });
            }
            Level {
                title: if uniform { "Even, 1.4 s apart".into() } else { format!("C {:.0}%, {:.1} s apart", c * 100.0, ioi as f32 / 1000.0) },
                summary: format!(
                    "All twelve notes over three octaves, a tone every {:.1} s{}. Keep C in your mind's ear and judge each note against it.",
                    ioi as f32 / 1000.0,
                    if uniform { ", all notes equally often".to_string() } else { format!(", C on {:.0}% of tones", c * 100.0) }
                ),
                pass: PassRule::Accuracy { min: 0.8, blocks: 2 },
                blocks: vec![
                    block(AP_BRADY, n, false, json!({
                        "type": "sing_pitch", "title": "Sing your C from memory", "count": 1,
                        "tags": [ANCHOR_CHECK_TAG],
                        "instructions": "Before hearing anything, sing C as you remember it. The real C plays afterwards.",
                        "params": { "classes": ["C"], "pre_mask": "none" },
                    })),
                    block(AP_BRADY, n, true, json!({
                        "type": "pitch_id", "title": "Paced stream", "count": 80, "pass_mark": 0.8,
                        "tags": ["ap.brady"], "params": params,
                        "instructions": "Tones come at a steady pace whether or not you answer. Hold C in mind and name each tone against it.",
                    })),
                ],
            }
        })
        .collect();
    Course {
        id: AP_BRADY.into(),
        title: "Fixed C anchor (Brady)".into(),
        short: "Anchor C".into(),
        category: "Absolute pitch".into(),
        description: "Brady's fixed-scale method: all twelve notes from the start, C heard far more often at first and the pace slow; C fades to an even share and the pace rises to a tone every 1.4 s. Starts by singing your C from memory.".into(),
        schedule: Schedule::Ladder,
        levels,
    }
}

const WEEKLY_TIMBRES: [&str; 5] = ["gm:0", "gm:42", "gm:71", "gm:6", "synth:square"];

fn weekly_course() -> Course {
    Course {
        id: AP_WEEKLY.into(),
        title: "Weekly test".into(),
        short: "Weekly test".into(),
        category: "Absolute pitch".into(),
        description: "120 notes, 24 each on piano, cello, clarinet, harpsichord and square wave, with no feedback, after the pitch-memory clearing sound.".into(),
        schedule: Schedule::Recurring { days: 7.0, on_test_day: true, after: None },
        levels: vec![Level {
            title: "Five timbres".into(),
            summary: "120 notes, five timbres, three octaves, no feedback.".into(),
            pass: PassRule::None,
            blocks: vec![block(AP_WEEKLY, 1, true, json!({
                "type": "pitch_id", "title": "Weekly test", "count": 120, "assessment": true,
                "pre_mask": "shepard_glissando", "tags": ["ap.weekly"],
                "params": { "classes": "all", "pool": "any", "timbres": WEEKLY_TIMBRES, "octaves": SPREAD,
                            "min_jump_semitones": 13, "mask": "none" },
            }))],
        }],
    }
}

fn ucsf_course() -> Course {
    let part = |title: &str, timbre: &str, pre: Option<&str>| {
        let mut v = json!({
            "type": "pitch_id", "title": title, "count": 36, "assessment": true, "tags": ["ap.ucsf_style"],
            "params": { "classes": "all", "pool": "any", "timbres": [timbre], "octaves": [2, 3, 4, 5, 6],
                        "min_jump_semitones": 13, "mask": "none" },
        });
        if let Some(m) = pre {
            v["pre_mask"] = json!(m);
        }
        block(AP_UCSF_STYLE, 1, true, v)
    };
    Course {
        id: AP_UCSF_STYLE.into(),
        title: "UCSF-style test".into(),
        short: "UCSF-style test".into(),
        category: "Absolute pitch".into(),
        description: "36 piano and 36 sine tones over five octaves, no feedback, scored as the UCSF test is: a point for an exact answer, three quarters for a semitone off. Not the UCSF test itself.".into(),
        schedule: Schedule::Recurring { days: 28.0, on_test_day: true, after: None },
        levels: vec![Level {
            title: "Piano and sine".into(),
            summary: "36 piano and 36 sine tones, five octaves, no feedback.".into(),
            pass: PassRule::None,
            blocks: vec![
                part("Piano", "gm:0", Some("shepard_glissando")),
                part("Sine", "synth:sine", None),
            ],
        }],
    }
}

fn maintenance_course() -> Course {
    Course {
        id: AP_MAINTENANCE.into(),
        title: "Maintenance".into(),
        short: "Maintenance".into(),
        category: "Absolute pitch".into(),
        description: "Five minutes of all twelve notes over three octaves and several timbres, twice a week, once note naming is complete.".into(),
        schedule: Schedule::Recurring { days: 3.5, on_test_day: false, after: Some(AP_NAMING.into()) },
        levels: vec![Level {
            title: "All twelve notes".into(),
            summary: "All twelve notes, three octaves, 2 s to answer.".into(),
            pass: PassRule::None,
            blocks: vec![block(AP_MAINTENANCE, 1, true, json!({
                "type": "pitch_id", "title": "Maintenance", "count": 40, "tags": ["ap.maintenance"],
                "params": { "classes": "all", "octaves": SPREAD, "min_jump_semitones": 13, "deadline_ms": 2000,
                            "mask": "random_tones" },
            }))],
        }],
    }
}

pub fn builtin_courses() -> Vec<Course> {
    let mut courses = vec![naming_course(), brady_course()];
    courses.extend(crate::ap_courses::courses());
    courses.extend([weekly_course(), ucsf_course(), maintenance_course()]);
    courses
}

pub fn find_course(id: &str) -> Option<Course> {
    builtin_courses().into_iter().find(|c| c.id == id)
}

/// The session for one level of a course, as started from the course page.
pub fn level_session(course: &Course, level: u32) -> Result<SessionSpec> {
    let l = course
        .levels
        .get(
            level
                .checked_sub(1)
                .ok_or_else(|| anyhow!("levels start at 1"))? as usize,
        )
        .ok_or_else(|| anyhow!("{} has {} levels", course.id, course.levels.len()))?;
    let title = if course.levels.len() > 1 {
        format!("{}: level {level} ({})", course.title, l.title)
    } else {
        course.title.clone()
    };
    Ok(SessionSpec {
        title,
        protocol: Some(course.id.clone()),
        planned_minutes: None,
        notes: Some(l.summary.clone()),
        seed: None,
        blocks: l.blocks.clone(),
    })
}

// ---------------------------------------------------------------------------------------------
// Progress
// ---------------------------------------------------------------------------------------------

/// One run of a block of a course level.
#[derive(Clone, Debug, Serialize, Deserialize, TS)]
pub struct BlockRun {
    pub session_id: String,
    pub block_idx: u32,
    pub at: String,
    pub trials: u32,
    pub correct: u32,
    pub accuracy: f64,
    /// A check block (the ones pass rules count).
    pub check: bool,
    /// Set on a detection stream. Each stream is its own run.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub d_prime: Option<f64>,
}

fn parse_time(s: &str) -> Option<DateTime<Utc>> {
    DateTime::parse_from_rfc3339(s)
        .ok()
        .map(|d| d.with_timezone(&Utc))
}

/// Unassisted trials tagged with a course (or a course level), newest first.
fn tagged_trials(db: &Db, tag: &str, limit: u32) -> Result<Vec<TrialRow>> {
    db.query_trials(
        &TrialFilter {
            tag: Some(tag.into()),
            assisted: Some(false),
            ..Default::default()
        },
        limit,
        0,
    )
}

/// Every run of a course level's blocks, oldest first.
pub fn level_runs(db: &Db, course: &str, level: u32) -> Result<Vec<BlockRun>> {
    let tag = CourseRef {
        course: course.into(),
        level,
        verify: false,
    }
    .tag();
    let trials = tagged_trials(db, &tag, 20_000)?;
    let mut runs: BTreeMap<(String, u32, u32), BlockRun> = BTreeMap::new();
    for t in trials.iter().rev() {
        if let Some(d) = t.metrics.get("d_prime").and_then(|v| v.as_f64()) {
            runs.insert(
                (t.session_id.clone(), t.block_idx, t.trial_idx),
                BlockRun {
                    session_id: t.session_id.clone(),
                    block_idx: t.block_idx,
                    at: t.started_at.clone(),
                    trials: 1,
                    correct: 0,
                    accuracy: 0.0,
                    check: t.tags.iter().any(|g| g == CHECK_TAG),
                    d_prime: Some(d),
                },
            );
            continue;
        }
        let r = runs
            .entry((t.session_id.clone(), t.block_idx, u32::MAX))
            .or_insert_with(|| BlockRun {
                session_id: t.session_id.clone(),
                block_idx: t.block_idx,
                at: t.started_at.clone(),
                trials: 0,
                correct: 0,
                accuracy: 0.0,
                check: t.tags.iter().any(|g| g == CHECK_TAG),
                d_prime: None,
            });
        if let Some(c) = t.correct {
            r.trials += 1;
            r.correct += c as u32;
        }
        if t.started_at < r.at {
            r.at = t.started_at.clone();
        }
    }
    let mut out: Vec<BlockRun> = runs
        .into_values()
        .filter(|r| r.trials > 0)
        .map(|mut r| {
            r.accuracy = r.correct as f64 / r.trials as f64;
            r
        })
        .collect();
    out.sort_by(|a, b| a.at.cmp(&b.at));
    Ok(out)
}

/// The tag every check block's trials carry.
pub const CHECK_TAG: &str = "course.check";

#[derive(Clone, Debug, Serialize, Deserialize, TS)]
pub struct PassStatus {
    pub met: bool,
    /// E.g. "2 of 4 checks passed; the 4th must be at least 12 h after the 3rd".
    pub summary: String,
    pub passes: u32,
    pub needed: u32,
    /// The earliest time the next check can count (the overnight gap), if one applies.
    pub next_after: Option<String>,
}

/// When a pass rule is judged.
pub struct RuleContext {
    pub now: DateTime<Utc>,
    /// When the current level started (when its chord was added, for spacing).
    pub level_started: Option<DateTime<Utc>>,
}

/// How far a level's check blocks go towards its pass rule.
pub fn evaluate(rule: &PassRule, runs: &[BlockRun]) -> PassStatus {
    evaluate_in(
        rule,
        runs,
        &RuleContext {
            now: Utc::now(),
            level_started: None,
        },
    )
}

pub fn evaluate_in(rule: &PassRule, runs: &[BlockRun], ctx: &RuleContext) -> PassStatus {
    let checks: Vec<&BlockRun> = runs.iter().filter(|r| r.check).collect();
    let pct = |m: f32| format!("{:.0}%", m * 100.0);
    match rule {
        PassRule::None => PassStatus {
            met: false,
            summary: String::new(),
            passes: 0,
            needed: 0,
            next_after: None,
        },
        PassRule::Accuracy { min, blocks } => {
            let needed = (*blocks).max(1);
            // Consecutive passing checks, counting back from the latest.
            let streak = checks
                .iter()
                .rev()
                .take_while(|r| r.accuracy + 1e-9 >= *min as f64)
                .count() as u32;
            let passes = streak.min(needed);
            let met = streak >= needed;
            let summary = if met {
                format!("Passed: {needed} checks in a row at {} or more.", pct(*min))
            } else if checks.is_empty() {
                format!("Needs {needed} checks in a row at {} or more.", pct(*min))
            } else {
                format!(
                    "{passes} of {needed} checks in a row at {} or more (last: {}).",
                    pct(*min),
                    pct(checks.last().unwrap().accuracy as f32)
                )
            };
            PassStatus {
                met,
                summary,
                passes,
                needed,
                next_after: None,
            }
        }
        PassRule::Verification {
            min,
            passes: needed,
            gap_hours,
        } => {
            let needed = (*needed).max(1);
            let gap = Duration::milliseconds((*gap_hours as f64 * 3_600_000.0) as i64);
            let mut times: Vec<DateTime<Utc>> = Vec::new();
            for r in checks.iter().filter(|r| r.accuracy + 1e-9 >= *min as f64) {
                let Some(at) = parse_time(&r.at) else {
                    continue;
                };
                if times.len() + 1 == needed as usize {
                    // The last pass only counts after the gap.
                    if at - *times.last().unwrap_or(&at) < gap && !times.is_empty() {
                        continue;
                    }
                }
                if times.len() < needed as usize {
                    times.push(at);
                }
            }
            let passes = times.len() as u32;
            let met = passes >= needed;
            let next_after = (!met && passes + 1 == needed && passes > 0)
                .then(|| (*times.last().unwrap() + gap).to_rfc3339());
            let summary = if met {
                format!(
                    "Passed: {needed} checks at {} or more, the last after a night's sleep.",
                    pct(*min)
                )
            } else if passes + 1 == needed && passes > 0 {
                format!(
                    "{passes} of {needed} checks at {} or more. The last one counts from {} on.",
                    pct(*min),
                    local_time(next_after.as_deref().unwrap_or_default())
                )
            } else {
                format!(
                    "{passes} of {needed} checks at {} or more; the last must come at least {gap_hours:.0} h after the one before.",
                    pct(*min)
                )
            };
            PassStatus {
                met,
                summary,
                passes,
                needed,
                next_after,
            }
        }
        PassRule::Detection {
            min_d_prime,
            streams,
        } => {
            let needed = (*streams).max(1);
            let scored: Vec<&&BlockRun> = checks.iter().filter(|r| r.d_prime.is_some()).collect();
            let streak = scored
                .iter()
                .rev()
                .take_while(|r| r.d_prime.unwrap_or(0.0) + 1e-9 >= *min_d_prime as f64)
                .count() as u32;
            let met = streak >= needed;
            let last = scored.last().and_then(|r| r.d_prime);
            let summary = if met {
                format!("Passed: {needed} streams in a row with d′ at least {min_d_prime:.1}.")
            } else if let Some(d) = last {
                format!(
                    "{streak} of {needed} streams in a row with d′ at least {min_d_prime:.1} (last: {d:.1})."
                )
            } else {
                format!("Needs {needed} streams with d′ at least {min_d_prime:.1}.")
            };
            PassStatus {
                met,
                summary,
                passes: streak.min(needed),
                needed,
                next_after: None,
            }
        }
        PassRule::Spacing { min, days } => {
            let last = checks.last();
            let acc_ok = last
                .map(|r| r.accuracy + 1e-9 >= *min as f64)
                .unwrap_or(false);
            let started = ctx
                .level_started
                .or_else(|| checks.first().and_then(|r| parse_time(&r.at)));
            let wait = Duration::seconds((*days as f64 * 86_400.0).round() as i64);
            let due = started.map(|t| t + wait);
            let time_ok = due.map(|d| ctx.now >= d).unwrap_or(false);
            let met = acc_ok && time_ok;
            let next_after = (acc_ok && !time_ok).then(|| due.unwrap().to_rfc3339());
            let summary = if met {
                format!(
                    "Passed: {} and {days:.0} days since this chord was added.",
                    pct(*min)
                )
            } else if let Some(at) = next_after.as_deref() {
                format!("Next chord due {}.", local_date(at))
            } else if let Some(r) = last {
                format!(
                    "Needs {} on the active chords (last: {}).",
                    pct(*min),
                    pct(r.accuracy as f32)
                )
            } else {
                format!(
                    "Needs {} on the active chords, then {days:.0} days before the next chord.",
                    pct(*min)
                )
            };
            PassStatus {
                met,
                summary,
                passes: u32::from(acc_ok),
                needed: 1,
                next_after,
            }
        }
    }
}

fn local_date(rfc: &str) -> String {
    parse_time(rfc)
        .map(|t| t.with_timezone(&Local).format("%a %e %b").to_string())
        .unwrap_or_default()
}

fn rule_context(db: &Db, course: &str, level: u32, now: DateTime<Utc>) -> Result<RuleContext> {
    let started = match db.course_state(course)? {
        Some((lvl, _, at)) if lvl == level => parse_time(&at),
        _ => None,
    };
    Ok(RuleContext {
        now,
        level_started: started,
    })
}

fn local_time(rfc: &str) -> String {
    parse_time(rfc)
        .map(|t| t.with_timezone(&Local).format("%a %H:%M").to_string())
        .unwrap_or_default()
}

/// A course's current level (1-based; `levels + 1` when complete).
pub fn current_level(db: &Db, course: &Course) -> Result<u32> {
    let n = course.levels.len() as u32;
    Ok(db
        .course_level(&course.id)?
        .map(|(l, _)| l)
        .unwrap_or(1)
        .clamp(1, n + 1))
}

/// Moves every ladder course on past the levels whose pass rule is now met.
pub fn refresh_progress(db: &Db) -> Result<()> {
    for c in builtin_courses() {
        if c.schedule != Schedule::Ladder {
            continue;
        }
        let mut level = current_level(db, &c)?;
        while (level as usize) <= c.levels.len() {
            let l = &c.levels[level as usize - 1];
            let status = evaluate_in(
                &l.pass,
                &level_runs(db, &c.id, level)?,
                &rule_context(db, &c.id, level, Utc::now())?,
            );
            if !status.met {
                break;
            }
            db.record_course_pass(
                &c.id,
                level,
                "passed",
                &json!({ "summary": status.summary }),
            )?;
            level += 1;
            db.set_course_level(&c.id, level, "passed")?;
        }
    }
    Ok(())
}

/// Sets a course's level by hand (from the course page or the coach).
pub fn set_level(db: &Db, course: &str, level: u32) -> Result<()> {
    let c = find_course(course).ok_or_else(|| anyhow!("unknown course {course}"))?;
    let n = c.levels.len() as u32;
    if !(1..=n + 1).contains(&level) {
        return Err(anyhow!(
            "{course} has levels 1–{n} ({} means complete)",
            n + 1
        ));
    }
    db.set_course_level(course, level, "manual")
}

// ---------------------------------------------------------------------------------------------
// Daily plan and status
// ---------------------------------------------------------------------------------------------

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema, TS)]
pub struct PlanCourse {
    pub course: String,
    pub minutes: f32,
    pub active: bool,
}

/// Which ladder courses go into the daily set and for how long, and the day tests are taken.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema, TS)]
pub struct DailyPlan {
    pub courses: Vec<PlanCourse>,
    /// 0 = Monday … 6 = Sunday.
    pub test_day: u32,
}

impl Default for DailyPlan {
    fn default() -> Self {
        DailyPlan {
            courses: vec![
                PlanCourse {
                    course: AP_NAMING.into(),
                    minutes: 15.0,
                    active: true,
                },
                PlanCourse {
                    course: AP_BRADY.into(),
                    minutes: 10.0,
                    active: true,
                },
            ],
            test_day: 6,
        }
    }
}

fn suggested_minutes(id: &str) -> f32 {
    match id {
        AP_SPEED => 4.0,
        AP_CHORD_COLORS => 3.0,
        AP_NOTE_COLORS => 8.0,
        AP_CONTRAST => 6.0,
        _ => 10.0,
    }
}

/// Preferences that change how course defaults are started.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema, TS)]
pub struct ApPrefs {
    /// Early colour, chord and contrast levels use errorless feedback.
    #[serde(default = "d_pref_true")]
    pub errorless_early: bool,
    /// Label set for the contrast course (`gervain_names` or `my_colors`).
    #[serde(default = "d_gervain")]
    pub contrast_labels: String,
}

fn d_pref_true() -> bool {
    true
}
fn d_gervain() -> String {
    crate::labels::GERVAIN_NAMES.into()
}

impl Default for ApPrefs {
    fn default() -> Self {
        serde_json::from_str("{}").unwrap()
    }
}

const PREFS_KEY: &str = "ap_prefs";

pub fn ap_prefs(db: &Db) -> Result<ApPrefs> {
    Ok(db.get_setting(PREFS_KEY)?.unwrap_or_default())
}

pub fn save_ap_prefs(db: &Db, prefs: &ApPrefs) -> Result<ApPrefs> {
    if crate::labels::builtin_set(&prefs.contrast_labels).is_none()
        && db
            .get_setting::<crate::labels::LabelSet>(&format!(
                "label_set:{}",
                prefs.contrast_labels
            ))?
            .is_none()
    {
        return Err(anyhow!("unknown label set {}", prefs.contrast_labels));
    }
    let mut prefs = prefs.clone();
    if prefs.contrast_labels.is_empty() {
        prefs.contrast_labels = d_gervain();
    }
    db.set_setting(PREFS_KEY, &prefs)?;
    Ok(prefs)
}

/// Turns off default errorless feedback, and points the contrast course at the chosen label set.
pub fn apply_session_prefs(spec: &mut SessionSpec, prefs: &ApPrefs) {
    for b in &mut spec.blocks {
        if !prefs.errorless_early
            && b.feedback == crate::schema::FeedbackMode::Errorless
            && b.tags.iter().any(|t| t == ERRORLESS_DEFAULT_TAG)
        {
            b.feedback = crate::schema::FeedbackMode::AfterTrial;
        }
        if b.tags.iter().any(|t| t == "ap.contrast") {
            if let crate::schema::Exercise::PitchId(p) = &mut b.exercise {
                p.labels = Some(prefs.contrast_labels.clone());
            }
        }
    }
}

const PLAN_KEY: &str = "ap_daily_plan";

pub fn daily_plan(db: &Db) -> Result<DailyPlan> {
    let mut plan: DailyPlan = db.get_setting(PLAN_KEY)?.unwrap_or_default();
    // Ladder courses added since the plan was saved start inactive.
    for c in builtin_courses() {
        if c.schedule == Schedule::Ladder && !plan.courses.iter().any(|p| p.course == c.id) {
            let minutes = suggested_minutes(&c.id);
            plan.courses.push(PlanCourse {
                course: c.id,
                minutes,
                active: false,
            });
        }
    }
    Ok(plan)
}

pub fn save_daily_plan(db: &Db, plan: &DailyPlan) -> Result<()> {
    if plan.test_day > 6 {
        return Err(anyhow!("test_day is 0 (Monday) to 6 (Sunday)"));
    }
    for p in &plan.courses {
        let c = find_course(&p.course).ok_or_else(|| anyhow!("unknown course {}", p.course))?;
        if c.schedule != Schedule::Ladder {
            return Err(anyhow!(
                "{} is a test; tests come up on their own schedule",
                p.course
            ));
        }
        if !(1.0..=90.0).contains(&p.minutes) {
            return Err(anyhow!("minutes for {} must be 1–90", p.course));
        }
    }
    db.set_setting(PLAN_KEY, plan)
}

#[derive(Clone, Debug, Serialize, Deserialize, TS)]
pub struct CourseStatus {
    pub id: String,
    pub title: String,
    pub short: String,
    pub description: String,
    /// `ladder` or `recurring`.
    pub kind: String,
    /// Current level (1-based); `levels + 1` when complete.
    pub level: u32,
    pub levels: u32,
    pub complete: bool,
    pub level_title: Option<String>,
    pub level_summary: Option<String>,
    /// Every level's title, for picking one.
    pub level_titles: Vec<String>,
    /// Set by hand rather than by passing.
    pub level_set_manually: bool,
    pub pass: Option<PassStatus>,
    pub last_practised: Option<String>,
    /// Due today (recurring tests).
    pub due: bool,
    /// Why it is due, or when it is next due.
    pub due_note: Option<String>,
    /// In the daily plan, with its minutes.
    pub in_plan: bool,
    pub minutes: Option<f32>,
}

fn days_since(at: Option<&str>, now: DateTime<Utc>) -> Option<f64> {
    at.and_then(parse_time)
        .map(|t| (now - t).num_minutes() as f64 / 1440.0)
}

const WEEKDAYS: [&str; 7] = [
    "Monday",
    "Tuesday",
    "Wednesday",
    "Thursday",
    "Friday",
    "Saturday",
    "Sunday",
];

/// Whether a recurring course is due, and a note saying why or when.
fn recurring_due(
    days: f32,
    on_test_day: bool,
    after_complete: bool,
    last: Option<&str>,
    plan: &DailyPlan,
    now: DateTime<Utc>,
) -> (bool, String) {
    if !after_complete {
        return (false, "Starts once note naming is complete.".into());
    }
    let since = days_since(last, now);
    let today = now.with_timezone(&Local).weekday().num_days_from_monday();
    let is_test_day = today == plan.test_day;
    let interval_ok = since.map(|d| d >= days as f64 - 0.5).unwrap_or(true);
    let day = WEEKDAYS[plan.test_day as usize];
    if on_test_day {
        // A missed test day is made up as soon as it is a day overdue.
        let overdue = since.map(|d| d >= days as f64 + 1.0).unwrap_or(false);
        if interval_ok && (is_test_day || overdue) {
            (
                true,
                if since.is_none() {
                    "Due: first run.".into()
                } else {
                    "Due today.".into()
                },
            )
        } else if !interval_ok {
            let next = last
                .and_then(parse_time)
                .map(|t| t + Duration::days(days.round() as i64));
            (
                false,
                format!(
                    "Next on the {} test day{}.",
                    day,
                    next.map(|n| format!(" from {}", n.with_timezone(&Local).format("%e %b")))
                        .unwrap_or_default()
                ),
            )
        } else {
            (false, format!("Next on {day}."))
        }
    } else if interval_ok {
        (
            true,
            if since.is_none() {
                "Due: first run.".into()
            } else {
                format!("Due: last run {:.0} days ago.", since.unwrap())
            },
        )
    } else {
        (
            false,
            format!(
                "Next due in {:.0} days.",
                (days as f64 - since.unwrap_or(0.0)).max(0.5).ceil()
            ),
        )
    }
}

pub fn course_statuses(db: &Db, now: DateTime<Utc>) -> Result<Vec<CourseStatus>> {
    refresh_progress(db)?;
    let plan = daily_plan(db)?;
    let courses = builtin_courses();
    let complete_of = |id: &str| -> Result<bool> {
        match courses.iter().find(|c| c.id == id) {
            Some(c) => Ok(current_level(db, c)? as usize > c.levels.len()),
            None => Ok(true),
        }
    };
    let mut out = Vec::new();
    for c in &courses {
        let level = current_level(db, c)?;
        let n = c.levels.len() as u32;
        let complete = level > n;
        let current = (!complete).then(|| &c.levels[level as usize - 1]);
        let last = tagged_trials(db, &format!("course.{}", c.id), 1)?
            .first()
            .map(|t| t.started_at.clone());
        let pass = match (current, &c.schedule) {
            (Some(l), Schedule::Ladder) => Some(evaluate_in(
                &l.pass,
                &level_runs(db, &c.id, level)?,
                &rule_context(db, &c.id, level, now)?,
            )),
            _ => None,
        };
        let (due, due_note) = match &c.schedule {
            Schedule::Ladder => (false, None),
            Schedule::Recurring {
                days,
                on_test_day,
                after,
            } => {
                let after_ok = match after {
                    Some(a) => complete_of(a)?,
                    None => true,
                };
                let (d, note) =
                    recurring_due(*days, *on_test_day, after_ok, last.as_deref(), &plan, now);
                (d, Some(note))
            }
        };
        let in_plan = plan.courses.iter().find(|p| p.course == c.id);
        out.push(CourseStatus {
            id: c.id.clone(),
            title: c.title.clone(),
            short: c.short.clone(),
            description: c.description.clone(),
            kind: match c.schedule {
                Schedule::Ladder => "ladder".into(),
                Schedule::Recurring { .. } => "recurring".into(),
            },
            level: level.min(n + 1),
            levels: n,
            complete: complete && c.schedule == Schedule::Ladder,
            level_title: current.map(|l| l.title.clone()),
            level_summary: current.map(|l| l.summary.clone()),
            level_titles: c.levels.iter().map(|l| l.title.clone()).collect(),
            level_set_manually: db
                .course_level(&c.id)?
                .map(|(_, s)| s == "manual")
                .unwrap_or(false),
            pass,
            last_practised: last,
            due,
            due_note,
            in_plan: in_plan.map(|p| p.active).unwrap_or(false),
            minutes: in_plan.map(|p| p.minutes),
        });
    }
    Ok(out)
}

// ---------------------------------------------------------------------------------------------
// The daily set
// ---------------------------------------------------------------------------------------------

#[derive(Clone, Debug, Serialize, Deserialize, TS)]
pub struct DailyItem {
    pub course: String,
    pub title: String,
    pub level: Option<u32>,
    pub minutes: f64,
    pub reason: String,
    pub test: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize, TS)]
pub struct DailySet {
    #[ts(type = "Record<string, unknown>")]
    pub spec: SessionSpec,
    pub items: Vec<DailyItem>,
    pub minutes: f64,
}

/// Seconds a block takes, from its generated trials.
fn block_seconds(b: &BlockSpec, ctx: &GenContext) -> f64 {
    let trials = generate_block(b, block_seed(1, 0, b), ctx).unwrap_or_default();
    trials
        .iter()
        .map(|t| estimate_trial_seconds(t) as f64)
        .sum::<f64>()
        + b.pre_mask.map(|m| m.seconds() as f64).unwrap_or(0.0)
}

/// A level's blocks fitted to `minutes`: check blocks and single trials keep their size, the
/// practice blocks are scaled.
fn fit_level(blocks: &[BlockSpec], minutes: f32, ctx: &GenContext) -> Vec<BlockSpec> {
    let fixed =
        |b: &BlockSpec| b.course.as_ref().map(|c| c.verify).unwrap_or(false) || b.count <= 3;
    let fixed_s: f64 = blocks
        .iter()
        .filter(|b| fixed(b))
        .map(|b| block_seconds(b, ctx))
        .sum();
    let scalable_s: f64 = blocks
        .iter()
        .filter(|b| !fixed(b))
        .map(|b| block_seconds(b, ctx))
        .sum();
    let budget = (minutes as f64 * 60.0 - fixed_s).max(60.0);
    let scale = if scalable_s > 0.0 {
        budget / scalable_s
    } else {
        1.0
    };
    blocks
        .iter()
        .map(|b| {
            let mut b = b.clone();
            if !fixed(&b) {
                b.count = ((b.count as f64 * scale).round() as u32).clamp(6, 400);
            }
            // A stream whose check is the stream itself scales too, but keeps a usable length.
            if b.count > 3 && fixed(&b) && scalable_s == 0.0 {
                let s = block_seconds(&b, ctx);
                if s > 0.0 {
                    // A detection stream is one long trial; a naming tone is a short one.
                    let per = s / b.count as f64;
                    let (min_n, max_n) = if per > 8.0 { (2, 24) } else { (20, 400) };
                    b.count = ((b.count as f64 * minutes as f64 * 60.0 / s).round() as u32)
                        .clamp(min_n, max_n);
                }
            }
            b
        })
        .collect()
}

pub fn compose_daily_set(db: &Db, ctx: &GenContext, now: DateTime<Utc>) -> Result<DailySet> {
    let statuses = course_statuses(db, now)?;
    let plan = daily_plan(db)?;
    let courses = builtin_courses();
    let mut items = Vec::new();
    let mut blocks: Vec<BlockSpec> = Vec::new();
    let mut add = |c: &Course,
                   level: u32,
                   blocks_in: Vec<BlockSpec>,
                   reason: String,
                   test: bool,
                   items: &mut Vec<DailyItem>| {
        let minutes: f64 = blocks_in.iter().map(|b| block_seconds(b, ctx)).sum::<f64>() / 60.0;
        let lvl = &c.levels[level as usize - 1];
        items.push(DailyItem {
            course: c.id.clone(),
            title: if c.levels.len() > 1 {
                format!("{}: level {level} ({})", c.title, lvl.title)
            } else {
                c.title.clone()
            },
            level: (c.levels.len() > 1).then_some(level),
            minutes: (minutes * 10.0).round() / 10.0,
            reason,
            test,
        });
        for mut b in blocks_in {
            b.title = Some(format!("{} · {}", c.short, b.display_title()));
            blocks.push(b);
        }
    };
    // Tests that are due come first, while nothing has been named yet today.
    for s in statuses.iter().filter(|s| s.kind == "recurring" && s.due) {
        let c = courses.iter().find(|c| c.id == s.id).unwrap();
        add(
            c,
            1,
            c.levels[0].blocks.clone(),
            s.due_note.clone().unwrap_or_default(),
            true,
            &mut items,
        );
    }
    // Then each active ladder course at its current level, in an order that rotates daily.
    let mut ladder: Vec<(&Course, &PlanCourse, &CourseStatus)> = plan
        .courses
        .iter()
        .filter(|p| p.active)
        .filter_map(|p| {
            let c = courses.iter().find(|c| c.id == p.course)?;
            let s = statuses.iter().find(|s| s.id == p.course)?;
            (!s.complete).then_some((c, p, s))
        })
        .collect();
    if !ladder.is_empty() {
        let day = now.with_timezone(&Local).num_days_from_ce() as usize;
        let k = day % ladder.len();
        ladder.rotate_left(k);
    }
    for (c, p, s) in ladder {
        let l = &c.levels[s.level as usize - 1];
        let reason = match &s.pass {
            Some(ps) if !ps.summary.is_empty() => {
                format!("Level {} of {}. {}", s.level, s.levels, ps.summary)
            }
            _ => format!("Level {} of {}.", s.level, s.levels),
        };
        add(
            c,
            s.level,
            fit_level(&l.blocks, p.minutes, ctx),
            reason,
            false,
            &mut items,
        );
    }
    if blocks.is_empty() {
        return Err(anyhow!(
            "Nothing to practise: turn on a course in the daily plan."
        ));
    }
    // Clear pitch memory before anything is heard.
    if blocks[0].pre_mask.is_none() {
        blocks[0].pre_mask = Some(Mask::ShepardGlissando);
    }
    let minutes = items.iter().map(|i| i.minutes).sum::<f64>()
        + if items.first().map(|i| i.test).unwrap_or(false) {
            0.0
        } else {
            20.0 / 60.0
        };
    let mut spec = SessionSpec {
        title: "Absolute pitch: today's set".into(),
        protocol: Some("ap_daily_set".into()),
        planned_minutes: Some(minutes as f32),
        notes: Some(
            items
                .iter()
                .map(|i| i.title.clone())
                .collect::<Vec<_>>()
                .join(" · "),
        ),
        seed: None,
        blocks,
    };
    apply_session_prefs(&mut spec, &ap_prefs(db)?);
    Ok(DailySet {
        spec,
        items,
        minutes: (minutes * 10.0).round() / 10.0,
    })
}

// ---------------------------------------------------------------------------------------------
// Dashboard
// ---------------------------------------------------------------------------------------------

/// Answers count towards mastery only when correct within this time.
pub const MASTERY_RT_MS: f64 = 2000.0;
/// The recent trials per class that mastery is measured over.
const MASTERY_WINDOW: usize = 40;
const MASTERY_MIN_TRIALS: usize = 20;

#[derive(Clone, Debug, Serialize, Deserialize, TS)]
pub struct ClassProgress {
    /// `C`, `C#`, …
    pub pitch_class: String,
    pub name: String,
    /// Recent naming trials (all methods), and the share correct within 2 s.
    pub naming_trials: u32,
    pub naming_in_time: Option<f64>,
    pub naming_accuracy: Option<f64>,
    pub mastered: bool,
    /// Recent sung trials of this note and the share in the right category.
    pub singing_trials: u32,
    pub singing_accuracy: Option<f64>,
    /// Mean d′ of recent detection streams for this note.
    pub detection_d_prime: Option<f64>,
    pub last_seen: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize, TS)]
pub struct TestRun {
    pub session_id: String,
    pub at: String,
    pub trials: u32,
    pub accuracy: f64,
    /// UCSF-style points as a share of the maximum (1 exact, 0.75 a semitone off).
    pub ucsf_points: f64,
    /// Mean absolute error in semitones, folded to the nearest octave (0 perfect, 3 chance).
    pub mad: Option<f64>,
    /// Accuracy per timbre.
    pub by_timbre: Vec<TimbreAccuracy>,
}

#[derive(Clone, Debug, Serialize, Deserialize, TS)]
pub struct TimbreAccuracy {
    pub timbre: String,
    pub trials: u32,
    pub accuracy: f64,
}

#[derive(Clone, Debug, Serialize, Deserialize, TS)]
pub struct AnchorPoint {
    pub at: String,
    /// Cents from the nearest real C (or whichever note was asked).
    pub cents: f64,
}

#[derive(Clone, Debug, Serialize, Deserialize, TS)]
pub struct ApDashboard {
    pub classes: Vec<ClassProgress>,
    pub classes_mastered: u32,
    pub courses: Vec<CourseStatus>,
    pub weekly: Vec<TestRun>,
    pub ucsf_style: Vec<TestRun>,
    /// Singing your anchor from memory, oldest first.
    pub anchor: Vec<AnchorPoint>,
    pub anchor_median_abs_cents: Option<f64>,
    /// Minutes of absolute-pitch sessions in the last 7 days.
    pub minutes_this_week: f64,
    pub plan: DailyPlan,
}

fn is_out_of_set(t: &TrialRow) -> bool {
    t.stimulus
        .get("out_of_set")
        .and_then(Value::as_bool)
        .unwrap_or(false)
}

fn test_runs(db: &Db, course: &str) -> Result<Vec<TestRun>> {
    let trials = tagged_trials(db, &format!("course.{course}"), 20_000)?;
    let mut by_session: BTreeMap<String, Vec<&TrialRow>> = BTreeMap::new();
    for t in &trials {
        if t.correct.is_some() && !is_out_of_set(t) {
            by_session.entry(t.session_id.clone()).or_default().push(t);
        }
    }
    let mut out: Vec<TestRun> = by_session
        .into_iter()
        .map(|(sid, rows)| {
            let n = rows.len() as f64;
            let correct = rows.iter().filter(|t| t.correct == Some(true)).count() as f64;
            let points: f64 = rows
                .iter()
                .filter_map(|t| t.metrics.get("ucsf_points").and_then(Value::as_f64))
                .sum();
            let errs: Vec<f64> = rows
                .iter()
                .filter_map(|t| t.metrics.get("class_error").and_then(Value::as_f64))
                .map(f64::abs)
                .collect();
            let mut timbres: BTreeMap<String, (u32, u32)> = BTreeMap::new();
            for t in &rows {
                let k = t
                    .stimulus
                    .get("timbre")
                    .and_then(Value::as_str)
                    .unwrap_or("?")
                    .to_string();
                let e = timbres.entry(k).or_default();
                e.0 += 1;
                e.1 += (t.correct == Some(true)) as u32;
            }
            TestRun {
                session_id: sid,
                at: rows
                    .iter()
                    .map(|t| t.started_at.clone())
                    .min()
                    .unwrap_or_default(),
                trials: n as u32,
                accuracy: correct / n,
                ucsf_points: points / n,
                mad: (!errs.is_empty()).then(|| errs.iter().sum::<f64>() / errs.len() as f64),
                by_timbre: timbres
                    .into_iter()
                    .map(|(timbre, (k, c))| TimbreAccuracy {
                        timbre,
                        trials: k,
                        accuracy: c as f64 / k as f64,
                    })
                    .collect(),
            }
        })
        .collect();
    out.sort_by(|a, b| a.at.cmp(&b.at));
    Ok(out)
}

pub fn ap_dashboard(db: &Db, now: DateTime<Utc>) -> Result<ApDashboard> {
    let courses = course_statuses(db, now)?;
    let since = (now - Duration::days(90)).to_rfc3339();
    let naming = db.query_trials(
        &TrialFilter {
            types: vec!["pitch_id".into()],
            assisted: Some(false),
            since: Some(since.clone()),
            ..Default::default()
        },
        30_000,
        0,
    )?;
    let singing = db.query_trials(
        &TrialFilter {
            types: vec!["sing_pitch".into()],
            assisted: Some(false),
            since: Some(since.clone()),
            ..Default::default()
        },
        10_000,
        0,
    )?;
    let detecting = db.query_trials(
        &TrialFilter {
            types: vec!["pitch_detect".into()],
            assisted: Some(false),
            since: Some(since),
            ..Default::default()
        },
        5_000,
        0,
    )?;
    let class_of = |t: &TrialRow| {
        t.stimulus
            .get("pitch_class")
            .and_then(Value::as_str)
            .map(String::from)
    };
    let classes: Vec<ClassProgress> = PitchClass::ALL
        .iter()
        .map(|pc| {
            let id = pc.name();
            // Newest first, as queried.
            let named: Vec<&TrialRow> = naming
                .iter()
                .filter(|t| {
                    t.correct.is_some() && !is_out_of_set(t) && class_of(t).as_deref() == Some(id)
                })
                .take(MASTERY_WINDOW)
                .collect();
            let sung: Vec<&TrialRow> = singing
                .iter()
                .filter(|t| t.correct.is_some() && class_of(t).as_deref() == Some(id))
                .take(MASTERY_WINDOW / 2)
                .collect();
            let detected: Vec<f64> = detecting
                .iter()
                .filter(|t| class_of(t).as_deref() == Some(id))
                .filter_map(|t| t.metrics.get("d_prime").and_then(|v| v.as_f64()))
                .take(8)
                .collect();
            let share = |rows: &[&TrialRow], f: &dyn Fn(&TrialRow) -> bool| {
                (!rows.is_empty())
                    .then(|| rows.iter().filter(|t| f(t)).count() as f64 / rows.len() as f64)
            };
            let in_time = share(&named, &|t| {
                t.correct == Some(true) && t.rt_ms.map(|r| r <= MASTERY_RT_MS).unwrap_or(false)
            });
            ClassProgress {
                pitch_class: id.to_string(),
                name: pc.display_name().to_string(),
                naming_trials: named.len() as u32,
                naming_in_time: in_time,
                naming_accuracy: share(&named, &|t| t.correct == Some(true)),
                mastered: named.len() >= MASTERY_MIN_TRIALS && in_time.unwrap_or(0.0) >= 0.9,
                singing_trials: sung.len() as u32,
                singing_accuracy: share(&sung, &|t| t.correct == Some(true)),
                detection_d_prime: (!detected.is_empty())
                    .then(|| detected.iter().sum::<f64>() / detected.len() as f64),
                last_seen: named.first().or(sung.first()).map(|t| t.started_at.clone()),
            }
        })
        .collect();
    let anchor_rows = tagged_trials(db, ANCHOR_CHECK_TAG, 60)?;
    let mut anchor: Vec<AnchorPoint> = anchor_rows
        .iter()
        .filter_map(|t| {
            let c = t.metrics.get("cents_error").and_then(Value::as_f64)?;
            Some(AnchorPoint {
                at: t.started_at.clone(),
                cents: c,
            })
        })
        .collect();
    anchor.reverse();
    let anchor_median_abs_cents = {
        let mut v: Vec<f64> = anchor
            .iter()
            .rev()
            .take(10)
            .map(|a| a.cents.abs())
            .collect();
        v.sort_by(|a, b| a.total_cmp(b));
        (!v.is_empty()).then(|| v[v.len() / 2])
    };
    let week_ago = now - Duration::days(7);
    let minutes_this_week = db
        .list_sessions(Some(SessionStatus::Completed), 200)?
        .iter()
        .filter(|s| {
            s.protocol
                .as_deref()
                .map(|p| p.starts_with("ap"))
                .unwrap_or(false)
        })
        .filter_map(|s| {
            let a = parse_time(s.started_at.as_deref()?)?;
            let b = parse_time(s.completed_at.as_deref()?)?;
            (a >= week_ago).then(|| (b - a).num_seconds().max(0) as f64 / 60.0)
        })
        .sum::<f64>();
    Ok(ApDashboard {
        classes_mastered: classes.iter().filter(|c| c.mastered).count() as u32,
        classes,
        courses,
        weekly: test_runs(db, AP_WEEKLY)?,
        ucsf_style: test_runs(db, AP_UCSF_STYLE)?,
        anchor,
        anchor_median_abs_cents,
        minutes_this_week: (minutes_this_week * 10.0).round() / 10.0,
        plan: daily_plan(db)?,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::storage::tests_support::sample_trial;
    use crate::validate::validate_session;
    use chrono::TimeZone;

    fn at(day: u32, hour: u32, min: u32) -> DateTime<Utc> {
        Utc.with_ymd_and_hms(2026, 10, day, hour, min, 0).unwrap()
    }

    fn run(at: DateTime<Utc>, accuracy: f64, check: bool) -> BlockRun {
        BlockRun {
            session_id: "s".into(),
            block_idx: 0,
            at: at.to_rfc3339(),
            trials: 20,
            correct: (accuracy * 20.0).round() as u32,
            accuracy,
            check,
            d_prime: None,
        }
    }

    /// `n` trials of one block of a course level, `correct_share` of them right.
    fn add_block(
        db: &Db,
        course: &str,
        level: u32,
        check: bool,
        when: DateTime<Utc>,
        n: u32,
        correct_share: f64,
    ) {
        let spec = SessionSpec {
            title: "t".into(),
            protocol: Some(course.into()),
            planned_minutes: None,
            notes: None,
            seed: None,
            blocks: vec![],
        };
        let sid = db.start_new_session(&spec, "test").unwrap();
        let tag = CourseRef {
            course: course.into(),
            level,
            verify: check,
        }
        .tag();
        for i in 0..n {
            let mut t = sample_trial(&sid, i, (i as f64) < correct_share * n as f64);
            t.tags = vec!["ap.id".into(), tag.clone()];
            if check {
                t.tags.push(CHECK_TAG.into());
            }
            t.started_at = (when + Duration::seconds(i as i64 * 3)).to_rfc3339();
            db.insert_trial(&t).unwrap();
        }
    }

    #[test]
    fn every_course_level_is_a_valid_session() {
        let ctx = GenContext::default();
        for c in builtin_courses() {
            for n in 1..=c.levels.len() as u32 {
                let spec = level_session(&c, n).unwrap();
                let r = validate_session(&spec, &ctx);
                assert!(r.ok, "{} level {n}: {:?}", c.id, r.errors);
                assert!(
                    r.warnings.is_empty(),
                    "{} level {n}: {:?}",
                    c.id,
                    r.warnings
                );
                for b in &spec.blocks {
                    let cr = b.course.as_ref().expect("course blocks are tagged");
                    assert_eq!((cr.course.as_str(), cr.level), (c.id.as_str(), n));
                }
                // Every level has something its pass rule can count.
                assert!(
                    spec.blocks
                        .iter()
                        .any(|b| b.course.as_ref().unwrap().verify),
                    "{} level {n}",
                    c.id
                );
            }
        }
        let naming = find_course(AP_NAMING).unwrap();
        assert_eq!(naming.levels.len(), 10);
        assert!(matches!(
            naming.levels[9].pass,
            PassRule::Verification { passes: 4, .. }
        ));
    }

    #[test]
    fn accuracy_rule_needs_consecutive_checks() {
        let rule = PassRule::Accuracy {
            min: 0.9,
            blocks: 2,
        };
        let runs = vec![
            run(at(1, 9, 0), 0.95, true),
            run(at(2, 9, 0), 0.5, false),
            run(at(2, 9, 10), 0.85, true),
        ];
        assert!(!evaluate(&rule, &runs).met);
        let mut more = runs.clone();
        more.push(run(at(3, 9, 0), 0.9, true));
        let s = evaluate(&rule, &more);
        assert!(!s.met && s.passes == 1, "{s:?}");
        more.push(run(at(4, 9, 0), 0.96, true));
        assert!(evaluate(&rule, &more).met);
    }

    #[test]
    fn spacing_needs_fourteen_days_after_a_perfect_check() {
        let rule = PassRule::Spacing {
            min: 1.0,
            days: 14.0,
        };
        let runs = vec![run(at(1, 9, 0), 1.0, true)];
        let ctx = |day| RuleContext {
            now: at(day, 9, 0),
            level_started: Some(at(1, 9, 0)),
        };
        let early = evaluate_in(&rule, &runs, &ctx(14));
        assert!(!early.met, "13 days is not enough: {early:?}");
        assert!(
            early.summary.contains("Next chord due"),
            "{}",
            early.summary
        );
        assert!(early.next_after.is_some());
        assert!(evaluate_in(&rule, &runs, &ctx(15)).met);
        let short = vec![run(at(1, 9, 0), 0.95, true)];
        let later = evaluate_in(&rule, &short, &ctx(20));
        assert!(!later.met, "{later:?}");
        assert!(later.next_after.is_none());
    }

    #[test]
    fn the_last_verification_needs_the_overnight_gap() {
        let rule = PassRule::Verification {
            min: 0.9,
            passes: 4,
            gap_hours: 12.0,
        };
        let mut runs = vec![
            run(at(1, 9, 0), 0.95, true),
            run(at(1, 10, 0), 0.6, true),
            run(at(1, 11, 0), 0.92, true),
            run(at(1, 12, 0), 0.93, true),
        ];
        let s = evaluate(&rule, &runs);
        assert_eq!(s.passes, 3);
        assert_eq!(
            s.next_after.as_deref(),
            Some(at(2, 0, 0).to_rfc3339().as_str())
        );
        runs.push(run(at(1, 23, 59), 0.97, true));
        assert!(!evaluate(&rule, &runs).met, "11 h 59 min is not enough");
        runs.push(run(at(2, 0, 0), 0.91, true));
        assert!(evaluate(&rule, &runs).met);
    }

    #[test]
    fn passing_checks_moves_the_course_on() {
        let db = Db::open_in_memory().unwrap();
        let naming = find_course(AP_NAMING).unwrap();
        assert_eq!(current_level(&db, &naming).unwrap(), 1);
        add_block(&db, AP_NAMING, 1, false, at(1, 9, 0), 40, 0.5);
        add_block(&db, AP_NAMING, 1, true, at(1, 9, 30), 24, 0.95);
        refresh_progress(&db).unwrap();
        assert_eq!(
            current_level(&db, &naming).unwrap(),
            1,
            "one check is not enough"
        );
        add_block(&db, AP_NAMING, 1, true, at(2, 9, 30), 24, 0.92);
        refresh_progress(&db).unwrap();
        assert_eq!(current_level(&db, &naming).unwrap(), 2);
        assert_eq!(db.course_passes(AP_NAMING).unwrap().len(), 1);
        set_level(&db, AP_NAMING, 5).unwrap();
        assert_eq!(current_level(&db, &naming).unwrap(), 5);
        assert!(set_level(&db, AP_NAMING, 12).is_err());
    }

    #[test]
    fn the_daily_set_puts_due_tests_first() {
        let db = Db::open_in_memory().unwrap();
        let ctx = GenContext::default();
        // 25 October 2026 is a Sunday, the default test day.
        let sunday = compose_daily_set(&db, &ctx, at(25, 12, 0)).unwrap();
        let courses: Vec<&str> = sunday.items.iter().map(|i| i.course.as_str()).collect();
        assert_eq!(&courses[..2], [AP_WEEKLY, AP_UCSF_STYLE]);
        assert!(courses.contains(&AP_NAMING) && courses.contains(&AP_BRADY));
        assert!(
            !courses.contains(&AP_MAINTENANCE),
            "maintenance waits for naming to be complete"
        );
        assert_eq!(sunday.spec.blocks[0].pre_mask, Some(Mask::ShepardGlissando));
        assert!(sunday.spec.blocks.iter().all(|b| b.course.is_some()));
        assert!(validate_session(&sunday.spec, &ctx).ok);

        let monday = compose_daily_set(&db, &ctx, at(26, 12, 0)).unwrap();
        assert!(monday.items.iter().all(|i| !i.test));
        // About the planned 25 minutes, plus the clearing sound.
        assert!((18.0..34.0).contains(&monday.minutes), "{}", monday.minutes);
        assert_eq!(monday.spec.blocks[0].pre_mask, Some(Mask::ShepardGlissando));
        // The order of the courses changes from day to day.
        let tuesday = compose_daily_set(&db, &ctx, at(27, 12, 0)).unwrap();
        assert_ne!(monday.items[0].course, tuesday.items[0].course);

        // A test done this week is not due again until next week.
        add_block(&db, AP_WEEKLY, 1, true, at(25, 13, 0), 120, 0.5);
        let next = compose_daily_set(&db, &ctx, at(25, 18, 0)).unwrap();
        assert!(!next.items.iter().any(|i| i.course == AP_WEEKLY));
        let week_later = compose_daily_set(&db, &ctx, at(1, 12, 0) + Duration::days(31)).unwrap();
        assert!(
            week_later.items.iter().any(|i| i.course == AP_WEEKLY),
            "{:?}",
            week_later.items
        );

        // Nothing active, nothing due: no set.
        let mut plan = daily_plan(&db).unwrap();
        for p in &mut plan.courses {
            p.active = false;
        }
        save_daily_plan(&db, &plan).unwrap();
        assert!(compose_daily_set(&db, &ctx, at(26, 12, 0)).is_err());
    }

    #[test]
    fn the_dashboard_pools_naming_per_note() {
        let db = Db::open_in_memory().unwrap();
        let spec = SessionSpec {
            title: "t".into(),
            protocol: Some("ap_naming".into()),
            planned_minutes: None,
            notes: None,
            seed: None,
            blocks: vec![],
        };
        let sid = db.start_new_session(&spec, "test").unwrap();
        let now = at(26, 12, 0);
        for i in 0..60u32 {
            // Even trials are C answered quickly; odd ones are E answered slowly.
            let mut t = sample_trial(&sid, i, true);
            t.rt_ms = Some(if i % 2 == 0 { 900.0 } else { 2600.0 });
            t.started_at = (now - Duration::hours(1) + Duration::seconds(i as i64)).to_rfc3339();
            db.insert_trial(&t).unwrap();
        }
        // An out-of-set tone does not count for its note.
        let mut o = sample_trial(&sid, 99, false);
        o.stimulus = json!({"pitch_class": "C", "out_of_set": true});
        o.started_at = now.to_rfc3339();
        db.insert_trial(&o).unwrap();
        let d = ap_dashboard(&db, now).unwrap();
        let c = d.classes.iter().find(|c| c.pitch_class == "C").unwrap();
        let e = d.classes.iter().find(|c| c.pitch_class == "E").unwrap();
        assert!(c.mastered && c.naming_in_time == Some(1.0), "{c:?}");
        assert!(
            !e.mastered && e.naming_accuracy == Some(1.0) && e.naming_in_time == Some(0.0),
            "{e:?}"
        );
        assert_eq!(d.classes_mastered, 1);
        assert_eq!(d.courses.len(), builtin_courses().len());
    }
}
