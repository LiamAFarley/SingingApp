//! The workshop pages: a course's lessons by module with their scores, and the user's own
//! custom levels for an activity (Functional Ear Trainer's "Custom levels").

use std::collections::BTreeMap;

use anyhow::{Result, anyhow};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use ts_rs::TS;

use crate::courses::{PassRule, current_level, find_course, tagged_trials};
use crate::generate::{GenContext, generate_block};
use crate::schema::{BlockSpec, SessionSpec};
use crate::storage::{Db, TrialRow, new_id};

/// A run shorter than this does not count towards a lesson's best score (an abandoned run).
const MIN_RUN: u32 = 10;

#[derive(Clone, Debug, Serialize, Deserialize, TS)]
pub struct LessonView {
    /// The course level this lesson is (1-based).
    pub level: u32,
    pub title: String,
    pub summary: String,
    /// No feedback until the end.
    pub test: bool,
    pub questions: u32,
    pub runs: u32,
    /// Best and latest score of a full run (0–1).
    pub best: Option<f64>,
    pub last: Option<f64>,
    pub last_at: Option<String>,
    pub passed: bool,
    /// The lesson's exercise settings, to start a custom level from.
    #[ts(type = "Record<string, unknown>")]
    pub params: Value,
}

#[derive(Clone, Debug, Serialize, Deserialize, TS)]
pub struct ModuleView {
    pub title: String,
    pub lessons: Vec<LessonView>,
    pub passed: u32,
}

#[derive(Clone, Debug, Serialize, Deserialize, TS)]
pub struct WorkshopView {
    pub id: String,
    pub title: String,
    pub description: String,
    /// The exercise its lessons use (`interval_id` …), which custom levels use too.
    pub exercise: String,
    /// The lesson the course is on (1-based); `lessons + 1` when every lesson is done.
    pub current: u32,
    pub lessons: u32,
    pub passed: u32,
    pub pass_mark: f32,
    pub modules: Vec<ModuleView>,
}

/// Scores of a set of trials, run by run: (accuracy, trials, started) per session block.
fn runs(trials: &[TrialRow]) -> Vec<(f64, u32, String)> {
    let mut by_run: BTreeMap<(String, u32), (u32, u32, String)> = BTreeMap::new();
    for t in trials {
        let Some(c) = t.correct else { continue };
        let r = by_run
            .entry((t.session_id.clone(), t.block_idx))
            .or_insert((0, 0, t.started_at.clone()));
        r.0 += 1;
        r.1 += c as u32;
        if t.started_at < r.2 {
            r.2 = t.started_at.clone();
        }
    }
    let mut out: Vec<(f64, u32, String)> = by_run
        .into_values()
        .map(|(n, c, at)| (c as f64 / n as f64, n, at))
        .collect();
    out.sort_by(|a, b| a.2.cmp(&b.2));
    out
}

fn exercise_of(b: &BlockSpec) -> String {
    serde_json::to_value(b)
        .ok()
        .and_then(|v| v.get("type").and_then(|t| t.as_str()).map(String::from))
        .unwrap_or_default()
}

/// A workshop course's lessons by module, with each lesson's scores.
pub fn workshop_view(db: &Db, course: &str) -> Result<WorkshopView> {
    let c = find_course(course).ok_or_else(|| anyhow!("unknown course {course}"))?;
    let prefix = format!("course.{course}.");
    let mut by_level: BTreeMap<u32, Vec<TrialRow>> = BTreeMap::new();
    for t in tagged_trials(db, &format!("course.{course}"), 200_000)? {
        let level = t
            .tags
            .iter()
            .find_map(|g| g.strip_prefix(&prefix).and_then(|n| n.parse::<u32>().ok()));
        if let Some(l) = level {
            by_level.entry(l).or_default().push(t);
        }
    }
    let mut modules: Vec<ModuleView> = Vec::new();
    let mut pass_mark = 0.9;
    for (i, l) in c.levels.iter().enumerate() {
        let level = i as u32 + 1;
        let min = match l.pass {
            PassRule::Accuracy { min, .. } => min,
            _ => 0.9,
        };
        pass_mark = min;
        let block = l.blocks.first();
        let questions = block.map(|b| b.count).unwrap_or(0);
        let full: Vec<(f64, u32, String)> =
            runs(by_level.get(&level).map(Vec::as_slice).unwrap_or(&[]))
                .into_iter()
                .filter(|r| r.1 >= MIN_RUN.min(questions.max(1)))
                .collect();
        let best = full
            .iter()
            .map(|r| r.0)
            .fold(None, |a: Option<f64>, x| Some(a.map_or(x, |a| a.max(x))));
        let view = LessonView {
            level,
            title: l.title.clone(),
            summary: l.summary.clone(),
            test: block
                .map(|b| b.feedback == crate::schema::FeedbackMode::None)
                .unwrap_or(false),
            questions,
            runs: full.len() as u32,
            best,
            last: full.last().map(|r| r.0),
            last_at: full.last().map(|r| r.2.clone()),
            passed: best.is_some_and(|b| b + 1e-9 >= min as f64),
            params: block
                .and_then(|b| serde_json::to_value(b).ok())
                .and_then(|v| v.get("params").cloned())
                .unwrap_or_else(|| json!({})),
        };
        let title = l.module.clone().unwrap_or_else(|| "Levels".into());
        match modules.last_mut() {
            Some(m) if m.title == title => m.lessons.push(view),
            _ => modules.push(ModuleView {
                title,
                lessons: vec![view],
                passed: 0,
            }),
        }
    }
    for m in &mut modules {
        m.passed = m.lessons.iter().filter(|l| l.passed).count() as u32;
    }
    Ok(WorkshopView {
        id: c.id.clone(),
        title: c.title.clone(),
        description: c.description.clone(),
        exercise: c
            .levels
            .first()
            .and_then(|l| l.blocks.first())
            .map(exercise_of)
            .unwrap_or_default(),
        current: current_level(db, &c)?,
        lessons: c.levels.len() as u32,
        passed: modules.iter().map(|m| m.passed).sum(),
        pass_mark,
        modules,
    })
}

// ---------------------------------------------------------------------------------------------
// Custom levels
// ---------------------------------------------------------------------------------------------

/// A level the user built for an activity: the exercise's settings and a question count.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema, TS)]
pub struct CustomLevel {
    /// Empty when saving a new one.
    #[serde(default)]
    pub id: String,
    /// The workshop course it belongs to (`rp_degrees` …).
    pub course: String,
    pub title: String,
    pub count: u32,
    /// The exercise's params, as in a session block.
    #[ts(type = "Record<string, unknown>")]
    pub params: Value,
}

#[derive(Clone, Debug, Serialize, Deserialize, TS)]
pub struct CustomLevelView {
    pub level: CustomLevel,
    pub runs: u32,
    pub best: Option<f64>,
    pub last: Option<f64>,
    pub last_at: Option<String>,
}

const CUSTOM_KEY: &str = "custom_levels";

fn custom_tag(id: &str) -> String {
    format!("custom.{id}")
}

fn all_custom(db: &Db) -> Result<Vec<CustomLevel>> {
    Ok(db.get_setting(CUSTOM_KEY)?.unwrap_or_default())
}

/// The block a custom level plays.
pub fn custom_block(level: &CustomLevel) -> Result<BlockSpec> {
    let c = find_course(&level.course).ok_or_else(|| anyhow!("unknown course {}", level.course))?;
    let kind = c
        .levels
        .first()
        .and_then(|l| l.blocks.first())
        .map(exercise_of)
        .ok_or_else(|| anyhow!("{} has no exercise", level.course))?;
    let mut tags = vec![level.course.clone()];
    if !level.id.is_empty() {
        tags.push(custom_tag(&level.id));
    }
    let v = json!({
        "type": kind, "title": level.title, "count": level.count, "feedback": "after_trial",
        "tags": tags, "params": level.params,
    });
    serde_json::from_value(v).map_err(|e| anyhow!("those settings do not fit {kind}: {e}"))
}

/// Saves a custom level (new when its id is empty) after checking it generates.
pub fn save_custom_level(db: &Db, ctx: &GenContext, level: &CustomLevel) -> Result<CustomLevel> {
    if level.title.trim().is_empty() {
        return Err(anyhow!("give the level a name"));
    }
    if !(1..=200).contains(&level.count) {
        return Err(anyhow!("questions must be 1–200"));
    }
    let mut level = level.clone();
    if level.id.is_empty() {
        level.id = new_id();
    }
    let b = custom_block(&level)?;
    generate_block(&b, 1, ctx).map_err(|e| anyhow!("{}", e.0))?;
    let mut all = all_custom(db)?;
    match all.iter_mut().find(|l| l.id == level.id) {
        Some(l) => *l = level.clone(),
        None => all.push(level.clone()),
    }
    db.set_setting(CUSTOM_KEY, &all)?;
    Ok(level)
}

pub fn delete_custom_level(db: &Db, id: &str) -> Result<()> {
    let mut all = all_custom(db)?;
    all.retain(|l| l.id != id);
    db.set_setting(CUSTOM_KEY, &all)
}

/// A course's custom levels with their scores, in the order they were made.
pub fn custom_levels(db: &Db, course: &str) -> Result<Vec<CustomLevelView>> {
    let mut out = Vec::new();
    for l in all_custom(db)?.into_iter().filter(|l| l.course == course) {
        let trials = tagged_trials(db, &custom_tag(&l.id), 50_000)?;
        let full: Vec<(f64, u32, String)> = runs(&trials)
            .into_iter()
            .filter(|r| r.1 >= MIN_RUN.min(l.count.max(1)))
            .collect();
        out.push(CustomLevelView {
            runs: full.len() as u32,
            best: full.iter().map(|r| r.0).reduce(f64::max),
            last: full.last().map(|r| r.0),
            last_at: full.last().map(|r| r.2.clone()),
            level: l,
        });
    }
    Ok(out)
}

/// The session for a custom level, saved or not (an unsaved one is a one-off try).
pub fn custom_session(level: &CustomLevel) -> Result<SessionSpec> {
    let c = find_course(&level.course).ok_or_else(|| anyhow!("unknown course {}", level.course))?;
    Ok(SessionSpec {
        title: format!("{}: {}", c.title, level.title),
        protocol: Some(format!("custom.{}", level.course)),
        planned_minutes: None,
        notes: None,
        seed: None,
        blocks: vec![custom_block(level)?],
    })
}

pub fn find_custom_level(db: &Db, id: &str) -> Result<CustomLevel> {
    all_custom(db)?
        .into_iter()
        .find(|l| l.id == id)
        .ok_or_else(|| anyhow!("unknown custom level {id}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::courses::CHECK_TAG;
    use crate::rp_courses::{RP_DEGREES, RP_INTERVALS};
    use crate::storage::tests_support::sample_trial;

    fn trial(session: &str, i: u32, tags: Vec<String>, correct: bool) -> TrialRow {
        let mut t = sample_trial(session, i, correct);
        t.tags = tags;
        t
    }

    fn session(db: &Db) -> String {
        let spec = SessionSpec {
            title: "t".into(),
            protocol: None,
            planned_minutes: None,
            notes: None,
            seed: None,
            blocks: vec![],
        };
        db.start_new_session(&spec, "test").unwrap()
    }

    #[test]
    fn lessons_group_by_module_with_scores() {
        let db = Db::open_in_memory().unwrap();
        let a = session(&db);
        for i in 0..20 {
            let tags = vec![format!("course.{RP_INTERVALS}.02"), CHECK_TAG.into()];
            db.insert_trial(&trial(&a, i, tags, i < 19)).unwrap();
        }
        let w = workshop_view(&db, RP_INTERVALS).unwrap();
        assert_eq!(w.modules.len(), 16);
        let l = &w.modules[0].lessons[1];
        assert_eq!(l.level, 2);
        assert_eq!(l.best, Some(0.95));
        assert!(l.passed);
        assert_eq!(w.modules[0].passed, 1);
        assert!(!w.modules[0].lessons[0].passed);
        assert_eq!(w.exercise, "interval_id");
    }

    #[test]
    fn custom_levels_save_check_and_score() {
        let db = Db::open_in_memory().unwrap();
        let ctx = GenContext::default();
        let bad = CustomLevel {
            id: String::new(),
            course: RP_DEGREES.into(),
            title: "Bad".into(),
            count: 20,
            params: json!({ "degrees": ["9"] }),
        };
        assert!(save_custom_level(&db, &ctx, &bad).is_err());
        let good = save_custom_level(
            &db,
            &ctx,
            &CustomLevel {
                title: "Low notes in G".into(),
                params: json!({ "keys": ["G"], "degrees": ["1", "2", "3"], "octaves": [-1] }),
                ..bad
            },
        )
        .unwrap();
        assert!(!good.id.is_empty());
        let s = session(&db);
        for i in 0..12 {
            db.insert_trial(&trial(&s, i, vec![custom_tag(&good.id)], i % 2 == 0))
                .unwrap();
        }
        let views = custom_levels(&db, RP_DEGREES).unwrap();
        assert_eq!(views.len(), 1);
        assert_eq!(views[0].best, Some(0.5));
        let spec = custom_session(&good).unwrap();
        assert!(spec.blocks[0].tags.contains(&custom_tag(&good.id)));
        delete_custom_level(&db, &good.id).unwrap();
        assert!(custom_levels(&db, RP_DEGREES).unwrap().is_empty());
    }
}
