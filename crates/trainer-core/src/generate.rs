//! Deterministic trial generation: (block spec, seed, context) → concrete trials.

use serde_json::json;

use crate::melody::generate_melody;
use crate::rng::{BalancedBag, Rng};
use crate::schema::*;
use crate::score::{Score, ScoreNote};
use crate::theory::{ChordQuality, Interval, Note, PitchClass, ScaleKind};
use crate::timbre::{PoolConfig, TimbreId, default_training_timbres, is_valid_timbre};

#[derive(Clone, Debug)]
pub struct GenContext {
    /// The singer's comfortable range (default C3–G4 until the user sets one).
    pub comfortable: NoteRange,
    pub pools: PoolConfig,
}

impl Default for GenContext {
    fn default() -> Self {
        GenContext {
            comfortable: NoteRange::new(48, 67),
            pools: PoolConfig::default(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, thiserror::Error)]
#[error("{0}")]
pub struct GenError(pub String);

fn err<T>(msg: impl Into<String>) -> Result<T, GenError> {
    Err(GenError(msg.into()))
}

/// The seed a block uses: its own `seed`, or one derived from the session seed and its index.
pub fn block_seed(session_seed: u64, block_idx: usize, block: &BlockSpec) -> u64 {
    block.seed.unwrap_or_else(|| {
        let mut r = Rng::derive(session_seed, block_idx as u64 + 1);
        r.next_u64()
    })
}

pub fn generate_block(
    block: &BlockSpec,
    seed: u64,
    ctx: &GenContext,
) -> Result<Vec<TrialSpec>, GenError> {
    let mut rng = Rng::new(seed);
    let n = block.count as usize;
    if n == 0 {
        return err("count must be at least 1");
    }
    if n > 1000 {
        return err("count must be at most 1000");
    }
    match &block.exercise {
        Exercise::PitchId(p) => gen_pitch_id(p, n, &mut rng, ctx),
        Exercise::SingPitch(p) => gen_sing_pitch(p, n, &mut rng, ctx),
        Exercise::PitchPairing(p) => gen_pairing(p, n, &mut rng, ctx),
        Exercise::TonalCenterId(p) => gen_tonal_center(p, n, &mut rng),
        Exercise::IntervalId(p) => gen_interval_id(p, n, &mut rng),
        Exercise::ChordId(p) => gen_chord_id(p, n, &mut rng),
        Exercise::ScaleId(p) => gen_scale_id(p, n, &mut rng),
        Exercise::PitchMatch(p) => gen_pitch_match(p, n, &mut rng, ctx),
        Exercise::Sustain(p) => gen_sustain(p, n, &mut rng, ctx),
        Exercise::SingInterval(p) => gen_sing_interval(p, n, &mut rng, ctx),
        Exercise::Pattern(p) => gen_pattern(p, n, &mut rng, ctx),
        Exercise::SightSing(p) => gen_sight_sing(p, n, &mut rng, ctx),
        Exercise::MelodicDictation(p) => gen_dictation(p, n, &mut rng, ctx),
    }
}

/// The pitch reference mode a trial is graded with unless the block overrides it.
pub fn default_mode(trial: &TrialSpec) -> PitchRefMode {
    match trial {
        TrialSpec::SingPitch(t) => {
            if t.target_midi.is_some() {
                PitchRefMode::Absolute
            } else {
                PitchRefMode::PitchClass
            }
        }
        TrialSpec::Sung(t) => {
            let unprompted = t.reference_beats.is_empty() && !t.guide;
            if t.exercise == ExerciseKind::Pattern && unprompted {
                PitchRefMode::RelativeDriftTolerant
            } else if t.exercise == ExerciseKind::SightSing && unprompted {
                PitchRefMode::RelativeToFirst
            } else {
                PitchRefMode::Absolute
            }
        }
        _ => PitchRefMode::Absolute,
    }
}

// ---------------------------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------------------------

fn check_timbres(list: &[TimbreId]) -> Result<(), GenError> {
    for t in list {
        if !is_valid_timbre(t) {
            return err(format!("unknown timbre `{t}`"));
        }
    }
    Ok(())
}

/// (timbre, octave) combinations allowed by the pool.
fn pool_combos(
    timbres: &[TimbreId],
    octaves: &[i32],
    pool: Pool,
    ctx: &GenContext,
) -> Result<Vec<(TimbreId, i32)>, GenError> {
    let timbres: Vec<TimbreId> = if timbres.is_empty() {
        match pool {
            Pool::Holdout => ctx.pools.holdout_timbres.clone(),
            _ => default_training_timbres(),
        }
    } else {
        timbres.to_vec()
    };
    check_timbres(&timbres)?;
    if octaves.is_empty() {
        return err("octaves must not be empty");
    }
    if let Some(o) = octaves.iter().find(|o| !(0..=8).contains(*o)) {
        return err(format!("octave {o} is outside 0–8"));
    }
    let mut combos = Vec::new();
    for t in &timbres {
        for &o in octaves {
            let holdout = ctx.pools.is_holdout(t, o);
            let ok = match pool {
                Pool::Training => !holdout,
                Pool::Holdout => holdout,
                Pool::Any => true,
            };
            if ok {
                combos.push((t.clone(), o));
            }
        }
    }
    if combos.is_empty() {
        return err(format!(
            "no {} stimuli: every timbre/octave combination is excluded by the pool (holdout timbres: {}; holdout octaves: {:?})",
            pool.as_str(),
            ctx.pools.holdout_timbres.join(", "),
            ctx.pools.holdout_octaves
        ));
    }
    Ok(combos)
}

fn gain_db(range: Option<[f32; 2]>, rng: &mut Rng) -> f32 {
    match range {
        Some([a, b]) => rng.range_f64(a.min(b) as f64, a.max(b) as f64) as f32,
        None => -6.0,
    }
}

fn pc_choices(classes: &[PitchClass], with_other: bool) -> Vec<Choice> {
    let mut sorted = classes.to_vec();
    sorted.sort();
    let mut v: Vec<Choice> = sorted
        .iter()
        .map(|p| Choice {
            id: p.name().to_string(),
            label: p.display_name().to_string(),
        })
        .collect();
    if with_other {
        v.push(Choice {
            id: "other".into(),
            label: "Other".into(),
        });
    }
    v
}

fn ev(midi: i32, start_ms: f64, dur_ms: f64, timbre: &str, gain: f32) -> SoundEvent {
    SoundEvent {
        midi,
        start_ms,
        dur_ms,
        timbre: timbre.to_string(),
        gain_db: gain,
        detune_cents: 0.0,
    }
}

fn pick_timbre(list: &[TimbreId], rng: &mut Rng) -> Result<TimbreId, GenError> {
    let list: Vec<TimbreId> = if list.is_empty() {
        vec!["gm:0".to_string()]
    } else {
        list.to_vec()
    };
    check_timbres(&list)?;
    Ok(rng.choose(&list).unwrap().clone())
}

/// A random note in range, avoiding an immediate repeat when possible.
fn pick_note(range: NoteRange, last: &mut Option<i32>, rng: &mut Rng) -> i32 {
    for _ in 0..8 {
        let m = rng.range_i32(range.low.0, range.high.0);
        if Some(m) != *last || range.span() == 0 {
            *last = Some(m);
            return m;
        }
    }
    let m = rng.range_i32(range.low.0, range.high.0);
    *last = Some(m);
    m
}

// ---------------------------------------------------------------------------------------------
// Absolute pitch
// ---------------------------------------------------------------------------------------------

fn gen_pitch_id(
    p: &PitchIdParams,
    n: usize,
    rng: &mut Rng,
    ctx: &GenContext,
) -> Result<Vec<TrialSpec>, GenError> {
    let classes = p.classes.classes();
    if classes.is_empty() {
        return err("classes must not be empty");
    }
    let others: Vec<PitchClass> = PitchClass::ALL
        .iter()
        .copied()
        .filter(|c| !classes.contains(c))
        .collect();
    let use_other = p.out_of_set_rate > 0.0 && !others.is_empty();
    let combos = pool_combos(&p.timbres, &p.octaves, p.pool, ctx)?;
    let mut bag = BalancedBag::new(classes.clone());
    let mut other_bag = BalancedBag::new(others.clone());
    let mut combo_bag = BalancedBag::new((0..combos.len()).collect::<Vec<_>>());
    let choices = match p.answer {
        PitchAnswer::PitchClass => pc_choices(&classes, use_other),
        PitchAnswer::NoteWithOctave => {
            let lo = *p.octaves.iter().min().unwrap();
            let hi = *p.octaves.iter().max().unwrap();
            (PitchClass::new(0).in_octave(lo)..=PitchClass::new(11).in_octave(hi))
                .map(|m| Choice {
                    id: Note(m).name(),
                    label: Note(m).display_name(),
                })
                .collect()
        }
    };
    let mut last_midi = None;
    let mut out = Vec::with_capacity(n);
    for _ in 0..n {
        let out_of_set = use_other && rng.chance(p.out_of_set_rate as f64);
        let mut pc = if out_of_set {
            other_bag.draw(rng).unwrap()
        } else {
            bag.draw(rng).unwrap()
        };
        let (timbre, octave) = combos[combo_bag.draw(rng).unwrap()].clone();
        let mut midi = pc.in_octave(octave);
        if Some(midi) == last_midi && classes.len() > 1 && !out_of_set {
            pc = bag.draw(rng).unwrap();
            midi = pc.in_octave(octave);
        }
        last_midi = Some(midi);
        let gain = gain_db(p.loudness_db, rng);
        let correct = if out_of_set {
            "other".to_string()
        } else {
            match p.answer {
                PitchAnswer::PitchClass => pc.name().to_string(),
                PitchAnswer::NoteWithOctave => Note(midi).name(),
            }
        };
        let holdout = ctx.pools.is_holdout(&timbre, octave);
        out.push(TrialSpec::Identify(IdentifyTrial {
            exercise: ExerciseKind::PitchId,
            prompt: match p.answer {
                PitchAnswer::PitchClass => "Name the note".into(),
                PitchAnswer::NoteWithOctave => "Name the note and octave".into(),
            },
            events: vec![SoundEvent {
                midi,
                start_ms: 0.0,
                dur_ms: p.duration_ms as f64,
                timbre: timbre.clone(),
                gain_db: gain,
                detune_cents: p.detune_cents,
            }],
            mask_before: p.mask,
            choices: choices.clone(),
            correct,
            deadline_ms: p.deadline_ms,
            reference: None,
            stimulus: json!({
                "pitch_class": pc.name(),
                "octave": octave,
                "midi": midi,
                "timbre": timbre,
                "holdout": holdout,
                "out_of_set": out_of_set,
                "gain_db": gain,
                "mask": p.mask,
                "class_set_size": classes.len(),
            }),
        }));
    }
    Ok(out)
}

fn gen_sing_pitch(
    p: &SingPitchParams,
    n: usize,
    rng: &mut Rng,
    ctx: &GenContext,
) -> Result<Vec<TrialSpec>, GenError> {
    let classes = p.classes.classes();
    if classes.is_empty() {
        return err("classes must not be empty");
    }
    check_timbres(std::slice::from_ref(&p.reveal_timbre))?;
    let mut bag = BalancedBag::new(classes);
    let centre = (ctx.comfortable.low.0 + ctx.comfortable.high.0) / 2;
    let mut out = Vec::with_capacity(n);
    for _ in 0..n {
        let pc = bag.draw(rng).unwrap();
        let target_midi = p.octave.map(|o| pc.in_octave(o));
        let reveal_midi = target_midi.unwrap_or_else(|| {
            // The octave of the class nearest the middle of the comfortable range.
            let base = pc.index() as i32;
            let k = ((centre - base) as f32 / 12.0).round() as i32;
            base + 12 * k
        });
        let prompt = match target_midi {
            Some(m) => format!("Sing {}", Note(m).display_name()),
            None => format!("Sing {}", pc.display_name()),
        };
        out.push(TrialSpec::SingPitch(SingPitchTrial {
            prompt,
            target_class: pc,
            target_midi,
            response_ms: p.response_ms.max(1000),
            delay_ms: p.delay_ms,
            pre_mask: p.pre_mask,
            reveal: p
                .reveal_target_after
                .then(|| vec![ev(reveal_midi, 0.0, 1200.0, &p.reveal_timbre, -6.0)]),
            stimulus: json!({
                "pitch_class": pc.name(),
                "octave": p.octave,
                "mask": p.pre_mask,
                "delay_ms": p.delay_ms,
            }),
        }));
    }
    Ok(out)
}

fn gen_pairing(
    p: &PitchPairingParams,
    n: usize,
    rng: &mut Rng,
    ctx: &GenContext,
) -> Result<Vec<TrialSpec>, GenError> {
    let classes = p.classes.classes();
    if classes.is_empty() {
        return err("classes must not be empty");
    }
    let combos = pool_combos(&p.timbres, &p.octaves, p.pool, ctx)?;
    let mut bag = BalancedBag::new(classes);
    let mut combo_bag = BalancedBag::new((0..combos.len()).collect::<Vec<_>>());
    let mut out = Vec::with_capacity(n);
    for _ in 0..n {
        let pc = bag.draw(rng).unwrap();
        let (timbre, octave) = combos[combo_bag.draw(rng).unwrap()].clone();
        let midi = pc.in_octave(octave);
        out.push(TrialSpec::Pairing(PairingTrial {
            events: vec![ev(midi, 0.0, p.duration_ms as f64, &timbre, -6.0)],
            label: pc.display_name().to_string(),
            gap_ms: p.gap_ms,
            stimulus: json!({ "pitch_class": pc.name(), "octave": octave, "timbre": timbre }),
        }));
    }
    Ok(out)
}

/// I–IV–V–I (or i–iv–V–i) in close position as (bass, upper voices), tonic between F3 and E4.
fn cadence_chords(tonic: PitchClass, mode: KeyMode) -> [(i32, [i32; 3]); 4] {
    let mut r = tonic.in_octave(3);
    if r < 53 {
        r += 12;
    }
    let third = if mode == KeyMode::Major { 4 } else { 3 };
    let sixth = if mode == KeyMode::Major { 9 } else { 8 };
    [
        (r - 12, [r, r + third, r + 7]),
        (r - 7, [r, r + 5, r + sixth]),
        (r - 5, [r - 1, r + 2, r + 7]),
        (r - 12, [r, r + third, r + 7]),
    ]
}

/// I–IV–V–I (or i–iv–V–i) in close position with a bass line, tonic between F3 and E4.
fn cadence_events(
    tonic: PitchClass,
    mode: KeyMode,
    tempo_bpm: f32,
    timbre: &str,
    start_ms: f64,
) -> Vec<SoundEvent> {
    let chords = cadence_chords(tonic, mode);
    let beat = 60_000.0 / tempo_bpm as f64;
    let mut events = Vec::new();
    for (i, (bass, upper)) in chords.iter().enumerate() {
        let t = start_ms + i as f64 * beat;
        let dur = if i == 3 { beat * 2.0 } else { beat * 0.95 };
        events.push(ev(*bass, t, dur, timbre, -8.0));
        for &m in upper {
            events.push(ev(m, t, dur, timbre, -10.0));
        }
    }
    events
}

fn gen_tonal_center(
    p: &TonalCenterIdParams,
    n: usize,
    rng: &mut Rng,
) -> Result<Vec<TrialSpec>, GenError> {
    let keys = p.keys.classes();
    if keys.is_empty() || p.modes.is_empty() {
        return err("keys and modes must not be empty");
    }
    let mut bag = BalancedBag::new(keys);
    let mut out = Vec::with_capacity(n);
    for _ in 0..n {
        let tonic = bag.draw(rng).unwrap();
        let mode = *rng.choose(&p.modes).unwrap();
        let timbre = pick_timbre(&p.timbres, rng)?;
        let mut events = Vec::new();
        let mut start = 0.0;
        if p.reference {
            events.push(ev(69, 0.0, 1000.0, &timbre, -6.0));
            start = 1800.0;
        }
        events.extend(cadence_events(tonic, mode, p.tempo_bpm, &timbre, start));
        out.push(TrialSpec::Identify(IdentifyTrial {
            exercise: ExerciseKind::TonalCenterId,
            prompt: if p.reference {
                "The first tone is A. Name the key".into()
            } else {
                "Name the key (tonic)".into()
            },
            events,
            mask_before: p.mask,
            choices: pc_choices(&PitchClass::ALL, false),
            correct: tonic.name().to_string(),
            deadline_ms: None,
            reference: None,
            stimulus: json!({
                "tonic": tonic.name(),
                "mode": mode,
                "reference": p.reference,
                "timbre": timbre,
            }),
        }));
    }
    Ok(out)
}

// ---------------------------------------------------------------------------------------------
// Relative pitch
// ---------------------------------------------------------------------------------------------

fn gen_interval_id(
    p: &IntervalIdParams,
    n: usize,
    rng: &mut Rng,
) -> Result<Vec<TrialSpec>, GenError> {
    if p.intervals.is_empty() || p.directions.is_empty() {
        return err("intervals and directions must not be empty");
    }
    let range = p.root_range.unwrap_or(NoteRange::new(48, 72));
    let mut bag = BalancedBag::new(p.intervals.clone());
    let choices: Vec<Choice> = {
        let mut iv = p.intervals.clone();
        iv.sort();
        iv.dedup();
        iv.iter()
            .map(|i| Choice {
                id: i.id().into(),
                label: i.long_name().into(),
            })
            .collect()
    };
    let note = p.note_ms.max(200) as f64;
    let mut out = Vec::with_capacity(n);
    let mut last = None;
    for _ in 0..n {
        let iv: Interval = bag.draw(rng).unwrap();
        let dir = *rng.choose(&p.directions).unwrap();
        let low = pick_note(range, &mut last, rng);
        let high = low + iv.semitones();
        let timbre = pick_timbre(&p.timbres, rng)?;
        let events = match dir {
            IntervalDirection::Ascending => vec![
                ev(low, 0.0, note, &timbre, -6.0),
                ev(high, note, note, &timbre, -6.0),
            ],
            IntervalDirection::Descending => vec![
                ev(high, 0.0, note, &timbre, -6.0),
                ev(low, note, note, &timbre, -6.0),
            ],
            IntervalDirection::Harmonic => {
                vec![
                    ev(low, 0.0, note * 1.6, &timbre, -8.0),
                    ev(high, 0.0, note * 1.6, &timbre, -8.0),
                ]
            }
        };
        out.push(TrialSpec::Identify(IdentifyTrial {
            exercise: ExerciseKind::IntervalId,
            prompt: "Which interval?".into(),
            events,
            mask_before: Mask::None,
            choices: choices.clone(),
            correct: iv.id().into(),
            deadline_ms: None,
            reference: None,
            stimulus: json!({
                "interval": iv.id(),
                "semitones": iv.semitones(),
                "direction": dir,
                "low_midi": low,
                "timbre": timbre,
            }),
        }));
    }
    Ok(out)
}

fn inversion_label(i: u8) -> &'static str {
    match i {
        0 => "root position",
        1 => "1st inversion",
        2 => "2nd inversion",
        _ => "3rd inversion",
    }
}

fn gen_chord_id(p: &ChordIdParams, n: usize, rng: &mut Rng) -> Result<Vec<TrialSpec>, GenError> {
    if p.qualities.is_empty() {
        return err("qualities must not be empty");
    }
    let inversions: Vec<u8> = if p.inversions.is_empty() {
        vec![0]
    } else {
        p.inversions.clone()
    };
    let range = p.root_range.unwrap_or(NoteRange::new(48, 67));
    let mut choices = Vec::new();
    for q in &p.qualities {
        if p.answer_inversion {
            for &inv in &inversions {
                if (inv as usize) < q.intervals().len() {
                    choices.push(Choice {
                        id: format!("{}/{}", q.id(), inv),
                        label: format!("{}, {}", q.long_name(), inversion_label(inv)),
                    });
                }
            }
        } else if !choices.iter().any(|c: &Choice| c.id == q.id()) {
            choices.push(Choice {
                id: q.id().into(),
                label: q.long_name().into(),
            });
        }
    }
    let mut bag = BalancedBag::new(p.qualities.clone());
    let dur = p.duration_ms.max(300) as f64;
    let mut out = Vec::with_capacity(n);
    let mut last = None;
    for _ in 0..n {
        let q: ChordQuality = bag.draw(rng).unwrap();
        let valid: Vec<u8> = inversions
            .iter()
            .copied()
            .filter(|&i| (i as usize) < q.intervals().len())
            .collect();
        let inv = *rng.choose(&valid).unwrap_or(&0);
        let root = pick_note(range, &mut last, rng);
        let notes = q.voicing(root, inv as usize);
        let timbre = pick_timbre(&p.timbres, rng)?;
        let step = 380.0;
        let mut events = Vec::new();
        match p.style {
            ChordStyle::Block => {
                for &m in &notes {
                    events.push(ev(m, 0.0, dur, &timbre, -9.0));
                }
            }
            ChordStyle::Arpeggio => {
                for (i, &m) in notes.iter().enumerate() {
                    events.push(ev(m, i as f64 * step, dur, &timbre, -9.0));
                }
            }
            ChordStyle::BlockThenArpeggio => {
                for &m in &notes {
                    events.push(ev(m, 0.0, dur * 0.8, &timbre, -9.0));
                }
                let t0 = dur * 0.8 + 250.0;
                for (i, &m) in notes.iter().enumerate() {
                    events.push(ev(m, t0 + i as f64 * step, step * 1.1, &timbre, -9.0));
                }
            }
        }
        let correct = if p.answer_inversion {
            format!("{}/{}", q.id(), inv)
        } else {
            q.id().to_string()
        };
        out.push(TrialSpec::Identify(IdentifyTrial {
            exercise: ExerciseKind::ChordId,
            prompt: if p.answer_inversion {
                "Which chord and inversion?".into()
            } else {
                "Which chord?".into()
            },
            events,
            mask_before: Mask::None,
            choices: choices.clone(),
            correct,
            deadline_ms: None,
            reference: None,
            stimulus: json!({
                "quality": q.id(),
                "inversion": inv,
                "root_midi": root,
                "timbre": timbre,
                "style": p.style,
            }),
        }));
    }
    Ok(out)
}

fn gen_scale_id(p: &ScaleIdParams, n: usize, rng: &mut Rng) -> Result<Vec<TrialSpec>, GenError> {
    if p.scales.is_empty() {
        return err("scales must not be empty");
    }
    let range = p.root_range.unwrap_or(NoteRange::new(48, 62));
    let mut bag = BalancedBag::new(p.scales.clone());
    let choices: Vec<Choice> = {
        let mut seen: Vec<ScaleKind> = Vec::new();
        for s in &p.scales {
            if !seen.contains(s) {
                seen.push(*s);
            }
        }
        seen.iter()
            .map(|s| Choice {
                id: s.id().into(),
                label: s.long_name().into(),
            })
            .collect()
    };
    let step = 60_000.0 / p.tempo_bpm.clamp(40.0, 400.0) as f64;
    let mut out = Vec::with_capacity(n);
    let mut last = None;
    for _ in 0..n {
        let kind: ScaleKind = bag.draw(rng).unwrap();
        let root = pick_note(range, &mut last, rng);
        let up = kind.octave_notes(root);
        let notes: Vec<i32> = match p.direction {
            ScaleDirection::Ascending => up.clone(),
            ScaleDirection::Descending => up.iter().rev().copied().collect(),
            ScaleDirection::UpDown => up
                .iter()
                .copied()
                .chain(up.iter().rev().skip(1).copied())
                .collect(),
        };
        let timbre = pick_timbre(&p.timbres, rng)?;
        let events: Vec<SoundEvent> = notes
            .iter()
            .enumerate()
            .map(|(i, &m)| {
                let last_note = i + 1 == notes.len();
                ev(
                    m,
                    i as f64 * step,
                    if last_note { step * 2.0 } else { step * 0.95 },
                    &timbre,
                    -6.0,
                )
            })
            .collect();
        out.push(TrialSpec::Identify(IdentifyTrial {
            exercise: ExerciseKind::ScaleId,
            prompt: "Which scale?".into(),
            events,
            mask_before: Mask::None,
            choices: choices.clone(),
            correct: kind.id().into(),
            deadline_ms: None,
            reference: None,
            stimulus: json!({ "scale": kind.id(), "root_midi": root, "direction": p.direction, "timbre": timbre }),
        }));
    }
    Ok(out)
}

// ---------------------------------------------------------------------------------------------
// Singing
// ---------------------------------------------------------------------------------------------

fn sung_range(r: Option<NoteRange>, ctx: &GenContext) -> NoteRange {
    r.unwrap_or(ctx.comfortable)
}

fn gen_pitch_match(
    p: &PitchMatchParams,
    n: usize,
    rng: &mut Rng,
    ctx: &GenContext,
) -> Result<Vec<TrialSpec>, GenError> {
    let range = sung_range(p.range, ctx);
    let mut out = Vec::with_capacity(n);
    let mut last = None;
    for _ in 0..n {
        let midi = pick_note(range, &mut last, rng);
        let timbre = pick_timbre(&p.timbres, rng)?;
        // One beat = one second on this timeline.
        let ref_s = p.reference_ms.max(300) as f32 / 1000.0;
        let resp_s = p.response_ms.max(1000) as f32 / 1000.0;
        let (sing_start, reference, guide) = match p.mode {
            MatchMode::After => (
                ref_s + 0.3,
                vec![TargetNote {
                    midi,
                    start_beat: 0.0,
                    beats: ref_s,
                }],
                false,
            ),
            MatchMode::Unison => (0.5, vec![], true),
        };
        out.push(TrialSpec::Sung(SungTrial {
            exercise: ExerciseKind::PitchMatch,
            prompt: match p.mode {
                MatchMode::After => "Listen, then sing the note".into(),
                MatchMode::Unison => "Sing along with the note".into(),
            },
            notes: vec![TargetNote { midi, start_beat: 0.0, beats: resp_s }],
            tempo_bpm: 60.0,
            beats_per_bar: 4,
            sing_start_beat: sing_start,
            total_beats: sing_start + resp_s + 0.2,
            reference_beats: reference,
            reference_timbre: timbre.clone(),
            backing: Backing::None,
            backing_volume: 0.8,
            guide,
            drone_midi: None,
            lane_targets: LaneTargets::Shown,
            timed: false,
            score: None,
            count_in_beat: None,
            sections: Vec::new(),
            follow_cursor: false,
            stimulus: json!({ "midi": midi, "note": Note(midi).name(), "timbre": timbre, "mode": p.mode }),
        }));
    }
    Ok(out)
}

fn gen_sustain(
    p: &SustainParams,
    n: usize,
    rng: &mut Rng,
    ctx: &GenContext,
) -> Result<Vec<TrialSpec>, GenError> {
    check_timbres(std::slice::from_ref(&p.timbre))?;
    let range = sung_range(p.range, ctx);
    let mut out = Vec::with_capacity(n);
    let mut last = None;
    for _ in 0..n {
        let midi = pick_note(range, &mut last, rng);
        let hold_s = p.hold_ms.max(1000) as f32 / 1000.0;
        out.push(TrialSpec::Sung(SungTrial {
            exercise: ExerciseKind::Sustain,
            prompt: format!("Hold {} steadily", Note(midi).display_name()),
            notes: vec![TargetNote { midi, start_beat: 0.0, beats: hold_s }],
            tempo_bpm: 60.0,
            beats_per_bar: 4,
            sing_start_beat: 1.3,
            total_beats: 1.3 + hold_s + 0.5,
            reference_beats: vec![TargetNote { midi, start_beat: 0.0, beats: 1.0 }],
            reference_timbre: p.timbre.clone(),
            backing: if p.drone { Backing::Drone } else { Backing::None },
            backing_volume: 0.5,
            guide: false,
            drone_midi: p.drone.then_some(midi),
            lane_targets: LaneTargets::Shown,
            timed: false,
            score: None,
            count_in_beat: None,
            sections: Vec::new(),
            follow_cursor: false,
            stimulus: json!({ "midi": midi, "note": Note(midi).name(), "drone": p.drone, "hold_ms": p.hold_ms }),
        }));
    }
    Ok(out)
}

struct Timeline {
    reference: Vec<TargetNote>,
    sing_start: f32,
    total: f32,
    guide: bool,
}

/// Lays out reference, count-in and singing for notes given relative to the sing start.
fn timeline(notes: &[TargetNote], opts: &SungOptions, reference: ReferenceMode) -> Timeline {
    let bpb = opts.beats_per_bar.max(1) as f32;
    let len = notes
        .iter()
        .map(|n| n.start_beat + n.beats)
        .fold(0.0, f32::max);
    let count_in = opts.count_in_bars as f32 * bpb;
    let (reference_notes, sing_start, guide) = match reference {
        ReferenceMode::Echo => {
            let ref_bars = (len / bpb).ceil().max(1.0);
            (notes.to_vec(), ref_bars * bpb + count_in.max(bpb), false)
        }
        ReferenceMode::Guided => (vec![], count_in.max(bpb), true),
        ReferenceMode::FirstNote => {
            let first = notes[0].midi;
            let lead = count_in.max(bpb);
            (
                vec![TargetNote {
                    midi: first,
                    start_beat: 0.0,
                    beats: (lead - 0.5).max(1.0),
                }],
                lead,
                false,
            )
        }
        ReferenceMode::None => (vec![], count_in.max(bpb), false),
    };
    Timeline {
        reference: reference_notes,
        sing_start,
        total: sing_start + len.ceil() + 1.0,
        guide,
    }
}

fn gen_sing_interval(
    p: &SingIntervalParams,
    n: usize,
    rng: &mut Rng,
    ctx: &GenContext,
) -> Result<Vec<TrialSpec>, GenError> {
    if p.intervals.is_empty() || p.directions.is_empty() {
        return err("intervals and directions must not be empty");
    }
    check_timbres(std::slice::from_ref(&p.sung.timbre))?;
    let range = sung_range(p.range, ctx);
    let reference = p.sung.reference.unwrap_or(ReferenceMode::FirstNote);
    let mut bag = BalancedBag::new(p.intervals.clone());
    let mut out = Vec::with_capacity(n);
    let mut last = None;
    for _ in 0..n {
        let iv: Interval = bag.draw(rng).unwrap();
        let dir = *rng.choose(&p.directions).unwrap();
        let s = iv.semitones();
        if s > range.span() {
            return err(format!(
                "interval {} does not fit in the range {}–{}",
                iv.id(),
                range.low,
                range.high
            ));
        }
        let first = match dir {
            SingDirection::Up => pick_note(
                NoteRange::new(range.low.0, range.high.0 - s),
                &mut last,
                rng,
            ),
            SingDirection::Down => pick_note(
                NoteRange::new(range.low.0 + s, range.high.0),
                &mut last,
                rng,
            ),
        };
        let second = if dir == SingDirection::Up {
            first + s
        } else {
            first - s
        };
        let notes = vec![
            TargetNote {
                midi: first,
                start_beat: 0.0,
                beats: 2.0,
            },
            TargetNote {
                midi: second,
                start_beat: 2.0,
                beats: 2.0,
            },
        ];
        let tl = timeline(&notes, &p.sung, reference);
        out.push(TrialSpec::Sung(SungTrial {
            exercise: ExerciseKind::SingInterval,
            prompt: format!(
                "Sing a {} {} from {}",
                iv.long_name().to_lowercase(),
                if dir == SingDirection::Up {
                    "up"
                } else {
                    "down"
                },
                Note(first).display_name()
            ),
            notes,
            tempo_bpm: p.sung.tempo_bpm.clamp(30.0, 240.0),
            beats_per_bar: p.sung.beats_per_bar.max(1),
            sing_start_beat: tl.sing_start,
            total_beats: tl.total,
            reference_beats: tl.reference,
            reference_timbre: p.sung.timbre.clone(),
            backing: p.sung.backing,
            backing_volume: p.sung.backing_volume,
            guide: tl.guide || p.sung.backing.has_melody(),
            drone_midi: (p.sung.backing == Backing::Drone).then_some(first),
            lane_targets: p.sung.lane_targets,
            timed: true,
            score: None,
            count_in_beat: None,
            sections: Vec::new(),
            follow_cursor: false,
            stimulus: json!({
                "interval": iv.id(),
                "direction": dir,
                "first_midi": first,
                "reference": reference,
                "backing": p.sung.backing,
            }),
        }));
    }
    Ok(out)
}

fn shape_offsets(up: Vec<i32>, shape: Shape) -> Vec<i32> {
    match shape {
        Shape::Up => up,
        Shape::Down => up.into_iter().rev().collect(),
        Shape::UpDown => {
            let back: Vec<i32> = up.iter().rev().skip(1).copied().collect();
            up.into_iter().chain(back).collect()
        }
    }
}

/// Semitone offsets of a pattern relative to its tonic, and a short description.
pub fn pattern_offsets(spec: &PatternSpec) -> Result<(Vec<i32>, String), GenError> {
    Ok(match spec {
        PatternSpec::Scale {
            kind,
            shape,
            octaves,
        } => {
            let octaves = (*octaves).clamp(1, 2) as i32;
            let mut up = Vec::new();
            for o in 0..octaves {
                up.extend(kind.steps().iter().map(|s| s + 12 * o));
            }
            up.push(12 * octaves);
            let desc = format!("{} scale{}", kind.long_name(), shape_label(*shape));
            (shape_offsets(up, *shape), desc)
        }
        PatternSpec::Arpeggio { quality, shape } => {
            let mut up: Vec<i32> = quality.intervals().to_vec();
            up.push(12);
            (
                shape_offsets(up, *shape),
                format!("{} arpeggio{}", quality.long_name(), shape_label(*shape)),
            )
        }
        PatternSpec::Degrees { degrees, scale } => {
            if degrees.is_empty() {
                return err("degrees must not be empty");
            }
            let v: Vec<i32> = degrees.iter().map(|&d| scale.degree_midi(0, d)).collect();
            let label = degrees
                .iter()
                .map(|d| d.to_string())
                .collect::<Vec<_>>()
                .join("-");
            (
                v,
                format!("Degrees {label} ({})", scale.long_name().to_lowercase()),
            )
        }
        PatternSpec::Notes { notes } => {
            if notes.is_empty() {
                return err("notes must not be empty");
            }
            let first = notes[0].0;
            (
                notes.iter().map(|n| n.0 - first).collect(),
                "Melody".to_string(),
            )
        }
    })
}

fn shape_label(shape: Shape) -> &'static str {
    match shape {
        Shape::Up => ", ascending",
        Shape::Down => ", descending",
        Shape::UpDown => ", up and down",
    }
}

fn gen_pattern(
    p: &PatternParams,
    n: usize,
    rng: &mut Rng,
    ctx: &GenContext,
) -> Result<Vec<TrialSpec>, GenError> {
    check_timbres(std::slice::from_ref(&p.sung.timbre))?;
    let (offsets, desc) = pattern_offsets(&p.pattern)?;
    let lo = *offsets.iter().min().unwrap();
    let hi = *offsets.iter().max().unwrap();
    let range = sung_range(p.range, ctx);
    let fixed_start = match (&p.pattern, p.keys) {
        (_, KeyPlan::Fixed) => Some(p.start.map(|s| s.0).unwrap_or(match &p.pattern {
            PatternSpec::Notes { notes } => notes[0].0,
            _ => range.low.0 - lo,
        })),
        _ => None,
    };
    let tonic_lo = range.low.0 - lo;
    let tonic_hi = range.high.0 - hi;
    if fixed_start.is_none() && tonic_hi < tonic_lo {
        return err(format!(
            "the pattern spans {} semitones, which does not fit the range {}–{}",
            hi - lo,
            range.low,
            range.high
        ));
    }
    let reference = p.sung.reference.unwrap_or(ReferenceMode::Echo);
    let bpn = p.beats_per_note.clamp(0.25, 8.0);
    let mut tonic = match p.keys {
        KeyPlan::Ascending => p
            .start
            .map(|s| s.0)
            .unwrap_or(tonic_lo)
            .clamp(tonic_lo, tonic_hi.max(tonic_lo)),
        KeyPlan::Descending => p
            .start
            .map(|s| s.0)
            .unwrap_or(tonic_hi)
            .clamp(tonic_lo, tonic_hi.max(tonic_lo)),
        _ => tonic_lo,
    };
    let mut last = None;
    let mut out = Vec::with_capacity(n);
    for i in 0..n {
        let t = match p.keys {
            KeyPlan::Fixed => fixed_start.unwrap(),
            KeyPlan::Random => pick_note(NoteRange::new(tonic_lo, tonic_hi), &mut last, rng),
            KeyPlan::Ascending => {
                if i > 0 {
                    tonic = if tonic + 1 > tonic_hi {
                        tonic_lo
                    } else {
                        tonic + 1
                    };
                }
                tonic
            }
            KeyPlan::Descending => {
                if i > 0 {
                    tonic = if tonic - 1 < tonic_lo {
                        tonic_hi
                    } else {
                        tonic - 1
                    };
                }
                tonic
            }
        };
        let notes: Vec<TargetNote> = offsets
            .iter()
            .enumerate()
            .map(|(k, off)| TargetNote {
                midi: t + off,
                start_beat: k as f32 * bpn,
                beats: bpn,
            })
            .collect();
        let tl = timeline(&notes, &p.sung, reference);
        let drone = (p.sung.backing == Backing::Drone).then(|| {
            let mut d = t;
            while d > range.low.0 {
                d -= 12;
            }
            d.max(t - 12)
        });
        out.push(TrialSpec::Sung(SungTrial {
            exercise: ExerciseKind::Pattern,
            prompt: format!("{desc}, from {}", Note(t + offsets[0]).display_name()),
            notes,
            tempo_bpm: p.sung.tempo_bpm.clamp(30.0, 240.0),
            beats_per_bar: p.sung.beats_per_bar.max(1),
            sing_start_beat: tl.sing_start,
            total_beats: tl.total,
            reference_beats: tl.reference,
            reference_timbre: p.sung.timbre.clone(),
            backing: p.sung.backing,
            backing_volume: p.sung.backing_volume,
            guide: tl.guide || p.sung.backing.has_melody(),
            drone_midi: drone,
            lane_targets: p.sung.lane_targets,
            timed: true,
            score: None,
            count_in_beat: None,
            sections: Vec::new(),
            follow_cursor: false,
            stimulus: json!({
                "pattern": desc,
                "tonic_midi": t,
                "tempo_bpm": p.sung.tempo_bpm,
                "reference": reference,
                "backing": p.sung.backing,
                "notes": offsets.len(),
            }),
        }));
    }
    Ok(out)
}

// ---------------------------------------------------------------------------------------------
// Sight singing
// ---------------------------------------------------------------------------------------------

/// The melody for trial `i`: generated, or the coach's melody `i mod n`.
fn melody_for(
    source: &MelodySource,
    i: usize,
    tempo_bpm: f32,
    rng: &mut Rng,
    ctx: &GenContext,
) -> Result<Score, GenError> {
    match source {
        MelodySource::Generate(m) => {
            generate_melody(m, ctx.comfortable, tempo_bpm, rng).map_err(|e| GenError(format!("melody: {e}")))
        }
        MelodySource::Given(list) => {
            if list.is_empty() {
                return err("melody.given must contain at least one melody");
            }
            let k = i % list.len();
            Score::from_compact(&list[k], tempo_bpm).map_err(|e| GenError(format!("melody.given[{k}]: {e}")))
        }
    }
}

/// Stimulus fields shared by sight singing and dictation.
fn score_stimulus(score: &Score) -> serde_json::Value {
    let (lo, hi) = score.range().unwrap_or((0, 0));
    let pitched: Vec<_> = score.pitched().map(|(_, p)| p).collect();
    let largest_leap = pitched
        .windows(2)
        .map(|w| (w[1].diatonic() - w[0].diatonic()).abs())
        .max()
        .unwrap_or(0);
    json!({
        "key": score.key.to_string(),
        "mode": score.key.mode,
        "time": score.time,
        "clef": score.clef,
        "bars": score.bars,
        "tempo_bpm": score.tempo_bpm,
        "notes": pitched.len(),
        "low": Note(lo).name(),
        "high": Note(hi).name(),
        "largest_leap_steps": largest_leap,
        "melody": score.pitch_names(),
    })
}

fn gen_sight_sing(
    p: &SightSingParams,
    n: usize,
    rng: &mut Rng,
    ctx: &GenContext,
) -> Result<Vec<TrialSpec>, GenError> {
    check_timbres(std::slice::from_ref(&p.timbre))?;
    if !(30.0..=220.0).contains(&p.tempo_bpm) {
        return err("tempo_bpm must be 30–220");
    }
    if p.backing.has_melody() {
        return err("backing `melody` would sing the answer; use `click`, `chords`, `drone` or `none` (Sing along is offered after each attempt)");
    }
    let mut out = Vec::with_capacity(n);
    for i in 0..n {
        let score = melody_for(&p.melody, i, p.tempo_bpm, rng, ctx)?;
        out.push(TrialSpec::Sung(sight_trial(score, p)));
    }
    Ok(out)
}

/// What plays before and with a notated melody.
struct ScoreTimeline<'a> {
    exercise: ExerciseKind,
    key: bool,
    first_note: bool,
    count_in_bars: u32,
    backing: Backing,
    backing_volume: f32,
    timbre: &'a str,
    /// Play the melody itself (dictation), not just the lead-in.
    guide: bool,
    follow_cursor: bool,
    /// Label of the section in which the melody runs.
    main_label: &'a str,
}

/// Lays out the cadence, first note, count-in and the melody's own stretch for a notated melody.
fn score_timeline(score: Score, o: &ScoreTimeline) -> SungTrial {
    let bpb = score.time.beats.max(1);
    let tonic = PitchClass::new(score.key.tonic_pc());
    let targets = score.targets();
    let mut reference = Vec::new();
    let mut sections = Vec::new();
    let mut t = 0.0f32;
    if o.key {
        for (k, (bass, upper)) in cadence_chords(tonic, score.key.mode).iter().enumerate() {
            let beats = if k == 3 { 2.0 } else { 0.95 };
            reference.push(TargetNote { midi: *bass, start_beat: t + k as f32, beats });
            for m in upper {
                reference.push(TargetNote { midi: *m, start_beat: t + k as f32, beats });
            }
        }
        sections.push(TimelineSection { label: "Establish key".into(), start_beat: 0.0, end_beat: 5.0 });
        t = 6.0;
    }
    if o.first_note {
        if let Some(first) = targets.first() {
            reference.push(TargetNote { midi: first.midi, start_beat: t, beats: 2.0 });
            sections.push(TimelineSection { label: "First note".into(), start_beat: t, end_beat: t + 2.0 });
            t += 3.0;
        }
    }
    let count_in = t.ceil();
    let sing_start = count_in + (o.count_in_bars * bpb) as f32;
    if o.count_in_bars > 0 {
        sections.push(TimelineSection { label: "Count-in".into(), start_beat: count_in, end_beat: sing_start });
    }
    let length = score.total_beats();
    sections.push(TimelineSection { label: o.main_label.into(), start_beat: sing_start, end_beat: sing_start + length });
    let drone = (o.backing == Backing::Drone).then(|| {
        let lo = score.range().map(|r| r.0).unwrap_or(48);
        let mut d = tonic.in_octave(2);
        while d + 12 <= lo - 5 {
            d += 12;
        }
        d
    });
    let stimulus = score_stimulus(&score);
    SungTrial {
        exercise: o.exercise,
        prompt: score.title.clone().unwrap_or_else(|| "Sing the melody".into()),
        notes: targets,
        tempo_bpm: score.tempo_bpm.clamp(30.0, 220.0),
        beats_per_bar: bpb,
        sing_start_beat: sing_start,
        total_beats: sing_start + length + 1.0,
        reference_beats: reference,
        reference_timbre: o.timbre.to_string(),
        backing: o.backing,
        backing_volume: o.backing_volume,
        guide: o.guide,
        drone_midi: drone,
        lane_targets: LaneTargets::HiddenUntilDone,
        timed: true,
        score: Some(score),
        count_in_beat: Some(count_in),
        sections,
        follow_cursor: o.follow_cursor,
        stimulus,
    }
}

fn sight_trial(score: Score, p: &SightSingParams) -> SungTrial {
    let mut t = score_timeline(
        score,
        &ScoreTimeline {
            exercise: ExerciseKind::SightSing,
            key: p.start.plays_key(),
            first_note: p.start.plays_first_note(),
            count_in_bars: p.count_in_bars,
            backing: p.backing,
            backing_volume: p.backing_volume,
            timbre: &p.timbre,
            guide: false,
            follow_cursor: p.follow_cursor,
            main_label: "Sing",
        },
    );
    t.stimulus["start"] = json!(p.start);
    t.stimulus["backing"] = json!(p.backing);
    t
}

// ---------------------------------------------------------------------------------------------
// Melodic dictation
// ---------------------------------------------------------------------------------------------

fn gen_dictation(
    p: &DictationParams,
    n: usize,
    rng: &mut Rng,
    ctx: &GenContext,
) -> Result<Vec<TrialSpec>, GenError> {
    check_timbres(std::slice::from_ref(&p.timbre))?;
    if !(30.0..=220.0).contains(&p.tempo_bpm) {
        return err("tempo_bpm must be 30–220");
    }
    let mut out = Vec::with_capacity(n);
    for i in 0..n {
        let score = melody_for(&p.melody, i, p.tempo_bpm, rng, ctx)?;
        // The editor starts with the first note (and any rest before it) when it is given.
        let mut given = score.clone();
        given.title = None;
        given.notes = if p.give_first_note {
            let first = score.notes.iter().position(|n| n.pitch.is_some()).unwrap_or(0);
            let mut lead: Vec<ScoreNote> = score.notes[..=first].to_vec();
            if let Some(f) = lead.first() {
                if f.start > 0 {
                    lead.insert(0, ScoreNote { pitch: None, start: 0, dur: f.start });
                }
            }
            lead
        } else {
            Vec::new()
        };
        let mut playback = score_timeline(
            score.clone(),
            &ScoreTimeline {
                exercise: ExerciseKind::MelodicDictation,
                key: p.play_key,
                first_note: false,
                count_in_bars: 1,
                backing: if p.click { Backing::Click } else { Backing::None },
                backing_volume: 0.6,
                timbre: &p.timbre,
                guide: true,
                follow_cursor: false,
                main_label: "Melody",
            },
        );
        playback.score = None;
        let mut stimulus = score_stimulus(&score);
        stimulus["first_note_given"] = json!(p.give_first_note);
        stimulus["grade_rhythm"] = json!(p.grade_rhythm);
        out.push(TrialSpec::Dictation(DictationTrial {
            prompt: "Write the melody".into(),
            score,
            given,
            playback,
            max_plays: p.max_plays,
            grade_rhythm: p.grade_rhythm,
            stimulus,
        }));
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn block(json: &str) -> BlockSpec {
        let mut v: serde_json::Value = serde_json::from_str(json).unwrap();
        v.as_object_mut()
            .unwrap()
            .entry("params")
            .or_insert(json!({}));
        serde_json::from_value(v).unwrap()
    }

    #[test]
    fn generation_is_deterministic() {
        let b = block(r#"{"type": "pitch_id", "count": 30}"#);
        let ctx = GenContext::default();
        let a = generate_block(&b, 99, &ctx).unwrap();
        let c = generate_block(&b, 99, &ctx).unwrap();
        assert_eq!(a, c);
        let d = generate_block(&b, 100, &ctx).unwrap();
        assert_ne!(a, d);
    }

    #[test]
    fn pitch_id_respects_pools_and_classes() {
        let ctx = GenContext::default();
        let b = block(
            r#"{"type": "pitch_id", "count": 120, "params": {"classes": ["C", "E", "G#"], "out_of_set_rate": 0.2, "octaves": [2, 3, 4, 5, 6]}}"#,
        );
        let trials = generate_block(&b, 5, &ctx).unwrap();
        let mut others = 0;
        for t in &trials {
            let TrialSpec::Identify(t) = t else { panic!() };
            let oct = t.stimulus["octave"].as_i64().unwrap() as i32;
            assert!(
                !ctx.pools.is_holdout_octave(oct),
                "training pool used holdout octave {oct}"
            );
            assert!(
                !ctx.pools
                    .is_holdout_timbre(t.stimulus["timbre"].as_str().unwrap())
            );
            assert_eq!(t.choices.len(), 4);
            if t.correct == "other" {
                others += 1;
                assert!(!["C", "E", "G#"].contains(&t.stimulus["pitch_class"].as_str().unwrap()));
            } else {
                assert!(["C", "E", "G#"].contains(&t.correct.as_str()));
            }
        }
        assert!((10..=45).contains(&others), "{others}");

        let h = block(r#"{"type": "pitch_id", "count": 40, "params": {"pool": "holdout"}}"#);
        for t in generate_block(&h, 5, &ctx).unwrap() {
            let TrialSpec::Identify(t) = t else { panic!() };
            assert_eq!(t.stimulus["holdout"], json!(true));
        }

        let bad = block(r#"{"type": "pitch_id", "params": {"timbres": ["gm:1"]}}"#);
        assert!(
            generate_block(&bad, 1, &ctx).is_err(),
            "gm:1 is holdout-only"
        );
    }

    #[test]
    fn sung_patterns_fit_the_range_and_lay_out_echo() {
        let ctx = GenContext {
            comfortable: NoteRange::new(50, 69),
            ..Default::default()
        };
        let b = block(
            r#"{"type": "pattern", "count": 10, "params": {"pattern": {"scale": {"kind": "major", "shape": "up_down"}}, "tempo_bpm": 90}}"#,
        );
        for t in generate_block(&b, 1, &ctx).unwrap() {
            let TrialSpec::Sung(t) = t else { panic!() };
            assert_eq!(t.notes.len(), 15);
            assert!(t.notes.iter().all(|n| (50..=69).contains(&n.midi)));
            // Echo: the reference fills four bars (15 beats), then a one-bar count-in.
            assert_eq!(t.reference_beats.len(), 15);
            assert_eq!(t.sing_start_beat, 20.0);
            assert!(t.total_beats >= t.sing_start_beat + 15.0);
        }
        let too_big = block(
            r#"{"type": "pattern", "params": {"pattern": {"scale": {"kind": "major", "octaves": 2}}, "range": {"low": "C4", "high": "G4"}}}"#,
        );
        assert!(generate_block(&too_big, 1, &ctx).is_err());
    }

    #[test]
    fn ascending_keys_move_up_by_semitones() {
        let ctx = GenContext {
            comfortable: NoteRange::new(48, 72),
            ..Default::default()
        };
        let b = block(
            r#"{"type": "pattern", "count": 4, "params": {"pattern": {"arpeggio": {"quality": "major"}}, "keys": "ascending", "start": "C3"}}"#,
        );
        let tonics: Vec<i32> = generate_block(&b, 1, &ctx)
            .unwrap()
            .iter()
            .map(|t| match t {
                TrialSpec::Sung(s) => s.notes[0].midi,
                _ => panic!(),
            })
            .collect();
        assert_eq!(tonics, vec![48, 49, 50, 51]);
    }

    #[test]
    fn sing_interval_uses_first_note_reference() {
        let ctx = GenContext::default();
        let b = block(r#"{"type": "sing_interval", "count": 12}"#);
        for t in generate_block(&b, 3, &ctx).unwrap() {
            let TrialSpec::Sung(t) = t else { panic!() };
            assert_eq!(t.reference_beats.len(), 1);
            assert_eq!(t.reference_beats[0].midi, t.notes[0].midi);
            assert!(
                ctx.comfortable.contains(t.notes[0].midi)
                    && ctx.comfortable.contains(t.notes[1].midi)
            );
            assert_eq!(default_mode(&TrialSpec::Sung(t)), PitchRefMode::Absolute);
        }
    }

    #[test]
    fn identification_generators_produce_valid_answers() {
        let ctx = GenContext::default();
        for json in [
            r#"{"type": "interval_id", "count": 24}"#,
            r#"{"type": "chord_id", "count": 24, "params": {"inversions": [0, 1, 2], "answer_inversion": true}}"#,
            r#"{"type": "scale_id", "count": 24}"#,
            r#"{"type": "tonal_center_id", "count": 24, "params": {"modes": ["major", "minor"]}}"#,
        ] {
            let b = block(json);
            for t in generate_block(&b, 8, &ctx).unwrap() {
                let TrialSpec::Identify(t) = t else { panic!() };
                assert!(
                    t.choices.iter().any(|c| c.id == t.correct),
                    "{json}: {} not in choices",
                    t.correct
                );
                assert!(!t.events.is_empty());
            }
        }
    }

    #[test]
    fn sight_singing_trials_lay_out_key_first_note_and_count_in() {
        let b = block(r#"{"type": "sight_sing", "count": 5, "seed": 7, "params": {"tempo_bpm": 90}}"#);
        let trials = generate_block(&b, 7, &GenContext::default()).unwrap();
        assert_eq!(trials.len(), 5);
        for t in &trials {
            let TrialSpec::Sung(t) = t else { panic!("not sung") };
            let score = t.score.as_ref().expect("score");
            assert_eq!(t.exercise, ExerciseKind::SightSing);
            assert_eq!(t.notes.len(), score.pitched().count());
            // Cadence (4 chords of 4 notes) plus the first note.
            assert_eq!(t.reference_beats.len(), 17);
            assert_eq!(t.reference_beats.last().unwrap().midi, t.notes[0].midi);
            assert_eq!(t.count_in_beat, Some(9.0));
            assert_eq!(t.sing_start_beat, 9.0 + score.time.beats as f32);
            let labels: Vec<&str> = t.sections.iter().map(|s| s.label.as_str()).collect();
            assert_eq!(labels, ["Establish key", "First note", "Count-in", "Sing"]);
            assert_eq!(default_mode(&TrialSpec::Sung(t.clone())), PitchRefMode::Absolute);
        }
    }

    #[test]
    fn sight_singing_uses_coach_melodies_and_relative_grading_without_a_start() {
        let b = block(r#"{"type": "sight_sing", "count": 3, "params": {"start": "none", "melody": {"given": [
            {"key": "F major", "time": "3/4", "notes": [{"note": "C4", "beats": 1}, {"note": "F4", "beats": 2}, {"note": "E4", "beats": 1}, {"note": "F4", "beats": 2}]}
        ]}}}"#);
        let trials = generate_block(&b, 1, &GenContext::default()).unwrap();
        let TrialSpec::Sung(t) = &trials[2] else { panic!() };
        assert!(t.reference_beats.is_empty());
        assert_eq!(t.count_in_beat, Some(0.0));
        assert_eq!(t.sing_start_beat, 3.0);
        assert_eq!(t.notes.iter().map(|n| n.midi).collect::<Vec<_>>(), [60, 65, 64, 65]);
        assert_eq!(default_mode(&trials[2]), PitchRefMode::RelativeToFirst);
        let bad = block(r#"{"type": "sight_sing", "count": 1, "params": {"backing": "melody"}}"#);
        assert!(generate_block(&bad, 1, &GenContext::default()).is_err());
    }

    #[test]
    fn dictation_trials_give_the_first_note_and_play_the_melody() {
        let b = block(r#"{"type": "melodic_dictation", "count": 4, "params": {"max_plays": 3}}"#);
        let trials = generate_block(&b, 3, &GenContext::default()).unwrap();
        for t in &trials {
            let TrialSpec::Dictation(t) = t else { panic!("not dictation") };
            assert_eq!(t.score.bars, 2);
            assert_eq!(t.given.notes.len(), 1);
            assert_eq!(t.given.notes[0], t.score.notes[0]);
            assert_eq!(t.given.key, t.score.key);
            assert!(t.playback.guide);
            assert!(t.playback.score.is_none());
            assert_eq!(t.playback.notes.len(), t.score.pitched().count());
            assert_eq!(t.max_plays, 3);
            assert_eq!(t.playback.sections.last().unwrap().label, "Melody");
        }
        let b = block(r#"{"type": "melodic_dictation", "count": 1, "params": {"give_first_note": false, "play_key": false}}"#);
        let TrialSpec::Dictation(t) = &generate_block(&b, 1, &GenContext::default()).unwrap()[0] else { panic!() };
        assert!(t.given.notes.is_empty());
        assert!(t.playback.reference_beats.is_empty());
    }
}
