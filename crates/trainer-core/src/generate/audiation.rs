//! Audiation exercises: tonal context (find home), tonal patterns and the pattern dialogue
//! (Gordon), working memory (N-back, melodic updating, same/different, probe), single-pass error
//! detection and mislabeled syllables (Karpinski, Chenette), cadences, and bass and guide-tone
//! singing. See docs/RELATIVE_PITCH.md.

use super::*;

// ---------------------------------------------------------------------------------------------
// Shared helpers
// ---------------------------------------------------------------------------------------------

/// Semitones above the tonic of degree ids ("1", "b3", "8" …), in order, without repeats.
fn degree_semis(ids: &[String]) -> Result<Vec<i32>, GenError> {
    let mut out = Vec::new();
    for d in ids {
        let s = if d.trim() == "8" {
            12
        } else {
            parse_degree(d).ok_or_else(|| {
                GenError(format!(
                    "unknown degree `{d}` (use 1–8 with an optional b or #)"
                ))
            })?
        };
        if !out.contains(&s) {
            out.push(s);
        }
    }
    out.sort();
    Ok(out)
}

/// The tonic's MIDI note that puts semitones `lo..=hi` above it inside `range`, or as near as
/// it gets.
fn place_tonic(tonic: PitchClass, lo: i32, hi: i32, range: NoteRange) -> i32 {
    let mut best = (i32::MAX, tonic.in_octave(4));
    for o in 1..=6 {
        let t = tonic.in_octave(o);
        let over = (range.low.0 - (t + lo)).max(0) + ((t + hi) - range.high.0).max(0);
        // Prefer the octave whose middle sits nearest the range's middle among those that fit.
        let mid = ((t + (lo + hi) / 2) - (range.low.0 + range.high.0) / 2).abs();
        let score = over * 100 + mid;
        if score < best.0 {
            best = (score, t);
        }
    }
    best.1
}

/// A scale index (0 = tonic, 7 = the tonic above, negative below) as semitones above the tonic.
fn index_semis(steps: &[i32], d: i32) -> i32 {
    let n = steps.len() as i32;
    12 * d.div_euclid(n) + steps[d.rem_euclid(n) as usize]
}

/// The tonic triad of a mode, as a short block chord under `tonic_midi`.
fn tonic_chord(tonic_midi: i32, third: i32, start_beat: f32, beats: f32) -> Vec<TargetNote> {
    let root = if tonic_midi >= 55 {
        tonic_midi - 12
    } else {
        tonic_midi
    };
    [0, third, 7]
        .iter()
        .map(|s| TargetNote {
            midi: root + s,
            start_beat,
            beats,
        })
        .collect()
}

fn third_of(mode: KeyMode) -> i32 {
    if mode == KeyMode::Major { 4 } else { 3 }
}

/// A sung trial with the fields these exercises do not use left empty.
#[allow(clippy::too_many_arguments)]
fn sung(
    exercise: ExerciseKind,
    prompt: String,
    tempo_bpm: f32,
    reference: Vec<TargetNote>,
    sing_start: f32,
    notes: Vec<TargetNote>,
    total: f32,
    sections: Vec<(String, f32, f32)>,
    timbre: &str,
    stimulus: serde_json::Value,
) -> SungTrial {
    SungTrial {
        exercise,
        prompt,
        notes,
        tempo_bpm,
        beats_per_bar: 4,
        sing_start_beat: sing_start,
        total_beats: total,
        reference_beats: reference,
        reference_timbre: timbre.to_string(),
        backing: Backing::None,
        backing_volume: 0.6,
        guide: false,
        drone_midi: None,
        lane_targets: LaneTargets::HiddenUntilDone,
        timed: false,
        stimulus,
        score: None,
        count_in_beat: None,
        sections: sections
            .into_iter()
            .map(|(label, start_beat, end_beat)| TimelineSection {
                label,
                start_beat,
                end_beat,
            })
            .collect(),
        follow_cursor: false,
        accompaniment: Vec::new(),
        just_cents: Vec::new(),
        silent: Vec::new(),
        reveal: None,
        free: None,
        score_until_beat: None,
    }
}

fn key_name(tonic: PitchClass, mode: KeyMode) -> String {
    format!(
        "{} {}",
        tonic.display_name(),
        if mode == KeyMode::Major {
            "major"
        } else {
            "minor"
        }
    )
}

/// Names of semitone offsets above the tonic, joined: "do mi sol".
fn names(labels: DegreeLabels, tonic: PitchClass, mode: KeyMode, semis: &[i32]) -> String {
    semis
        .iter()
        .map(|s| note_in_key(labels, tonic, mode, *s))
        .collect::<Vec<_>>()
        .join(" ")
}

/// A walk over scale indexes drawn from `allowed` (sorted), mostly by step, without repeating a
/// note straight away, starting on the tonic triad when it can.
fn index_walk(allowed: &[i32], len: usize, rng: &mut Rng) -> Vec<i32> {
    let starts: Vec<i32> = allowed
        .iter()
        .copied()
        .filter(|d| [0, 2, 4].contains(&d.rem_euclid(7)))
        .collect();
    let mut cur = *rng
        .choose(if starts.is_empty() { allowed } else { &starts })
        .unwrap();
    let mut out = vec![cur];
    while out.len() < len {
        let pos = allowed.iter().position(|d| *d == cur).unwrap_or(0) as i32;
        let near: Vec<i32> = [-1, 1, -1, 1, -2, 2]
            .iter()
            .filter_map(|k| {
                usize::try_from(pos + k)
                    .ok()
                    .and_then(|i| allowed.get(i))
                    .copied()
            })
            .filter(|d| *d != cur)
            .collect();
        let next = if near.is_empty() || rng.chance(0.15) {
            *rng.choose(allowed).unwrap()
        } else {
            *rng.choose(&near).unwrap()
        };
        if next == cur {
            continue;
        }
        out.push(next);
        cur = next;
    }
    out
}

/// The scale step above or below `d` (within do to the upper do).
fn neighbour(d: i32, rng: &mut Rng) -> i32 {
    let up = d <= 0 || (d < 7 && rng.chance(0.5));
    if up { d + 1 } else { d - 1 }
}

/// Scale indexes of degree ids in a mode; an id outside the mode is an error.
fn degree_indexes(ids: &[String], mode: KeyMode) -> Result<Vec<i32>, GenError> {
    let steps = mode_steps(mode);
    let mut out = Vec::new();
    for s in degree_semis(ids)? {
        let i = if s == 12 {
            7
        } else {
            steps
                .iter()
                .position(|x| *x == s)
                .ok_or_else(|| {
                    GenError(format!(
                        "degree {} is not in the {} scale; use the mode's degrees (b3, b6 and b7 in minor)",
                        DEGREE_NUMBERS[s as usize],
                        if mode == KeyMode::Major { "major" } else { "minor" }
                    ))
                })? as i32
        };
        out.push(i);
    }
    if out.len() < 2 {
        return err("degrees needs at least two degrees");
    }
    Ok(out)
}

/// Transposes a list of key options per trial with balanced tonics.
fn keys_for(
    keys: &ClassSet,
    modes: &[KeyMode],
    change: KeyChange,
    n: usize,
    rng: &mut Rng,
) -> Result<Vec<(PitchClass, KeyMode)>, GenError> {
    key_sequence(keys, modes, change, n, rng)
}

// ---------------------------------------------------------------------------------------------
// Find home (resting tone)
// ---------------------------------------------------------------------------------------------

/// The resting-tone syllable of a tonality (movable do, la-based minor, as in Music Learning
/// Theory) and its name.
fn tonality(kind: ScaleKind) -> Option<(&'static str, &'static str)> {
    Some(match kind {
        ScaleKind::Major => ("do", "major"),
        ScaleKind::NaturalMinor => ("la", "minor"),
        ScaleKind::HarmonicMinor => ("la", "harmonic minor (raised si)"),
        ScaleKind::Dorian => ("re", "Dorian"),
        ScaleKind::Phrygian => ("mi", "Phrygian"),
        ScaleKind::Lydian => ("fa", "Lydian"),
        ScaleKind::Mixolydian => ("sol", "Mixolydian"),
        ScaleKind::Locrian => ("ti", "Locrian"),
        _ => return None,
    })
}

/// A phrase over a mode's scale that outlines its tonic triad, then stops on a note other than
/// the tonic: scale indexes, tonic 0.
fn tonal_phrase(len: usize, rng: &mut Rng) -> Vec<i32> {
    for _ in 0..50 {
        let mut d: i32 = *rng.choose(&[0, 2, 4, 0]).unwrap();
        let mut out = vec![d];
        while out.len() < len {
            let i = out.len();
            d = if i % 4 == 0 {
                // Strong positions land on the tonic triad.
                let near: Vec<i32> = [-3, 0, 2, 4, 7]
                    .into_iter()
                    .filter(|t| (*t - d).abs() <= 3 && *t != d)
                    .collect();
                *rng.choose(if near.is_empty() { &[0][..] } else { &near })
                    .unwrap()
            } else {
                let step = *rng.choose(&[-1, 1, -1, 1, 2, -2]).unwrap();
                (d + step).clamp(-3, 7)
            };
            if Some(&d) == out.last() {
                d = (d + 1).min(7);
            }
            out.push(d);
        }
        // Stop on a note that wants to move: 2, 7, 4 or 5 of the mode.
        let last = *out.last().unwrap();
        let ends: Vec<i32> = [-1, 1, 3, 4, 6]
            .into_iter()
            .filter(|e| (*e - last).abs() <= 2 && Some(e) != out.get(len.saturating_sub(2)))
            .collect();
        *out.last_mut().unwrap() = *rng
            .choose(if ends.is_empty() { &[1][..] } else { &ends })
            .unwrap();
        // At least two tonics, so the tonality can be heard.
        if out.iter().filter(|x| x.rem_euclid(7) == 0).count() >= 2 {
            return out;
        }
    }
    let mut v: Vec<i32> = (0..len as i32)
        .map(|i| [0, 2, 4, 2][i as usize % 4])
        .collect();
    *v.last_mut().unwrap() = 1;
    v
}

pub(super) fn gen_resting_tone(
    p: &RestingToneParams,
    n: usize,
    rng: &mut Rng,
    ctx: &GenContext,
) -> Result<Vec<TrialSpec>, GenError> {
    if p.modes.is_empty() {
        return err("modes must not be empty");
    }
    for m in &p.modes {
        if tonality(*m).is_none() {
            return err(format!(
                "{} has no resting tone here; use major, natural_minor, harmonic_minor, dorian, phrygian, lydian, mixolydian or locrian",
                m.id()
            ));
        }
    }
    if !(4..=16).contains(&p.notes) {
        return err("notes must be 4–16");
    }
    if p.answer == RestingAnswer::Name
        && p.modes
            .iter()
            .map(|m| m.id())
            .collect::<std::collections::BTreeSet<_>>()
            .len()
            < 2
    {
        return err("naming the resting tone needs at least two tonalities to choose from");
    }
    if !(40.0..=200.0).contains(&p.tempo_bpm) {
        return err("tempo_bpm must be 40–200");
    }
    let mut kinds: Vec<ScaleKind> = Vec::new();
    for m in &p.modes {
        if !kinds.contains(m) {
            kinds.push(*m);
        }
    }
    let mut mode_bag = BalancedBag::allowing_repeats(kinds.clone());
    let mut key_bag = BalancedBag::allowing_repeats(p.keys.classes());
    let range = ctx.comfortable;
    let beat = 60_000.0 / p.tempo_bpm as f64;
    let mut out = Vec::with_capacity(n);
    for _ in 0..n {
        let kind = mode_bag.draw(rng).unwrap();
        let tonic = key_bag.draw(rng).unwrap();
        let (syllable, name) = tonality(kind).unwrap();
        let steps = kind.steps();
        let t = place_tonic(tonic, -5, 12, range);
        let phrase = tonal_phrase(p.notes as usize, rng);
        let midis: Vec<i32> = phrase.iter().map(|d| t + index_semis(steps, *d)).collect();
        let timbre = pick_timbre(&p.timbres, rng)?;
        let stimulus = json!({
            "tonality": kind.id(), "tonic": tonic.name(), "resting_syllable": syllable,
            "phrase": midis, "answer": p.answer,
        });
        let reveal = format!("The resting tone was {syllable}: {name}.");
        match p.answer {
            RestingAnswer::Sing => {
                let len = midis.len() as f32;
                let reference: Vec<TargetNote> = midis
                    .iter()
                    .enumerate()
                    .map(|(i, m)| TargetNote {
                        midi: *m,
                        start_beat: i as f32,
                        beats: 0.92,
                    })
                    .collect();
                let sing = len + 1.5;
                // The tonic nearest the last notes, so any octave reads naturally.
                let home = [t - 12, t, t + 12]
                    .into_iter()
                    .min_by_key(|x| (x - midis[midis.len() - 1]).abs())
                    .unwrap();
                let mut trial = sung(
                    ExerciseKind::RestingTone,
                    "Listen; when it stops, breathe and sing the resting tone on “bum”".into(),
                    p.tempo_bpm,
                    reference,
                    sing,
                    vec![TargetNote {
                        midi: home,
                        start_beat: 0.0,
                        beats: 2.5,
                    }],
                    sing + 3.0,
                    vec![
                        ("Listen".into(), 0.0, len),
                        ("Breathe".into(), len, sing),
                        ("Sing the resting tone".into(), sing, sing + 3.0),
                    ],
                    &timbre,
                    stimulus,
                );
                trial.reveal = Some(reveal);
                out.push(TrialSpec::Sung(trial));
            }
            RestingAnswer::Name => {
                let events: Vec<SoundEvent> = midis
                    .iter()
                    .enumerate()
                    .map(|(i, m)| ev(*m, i as f64 * beat, beat * 0.92, &timbre, -6.0))
                    .collect();
                let end = midis.len() as f64 * beat;
                // Afterwards the phrase walks home.
                let last = *phrase.last().unwrap();
                let home = if last.rem_euclid(7) >= 4 || last < 0 {
                    last + (7 - last.rem_euclid(7))
                } else {
                    last - last.rem_euclid(7)
                };
                let walk: Vec<i32> = if home > last {
                    (last..=home).collect()
                } else {
                    (home..=last).rev().collect()
                };
                let after: Vec<SoundEvent> = walk
                    .iter()
                    .enumerate()
                    .map(|(i, d)| {
                        let last = i + 1 == walk.len();
                        ev(
                            t + index_semis(steps, *d),
                            i as f64 * beat * 0.6,
                            if last { beat * 2.0 } else { beat * 0.55 },
                            &timbre,
                            -6.0,
                        )
                    })
                    .collect();
                let choices = kinds
                    .iter()
                    .map(|k| {
                        let (s, nm) = tonality(*k).unwrap();
                        Choice::new(k.id(), format!("{s} · {nm}"))
                    })
                    .collect();
                out.push(TrialSpec::Identify(IdentifyTrial {
                    exercise: ExerciseKind::RestingTone,
                    prompt: "Where is home? Name the resting tone".into(),
                    events,
                    choices,
                    correct: kind.id().into(),
                    answer_after_ms: Some(end + beat),
                    after_answer: Some(after),
                    explain: Some(reveal),
                    stimulus,
                    ..IdentifyTrial::skeleton()
                }));
            }
        }
    }
    Ok(out)
}

// ---------------------------------------------------------------------------------------------
// Tonal patterns (Gordon)
// ---------------------------------------------------------------------------------------------

/// Chord tones of a function, in semitones from the tonic, over the range a pattern may use.
fn function_tones(f: PatternFunction, mode: KeyMode) -> Vec<i32> {
    let minor = mode == KeyMode::Minor;
    match (f, minor) {
        (PatternFunction::Tonic, false) => vec![-5, 0, 4, 7, 12],
        (PatternFunction::Tonic, true) => vec![-5, 0, 3, 7, 12],
        // V7: sol ti re fa (the raised seventh in minor).
        (PatternFunction::Dominant, _) => vec![-5, -1, 2, 5, 7, 11],
        (PatternFunction::Subdominant, false) => vec![-7, -3, 0, 5, 9, 12],
        (PatternFunction::Subdominant, true) => vec![-7, -4, 0, 5, 8, 12],
        (PatternFunction::Tendency, _) => Vec::new(),
    }
}

/// Two-note resolutions of a mode's tendency tones, (from, to) in semitones from the tonic.
fn tendencies(mode: KeyMode) -> Vec<(i32, i32)> {
    match mode {
        KeyMode::Major => vec![
            (-1, 0),
            (5, 4),
            (2, 0),
            (9, 7),
            (6, 7),
            (10, 9),
            (1, 0),
            (8, 7),
        ],
        KeyMode::Minor => vec![(-1, 0), (5, 3), (2, 0), (8, 7), (6, 7), (1, 0)],
    }
}

fn function_name(f: PatternFunction) -> &'static str {
    match f {
        PatternFunction::Tonic => "tonic",
        PatternFunction::Dominant => "dominant",
        PatternFunction::Subdominant => "subdominant",
        PatternFunction::Tendency => "tendency",
    }
}

/// A pattern of `len` chord tones of `f`: different neighbours, within an octave and a third.
fn make_pattern(f: PatternFunction, mode: KeyMode, len: usize, rng: &mut Rng) -> Vec<i32> {
    if f == PatternFunction::Tendency {
        let (a, b) = *rng.choose(&tendencies(mode)).unwrap();
        return vec![a, b];
    }
    let tones = function_tones(f, mode);
    let central: Vec<i32> = tones
        .iter()
        .copied()
        .filter(|t| (-1..=7).contains(t))
        .collect();
    for _ in 0..40 {
        let mut out = vec![*rng.choose(&central).unwrap()];
        while out.len() < len {
            let cur = *out.last().unwrap();
            let next: Vec<i32> = tones
                .iter()
                .copied()
                .filter(|t| *t != cur && (t - cur).abs() <= 8)
                .collect();
            out.push(*rng.choose(&next).unwrap());
        }
        let span = out.iter().max().unwrap() - out.iter().min().unwrap();
        if span <= 15 {
            return out;
        }
    }
    tones.into_iter().take(len).collect()
}

pub(super) fn gen_tonal_pattern(
    p: &TonalPatternParams,
    n: usize,
    rng: &mut Rng,
    ctx: &GenContext,
) -> Result<Vec<TrialSpec>, GenError> {
    if p.functions.is_empty() || p.lengths.is_empty() {
        return err("functions and lengths must not be empty");
    }
    if p.lengths.iter().any(|l| !(2..=5).contains(l)) {
        return err("lengths must be 2–5");
    }
    if !(0.0..=8.0).contains(&p.breath_beats) {
        return err("breath_beats must be 0–8");
    }
    check_timbres(std::slice::from_ref(&p.timbre))?;
    let keys = keys_for(&p.keys, &p.modes, p.key_change, n, rng)?;
    let range = sung_range(p.range, ctx);
    let mut fbag = BalancedBag::allowing_repeats(p.functions.clone());
    let mut out = Vec::with_capacity(n);
    let mut current: Option<(PatternFunction, Vec<i32>)> = None;
    for (i, (tonic, mode)) in keys.into_iter().enumerate() {
        let teaching = match p.passes {
            PatternPasses::Teaching => true,
            PatternPasses::Evaluation => false,
            PatternPasses::TeachingThenEvaluation => i % 2 == 0,
        };
        // A new pattern for each teaching pass (or each trial without pairs).
        let fresh =
            current.is_none() || p.passes != PatternPasses::TeachingThenEvaluation || teaching;
        if fresh {
            let f = fbag.draw(rng).unwrap();
            let len = *rng.choose(&p.lengths).unwrap() as usize;
            let mut pat = make_pattern(f, mode, len, rng);
            if current.as_ref().is_some_and(|(_, prev)| *prev == pat) {
                pat = make_pattern(f, mode, len, rng);
            }
            current = Some((f, pat));
        }
        let (f, pat) = current.clone().unwrap();
        let lo = *pat.iter().min().unwrap();
        let hi = *pat.iter().max().unwrap();
        let t = place_tonic(tonic, lo, hi, range);
        let len = pat.len() as f32;
        let mut reference = tonic_chord(t, third_of(mode), 0.0, 1.5);
        for (k, s) in pat.iter().enumerate() {
            reference.push(TargetNote {
                midi: t + s,
                start_beat: 2.0 + k as f32,
                beats: 0.92,
            });
        }
        let sing = 2.0 + len + p.breath_beats.max(0.5);
        let notes: Vec<TargetNote> = pat
            .iter()
            .enumerate()
            .map(|(k, s)| TargetNote {
                midi: t + s,
                start_beat: k as f32,
                beats: 0.92,
            })
            .collect();
        let syl = names(p.labels, tonic, mode, &pat);
        let prompt = match p.syllables {
            PatternSyllables::Neutral => "Echo it on “bum”".to_string(),
            PatternSyllables::Solfege => format!("Echo in solfège: {syl}"),
            PatternSyllables::Translate => "You hear it on “bum”: echo it in solfège".to_string(),
        };
        let sing_label = if teaching {
            "Sing (tonic sounding)"
        } else {
            "Sing alone"
        };
        let mut trial = sung(
            ExerciseKind::TonalPattern,
            prompt,
            p.tempo_bpm,
            reference,
            sing,
            notes,
            sing + len + 1.0,
            vec![
                ("Key".into(), 0.0, 1.5),
                ("Listen".into(), 2.0, 2.0 + len),
                ("Breathe and audiate".into(), 2.0 + len, sing),
                (sing_label.into(), sing, sing + len + 1.0),
            ],
            &p.timbre,
            json!({
                "function": f, "mode": mode, "key": tonic.name(), "pattern": pat,
                "pass": if teaching { "teaching" } else { "evaluation" }, "syllables": p.syllables,
            }),
        );
        if teaching {
            trial.drone_midi = Some(if t - 12 >= 36 { t - 12 } else { t });
        }
        trial.reveal = Some(match p.syllables {
            PatternSyllables::Neutral => format!("A {} pattern.", function_name(f)),
            _ => format!("It was {syl}, a {} pattern.", function_name(f)),
        });
        out.push(TrialSpec::Sung(trial));
    }
    Ok(out)
}

// ---------------------------------------------------------------------------------------------
// Pattern dialogue
// ---------------------------------------------------------------------------------------------

pub(super) fn gen_pattern_dialogue(
    p: &PatternDialogueParams,
    n: usize,
    rng: &mut Rng,
    ctx: &GenContext,
) -> Result<Vec<TrialSpec>, GenError> {
    check_timbres(std::slice::from_ref(&p.timbre))?;
    let keys = keys_for(&p.keys, &p.modes, p.key_change, n, rng)?;
    let range = sung_range(p.range, ctx);
    let funcs = [PatternFunction::Tonic, PatternFunction::Dominant];
    let mut fbag = BalancedBag::allowing_repeats(funcs.to_vec());
    let mut out = Vec::with_capacity(n);
    for (tonic, mode) in keys {
        let f = fbag.draw(rng).unwrap();
        let first_pitch = p.stage == DialogueStage::FirstPitch;
        let len = if first_pitch {
            4
        } else {
            *rng.choose(&[3usize, 4]).unwrap()
        };
        let pat = make_pattern(f, mode, len, rng);
        let t = place_tonic(tonic, -5, 12, range);
        // Quick in the first-pitch stage: two notes a beat.
        let step = if first_pitch { 0.5 } else { 1.0 };
        let mut reference = tonic_chord(t, third_of(mode), 0.0, 1.5);
        for (k, s) in pat.iter().enumerate() {
            reference.push(TargetNote {
                midi: t + s,
                start_beat: 2.0 + k as f32 * step,
                beats: step * 0.9,
            });
        }
        let end = 2.0 + len as f32 * step;
        let sing = end + p.breath_beats.max(0.5);
        let label = |s: i32| note_in_key(p.labels, tonic, mode, s);
        let stimulus = json!({ "stage": p.stage, "function": f, "mode": mode, "key": tonic.name(), "pattern": pat });
        let trial = match p.stage {
            DialogueStage::FirstPitch => {
                let mut tr = sung(
                    ExerciseKind::PatternDialogue,
                    "Sing back only the first note of the pattern".into(),
                    p.tempo_bpm,
                    reference,
                    sing,
                    vec![TargetNote {
                        midi: t + pat[0],
                        start_beat: 0.0,
                        beats: 2.0,
                    }],
                    sing + 3.0,
                    vec![
                        ("Listen".into(), 2.0, end),
                        ("Breathe".into(), end, sing),
                        ("Sing the first note".into(), sing, sing + 3.0),
                    ],
                    &p.timbre,
                    stimulus,
                );
                tr.reveal = Some(format!(
                    "The pattern was {}; it began on {}.",
                    names(p.labels, tonic, mode, &pat),
                    label(pat[0])
                ));
                tr
            }
            DialogueStage::SameFunction | DialogueStage::OppositeFunction => {
                let answer = if p.stage == DialogueStage::SameFunction {
                    f
                } else if f == PatternFunction::Tonic {
                    PatternFunction::Dominant
                } else {
                    PatternFunction::Tonic
                };
                let tones = function_tones(answer, mode);
                let mut allowed: Vec<i32> = tones
                    .iter()
                    .map(|s| (tonic.index() as i32 + s).rem_euclid(12))
                    .collect();
                allowed.sort();
                allowed.dedup();
                let names_pc: Vec<(i32, String)> = allowed
                    .iter()
                    .map(|pc| (*pc, label(pc - tonic.index() as i32)))
                    .collect();
                let what = if p.stage == DialogueStage::SameFunction {
                    format!("a different {} pattern", function_name(answer))
                } else {
                    format!("a {} pattern", function_name(answer))
                };
                let mut tr = sung(
                    ExerciseKind::PatternDialogue,
                    format!("Answer with {what} of 3 or 4 notes"),
                    p.tempo_bpm,
                    reference,
                    sing,
                    Vec::new(),
                    sing + 6.0,
                    vec![
                        ("Listen".into(), 2.0, end),
                        ("Breathe".into(), end, sing),
                        (format!("Answer: {what}"), sing, sing + 6.0),
                    ],
                    &p.timbre,
                    stimulus,
                );
                tr.free = Some(FreeResponse {
                    allowed,
                    names: names_pc,
                    min_notes: 3,
                    max_notes: 5,
                    differ_from: if p.stage == DialogueStage::SameFunction {
                        pat.iter()
                            .map(|s| (tonic.index() as i32 + s).rem_euclid(12))
                            .collect()
                    } else {
                        Vec::new()
                    },
                    what,
                });
                tr.reveal = Some(format!(
                    "It sang {}, a {} pattern.",
                    names(p.labels, tonic, mode, &pat),
                    function_name(f)
                ));
                tr
            }
        };
        out.push(TrialSpec::Sung(trial));
    }
    Ok(out)
}

// ---------------------------------------------------------------------------------------------
// Musical N-back
// ---------------------------------------------------------------------------------------------

/// A stream of degrees with matches `n` back at about `rate`; returns the degrees and which match.
fn n_back_stream(
    degrees: &[i32],
    n: usize,
    len: usize,
    rate: f32,
    rng: &mut Rng,
) -> (Vec<i32>, Vec<bool>) {
    let mut seq: Vec<i32> = Vec::with_capacity(len);
    let mut target = Vec::with_capacity(len);
    for i in 0..len {
        let back = (i >= n).then(|| seq[i - n]);
        let hit = back.is_some() && rng.chance(rate as f64);
        let d = if hit {
            back.unwrap()
        } else {
            let others: Vec<i32> = degrees
                .iter()
                .copied()
                .filter(|d| Some(*d) != back && Some(d) != seq.last())
                .collect();
            *rng.choose(if others.is_empty() { degrees } else { &others })
                .unwrap()
        };
        target.push(hit);
        seq.push(d);
    }
    (seq, target)
}

pub(super) fn gen_n_back(
    p: &NBackParams,
    n: usize,
    rng: &mut Rng,
    ctx: &GenContext,
) -> Result<Vec<TrialSpec>, GenError> {
    if !(1..=3).contains(&p.n) {
        return err("n must be 1–3");
    }
    if !(6..=60).contains(&p.length) {
        return err("length must be 6–60");
    }
    if !(800..=5000).contains(&p.ioi_ms) {
        return err("ioi_ms must be 800–5000");
    }
    if !(0.1..=0.6).contains(&p.target_rate) {
        return err("target_rate must be 0.1–0.6");
    }
    check_timbres(std::slice::from_ref(&p.timbre))?;
    let semis = degree_semis(&p.degrees)?;
    if semis.len() < 2 {
        return err("degrees needs at least two degrees");
    }
    let keys = keys_for(&p.keys, &p.modes, KeyChange::PerTrial, n, rng)?;
    let range = sung_range(p.range, ctx);
    let back = p.n as usize;
    let mut out = Vec::with_capacity(n);
    for (tonic, mode) in keys {
        let lo = *semis.first().unwrap();
        let hi = *semis.last().unwrap();
        let t = place_tonic(tonic, lo, hi, range);
        let (seq, targets) = n_back_stream(&semis, back, p.length as usize, p.target_rate, rng);
        let ids: Vec<&str> = seq
            .iter()
            .map(|s| {
                if *s == 12 {
                    "8"
                } else {
                    DEGREE_IDS[*s as usize]
                }
            })
            .collect();
        let drone = if t - 12 >= 36 { t - 12 } else { t };
        let stimulus = json!({
            "n": back, "key": tonic.name(), "mode": mode, "degrees": ids,
            "matches": targets.iter().filter(|x| **x).count(), "mode_of_answer": p.mode,
        });
        match p.mode {
            NBackMode::Recognize => {
                let ioi = p.ioi_ms as f64;
                let tones = seq
                    .iter()
                    .zip(&targets)
                    .map(|(s, hit)| {
                        let mut events = vec![ev(t + s, 0.0, ioi * 0.8, &p.timbre, -6.0)];
                        if p.drone {
                            events.push(ev(drone, 0.0, ioi, "gm:52", -20.0));
                        }
                        DetectTone {
                            events,
                            target: *hit,
                            far: false,
                            midi: t + s,
                        }
                    })
                    .collect();
                let prompt_events = cadence_events(tonic, mode, 100.0, &p.timbre, 0.0);
                out.push(TrialSpec::Detect(DetectTrial {
                    exercise: ExerciseKind::NBack,
                    prompt: format!(
                        "Tap Match (space) when the note is the same as {}",
                        if back == 1 {
                            "the one before".to_string()
                        } else {
                            format!("the one {back} back")
                        }
                    ),
                    target_class: tonic,
                    target_label: if back == 1 {
                        "Same as the last".into()
                    } else {
                        format!("Same as {back} back")
                    },
                    target_color: None,
                    prompt_events,
                    prompt_ms: 4200,
                    speech: None,
                    tones,
                    window_ms: p.ioi_ms,
                    marks: p.marks,
                    stimulus,
                }));
            }
            NBackMode::Sing => {
                // One note a beat; the voice follows `back` notes behind.
                let tempo = (60_000.0 / p.ioi_ms as f64) as f32;
                let mut reference = Vec::new();
                for (k, (bass, upper)) in cadence_chords(tonic, mode).iter().enumerate() {
                    let beats = if k == 3 { 1.5 } else { 0.9 };
                    reference.push(TargetNote {
                        midi: *bass,
                        start_beat: k as f32,
                        beats,
                    });
                    for m in upper {
                        reference.push(TargetNote {
                            midi: *m,
                            start_beat: k as f32,
                            beats,
                        });
                    }
                }
                let sing = 5.0;
                let len = seq.len() as f32;
                let notes: Vec<TargetNote> = seq
                    .iter()
                    .enumerate()
                    .map(|(i, s)| TargetNote {
                        midi: t + s,
                        start_beat: (i + back) as f32,
                        beats: 0.9,
                    })
                    .collect();
                let total = sing + len + back as f32 + 1.0;
                let mut tr = sung(
                    ExerciseKind::NBack,
                    format!(
                        "Canon: as each note sounds, sing the one {} (headphones)",
                        if back == 1 {
                            "before it".to_string()
                        } else {
                            format!("{back} before it")
                        }
                    ),
                    tempo,
                    reference,
                    sing,
                    notes,
                    total,
                    vec![
                        ("Key".into(), 0.0, 4.0),
                        ("Listen".into(), sing, sing + back as f32),
                        (format!("Sing {back} behind"), sing + back as f32, total),
                    ],
                    &p.timbre,
                    stimulus,
                );
                tr.accompaniment = seq
                    .iter()
                    .enumerate()
                    .map(|(i, s)| TargetNote {
                        midi: t + s,
                        start_beat: i as f32,
                        beats: 0.85,
                    })
                    .collect();
                tr.timed = true;
                if p.drone {
                    tr.drone_midi = Some(drone);
                }
                out.push(TrialSpec::Sung(tr));
            }
        }
    }
    Ok(out)
}

// ---------------------------------------------------------------------------------------------
// Melodic updating
// ---------------------------------------------------------------------------------------------

/// One change to a melody of scale indexes (None for a left-out note), with its instruction.
fn apply_update(
    op: UpdateOp,
    mel: &mut [Option<i32>],
    name: &dyn Fn(i32) -> String,
    rng: &mut Rng,
) -> Option<String> {
    let sung: Vec<usize> = (0..mel.len()).filter(|i| mel[*i].is_some()).collect();
    match op {
        UpdateOp::ReplaceDegree => {
            let present: Vec<i32> = sung.iter().map(|i| mel[*i].unwrap()).collect();
            let x = *rng.choose(&present)?;
            let options: Vec<i32> = [x - 1, x + 1]
                .into_iter()
                .filter(|y| (0..=7).contains(y))
                .collect();
            let y = *rng.choose(&options)?;
            for m in mel.iter_mut() {
                if *m == Some(x) {
                    *m = Some(y);
                }
            }
            Some(format!("every {} becomes {}", name(x), name(y)))
        }
        UpdateOp::ReplacePosition => {
            let k = *rng.choose(&sung)?;
            let x = mel[k].unwrap();
            let options: Vec<i32> = (0..=7).filter(|y| *y != x && (y - x).abs() <= 3).collect();
            let y = *rng.choose(&options)?;
            mel[k] = Some(y);
            Some(format!("note {} becomes {}", k + 1, name(y)))
        }
        UpdateOp::Shift => {
            let max = sung.iter().map(|i| mel[*i].unwrap()).max()?;
            let min = sung.iter().map(|i| mel[*i].unwrap()).min()?;
            let up = if max >= 7 {
                false
            } else if min <= 0 {
                true
            } else {
                rng.chance(0.5)
            };
            let d = if up { 1 } else { -1 };
            for m in mel.iter_mut().flatten() {
                *m += d;
            }
            let first = mel.iter().flatten().next().copied()?;
            Some(format!(
                "move it all {} a step (it starts on {})",
                if up { "up" } else { "down" },
                name(first)
            ))
        }
        UpdateOp::Retrograde => {
            mel.reverse();
            Some("sing it backwards".into())
        }
        UpdateOp::Omit => {
            if sung.len() < 3 {
                return None;
            }
            let k = *rng.choose(&sung[1..])?;
            mel[k] = None;
            Some(format!("leave out note {} (keep its beat silent)", k + 1))
        }
    }
}

pub(super) fn gen_melodic_updating(
    p: &MelodicUpdatingParams,
    n: usize,
    rng: &mut Rng,
    ctx: &GenContext,
) -> Result<Vec<TrialSpec>, GenError> {
    if p.notes.is_empty() || p.notes.iter().any(|l| !(3..=8).contains(l)) {
        return err("notes must be 3–8");
    }
    if !(1..=3).contains(&p.changes) {
        return err("changes must be 1–3");
    }
    if p.operations.is_empty() {
        return err("operations must not be empty");
    }
    if !(1.0..=16.0).contains(&p.think_beats) {
        return err("think_beats must be 1–16");
    }
    check_timbres(std::slice::from_ref(&p.timbre))?;
    let keys = keys_for(&p.keys, &p.modes, p.key_change, n, rng)?;
    let range = sung_range(p.range, ctx);
    let mut out = Vec::with_capacity(n);
    for (tonic, mode) in keys {
        let allowed = degree_indexes(&p.degrees, mode)?;
        let steps = mode_steps(mode);
        let semis = |d: i32| index_semis(&steps, d);
        let len = *rng.choose(&p.notes).unwrap() as usize;
        let original = index_walk(&allowed, len, rng);
        let name = |d: i32| note_in_key(p.labels, tonic, mode, semis(d));
        let mut mel: Vec<Option<i32>> = original.iter().map(|d| Some(*d)).collect();
        let mut said = Vec::new();
        let mut ops = p.operations.clone();
        for _ in 0..p.changes {
            for _ in 0..8 {
                let op = *rng.choose(&ops).unwrap();
                if let Some(text) = apply_update(op, &mut mel, &name, rng) {
                    said.push(text);
                    // Different changes when chaining, if there are others.
                    if ops.len() > 1 {
                        ops.retain(|o| *o != op);
                    }
                    break;
                }
            }
        }
        let all: Vec<i32> = original
            .iter()
            .copied()
            .chain(mel.iter().flatten().copied())
            .collect();
        let t = place_tonic(
            tonic,
            semis(*all.iter().min().unwrap()),
            semis(*all.iter().max().unwrap()),
            range,
        );
        let l = len as f32;
        let mut reference = tonic_chord(t, third_of(mode), 0.0, 1.5);
        for (k, d) in original.iter().enumerate() {
            reference.push(TargetNote {
                midi: t + semis(*d),
                start_beat: 2.0 + k as f32,
                beats: 0.92,
            });
        }
        let sing = 2.0 + l + p.think_beats;
        let notes: Vec<TargetNote> = mel
            .iter()
            .enumerate()
            .filter_map(|(k, d)| {
                d.map(|d| TargetNote {
                    midi: t + semis(d),
                    start_beat: k as f32,
                    beats: 0.92,
                })
            })
            .collect();
        let instruction = {
            let mut s = said.join(", then ");
            if let Some(c) = s.get_mut(0..1) {
                c.make_ascii_uppercase();
            }
            s
        };
        let changed: Vec<String> = mel
            .iter()
            .map(|d| d.map(&name).unwrap_or_else(|| "–".into()))
            .collect();
        let mut tr = sung(
            ExerciseKind::MelodicUpdating,
            "Listen, change it in your head as told, then sing the new melody".into(),
            p.tempo_bpm,
            reference,
            sing,
            notes,
            sing + l + 1.0,
            vec![
                ("Listen".into(), 2.0, 2.0 + l),
                (
                    format!("{instruction}: rehearse it silently"),
                    2.0 + l,
                    sing,
                ),
                ("Sing the changed melody".into(), sing, sing + l + 1.0),
            ],
            &p.timbre,
            json!({
                "key": tonic.name(), "mode": mode, "original": original, "changed": mel,
                "changes": said,
            }),
        );
        tr.reveal = Some(format!(
            "{} → {}",
            original
                .iter()
                .map(|d| name(*d))
                .collect::<Vec<_>>()
                .join(" "),
            changed.join(" ")
        ));
        out.push(TrialSpec::Sung(tr));
    }
    Ok(out)
}

// ---------------------------------------------------------------------------------------------
// Same/different and probe
// ---------------------------------------------------------------------------------------------

pub(super) fn gen_pattern_memory(
    p: &PatternMemoryParams,
    n: usize,
    rng: &mut Rng,
) -> Result<Vec<TrialSpec>, GenError> {
    if p.notes.is_empty() || p.notes.iter().any(|l| !(3..=8).contains(l)) {
        return err("notes must be 3–8");
    }
    if !(300..=6000).contains(&p.gap_ms) {
        return err("gap_ms must be 300–6000");
    }
    let keys = keys_for(&p.keys, &p.modes, p.key_change, n, rng)?;
    let beat = 60_000.0 / p.tempo_bpm.clamp(40.0, 220.0) as f64;
    let mut answers = BalancedBag::allowing_repeats(vec![true, false]);
    let mut out = Vec::with_capacity(n);
    for (tonic, mode) in keys {
        let allowed = degree_indexes(&p.degrees, mode)?;
        let steps = mode_steps(mode);
        let semis = |d: i32| index_semis(&steps, d);
        let name = |s: i32| note_in_key(p.labels, tonic, mode, s);
        let len = *rng.choose(&p.notes).unwrap() as usize;
        let mel = index_walk(&allowed, len, rng);
        let t = place_tonic(tonic, 0, 12, NoteRange::new(55, 76));
        let timbre = pick_timbre(&p.timbres, rng)?;
        let yes = answers.draw(rng).unwrap();
        let mut events: Vec<SoundEvent> = tonic_chord(t, third_of(mode), 0.0, 1.0)
            .iter()
            .map(|c| ev(c.midi, 0.0, beat, &timbre, -10.0))
            .collect();
        let start = beat * 1.6;
        let play = |midis: &[i32], at: f64, events: &mut Vec<SoundEvent>| {
            for (k, m) in midis.iter().enumerate() {
                events.push(ev(*m, at + k as f64 * beat, beat * 0.9, &timbre, -6.0));
            }
        };
        let first: Vec<i32> = mel.iter().map(|d| t + semis(*d)).collect();
        play(&first, start, &mut events);
        let after_first = start + len as f64 * beat + p.gap_ms as f64;
        let (prompt, choices, correct, explain, end) = match p.task {
            MemoryTask::SameDifferent => {
                let shift = if p.transpose {
                    *rng.choose(&[-5, -3, -2, 2, 3, 5]).unwrap()
                } else {
                    0
                };
                let mut second: Vec<i32> = first.iter().map(|m| m + shift).collect();
                let mut explain = "They were the same.".to_string();
                if !yes {
                    let k = rng.range_i32(1, len as i32 - 1) as usize;
                    let chromatic = match p.change {
                        ChangeSize::Step => false,
                        ChangeSize::Chromatic => true,
                        ChangeSize::Any => rng.chance(0.5),
                    };
                    let old = mel[k];
                    let outside: Vec<i32> = [semis(old) - 1, semis(old) + 1]
                        .into_iter()
                        .filter(|s| !steps.contains(&s.rem_euclid(12)))
                        .collect();
                    let new_semis = if chromatic && !outside.is_empty() {
                        *rng.choose(&outside).unwrap()
                    } else {
                        semis(neighbour(old, rng))
                    };
                    second[k] = t + new_semis + shift;
                    explain = format!(
                        "Note {} changed: {} → {}.",
                        k + 1,
                        name(semis(old)),
                        name(new_semis)
                    );
                }
                play(&second, after_first, &mut events);
                (
                    if p.transpose {
                        "Same melody (in another key), or one note changed?"
                    } else {
                        "The same, or one note changed?"
                    },
                    vec![
                        Choice::new("same", "Same"),
                        Choice::new("different", "Different"),
                    ],
                    if yes { "same" } else { "different" },
                    explain,
                    after_first + len as f64 * beat,
                )
            }
            MemoryTask::Probe => {
                let probe = if yes {
                    *rng.choose(&mel).unwrap()
                } else {
                    let near: Vec<i32> = (mel.iter().min().unwrap() - 1
                        ..=mel.iter().max().unwrap() + 1)
                        .filter(|d| !mel.contains(d) && (-2..=8).contains(d))
                        .collect();
                    if near.is_empty() {
                        mel.iter().max().unwrap() + 1
                    } else {
                        *rng.choose(&near).unwrap()
                    }
                };
                // Is the probe in the melody? A different octave of the same degree is not.
                let present = mel.contains(&probe);
                events.push(ev(t + semis(probe), after_first, beat * 1.2, &timbre, -6.0));
                let pos = mel.iter().position(|d| *d == probe);
                (
                    "Was this note in the melody?",
                    vec![Choice::new("yes", "Yes, it was"), Choice::new("no", "No")],
                    if present { "yes" } else { "no" },
                    match pos {
                        Some(k) => format!("{} was note {}.", name(semis(probe)), k + 1),
                        None => format!("{} was not in it.", name(semis(probe))),
                    },
                    after_first + beat,
                )
            }
        };
        out.push(TrialSpec::Identify(IdentifyTrial {
            exercise: ExerciseKind::PatternMemory,
            prompt: prompt.into(),
            events,
            choices,
            correct: correct.into(),
            answer_after_ms: Some(end),
            explain: Some(explain),
            stimulus: json!({ "task": p.task, "key": tonic.name(), "mode": mode, "melody": mel, "transpose": p.transpose }),
            ..IdentifyTrial::skeleton()
        }));
    }
    Ok(out)
}

// ---------------------------------------------------------------------------------------------
// Single-pass error detection
// ---------------------------------------------------------------------------------------------

pub(super) fn gen_error_detection(
    p: &ErrorDetectionParams,
    n: usize,
    rng: &mut Rng,
    ctx: &GenContext,
) -> Result<Vec<TrialSpec>, GenError> {
    check_timbres(std::slice::from_ref(&p.timbre))?;
    if !(0.0..=60.0).contains(&p.pre_audiation_s) {
        return err("pre_audiation_s must be 0–60");
    }
    let melodies = Melodies::new(&p.melody, ctx, rng)?;
    let beat = 60_000.0 / p.tempo_bpm.clamp(30.0, 200.0) as f64;
    let countdown = p.pre_audiation_s as f64 * 1000.0;
    let mut out = Vec::with_capacity(n);
    for i in 0..n {
        let (score, _) = melodies.score(i, p.tempo_bpm, rng, ctx)?;
        let targets = score.targets();
        if targets.len() < 3 {
            return err("the melodies need at least three notes");
        }
        let tonic = PitchClass::new(score.key.tonic_pc());
        let mode = score.key.mode;
        let steps = mode_steps(mode);
        let in_key = |m: i32| steps.contains(&(m - tonic.index() as i32).rem_euclid(12));
        let k = rng.range_i32(1, targets.len() as i32 - 1) as usize;
        let written = targets[k].midi;
        let chromatic = match p.change {
            ChangeSize::Step => false,
            ChangeSize::Chromatic => true,
            ChangeSize::Any => rng.chance(0.5),
        };
        let up = rng.chance(0.5);
        let played = if chromatic {
            let c = written + if up { 1 } else { -1 };
            if in_key(c) {
                written + if up { -1 } else { 1 }
            } else {
                c
            }
        } else {
            // The next note of the scale up or down, not the neighbouring written note.
            let dir = if up { 1 } else { -1 };
            let mut m = written + dir;
            while !in_key(m) {
                m += dir;
            }
            m
        };
        let rel = |m: i32| m - tonic.index() as i32;
        let low = targets.iter().map(|t| t.midi).min().unwrap();
        let mut reference_tonic = tonic.in_octave(3);
        while reference_tonic + 12 <= low {
            reference_tonic += 12;
        }
        let mut events = vec![ev(reference_tonic, 0.0, 1500.0, &p.timbre, -6.0)];
        for (j, tn) in targets.iter().enumerate() {
            let m = if j == k { played } else { tn.midi };
            events.push(ev(
                m,
                countdown + tn.start_beat as f64 * beat,
                tn.beats as f64 * beat * 0.95,
                &p.timbre,
                -6.0,
            ));
        }
        let choices = (0..targets.len())
            .map(|j| Choice::new(j.to_string(), format!("{}", j + 1)))
            .collect();
        out.push(TrialSpec::Identify(IdentifyTrial {
            exercise: ExerciseKind::ErrorDetection,
            prompt: format!(
                "Read and audiate the melody{}; it plays once. Which note was played wrong?",
                if countdown > 0.0 { " until it starts" } else { "" }
            ),
            events,
            choices,
            correct: k.to_string(),
            answer_after_ms: Some(countdown),
            explain: Some(format!(
                "Note {} is written {} and was played {}.",
                k + 1,
                note_in_key(p.labels, tonic, mode, rel(written)),
                note_in_key(p.labels, tonic, mode, rel(played))
            )),
            score: Some(score),
            no_replay: true,
            stimulus: json!({ "wrong_note": k, "written": written, "played": played, "chromatic": chromatic }),
            ..IdentifyTrial::skeleton()
        }));
    }
    Ok(out)
}

// ---------------------------------------------------------------------------------------------
// Mislabeled syllables (solfège Stroop)
// ---------------------------------------------------------------------------------------------

pub(super) fn gen_syllable_check(
    p: &SyllableCheckParams,
    n: usize,
    rng: &mut Rng,
) -> Result<Vec<TrialSpec>, GenError> {
    if p.notes.is_empty() || p.notes.iter().any(|l| !(3..=10).contains(l)) {
        return err("notes must be 3–10");
    }
    let keys = keys_for(&p.keys, &p.modes, p.key_change, n, rng)?;
    let beat = 60_000.0 / p.tempo_bpm.clamp(40.0, 220.0) as f64;
    let mut out = Vec::with_capacity(n);
    for (tonic, mode) in keys {
        let allowed = degree_indexes(&p.degrees, mode)?;
        let steps = mode_steps(mode);
        let semis = |d: i32| index_semis(&steps, d);
        let name = |d: i32| note_in_key(p.labels, tonic, mode, semis(d));
        let len = *rng.choose(&p.notes).unwrap() as usize;
        let mel = index_walk(&allowed, len, rng);
        let t = place_tonic(tonic, 0, 12, NoteRange::new(55, 76));
        let timbre = pick_timbre(&p.timbres, rng)?;
        let k = rng.below(len);
        // The wrong label is a neighbour's name, the most tempting mistake.
        let wrong = neighbour(mel[k], rng);
        let mut events: Vec<SoundEvent> = tonic_chord(t, third_of(mode), 0.0, 1.0)
            .iter()
            .map(|c| ev(c.midi, 0.0, beat, &timbre, -10.0))
            .collect();
        for (j, d) in mel.iter().enumerate() {
            events.push(ev(
                t + semis(*d),
                beat * 1.6 + j as f64 * beat,
                beat * 0.9,
                &timbre,
                -6.0,
            ));
        }
        let choices = mel
            .iter()
            .enumerate()
            .map(|(j, d)| {
                Choice::new(
                    j.to_string(),
                    format!(
                        "{} · {}",
                        j + 1,
                        if j == k { name(wrong) } else { name(*d) }
                    ),
                )
            })
            .collect();
        out.push(TrialSpec::Identify(IdentifyTrial {
            exercise: ExerciseKind::SyllableCheck,
            prompt: "One syllable is wrong. Which?".into(),
            events,
            choices,
            correct: k.to_string(),
            answer_after_ms: Some(beat * 1.6),
            explain: Some(format!(
                "Note {} was {}, not {}.",
                k + 1,
                name(mel[k]),
                name(wrong)
            )),
            stimulus: json!({ "key": tonic.name(), "mode": mode, "melody": mel, "wrong": k }),
            ..IdentifyTrial::skeleton()
        }));
    }
    Ok(out)
}

// ---------------------------------------------------------------------------------------------
// Harmony: chords in a key, voice leading, cadences, bass and guide-tone lines
// ---------------------------------------------------------------------------------------------

/// A chord in a key: its scale-degree root (0–6) and whether it has a seventh.
#[derive(Clone, Copy, Debug, PartialEq)]
struct KeyChord {
    root: i32,
    seventh: bool,
}

const fn ch(root: i32, seventh: bool) -> KeyChord {
    KeyChord { root, seventh }
}

/// The chord's tones in semitones above the tonic, root first: stacked thirds of the scale,
/// with the raised leading tone on V and vii in minor.
fn chord_semis(c: KeyChord, mode: KeyMode) -> Vec<i32> {
    let mut steps = mode_steps(mode).to_vec();
    if mode == KeyMode::Minor && (c.root == 4 || c.root == 6) {
        steps[6] = 11;
    }
    let count = if c.seventh { 4 } else { 3 };
    (0..count)
        .map(|k| index_semis(&steps, c.root + 2 * k))
        .collect()
}

fn roman(c: KeyChord, mode: KeyMode) -> String {
    let s = chord_semis(c, mode);
    let third = s[1] - s[0];
    let fifth = s[2] - s[0];
    let base = ["I", "II", "III", "IV", "V", "VI", "VII"][c.root as usize];
    let mut r = if third == 4 {
        base.to_string()
    } else {
        base.to_lowercase()
    };
    match (fifth, s.get(3).map(|x| x - s[0])) {
        (6, Some(10)) => r.push_str("ø7"),
        (6, Some(_)) => r.push_str("°7"),
        (6, None) => r.push('°'),
        (_, Some(11)) => r.push_str("maj7"),
        (_, Some(_)) => r.push('7'),
        _ => {}
    }
    r
}

/// Four-part voicings: a root-position bass and three upper voices moving as little as they can.
/// `soprano_end` fixes the top voice of the last chord to a pitch class above the tonic.
fn voice_progression(
    chords: &[KeyChord],
    mode: KeyMode,
    tonic_pc: i32,
    soprano_end: Option<i32>,
) -> Vec<(i32, Vec<i32>)> {
    let mut out: Vec<(i32, Vec<i32>)> = Vec::new();
    for (i, c) in chords.iter().enumerate() {
        let tones = chord_semis(*c, mode);
        let pcs: Vec<i32> = tones
            .iter()
            .map(|s| (tonic_pc + s).rem_euclid(12))
            .collect();
        // Bass: the root between E2 and D3, nearest the last bass.
        let prev_bass = out.last().map(|o| o.0).unwrap_or(48);
        let bass = (40..=55i32)
            .filter(|m| m.rem_euclid(12) == pcs[0])
            .min_by_key(|m| (m - prev_bass).abs())
            .unwrap();
        // Upper voices: three distinct chord tones (the seventh, third and root or fifth for a
        // seventh chord), each between G3 and A5, close enough to sit within an octave.
        let wanted: Vec<i32> = if pcs.len() == 4 {
            vec![pcs[0], pcs[1], pcs[3]]
        } else {
            pcs.clone()
        };
        let mut best: Option<(i32, Vec<i32>)> = None;
        for a in 55..=69i32 {
            if !wanted.contains(&a.rem_euclid(12)) {
                continue;
            }
            let mut v = vec![a];
            let mut m = a;
            while v.len() < 3 {
                m += 1;
                if wanted.contains(&m.rem_euclid(12))
                    && !v.iter().any(|x| x.rem_euclid(12) == m.rem_euclid(12))
                {
                    v.push(m);
                }
            }
            if v[2] - v[0] > 12 || v[2] > 81 {
                continue;
            }
            // The last chord's top voice, when it is fixed.
            let wanted_top = soprano_end.filter(|_| i + 1 == chords.len());
            if wanted_top.is_some_and(|s| v[2].rem_euclid(12) != (tonic_pc + s).rem_euclid(12)) {
                continue;
            }
            let cost = match out.last() {
                Some((_, prev)) => prev.iter().zip(&v).map(|(x, y)| (x - y).abs()).sum::<i32>(),
                None => (v[2] - 70).abs(),
            };
            if best.as_ref().is_none_or(|b| cost < b.0) {
                best = Some((cost, v));
            }
        }
        out.push((
            bass,
            best.map(|b| b.1)
                .unwrap_or_else(|| pcs.iter().map(|p| 60 + p).collect()),
        ));
    }
    out
}

pub(super) fn gen_cadence_id(
    p: &CadenceIdParams,
    n: usize,
    rng: &mut Rng,
) -> Result<Vec<TrialSpec>, GenError> {
    if p.cadences.is_empty() {
        return err("cadences must not be empty");
    }
    if p.lead_in > 4 {
        return err("lead_in must be 0–4");
    }
    let keys = keys_for(&p.keys, &p.modes, KeyChange::PerTrial, n, rng)?;
    let beat = 60_000.0 / p.tempo_bpm.clamp(30.0, 200.0) as f64;
    let mut kinds: Vec<CadenceKind> = Vec::new();
    for c in &p.cadences {
        if !kinds.contains(c) {
            kinds.push(*c);
        }
    }
    let mut bag = BalancedBag::allowing_repeats(kinds.clone());
    let mut out = Vec::with_capacity(n);
    for (tonic, mode) in keys {
        let kind = bag.draw(rng).unwrap();
        let ending: Vec<KeyChord> = match kind {
            CadenceKind::PerfectAuthentic | CadenceKind::ImperfectAuthentic => {
                vec![ch(4, rng.chance(0.4)), ch(0, false)]
            }
            CadenceKind::Plagal => vec![ch(3, false), ch(0, false)],
            CadenceKind::Half => vec![
                *rng.choose(&[ch(0, false), ch(3, false), ch(1, false)])
                    .unwrap(),
                ch(4, false),
            ],
            CadenceKind::Deceptive => vec![ch(4, rng.chance(0.4)), ch(5, false)],
        };
        let mut chords = Vec::new();
        if p.lead_in > 0 {
            chords.push(ch(0, false));
            for _ in 1..p.lead_in {
                let last = *chords.last().unwrap();
                let opts: Vec<KeyChord> = [ch(5, false), ch(3, false), ch(1, false), ch(0, false)]
                    .into_iter()
                    .filter(|c| *c != last)
                    .collect();
                chords.push(*rng.choose(&opts).unwrap());
            }
            if chords.last() == ending.first() {
                chords.pop();
            }
        }
        chords.extend(ending);
        let soprano = match kind {
            CadenceKind::PerfectAuthentic => Some(0),
            CadenceKind::ImperfectAuthentic => {
                Some(if rng.chance(0.6) { third_of(mode) } else { 7 })
            }
            _ => None,
        };
        let voiced = voice_progression(&chords, mode, tonic.index() as i32, soprano);
        let timbre = pick_timbre(&p.timbres, rng)?;
        let mut events = Vec::new();
        for (i, (bass, upper)) in voiced.iter().enumerate() {
            let last = i + 1 == voiced.len();
            let dur = if last { beat * 2.0 } else { beat * 0.95 };
            events.push(ev(*bass, i as f64 * beat, dur, &timbre, -7.0));
            for m in upper {
                events.push(ev(*m, i as f64 * beat, dur, &timbre, -10.0));
            }
        }
        let romans: Vec<String> = chords.iter().map(|c| roman(*c, mode)).collect();
        out.push(TrialSpec::Identify(IdentifyTrial {
            exercise: ExerciseKind::CadenceId,
            prompt: "How does the phrase end?".into(),
            events,
            choices: kinds.iter().map(|k| Choice::new(k.id(), k.name())).collect(),
            correct: kind.id().into(),
            answer_after_ms: Some(beat * (voiced.len() as f64 - 1.0)),
            explain: Some(format!("{}: {}.", key_name(tonic, mode), romans.join(" – "))),
            stimulus: json!({ "cadence": kind, "key": tonic.name(), "mode": mode, "chords": romans }),
            ..IdentifyTrial::skeleton()
        }));
    }
    Ok(out)
}

fn progression_chords(p: Progression, mode: KeyMode) -> Vec<KeyChord> {
    let minor = mode == KeyMode::Minor;
    match p {
        Progression::IIvVI => vec![ch(0, false), ch(3, false), ch(4, false), ch(0, false)],
        Progression::IViIvVI => vec![
            ch(0, false),
            ch(5, false),
            ch(3, false),
            ch(4, false),
            ch(0, false),
        ],
        Progression::IIvIVI => vec![
            ch(0, false),
            ch(3, false),
            ch(0, false),
            ch(4, false),
            ch(0, false),
        ],
        Progression::IIiVI => vec![ch(0, false), ch(1, false), ch(4, false), ch(0, false)],
        Progression::IViIiVI => vec![
            ch(0, false),
            ch(5, false),
            ch(1, false),
            ch(4, false),
            ch(0, false),
        ],
        Progression::IiVI7 => vec![ch(1, true), ch(4, true), ch(0, !minor)],
        Progression::Turnaround7 => vec![
            ch(0, !minor),
            ch(5, true),
            ch(1, true),
            ch(4, true),
            ch(0, !minor),
        ],
    }
}

pub(super) fn gen_bass_line(
    p: &BassLineParams,
    n: usize,
    rng: &mut Rng,
    ctx: &GenContext,
) -> Result<Vec<TrialSpec>, GenError> {
    if p.progressions.is_empty() {
        return err("progressions must not be empty");
    }
    if !(1.0..=8.0).contains(&p.beats_per_chord) {
        return err("beats_per_chord must be 1–8");
    }
    check_timbres(std::slice::from_ref(&p.timbre))?;
    let keys = keys_for(&p.keys, &p.modes, p.key_change, n, rng)?;
    let range = sung_range(p.range, ctx);
    let mut bag = BalancedBag::allowing_repeats(p.progressions.clone());
    let bpc = p.beats_per_chord;
    let mut out = Vec::with_capacity(n);
    for (tonic, mode) in keys {
        let prog = bag.draw(rng).unwrap();
        let chords = progression_chords(prog, mode);
        let tpc = tonic.index() as i32;
        // The sung line as pitch classes above the tonic, then placed in the range.
        let line_semis: Vec<i32> = match p.line {
            BassLineKind::Roots => chords.iter().map(|c| chord_semis(*c, mode)[0]).collect(),
            BassLineKind::GuideTones => {
                let mut v: Vec<i32> = Vec::new();
                for c in &chords {
                    let s = chord_semis(*c, mode);
                    let mut cands = vec![s[1]];
                    if s.len() == 4 {
                        cands.push(s[3]);
                    }
                    let next = match v.last() {
                        // The first chord starts on its seventh when it has one, as the line
                        // then falls by step into the next chord's third.
                        None => *cands.last().unwrap(),
                        Some(prev) => {
                            let near = |x: i32| {
                                let d = (x - prev).rem_euclid(12);
                                d.min(12 - d)
                            };
                            *cands.iter().min_by_key(|x| near(**x)).unwrap()
                        }
                    };
                    v.push(next);
                }
                v
            }
        };
        // Place each note nearest the one before, inside the range (the first near its middle).
        let mid = (range.low.0 + range.high.0) / 2;
        let mut line: Vec<i32> = Vec::new();
        for s in &line_semis {
            let pc = (tpc + s).rem_euclid(12);
            let target = line
                .last()
                .copied()
                .unwrap_or(mid - if p.line == BassLineKind::Roots { 3 } else { 0 });
            let m = (range.low.0..=range.high.0)
                .filter(|m| m.rem_euclid(12) == pc)
                .min_by_key(|m| (m - target).abs())
                .unwrap_or(target);
            line.push(m);
        }
        let voiced = voice_progression(&chords, mode, tpc, None);
        let t = place_tonic(tonic, 0, 7, range);
        let mut reference = tonic_chord(t, third_of(mode), 0.0, 2.0);
        if p.backing == LineBacking::TonicOnly {
            reference = vec![TargetNote {
                midi: t,
                start_beat: 0.0,
                beats: 2.0,
            }];
        }
        let sing = 6.0;
        let notes: Vec<TargetNote> = line
            .iter()
            .enumerate()
            .map(|(i, m)| TargetNote {
                midi: *m,
                start_beat: i as f32 * bpc,
                beats: bpc * 0.95,
            })
            .collect();
        let mut accompaniment = Vec::new();
        if p.backing != LineBacking::TonicOnly {
            for (i, (bass, upper)) in voiced.iter().enumerate() {
                let start_beat = i as f32 * bpc;
                if p.backing == LineBacking::WithBass {
                    accompaniment.push(TargetNote {
                        midi: *bass,
                        start_beat,
                        beats: bpc * 0.95,
                    });
                }
                for m in upper {
                    accompaniment.push(TargetNote {
                        midi: *m,
                        start_beat,
                        beats: bpc * 0.95,
                    });
                }
            }
        }
        let romans: Vec<String> = chords.iter().map(|c| roman(*c, mode)).collect();
        let what = match p.line {
            BassLineKind::Roots => "the root of each chord",
            BassLineKind::GuideTones => "the guide tones (3rds and 7ths)",
        };
        let how = match p.backing {
            LineBacking::WithBass => ", along with the bass",
            LineBacking::WithoutBass => ": the bass is left out",
            LineBacking::TonicOnly => ", hearing the chords in your head",
        };
        let total = sing + chords.len() as f32 * bpc + 1.0;
        let mut tr = sung(
            ExerciseKind::BassLine,
            format!("{}: sing {what}{how}", romans.join(" – ")),
            p.tempo_bpm,
            reference,
            sing,
            notes,
            total,
            vec![
                ("Key".into(), 0.0, 2.0),
                ("Count-in".into(), 2.0, sing),
                ("Sing".into(), sing, total),
            ],
            &p.timbre,
            json!({ "progression": prog, "chords": romans, "line": p.line, "backing": p.backing, "key": tonic.name(), "mode": mode }),
        );
        tr.accompaniment = accompaniment;
        tr.count_in_beat = Some(2.0);
        tr.timed = true;
        tr.reveal = Some(format!(
            "The line: {}.",
            line.iter()
                .map(|m| note_in_key(p.labels, tonic, mode, m - tpc))
                .collect::<Vec<_>>()
                .join(" ")
        ));
        out.push(TrialSpec::Sung(tr));
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn block(json: serde_json::Value) -> BlockSpec {
        serde_json::from_value(json).unwrap()
    }

    #[test]
    fn n_back_streams_mark_their_matches() {
        let ctx = GenContext::default();
        for n in 1..=3 {
            let b =
                block(json!({ "type": "n_back", "count": 3, "params": { "n": n, "length": 30 } }));
            for t in generate_block(&b, 9, &ctx).unwrap() {
                let TrialSpec::Detect(t) = t else { panic!() };
                assert_eq!(t.exercise, ExerciseKind::NBack);
                let m: Vec<i32> = t.tones.iter().map(|x| x.midi).collect();
                for (i, tone) in t.tones.iter().enumerate() {
                    assert_eq!(
                        tone.target,
                        i >= n && m[i] == m[i - n],
                        "n {n} at {i}: {m:?}"
                    );
                }
                assert!(t.tones.iter().any(|x| x.target));
            }
        }
        let canon = block(
            json!({ "type": "n_back", "count": 2, "params": { "n": 2, "mode": "sing", "length": 10 } }),
        );
        let TrialSpec::Sung(t) = &generate_block(&canon, 1, &ctx).unwrap()[0] else {
            panic!()
        };
        // Each sung note is the one played two beats before.
        for (k, note) in t.notes.iter().enumerate() {
            assert_eq!(note.midi, t.accompaniment[k].midi);
            assert_eq!(note.start_beat, t.accompaniment[k].start_beat + 2.0);
        }
    }

    #[test]
    fn melodic_updates_change_what_is_sung() {
        let ctx = GenContext::default();
        for op in [
            "replace_degree",
            "replace_position",
            "shift",
            "retrograde",
            "omit",
        ] {
            let b = block(
                json!({ "type": "melodic_updating", "count": 6, "params": { "operations": [op], "notes": [5] } }),
            );
            for t in generate_block(&b, 3, &ctx).unwrap() {
                let TrialSpec::Sung(t) = t else { panic!() };
                let heard: Vec<i32> = t.reference_beats.iter().skip(3).map(|n| n.midi).collect();
                let sung: Vec<i32> = t.notes.iter().map(|n| n.midi).collect();
                match op {
                    "retrograde" => {
                        assert_eq!(sung, heard.iter().rev().copied().collect::<Vec<_>>())
                    }
                    "omit" => assert_eq!(sung.len(), heard.len() - 1),
                    _ => assert_ne!(sung, heard, "{op}"),
                }
                assert!(t.sections.iter().any(|s| s.label.contains("rehearse")));
            }
        }
    }

    #[test]
    fn find_home_ends_away_from_home() {
        let ctx = GenContext::default();
        let b = block(
            json!({ "type": "resting_tone", "count": 16, "params": { "modes": ["major", "dorian", "mixolydian", "natural_minor"] } }),
        );
        for t in generate_block(&b, 4, &ctx).unwrap() {
            let TrialSpec::Sung(t) = t else { panic!() };
            let last = t.reference_beats.last().unwrap().midi;
            assert_ne!((last - t.notes[0].midi).rem_euclid(12), 0);
        }
        let named = block(
            json!({ "type": "resting_tone", "count": 8, "params": { "answer": "name", "modes": ["major", "natural_minor", "dorian"] } }),
        );
        for t in generate_block(&named, 4, &ctx).unwrap() {
            let TrialSpec::Identify(t) = t else { panic!() };
            assert_eq!(t.choices.len(), 3);
        }
    }

    #[test]
    fn cadences_end_as_named() {
        let ctx = GenContext::default();
        let b = block(json!({ "type": "cadence_id", "count": 40, "params": {
            "cadences": ["perfect_authentic", "imperfect_authentic", "plagal", "half", "deceptive"], "modes": ["major", "minor"] } }));
        for t in generate_block(&b, 2, &ctx).unwrap() {
            let TrialSpec::Identify(t) = t else { panic!() };
            let chords = t.stimulus["chords"].as_array().unwrap();
            let last = chords.last().unwrap().as_str().unwrap().to_lowercase();
            let before = chords[chords.len() - 2].as_str().unwrap().to_lowercase();
            match t.correct.as_str() {
                "half" => assert!(last.starts_with('v') && !last.starts_with("vi")),
                "deceptive" => assert!(last.starts_with("vi") && before.starts_with('v')),
                "plagal" => assert!(last.starts_with('i') && before.starts_with("iv")),
                _ => assert!(last.starts_with('i') && before.starts_with('v')),
            }
            // A perfect cadence ends with the tonic on top.
            if t.correct == "perfect_authentic" {
                let end = t.events.iter().map(|e| e.start_ms).fold(0.0, f64::max);
                let top = t
                    .events
                    .iter()
                    .filter(|e| e.start_ms == end)
                    .map(|e| e.midi)
                    .max()
                    .unwrap();
                let tonic = t.stimulus["key"]
                    .as_str()
                    .unwrap()
                    .parse::<PitchClass>()
                    .unwrap();
                assert_eq!(top.rem_euclid(12), tonic.index() as i32);
            }
        }
    }

    #[test]
    fn guide_tones_move_by_step() {
        let ctx = GenContext::default();
        let b = block(
            json!({ "type": "bass_line", "count": 10, "params": { "line": "guide_tones", "progressions": ["ii_v_i7"], "keys": ["C"] } }),
        );
        for t in generate_block(&b, 1, &ctx).unwrap() {
            let TrialSpec::Sung(t) = t else { panic!() };
            let m: Vec<i32> = t.notes.iter().map(|n| n.midi).collect();
            // C: ii7 seventh C, V7 third B, Imaj7 seventh B: do – ti – ti.
            assert_eq!(
                m.iter().map(|x| x.rem_euclid(12)).collect::<Vec<_>>(),
                [0, 11, 11]
            );
            assert!(m.windows(2).all(|w| (w[0] - w[1]).abs() <= 2));
        }
    }

    #[test]
    fn singback_flash_and_silent_bars_lay_out_the_timeline() {
        let ctx = GenContext::default();
        let melody =
            json!({ "generate": { "keys": ["C"], "bars": 4, "rhythms": ["half", "quarter"] } });
        let back = block(
            json!({ "type": "sight_sing", "count": 3, "params": { "melody": melody, "mode": "singback" } }),
        );
        for t in generate_block(&back, 2, &ctx).unwrap() {
            let TrialSpec::Sung(t) = t else { panic!() };
            assert!(t.score.is_none(), "the score is hidden for singback");
            // The melody plays before singing, note for note.
            let heard: Vec<i32> = t
                .reference_beats
                .iter()
                .rev()
                .take(t.notes.len())
                .rev()
                .map(|n| n.midi)
                .collect();
            assert_eq!(heard, t.notes.iter().map(|n| n.midi).collect::<Vec<_>>());
            let listen = t.sections.iter().find(|s| s.label == "Listen").unwrap();
            assert!(listen.end_beat <= t.count_in_beat.unwrap());
        }
        let flash = block(
            json!({ "type": "sight_sing", "count": 2, "params": { "melody": melody, "mode": "flash", "flash_s": 10 } }),
        );
        let TrialSpec::Sung(t) = &generate_block(&flash, 2, &ctx).unwrap()[0] else {
            panic!()
        };
        assert!(t.score.is_some());
        assert_eq!(t.reference_beats.len(), 1, "only the tonic");
        assert!(t.score_until_beat.unwrap() <= t.count_in_beat.unwrap());
        let silent = block(
            json!({ "type": "sight_sing", "count": 2, "params": { "melody": melody, "silent": "bars" } }),
        );
        let TrialSpec::Sung(t) = &generate_block(&silent, 2, &ctx).unwrap()[0] else {
            panic!()
        };
        // Bars 2 and 3 of four are silent.
        for (i, n) in t.notes.iter().enumerate() {
            let bar = (n.start_beat / 4.0).floor() as u32 + 1;
            assert_eq!(t.silent.contains(&i), bar == 2 || bar == 3);
        }
        assert!(t.sections.iter().any(|s| s.label == "Sing silently"));
    }

    #[test]
    fn scaffolds_add_a_stepping_stone() {
        let ctx = GenContext::default();
        let add = block(
            json!({ "type": "sing_interval", "count": 6, "params": { "intervals": ["M6"], "directions": ["up"], "scaffold": "additive" } }),
        );
        for t in generate_block(&add, 1, &ctx).unwrap() {
            let TrialSpec::Sung(t) = t else { panic!() };
            let m: Vec<i32> = t.notes.iter().map(|n| n.midi).collect();
            assert_eq!((m[1] - m[0], m[2] - m[0]), (7, 9), "a 5th, then a step");
        }
        let comp = block(
            json!({ "type": "sing_interval", "count": 6, "params": { "intervals": ["m7"], "directions": ["down"], "scaffold": "complementary", "range": { "low": "C3", "high": "C5" } } }),
        );
        for t in generate_block(&comp, 1, &ctx).unwrap() {
            let TrialSpec::Sung(t) = t else { panic!() };
            let m: Vec<i32> = t.notes.iter().map(|n| n.midi).collect();
            assert_eq!(
                (m[1] - m[0], m[2] - m[0]),
                (2, -10),
                "a 2nd up, then the octave down"
            );
            assert!(t.prompt.contains("via"));
        }
    }

    #[test]
    fn missed_degrees_come_up_more_often() {
        let mut ctx = GenContext::default();
        ctx.weak.insert("scale_degree_id:6".into(), 0.6);
        let b = block(
            json!({ "type": "scale_degree_id", "count": 200, "params": { "upper_tonic": false } }),
        );
        let count = |ctx: &GenContext| {
            generate_block(&b, 3, ctx)
                .unwrap()
                .iter()
                .filter(|t| matches!(t, TrialSpec::Identify(t) if t.correct == "6"))
                .count()
        };
        let weighted = count(&ctx);
        let even = count(&GenContext::default());
        assert!(weighted > even * 2, "{weighted} vs {even}");
    }

    #[test]
    fn memory_tasks_answer_both_ways() {
        let ctx = GenContext::default();
        for task in ["same_different", "probe"] {
            let b =
                block(json!({ "type": "pattern_memory", "count": 30, "params": { "task": task } }));
            let answers: std::collections::BTreeSet<String> = generate_block(&b, 5, &ctx)
                .unwrap()
                .into_iter()
                .map(|t| match t {
                    TrialSpec::Identify(t) => t.correct,
                    _ => panic!(),
                })
                .collect();
            assert_eq!(answers.len(), 2, "{task}");
        }
    }
}
