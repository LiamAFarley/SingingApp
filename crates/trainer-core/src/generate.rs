//! Deterministic trial generation: (block spec, seed, context) → concrete trials.

mod audiation;

use serde_json::json;

use crate::library::{self, LibraryPiece};
use crate::melody::{MelodyParams, generate_melody};
use crate::rng::{BalancedBag, Rng, WeightedBag};
use crate::schema::*;
use crate::score::{ClefChoice, CompactMelody, KeySig, Pitch, Score, ScoreNote, note_events};
use crate::theory::{ChordQuality, Interval, Note, PitchClass, ScaleKind};
use crate::timbre::{
    PoolConfig, TimbreId, default_training_timbres, is_valid_timbre, octave_usable,
    soundfont_installed,
};

#[derive(Clone, Debug)]
pub struct GenContext {
    /// The singer's comfortable range (default C3–G4 until the user sets one).
    pub comfortable: NoteRange,
    pub pools: PoolConfig,
    /// Saved anchor songs (for `anchor_song`).
    pub anchors: Vec<Anchor>,
    /// The melody library, in natural order of id.
    pub library: Vec<LibraryPiece>,
    /// Saved label sets. Empty falls back to the built-in sets.
    pub labels: Vec<crate::labels::LabelSet>,
    /// File stems of the extra SoundFont banks that are installed; `None` when not known.
    pub soundfonts: Option<Vec<String>>,
    /// Recent error rate per answer, keyed `exercise:answer` (`scale_degree_id:b6`), for
    /// adaptive drills.
    pub weak: std::collections::BTreeMap<String, f64>,
}

impl Default for GenContext {
    fn default() -> Self {
        GenContext {
            comfortable: NoteRange::new(48, 67),
            pools: PoolConfig::default(),
            anchors: Vec::new(),
            library: Vec::new(),
            labels: Vec::new(),
            soundfonts: None,
            weak: std::collections::BTreeMap::new(),
        }
    }
}

impl GenContext {
    pub fn label_set(&self, id: &str) -> Option<crate::labels::LabelSet> {
        self.labels
            .iter()
            .find(|s| s.id == id)
            .cloned()
            .or_else(|| crate::labels::builtin_set(id))
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
    let mut trials = match &block.exercise {
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
        Exercise::AnchorSong(p) => gen_anchor_song(p, n, &mut rng, ctx),
        Exercise::IncidentalPairing(p) => gen_incidental(p, n, &mut rng, ctx),
        Exercise::ScaleDegreeId(p) => gen_scale_degree(p, n, &mut rng, ctx),
        Exercise::IntervalComparison(p) => gen_interval_comparison(p, n, &mut rng),
        Exercise::Harmony(p) => gen_harmony(p, n, &mut rng, ctx),
        Exercise::PitchDetect(p) => gen_pitch_detect(p, n, &mut rng, ctx),
        Exercise::ChordColorId(p) => gen_chord_color(p, n, &mut rng, ctx),
        Exercise::RestingTone(p) => audiation::gen_resting_tone(p, n, &mut rng, ctx),
        Exercise::TonalPattern(p) => audiation::gen_tonal_pattern(p, n, &mut rng, ctx),
        Exercise::PatternDialogue(p) => audiation::gen_pattern_dialogue(p, n, &mut rng, ctx),
        Exercise::NBack(p) => audiation::gen_n_back(p, n, &mut rng, ctx),
        Exercise::MelodicUpdating(p) => audiation::gen_melodic_updating(p, n, &mut rng, ctx),
        Exercise::PatternMemory(p) => audiation::gen_pattern_memory(p, n, &mut rng),
        Exercise::ErrorDetection(p) => audiation::gen_error_detection(p, n, &mut rng, ctx),
        Exercise::SyllableCheck(p) => audiation::gen_syllable_check(p, n, &mut rng),
        Exercise::CadenceId(p) => audiation::gen_cadence_id(p, n, &mut rng),
        Exercise::BassLine(p) => audiation::gen_bass_line(p, n, &mut rng, ctx),
    }?;
    if block.effective_feedback().shows_trial_result() {
        color_answers(&mut trials, ctx);
    }
    Ok(trials)
}

/// Absolute-pitch trials that show their answers also show each note's colour: the buttons are
/// tinted, and the played note's colour sits behind the result, so naming a note reinforces its
/// colour too. Colours a label set already gives are kept.
fn color_answers(trials: &mut [TrialSpec], ctx: &GenContext) {
    let Some(set) = ctx.label_set(crate::labels::TANEDA_COLORS) else {
        return;
    };
    let color_of =
        |pc: PitchClass| crate::labels::class_entry(&set, pc).and_then(|e| e.color.clone());
    // "C#" or "C#4".
    let class_of = |id: &str| -> Option<PitchClass> {
        id.trim_end_matches(|c: char| c.is_ascii_digit() || c == '-')
            .parse()
            .ok()
    };
    let played = |stimulus: &serde_json::Value| {
        stimulus
            .get("pitch_class")
            .and_then(|v| v.as_str())
            .and_then(|s| s.parse::<PitchClass>().ok())
    };
    for trial in trials {
        match trial {
            TrialSpec::Identify(t)
                if matches!(
                    t.exercise,
                    ExerciseKind::PitchId | ExerciseKind::TonalCenterId
                ) =>
            {
                for c in &mut t.choices {
                    if c.color.is_none() {
                        c.color = class_of(&c.id).and_then(color_of);
                    }
                }
                if t.flash_color.is_none() {
                    t.answer_color = played(&t.stimulus)
                        .or_else(|| class_of(&t.correct))
                        .and_then(color_of);
                }
            }
            TrialSpec::SingPitch(t) => t.answer_color = color_of(t.target_class),
            TrialSpec::Pairing(t) if t.flash_color.is_none() => {
                t.flash_color = played(&t.stimulus).and_then(color_of);
            }
            TrialSpec::Detect(t)
                if t.exercise == ExerciseKind::PitchDetect && t.target_color.is_none() =>
            {
                t.target_color = color_of(t.target_class);
            }
            _ => {}
        }
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
            } else if t.exercise == ExerciseKind::Harmony {
                // Against another voice the octave changes the interval.
                PitchRefMode::Absolute
            } else {
                // Not absolute pitch: the line may be sung in another octave.
                PitchRefMode::AnyOctave
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

/// (timbre, octave) combinations allowed by the pool. Octaves a timbre cannot play in tune are
/// left out for that timbre, and so are SoundFont banks that are not installed.
fn pool_combos(
    timbres: &[TimbreId],
    octaves: &[i32],
    pool: Pool,
    ctx: &GenContext,
) -> Result<Vec<(TimbreId, i32)>, GenError> {
    let installed = ctx.soundfonts.as_deref();
    let timbres: Vec<TimbreId> = if timbres.is_empty() {
        match pool {
            Pool::Holdout => ctx.pools.holdout_timbres.clone(),
            _ => default_training_timbres(installed),
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
    let missing: Vec<&TimbreId> = timbres
        .iter()
        .filter(|t| !soundfont_installed(t, installed))
        .collect();
    let mut combos = Vec::new();
    for t in timbres.iter().filter(|t| soundfont_installed(t, installed)) {
        for &o in octaves {
            if !octave_usable(t, o) {
                continue;
            }
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
        if !missing.is_empty() {
            return err(format!(
                "no stimuli: these SoundFont timbres are not installed: {}",
                missing
                    .iter()
                    .map(|t| t.as_str())
                    .collect::<Vec<_>>()
                    .join(", ")
            ));
        }
        return err(format!(
            "no {} stimuli: every timbre/octave combination is excluded by the pool or by the timbres' ranges (holdout timbres: {}; holdout octaves: {:?})",
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

/// The answer id for a tone from outside a training subset.
pub const OTHER: &str = "other";

fn pc_choices(classes: &[PitchClass]) -> Vec<Choice> {
    let mut sorted = classes.to_vec();
    sorted.sort();
    sorted
        .iter()
        .map(|p| Choice::new(p.name().to_string(), p.display_name().to_string()))
        .collect()
}

/// The note of class `pc` nearest to `midi` (the lower one on a tie).
fn nearest_of_class(pc: PitchClass, midi: i32) -> i32 {
    let below = midi - (midi - pc.index() as i32).rem_euclid(12);
    if midi - below > 6 { below + 12 } else { below }
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

/// The classes a naming block draws: evenly, or by `class_weights`.
enum ClassDraw {
    Even(BalancedBag<PitchClass>),
    Weighted(WeightedBag<PitchClass>),
}

impl ClassDraw {
    fn new(
        classes: &[PitchClass],
        weights: &std::collections::BTreeMap<String, f32>,
    ) -> Result<Self, GenError> {
        if weights.is_empty() {
            return Ok(ClassDraw::Even(BalancedBag::allowing_repeats(
                classes.to_vec(),
            )));
        }
        let mut listed: Vec<(PitchClass, f64)> = Vec::new();
        for (k, w) in weights {
            let pc: PitchClass = k
                .parse()
                .map_err(|_| GenError(format!("class_weights: `{k}` is not a pitch class")))?;
            if !classes.contains(&pc) {
                return err(format!(
                    "class_weights: {k} is not one of the block's classes"
                ));
            }
            if !(*w >= 0.0 && w.is_finite()) {
                return err(format!(
                    "class_weights: the weight of {k} must be zero or more"
                ));
            }
            listed.push((pc, *w as f64));
        }
        let listed_sum: f64 = listed.iter().map(|l| l.1).sum();
        let rest: Vec<PitchClass> = classes
            .iter()
            .copied()
            .filter(|c| !listed.iter().any(|l| l.0 == *c))
            .collect();
        let mut all = listed;
        if !rest.is_empty() {
            if listed_sum > 1.0 + 1e-6 {
                return err(
                    "class_weights: the listed weights add up to more than 1, leaving nothing for the other classes",
                );
            }
            let each = (1.0 - listed_sum) / rest.len() as f64;
            all.extend(rest.into_iter().map(|c| (c, each)));
        }
        if all.iter().map(|a| a.1).sum::<f64>() <= 0.0 {
            return err("class_weights: at least one class needs a weight above zero");
        }
        Ok(ClassDraw::Weighted(WeightedBag::new(all, 24)))
    }

    fn draw(&mut self, rng: &mut Rng) -> PitchClass {
        match self {
            ClassDraw::Even(b) => b.draw(rng),
            ClassDraw::Weighted(b) => b.draw(rng),
        }
        .expect("classes are not empty")
    }

    /// `new` was played instead of the drawn `old`: an even draw gets `old` back and gives up one
    /// pending `new`, so each class still comes up equally often.
    fn replace(&mut self, old: PitchClass, new: PitchClass, rng: &mut Rng) {
        if let ClassDraw::Even(b) = self {
            if old != new {
                b.take(&new);
                b.put_back(old, rng);
            }
        }
    }

    fn refill_if_empty(&mut self, rng: &mut Rng) {
        if let ClassDraw::Even(b) = self {
            b.refill_if_empty(rng);
        }
    }

    /// Whether `c` is still to come in this cycle (always true for weighted draws).
    fn pending(&self, c: PitchClass) -> bool {
        match self {
            ClassDraw::Even(b) => b.pending(&c),
            ClassDraw::Weighted(_) => true,
        }
    }
}

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
    if !(0.0..=0.5).contains(&p.out_of_set_rate) {
        return err("out_of_set_rate must be between 0 and 0.5");
    }
    if p.min_jump_semitones > 24 {
        return err("min_jump_semitones must be at most 24");
    }
    if p.min_octave_step > 3 {
        return err("min_octave_step must be at most 3");
    }
    let others: Vec<PitchClass> = PitchClass::ALL
        .iter()
        .copied()
        .filter(|c| !classes.contains(c))
        .collect();
    let use_other = p.out_of_set_rate > 0.0 && !others.is_empty();
    if use_other && p.answer == PitchAnswer::NoteWithOctave {
        return err("out_of_set_rate needs answer pitch_class (\"Other\" is a pitch-class answer)");
    }
    let pace_ms = match p.pacing {
        Pacing::Respond => None,
        Pacing::Stream { ioi_ms } => {
            if ioi_ms < 800 || ioi_ms < p.duration_ms + 300 {
                return err(
                    "pacing.stream.ioi_ms must be at least 800 ms and 300 ms longer than duration_ms",
                );
            }
            Some(ioi_ms)
        }
    };
    // A stream leaves no room for a mask between tones.
    let mask = if pace_ms.is_some() {
        Mask::None
    } else {
        p.mask
    };
    let combos = pool_combos(&p.timbres, &p.octaves, p.pool, ctx)?;
    let mut bag = ClassDraw::new(&classes, &p.class_weights)?;
    let mut other_bag = BalancedBag::new(others.clone());
    let mut combo_bag = BalancedBag::new((0..combos.len()).collect::<Vec<_>>());
    let lo = *p.octaves.iter().min().unwrap();
    let hi = *p.octaves.iter().max().unwrap();
    let all_notes = PitchClass::new(0).in_octave(lo)..=PitchClass::new(11).in_octave(hi);
    let mut choices = match p.answer {
        PitchAnswer::PitchClass => pc_choices(&classes),
        PitchAnswer::NoteWithOctave => all_notes
            .clone()
            .map(|m| Choice::new(Note(m).name(), Note(m).display_name()))
            .collect(),
    };
    if use_other {
        choices.push(Choice::new(OTHER, "Other"));
    }
    let label_set = p.labels.as_deref().and_then(|id| ctx.label_set(id));
    if let Some(set) = &label_set {
        style_choices(&mut choices, set, p.hide_note_names);
        if p.hide_note_names {
            // In pitch order the buttons would map onto the keyboard; alphabetical says nothing.
            choices.sort_by_key(|c| (c.id == OTHER, c.label.to_lowercase()));
        }
    }
    // The octaves each timbre may use, for the jump rule and out-of-set tones.
    let octaves_of = |timbre: &TimbreId| -> Vec<i32> {
        combos
            .iter()
            .filter(|(t, _)| t == timbre)
            .map(|(_, o)| *o)
            .collect()
    };
    let min_jump = p.min_jump_semitones as i32;
    let step = p.min_octave_step as i32;
    let mut last_midi: Option<i32> = None;
    let mut out = Vec::with_capacity(n);
    for _ in 0..n {
        let out_of_set = use_other && rng.chance(p.out_of_set_rate as f64);
        let mut pc = if out_of_set {
            other_bag.draw(rng).unwrap()
        } else {
            bag.draw(rng)
        };
        let (mut timbre, mut octave) = combos[combo_bag.draw(rng).unwrap()].clone();
        if out_of_set {
            // At the edges of the range, where a small set's highest and lowest notes would
            // otherwise be guessable by height alone.
            let octs = octaves_of(&timbre);
            let (lo_t, hi_t) = (*octs.iter().min().unwrap(), *octs.iter().max().unwrap());
            octave = if rng.chance(0.5) { lo_t } else { hi_t };
        } else if let (true, Some(prev)) = (step > 0, last_midi) {
            // Move by octaves without looking at the note, so the octave gives nothing away.
            let prev_oct = prev.div_euclid(12) - 1;
            let far = |o: i32| (o - prev_oct).abs() >= step;
            if !far(octave) {
                let same: Vec<i32> = octaves_of(&timbre)
                    .into_iter()
                    .filter(|&o| far(o))
                    .collect();
                if let Some(&o) = rng.choose(&same) {
                    octave = o;
                } else {
                    let any: Vec<&(TimbreId, i32)> =
                        combos.iter().filter(|(_, o)| far(*o)).collect();
                    if let Some(&&(ref t, o)) = rng.choose(&any) {
                        timbre = t.clone();
                        octave = o;
                    }
                }
            }
        }
        let mut midi = pc.in_octave(octave);
        if Some(midi) == last_midi && !out_of_set {
            // The same note again: play the class in another octave when there is one. In one
            // octave the repeat stands; never repeating would make the next answer guessable.
            let octs: Vec<i32> = octaves_of(&timbre)
                .into_iter()
                .filter(|&o| o != octave)
                .collect();
            if let Some(&o) = rng.choose(&octs) {
                octave = o;
            }
            midi = pc.in_octave(octave);
        }
        let mut relaxed = false;
        if let (true, Some(prev)) = (min_jump > 0, last_midi) {
            if (midi - prev).abs() < min_jump {
                let far = |c: PitchClass, o: i32| (c.in_octave(o) - prev).abs() >= min_jump;
                let octs = octaves_of(&timbre);
                let same_class: Vec<i32> = octs.iter().copied().filter(|&o| far(pc, o)).collect();
                if let Some(&o) = rng.choose(&same_class) {
                    octave = o;
                } else {
                    let group = if out_of_set { &others } else { &classes };
                    let cands: Vec<(PitchClass, i32)> = group
                        .iter()
                        .flat_map(|&c| octs.iter().map(move |&o| (c, o)))
                        .filter(|&(c, o)| far(c, o))
                        .collect();
                    // Prefer a class still to come, so the counts stay even.
                    if !out_of_set {
                        bag.refill_if_empty(rng);
                    }
                    let pending: Vec<(PitchClass, i32)> = cands
                        .iter()
                        .copied()
                        .filter(|&(c, _)| out_of_set || bag.pending(c))
                        .collect();
                    let pool = if pending.is_empty() { &cands } else { &pending };
                    match rng.choose(pool) {
                        Some(&(c, o)) => {
                            if !out_of_set {
                                bag.replace(pc, c, rng);
                            }
                            pc = c;
                            octave = o;
                        }
                        None => relaxed = true,
                    }
                }
                midi = pc.in_octave(octave);
            }
        }
        last_midi = Some(midi);
        let gain = gain_db(p.loudness_db, rng);
        let correct = if out_of_set {
            OTHER.to_string()
        } else {
            match p.answer {
                PitchAnswer::PitchClass => pc.name().to_string(),
                PitchAnswer::NoteWithOctave => Note(midi).name(),
            }
        };
        let tone = |m: i32| SoundEvent {
            midi: m,
            start_ms: 0.0,
            dur_ms: p.duration_ms as f64,
            timbre: timbre.clone(),
            gain_db: gain,
            detune_cents: p.detune_cents,
        };
        // Each answer as it would have sounded: the nearest note of that name, same timbre.
        // "Other" plays the tone itself when it was from outside the set.
        let mut choice_sounds: Vec<ChoiceSound> = match p.answer {
            PitchAnswer::PitchClass => classes
                .iter()
                .map(|c| ChoiceSound {
                    id: c.name().to_string(),
                    events: vec![tone(nearest_of_class(*c, midi))],
                })
                .collect(),
            PitchAnswer::NoteWithOctave => all_notes
                .clone()
                .map(|m| ChoiceSound {
                    id: Note(m).name(),
                    events: vec![tone(m)],
                })
                .collect(),
        };
        if out_of_set {
            choice_sounds.push(ChoiceSound {
                id: OTHER.into(),
                events: vec![tone(midi)],
            });
        }
        let holdout = ctx.pools.is_holdout(&timbre, octave);
        let mut stimulus = json!({
            "pitch_class": pc.name(),
            "octave": octave,
            "midi": midi,
            "timbre": timbre,
            "holdout": holdout,
            "gain_db": gain,
            "mask": mask,
            "class_set_size": classes.len(),
        });
        if use_other {
            stimulus["out_of_set"] = json!(out_of_set);
        }
        if min_jump > 0 {
            stimulus["min_jump"] = json!(min_jump);
        }
        if step > 0 {
            stimulus["min_octave_step"] = json!(step);
        }
        if relaxed {
            stimulus["constraint_relaxed"] = json!(true);
        }
        if let Some(ms) = pace_ms {
            stimulus["pace_ms"] = json!(ms);
        }
        if !p.class_weights.is_empty() {
            stimulus["weighted"] = json!(true);
        }
        out.push(TrialSpec::Identify(IdentifyTrial {
            answer_after_ms: None,
            after_answer: None,
            cue: None,
            pace_ms,
            exercise: ExerciseKind::PitchId,
            prompt: match p.answer {
                PitchAnswer::PitchClass => "Name the note".into(),
                PitchAnswer::NoteWithOctave => "Name the note and octave".into(),
            },
            events: vec![tone(midi)],
            mask_before: mask,
            choices: choices.clone(),
            correct,
            deadline_ms: p.deadline_ms,
            reference: None,
            stimulus,
            choice_sounds,
            // A tone from outside the set is answered Other and flashes nothing.
            flash_color: (p.color_flash && !out_of_set)
                .then(|| {
                    label_set
                        .as_ref()
                        .and_then(|s| s.entries.get(pc.name()).and_then(|e| e.color.clone()))
                })
                .flatten(),
            label_set: p.labels.clone(),
            requeue_misses: p.requeue_misses && !out_of_set,
            requeue_octaves: p.octaves.clone(),
            ..IdentifyTrial::skeleton()
        }));
        if let Some(id) = &p.labels {
            if let TrialSpec::Identify(t) = out.last_mut().unwrap() {
                t.stimulus["label_set"] = json!(id);
            }
        }
    }
    Ok(out)
}

fn style_choices(choices: &mut [Choice], set: &crate::labels::LabelSet, hide_names: bool) {
    for c in choices {
        let Some(entry) = set.entries.get(&c.id) else {
            continue;
        };
        c.color = entry.color.clone();
        c.word = entry.word.clone();
        if hide_names {
            if let Some(word) = &entry.word {
                c.label = word.clone();
            }
        } else if let Some(name) = &entry.name {
            c.label = name.clone();
        }
    }
}

/// A missed class, asked again later. Notes move to another octave when the block has one.
pub fn requeue_trial(trial: &TrialSpec) -> Option<TrialSpec> {
    let TrialSpec::Identify(t) = trial else {
        return None;
    };
    if !t.requeue_misses {
        return None;
    }
    if t.stimulus
        .get("out_of_set")
        .and_then(|v| v.as_bool())
        .unwrap_or(false)
    {
        return None;
    }
    if t.exercise == ExerciseKind::ChordColorId {
        let mut n = t.clone();
        if let Some(obj) = n.stimulus.as_object_mut() {
            obj.insert("requeued".into(), json!(true));
        }
        return Some(TrialSpec::Identify(n));
    }
    let oct = t
        .stimulus
        .get("octave")
        .and_then(|v| v.as_i64())
        .unwrap_or(4) as i32;
    let midi = t
        .stimulus
        .get("midi")
        .and_then(|v| v.as_i64())
        .unwrap_or(60) as i32;
    let new_oct = t
        .requeue_octaves
        .iter()
        .copied()
        .find(|o| *o != oct)
        .unwrap_or(oct);
    let delta = (new_oct - oct) * 12;
    let mut n = t.clone();
    for e in &mut n.events {
        e.midi += delta;
    }
    for cs in &mut n.choice_sounds {
        for e in &mut cs.events {
            e.midi += delta;
        }
    }
    if let Some(obj) = n.stimulus.as_object_mut() {
        obj.insert("octave".into(), json!(new_oct));
        obj.insert("midi".into(), json!(midi + delta));
        obj.insert("requeued".into(), json!(true));
    }
    Some(TrialSpec::Identify(n))
}

fn gen_pitch_detect(
    p: &PitchDetectParams,
    n: usize,
    rng: &mut Rng,
    ctx: &GenContext,
) -> Result<Vec<TrialSpec>, GenError> {
    if !(8..=64).contains(&p.tones) {
        return err("tones must be between 8 and 64");
    }
    if !(0.05..=0.5).contains(&p.target_rate) {
        return err("target_rate must be between 0.05 and 0.5");
    }
    if !(400..=5000).contains(&p.window_ms) {
        return err("window_ms must be between 400 and 5000");
    }
    if p.tone_ms + 40 >= p.window_ms {
        return err("tone_ms must be shorter than the response window");
    }
    if p.min_jump_semitones > 24 {
        return err("min_jump_semitones must be at most 24");
    }
    check_timbres(std::slice::from_ref(&p.prompt_timbre))?;
    let classes = p.classes.classes();
    if classes.len() < 2 {
        return err("pitch_detect needs at least two classes");
    }
    let combos = pool_combos(&p.timbres, &p.octaves, p.pool, ctx)?;
    let set = p.labels.as_deref().and_then(|id| ctx.label_set(id));
    let n_target = ((p.tones as f32 * p.target_rate).round() as u32).clamp(1, p.tones - 1) as usize;
    let prompt_oct = if p.octaves.contains(&4) {
        4
    } else {
        p.octaves[p.octaves.len() / 2]
    };
    let mut out = Vec::with_capacity(n);
    for i in 0..n {
        let target = classes[i % classes.len()];
        let prompt_midi = target.in_octave(prompt_oct);
        let mut slots: Vec<usize> = (0..p.tones as usize).collect();
        rng.shuffle(&mut slots);
        let mut is_target = vec![false; p.tones as usize];
        for s in slots.into_iter().take(n_target) {
            is_target[s] = true;
        }
        let others: Vec<PitchClass> = classes.iter().copied().filter(|c| *c != target).collect();
        let mut other_bag = BalancedBag::new(others.clone());
        let mut combo_bag = BalancedBag::new((0..combos.len()).collect::<Vec<_>>());
        let octaves_of = |timbre: &TimbreId| -> Vec<i32> {
            combos
                .iter()
                .filter(|(t, _)| t == timbre)
                .map(|(_, o)| *o)
                .collect()
        };
        let mut last: Option<i32> = None;
        let mut relaxed = false;
        let mut tones = Vec::with_capacity(p.tones as usize);
        for target_here in is_target {
            let mut pc = if target_here {
                target
            } else {
                other_bag.draw(rng).unwrap()
            };
            let (timbre, mut octave) = combos[combo_bag.draw(rng).unwrap()].clone();
            let octs = octaves_of(&timbre);
            if let Some(prev) = last {
                let min_jump = p.min_jump_semitones as i32;
                let far = |c: PitchClass, o: i32| {
                    min_jump == 0 || (c.in_octave(o) - prev).abs() >= min_jump
                };
                if min_jump > 0 && (pc.in_octave(octave) - prev).abs() < min_jump {
                    if target_here {
                        let same: Vec<i32> =
                            octs.iter().copied().filter(|&o| far(target, o)).collect();
                        match rng.choose(&same) {
                            Some(&o) => octave = o,
                            None => relaxed = true,
                        }
                        pc = target;
                    } else {
                        let cands: Vec<(PitchClass, i32)> = others
                            .iter()
                            .flat_map(|&c| octs.iter().map(move |&o| (c, o)))
                            .filter(|&(c, o)| far(c, o))
                            .collect();
                        match rng.choose(&cands) {
                            Some(&(c, o)) => {
                                pc = c;
                                octave = o;
                            }
                            None => relaxed = true,
                        }
                    }
                }
            }
            let midi = pc.in_octave(octave);
            last = Some(midi);
            tones.push(DetectTone {
                events: vec![ev(midi, 0.0, p.tone_ms as f64, &timbre, -6.0)],
                target: target_here && pc == target,
                far: (midi - prompt_midi).abs() > 12,
                midi,
            });
        }
        let entry = set
            .as_ref()
            .and_then(|s| crate::labels::class_entry(s, target));
        let label = entry
            .and_then(|e| e.word.clone())
            .unwrap_or_else(|| target.display_name().to_string());
        let speech = p.speak.then(|| {
            entry
                .and_then(|e| e.speech.clone())
                .unwrap_or_else(|| format!("notes/{}.wav", target.name()))
        });
        let mut stimulus = json!({
            "pitch_class": target.name(),
            "target_class": target.name(),
            "prompt_midi": prompt_midi,
            "tones": tones.len(),
            "targets": tones.iter().filter(|t| t.target).count(),
            "window_ms": p.window_ms,
        });
        if relaxed {
            stimulus["constraint_relaxed"] = json!(true);
        }
        if let Some(id) = &p.labels {
            stimulus["label_set"] = json!(id);
        }
        out.push(TrialSpec::Detect(DetectTrial {
            exercise: ExerciseKind::PitchDetect,
            prompt: format!("Listen for {label}"),
            target_class: target,
            target_label: label,
            target_color: entry.and_then(|e| e.color.clone()),
            prompt_events: vec![ev(
                prompt_midi,
                0.0,
                p.prompt_ms as f64,
                &p.prompt_timbre,
                -3.0,
            )],
            prompt_ms: p.prompt_ms,
            speech,
            tones,
            window_ms: p.window_ms,
            marks: p.marks,
            stimulus,
        }));
    }
    Ok(out)
}

fn gen_chord_color(
    p: &ChordColorParams,
    n: usize,
    rng: &mut Rng,
    ctx: &GenContext,
) -> Result<Vec<TrialSpec>, GenError> {
    if !(-3..=3).contains(&p.register_shift) {
        return err("register_shift must be between -3 and 3");
    }
    check_timbres(std::slice::from_ref(&p.timbre))?;
    let ids: Vec<String> = if p.chords.is_empty() {
        crate::labels::EGUCHI
            .iter()
            .filter(|c| c.white)
            .map(|c| c.id.to_string())
            .collect()
    } else {
        p.chords.clone()
    };
    if ids.is_empty() {
        return err("chords must not be empty");
    }
    let mut chords = Vec::new();
    for id in &ids {
        let c = crate::labels::eguchi(id)
            .ok_or_else(|| GenError(format!("unknown chord colour `{id}`")))?;
        if !chords
            .iter()
            .any(|x: &&crate::labels::EguchiChord| x.id == c.id)
        {
            chords.push(c);
        }
    }
    let overrides = ctx.label_set(crate::labels::EGUCHI_CHORDS);
    let choices: Vec<Choice> = chords
        .iter()
        .map(|c| {
            let over = overrides.as_ref().and_then(|s| s.entries.get(c.id));
            let color = over
                .and_then(|e| e.color.clone())
                .unwrap_or_else(|| c.color.to_string());
            let word = over
                .and_then(|e| e.word.clone())
                .unwrap_or_else(|| c.word.to_string());
            let mut choice = Choice::new(c.id, word).swatch(color, "");
            if !c.white {
                choice.word = Some(c.name.to_string());
            }
            choice
        })
        .collect();
    let shift = p.register_shift * 12;
    let mut bag =
        BalancedBag::allowing_repeats(chords.iter().map(|c| c.id.to_string()).collect::<Vec<_>>());
    let white: Vec<PitchClass> = [0, 2, 4, 5, 7, 9, 11]
        .into_iter()
        .map(PitchClass::new)
        .collect();
    let any_white = chords.iter().any(|c| c.white);
    let mut note_classes = if any_white { white.clone() } else { Vec::new() };
    for c in &chords {
        for m in c.notes {
            let pc = PitchClass::of_midi(*m);
            if !note_classes.contains(&pc) {
                note_classes.push(pc);
            }
        }
    }
    let mut note_choices = pc_choices(&note_classes);
    let note_set = ctx.label_set(&p.note_labels);
    if let Some(set) = &note_set {
        style_choices(&mut note_choices, set, false);
    }
    let mut out = Vec::with_capacity(n * if p.single_note_after { 2 } else { 1 });
    let base_shift = shift;
    let mut last: Option<(String, i32)> = None;
    for _ in 0..n {
        let id = bag.draw(rng).unwrap();
        let chord = crate::labels::eguchi(&id).unwrap();
        // The same chord twice in a row is played an octave away from the last time (and never
        // more than an octave from its own register): same colour, not the same sound.
        let shift = match &last {
            Some((prev, s)) if *prev == id => {
                if *s == base_shift {
                    base_shift + if rng.chance(0.5) { 12 } else { -12 }
                } else {
                    base_shift
                }
            }
            _ => base_shift,
        };
        last = Some((id.clone(), shift));
        let play = |notes: &[i32]| -> Vec<SoundEvent> {
            notes
                .iter()
                .map(|m| ev(m + shift, 0.0, p.duration_ms as f64, &p.timbre, -8.0))
                .collect()
        };
        let choice_sounds = chords
            .iter()
            .map(|c| ChoiceSound {
                id: c.id.to_string(),
                events: play(c.notes),
            })
            .collect();
        let color = choices
            .iter()
            .find(|c| c.id == chord.id)
            .and_then(|c| c.color.clone());
        out.push(TrialSpec::Identify(IdentifyTrial {
            exercise: ExerciseKind::ChordColorId,
            prompt: "Name the colour".into(),
            events: play(chord.notes),
            mask_before: Mask::None,
            choices: choices.clone(),
            correct: chord.id.into(),
            deadline_ms: None,
            reference: None,
            stimulus: json!({
                "chord": chord.id,
                "notes": chord.notes.iter().map(|m| PitchClass::of_midi(*m).name()).collect::<Vec<_>>(),
                "midi": chord.notes.iter().map(|m| m + shift).collect::<Vec<_>>(),
                "label_set": crate::labels::EGUCHI_CHORDS,
            }),
            cue: None,
            answer_after_ms: None,
            after_answer: None,
            explain: None,
            note_choices: Vec::new(),
            choice_sounds,
            pace_ms: None,
            flash_color: color,
            answer_color: None,
            label_set: Some(crate::labels::EGUCHI_CHORDS.into()),
            requeue_misses: p.requeue_misses,
            requeue_octaves: Vec::new(),
            score: None,
            no_replay: false,
        }));
        if p.single_note_after {
            let from_chord = !any_white || rng.chance(0.5);
            let pc = if from_chord {
                PitchClass::of_midi(*rng.choose(chord.notes).unwrap())
            } else {
                *rng.choose(&white).unwrap()
            };
            let center = chord.notes[chord.notes.len() / 2] + shift;
            let midi = nearest_of_class(pc, center);
            let octave = (midi / 12) - 1;
            let sounds: Vec<ChoiceSound> = note_classes
                .iter()
                .map(|c| ChoiceSound {
                    id: c.name().to_string(),
                    events: vec![ev(nearest_of_class(*c, midi), 0.0, 900.0, &p.timbre, -6.0)],
                })
                .collect();
            let mut stimulus = json!({
                "pitch_class": pc.name(),
                "octave": octave,
                "midi": midi,
                "context": "chord_extraction",
                "chord": chord.id,
                "timbre": p.timbre,
            });
            if note_set.is_some() {
                stimulus["label_set"] = json!(p.note_labels);
            }
            out.push(TrialSpec::Identify(IdentifyTrial {
                exercise: ExerciseKind::PitchId,
                prompt: "Name the note".into(),
                events: vec![ev(midi, 0.0, 900.0, &p.timbre, -6.0)],
                mask_before: Mask::None,
                choices: note_choices.clone(),
                correct: pc.name().to_string(),
                deadline_ms: Some(4000),
                reference: None,
                stimulus,
                cue: None,
                answer_after_ms: None,
                after_answer: None,
                explain: None,
                note_choices: Vec::new(),
                choice_sounds: sounds,
                pace_ms: None,
                flash_color: note_set
                    .as_ref()
                    .and_then(|s| crate::labels::class_entry(s, pc).and_then(|e| e.color.clone())),
                answer_color: None,
                label_set: note_set.as_ref().map(|_| p.note_labels.clone()),
                requeue_misses: false,
                requeue_octaves: Vec::new(),
                score: None,
                no_replay: false,
            }));
        }
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
            exercise: ExerciseKind::SingPitch,
            answer_color: None,
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
    let mut bag = BalancedBag::allowing_repeats(classes);
    let mut combo_bag = BalancedBag::new((0..combos.len()).collect::<Vec<_>>());
    let mut out = Vec::with_capacity(n);
    for _ in 0..n {
        let pc = bag.draw(rng).unwrap();
        let (timbre, octave) = combos[combo_bag.draw(rng).unwrap()].clone();
        let midi = pc.in_octave(octave);
        let set = p.labels.as_deref().and_then(|id| ctx.label_set(id));
        let entry = set.as_ref().and_then(|s| crate::labels::class_entry(s, pc));
        let label = if p.hide_note_names {
            entry
                .and_then(|e| e.word.clone())
                .unwrap_or_else(|| pc.display_name().to_string())
        } else {
            entry
                .and_then(|e| e.word.clone())
                .map(|w| format!("{} · {}", pc.display_name(), w))
                .unwrap_or_else(|| pc.display_name().to_string())
        };
        let flash = p
            .color_flash
            .then(|| entry.and_then(|e| e.color.clone()))
            .flatten();
        let speech = entry.and_then(|e| e.speech.clone());
        let mut stimulus = json!({ "pitch_class": pc.name(), "octave": octave, "timbre": timbre });
        if let Some(id) = &p.labels {
            stimulus["label_set"] = json!(id);
        }
        out.push(TrialSpec::Pairing(PairingTrial {
            events: vec![ev(midi, 0.0, p.duration_ms as f64, &timbre, -6.0)],
            label,
            gap_ms: p.gap_ms,
            stimulus,
            flash_color: flash,
            speech,
        }));
    }
    Ok(out)
}

// ---------------------------------------------------------------------------------------------
// Anchor songs and incidental pairing
// ---------------------------------------------------------------------------------------------

fn gen_anchor_song(
    p: &AnchorSongParams,
    n: usize,
    rng: &mut Rng,
    ctx: &GenContext,
) -> Result<Vec<TrialSpec>, GenError> {
    check_timbres(std::slice::from_ref(&p.timbre))?;
    let wanted = |a: &Anchor| {
        p.anchors.is_empty()
            || p.anchors
                .iter()
                .any(|w| w == &a.id || w.eq_ignore_ascii_case(&a.title))
    };
    let mut anchors: Vec<&Anchor> = ctx.anchors.iter().filter(|a| wanted(a)).collect();
    if p.mode == AnchorMode::KeyCheck {
        anchors.retain(|a| a.melody.is_some());
    }
    if anchors.is_empty() {
        return err(match (ctx.anchors.is_empty(), p.mode) {
            (true, _) => {
                "no anchor songs are saved yet: add some in Settings → Anchor songs (or with save_anchor)"
            }
            (false, AnchorMode::KeyCheck) => {
                "key_check needs anchors with a melody (the opening phrase in the original key)"
            }
            (false, AnchorMode::SingFirstNote) => "none of the listed anchors are saved",
        });
    }
    if p.mode == AnchorMode::KeyCheck && p.shifts.iter().any(|s| *s == 0 || s.abs() > 6) {
        return err("shifts must be non-zero and at most 6 semitones");
    }
    let mut bag = BalancedBag::new((0..anchors.len()).collect::<Vec<_>>());
    let shifts = if p.shifts.is_empty() {
        d_shifts()
    } else {
        p.shifts.clone()
    };
    let mut shift_bag = BalancedBag::new(shifts.clone());
    let mut out = Vec::with_capacity(n);
    for _ in 0..n {
        let a = anchors[bag.draw(rng).unwrap()];
        let first = a.first_note;
        match p.mode {
            AnchorMode::SingFirstNote => {
                let pc = PitchClass::of_midi(first.midi());
                out.push(TrialSpec::SingPitch(SingPitchTrial {
                    exercise: ExerciseKind::AnchorSong,
                    answer_color: None,
                    prompt: format!("Sing the first note of “{}”", a.title),
                    target_class: pc,
                    target_midi: None,
                    response_ms: p.response_ms.max(1000),
                    delay_ms: 0,
                    pre_mask: p.pre_mask,
                    reveal: p
                        .reveal_target_after
                        .then(|| vec![ev(first.midi(), 0.0, 1400.0, &p.timbre, -6.0)]),
                    stimulus: json!({
                        "anchor_id": a.id,
                        "title": a.title,
                        "pitch_class": pc.name(),
                        "first_note": first.name(),
                        "mode": p.mode,
                    }),
                }));
            }
            AnchorMode::KeyCheck => {
                let melody = a.melody.as_ref().unwrap();
                let score = Score::from_compact(melody, 90.0)
                    .map_err(|e| GenError(format!("anchor “{}”: {e}", a.title)))?;
                let original = rng.chance(p.original_rate.clamp(0.0, 1.0) as f64);
                let shift = if original {
                    0
                } else {
                    shift_bag.draw(rng).unwrap()
                };
                let opening = note_events(&score.notes, score.time, score.tempo_bpm, &p.timbre);
                let moved = |by: i32| -> Vec<SoundEvent> {
                    opening
                        .iter()
                        .cloned()
                        .map(|mut e| {
                            e.midi += by;
                            e
                        })
                        .collect()
                };
                let events = moved(shift);
                out.push(TrialSpec::Identify(IdentifyTrial {
                    answer_after_ms: None,
                    after_answer: None,
                    cue: None,
                    pace_ms: None,
                    exercise: ExerciseKind::AnchorSong,
                    prompt: format!("“{}”: original key or shifted?", a.title),
                    events,
                    mask_before: p.pre_mask,
                    choices: vec![
                        Choice::new("original", "Original key"),
                        Choice::new("shifted", "Shifted"),
                    ],
                    correct: if original { "original" } else { "shifted" }.into(),
                    deadline_ms: None,
                    reference: None,
                    stimulus: json!({
                        "anchor_id": a.id,
                        "title": a.title,
                        "shift": shift,
                        "original": original,
                        "mode": p.mode,
                    }),
                    choice_sounds: vec![
                        ChoiceSound {
                            id: "original".into(),
                            events: moved(0),
                        },
                        ChoiceSound {
                            id: "shifted".into(),
                            events: moved(if original { shifts[0] } else { shift }),
                        },
                    ],

                    ..IdentifyTrial::skeleton()
                }));
            }
        }
    }
    Ok(out)
}

fn d_shifts() -> Vec<i32> {
    vec![-2, -1, 1, 2]
}

fn gen_incidental(
    p: &IncidentalPairingParams,
    n: usize,
    rng: &mut Rng,
    ctx: &GenContext,
) -> Result<Vec<TrialSpec>, GenError> {
    let classes = p.classes.classes();
    if classes.len() < 2 {
        return err("classes needs at least two pitch classes");
    }
    if !(0.0..=0.5).contains(&p.mismatch_rate) {
        return err("mismatch_rate must be 0–0.5");
    }
    let combos = pool_combos(&p.timbres, &p.octaves, p.pool, ctx)?;
    let color = |pc: PitchClass| PAIRING_COLORS[pc.index() as usize];
    let mut sorted = classes.clone();
    sorted.sort();
    let choices: Vec<Choice> = sorted
        .iter()
        .map(|pc| Choice::new(color(*pc).0, color(*pc).1))
        .collect();
    let mut bag = BalancedBag::allowing_repeats(classes.clone());
    let mut combo_bag = BalancedBag::new((0..combos.len()).collect::<Vec<_>>());
    let mut out = Vec::with_capacity(n);
    for _ in 0..n {
        // The colour shown; its tone usually comes with it.
        let shown = bag.draw(rng).unwrap();
        let matched = !rng.chance(p.mismatch_rate as f64);
        let tone = if matched {
            shown
        } else {
            let others: Vec<PitchClass> = classes.iter().copied().filter(|c| *c != shown).collect();
            *rng.choose(&others).unwrap()
        };
        let (timbre, octave) = combos[combo_bag.draw(rng).unwrap()].clone();
        let midi = tone.in_octave(octave);
        out.push(TrialSpec::Identify(IdentifyTrial {
            answer_after_ms: None,
            after_answer: None,
            exercise: ExerciseKind::IncidentalPairing,
            prompt: "Press the colour".into(),
            events: vec![ev(midi, 0.0, p.duration_ms as f64, &timbre, -6.0)],
            mask_before: Mask::None,
            choices: choices.clone(),
            correct: color(shown).0.into(),
            deadline_ms: Some(3000),
            reference: None,
            choice_sounds: Vec::new(),
            stimulus: json!({
                "color": color(shown).0,
                "pitch_class": tone.name(),
                "color_class": shown.name(),
                "matched": matched,
                "octave": octave,
                "timbre": timbre,
            }),
            cue: Some(VisualCue {
                color: color(shown).0.into(),
                delay_ms: p.cue_delay_ms,
            }),
            pace_ms: None,

            ..IdentifyTrial::skeleton()
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
            answer_after_ms: None,
            after_answer: None,
            cue: None,
            pace_ms: None,
            exercise: ExerciseKind::TonalCenterId,
            prompt: if p.reference {
                "The first tone is A. Name the key".into()
            } else {
                "Name the key (tonic)".into()
            },
            events,
            mask_before: p.mask,
            choices: pc_choices(&PitchClass::ALL),
            correct: tonic.name().to_string(),
            deadline_ms: None,
            reference: None,
            stimulus: json!({
                "tonic": tonic.name(),
                "mode": mode,
                "reference": p.reference,
                "timbre": timbre,
            }),
            // Each key's cadence, in the same mode and sound.
            choice_sounds: PitchClass::ALL
                .iter()
                .map(|pc| ChoiceSound {
                    id: pc.name().into(),
                    events: cadence_events(*pc, mode, p.tempo_bpm, &timbre, 0.0),
                })
                .collect(),

            ..IdentifyTrial::skeleton()
        }));
    }
    Ok(out)
}

// ---------------------------------------------------------------------------------------------
// Relative pitch
// ---------------------------------------------------------------------------------------------

/// When a cadence is due under `plan`, the older `cadence_each_trial` and the key plan.
fn cadence_plan(plan: Option<CadencePlan>, each: Option<bool>, change: KeyChange) -> CadencePlan {
    plan.unwrap_or(match (each, change) {
        (Some(true), _) => CadencePlan::Every,
        (_, KeyChange::PerTrial) => CadencePlan::KeyChange,
        (_, KeyChange::PerBlock) => CadencePlan::Start,
    })
}

fn cadence_due(plan: CadencePlan, i: usize, key_changed: bool) -> bool {
    match plan {
        CadencePlan::Every => true,
        CadencePlan::Start => i == 0,
        CadencePlan::KeyChange => i == 0 || key_changed,
    }
}

/// The name of the note `rel` semitones above the tonic, as `labels` names notes in a key.
fn note_in_key(labels: DegreeLabels, tonic: PitchClass, mode: KeyMode, rel: i32) -> String {
    let rel = rel.rem_euclid(12);
    match labels {
        DegreeLabels::Numbers => DEGREE_NUMBERS[rel as usize].into(),
        DegreeLabels::Solfege => DEGREE_SOLFEGE[rel as usize].into(),
        DegreeLabels::FixedDo => {
            fixed_do(tonic, mode, rel, degree_number(DEGREE_IDS[rel as usize]))
        }
    }
}

/// The lower and upper note of an interval of `s` semitones whose first note heard is `first`.
fn from_first(first: i32, s: i32, dir: IntervalDirection) -> (i32, i32) {
    match dir {
        IntervalDirection::Descending => (first - s, first),
        _ => (first, first + s),
    }
}

fn gen_interval_id(
    p: &IntervalIdParams,
    n: usize,
    rng: &mut Rng,
) -> Result<Vec<TrialSpec>, GenError> {
    if p.intervals.is_empty() || p.directions.is_empty() {
        return err("intervals and directions must not be empty");
    }
    if !(0.0..=0.5).contains(&p.other_rate) {
        return err("other_rate must be between 0 and 0.5");
    }
    if p.start_octaves.is_empty() || p.start_octaves.iter().any(|o| o.abs() > 2) {
        return err("start_octaves must be between -2 and 2");
    }
    let range = p.root_range.unwrap_or(NoteRange::new(48, 72));
    let mut bag = BalancedBag::allowing_repeats(p.intervals.clone());
    let mut choice_ivs = p.intervals.clone();
    choice_ivs.sort();
    choice_ivs.dedup();
    let others: Vec<Interval> = if p.other_intervals.is_empty() {
        Interval::ALL
            .iter()
            .copied()
            .filter(|i| (1..=12).contains(&i.semitones()) && !choice_ivs.contains(i))
            .collect()
    } else {
        p.other_intervals
            .iter()
            .copied()
            .filter(|i| !choice_ivs.contains(i))
            .collect()
    };
    let use_other = p.other_rate > 0.0 && !others.is_empty();
    let mut choices: Vec<Choice> = choice_ivs
        .iter()
        .map(|i| Choice::new(i.id(), i.long_name()))
        .collect();
    if use_other {
        choices.push(Choice::new(OTHER, "Other"));
    }
    if choices.len() < 2 {
        return err("one interval alone is not a question: add another, or set other_rate");
    }
    let note = p.note_ms.max(200) as f64;
    let keys = if p.in_key {
        key_sequence(&p.keys, &p.modes, p.key_change, n, rng)?
    } else {
        Vec::new()
    };
    let plan = cadence_plan(p.cadence, p.cadence_each_trial, p.key_change);
    let cadence_bpm = p.cadence_bpm.unwrap_or(100.0);
    if !(40.0..=220.0).contains(&cadence_bpm) {
        return err("cadence_bpm must be 40–220");
    }
    let beat = 60_000.0 / cadence_bpm as f64;
    let start_note = p.start_note.map(|n| n.0).unwrap_or(60);
    let mut prev_key = None;
    let mut out = Vec::with_capacity(n);
    let mut last = None;
    for i in 0..n {
        let out_of_set = use_other && rng.chance(p.other_rate as f64);
        let mut dir = *rng.choose(&p.directions).unwrap();
        let timbre = pick_timbre(&p.timbres, rng)?;
        let octave = *rng.choose(&p.start_octaves).unwrap();
        // In a key: a cadence (when due), then two notes placed by `start` and `chromatic`.
        let mut lead = Vec::new();
        let mut t0 = 0.0;
        let key_info: serde_json::Value;
        let mut explain = None;
        let (iv, low) = if p.in_key {
            let (tonic, mode) = keys[i];
            let r = cadence_chords(tonic, mode)[0].1[0];
            let steps = mode_steps(mode);
            let in_scale = |m: i32| steps.contains(&(m - r).rem_euclid(12));
            // Where an interval can go here: its lower notes, with the first note heard where
            // `start` puts it and both notes in the scale unless chromatic.
            let placements = |iv: Interval, dir: IntervalDirection| -> Vec<i32> {
                let s = iv.semitones();
                let firsts: Vec<i32> = match p.start {
                    IntervalStart::Tonic => vec![r + 12 * octave],
                    IntervalStart::Fixed => vec![start_note + 12 * octave],
                    IntervalStart::Random => (r - 5..=r + 12)
                        .filter(|m| in_scale(*m))
                        .map(|m| m + 12 * octave)
                        .collect(),
                };
                firsts
                    .into_iter()
                    .map(|f| from_first(f, s, dir).0)
                    .filter(|&lo| p.chromatic || (in_scale(lo) && in_scale(lo + s)))
                    .collect()
            };
            let iv = if out_of_set {
                let fits: Vec<Interval> = others
                    .iter()
                    .copied()
                    .filter(|o| !placements(*o, dir).is_empty())
                    .collect();
                rng.choose(&fits).copied()
            } else {
                None
            };
            let out_of_set = iv.is_some();
            let iv = iv.unwrap_or_else(|| bag.draw(rng).unwrap());
            // Some intervals fit the key only one way from do (m2 down, not up, in major).
            if placements(iv, dir).is_empty() {
                if let Some(&d) = p
                    .directions
                    .iter()
                    .find(|d| !placements(iv, **d).is_empty())
                {
                    dir = d;
                }
            }
            let cands = placements(iv, dir);
            let Some(&low) = rng.choose(&cands) else {
                return err(format!(
                    "{} does not occur {} in a {} scale; set chromatic for notes outside it",
                    iv.long_name().to_lowercase(),
                    if p.start == IntervalStart::Tonic {
                        "from the tonic"
                    } else {
                        "between two notes"
                    },
                    if mode == KeyMode::Major {
                        "major"
                    } else {
                        "natural minor"
                    }
                ));
            };
            let changed = prev_key != Some((tonic, mode));
            let cadence = cadence_due(plan, i, changed);
            prev_key = Some((tonic, mode));
            if cadence {
                lead = cadence_events(tonic, mode, cadence_bpm, &timbre, 0.0);
                t0 = 6.0 * beat;
            }
            let s = iv.semitones();
            let (a, b) = match dir {
                IntervalDirection::Descending => (low + s, low),
                _ => (low, low + s),
            };
            explain = Some(format!(
                "{} → {}",
                note_in_key(p.labels, tonic, mode, a - r),
                note_in_key(p.labels, tonic, mode, b - r)
            ));
            key_info = json!({
                "key": tonic.name(),
                "mode": mode,
                "degrees": format!("{}-{}", DEGREE_IDS[(low - r).rem_euclid(12) as usize], DEGREE_IDS[(low + s - r).rem_euclid(12) as usize]),
                "cadence": cadence,
                "diatonic": in_scale(low) && in_scale(low + s),
                "out_of_set": out_of_set,
            });
            (iv, low)
        } else {
            let iv = if out_of_set {
                *rng.choose(&others).unwrap()
            } else {
                bag.draw(rng).unwrap()
            };
            let low = match p.start {
                IntervalStart::Random => pick_note(range, &mut last, rng),
                IntervalStart::Tonic | IntervalStart::Fixed => {
                    from_first(start_note + 12 * octave, iv.semitones(), dir).0
                }
            };
            key_info = json!({ "out_of_set": out_of_set });
            (iv, low)
        };
        let out_of_set = key_info["out_of_set"].as_bool().unwrap_or(false);
        let high = low + iv.semitones();
        let mut events = lead;
        events.extend(interval_events(low, high, dir, note, t0, &timbre));
        let mut stimulus = json!({
            "interval": iv.id(),
            "semitones": iv.semitones(),
            "direction": dir,
            "low_midi": low,
            "timbre": timbre,
            "in_key": p.in_key,
            "start": p.start,
        });
        if let serde_json::Value::Object(k) = key_info {
            stimulus.as_object_mut().unwrap().extend(k);
        }
        let first_heard = if dir == IntervalDirection::Descending {
            high
        } else {
            low
        };
        let mut choice_sounds: Vec<ChoiceSound> = choice_ivs
            .iter()
            .map(|c| {
                // Every interval from the note heard first.
                let (l, h) = from_first(first_heard, c.semitones(), dir);
                ChoiceSound {
                    id: c.id().into(),
                    events: interval_events(l, h, dir, note, 0.0, &timbre),
                }
            })
            .collect();
        if use_other {
            choice_sounds.push(ChoiceSound {
                id: OTHER.into(),
                events: interval_events(low, high, dir, note, 0.0, &timbre),
            });
        }
        out.push(TrialSpec::Identify(IdentifyTrial {
            answer_after_ms: (t0 > 0.0).then_some(t0),
            exercise: ExerciseKind::IntervalId,
            prompt: if p.in_key {
                "Which interval? (in the key)".into()
            } else {
                "Which interval?".into()
            },
            events,
            mask_before: Mask::None,
            choices: choices.clone(),
            correct: if out_of_set {
                OTHER.into()
            } else {
                iv.id().into()
            },
            explain: explain.map(|x| {
                if out_of_set {
                    format!("{}, {x}", iv.long_name().to_lowercase())
                } else {
                    x
                }
            }),
            stimulus,
            choice_sounds,
            ..IdentifyTrial::skeleton()
        }));
    }
    Ok(out)
}

/// Two notes, one after the other or together, starting at `at` ms.
fn interval_events(
    low: i32,
    high: i32,
    dir: IntervalDirection,
    note: f64,
    at: f64,
    timbre: &str,
) -> Vec<SoundEvent> {
    match dir {
        IntervalDirection::Ascending => vec![
            ev(low, at, note, timbre, -6.0),
            ev(high, at + note, note, timbre, -6.0),
        ],
        IntervalDirection::Descending => vec![
            ev(high, at, note, timbre, -6.0),
            ev(low, at + note, note, timbre, -6.0),
        ],
        IntervalDirection::Harmonic => vec![
            ev(low, at, note * 1.6, timbre, -8.0),
            ev(high, at, note * 1.6, timbre, -8.0),
        ],
    }
}

// ---------------------------------------------------------------------------------------------
// Keys, scale degrees and interval comparison
// ---------------------------------------------------------------------------------------------

/// Scale-degree ids numbered against the major scale, by semitones above the tonic.
pub const DEGREE_IDS: [&str; 12] = [
    "1", "b2", "2", "b3", "3", "4", "#4", "5", "b6", "6", "b7", "7",
];
const DEGREE_NUMBERS: [&str; 12] = [
    "1", "♭2", "2", "♭3", "3", "4", "♯4", "5", "♭6", "6", "♭7", "7",
];
const DEGREE_SOLFEGE: [&str; 12] = [
    "do", "ra", "re", "me", "mi", "fa", "fi", "sol", "le", "la", "te", "ti",
];

fn mode_steps(mode: KeyMode) -> [i32; 7] {
    match mode {
        KeyMode::Major => [0, 2, 4, 5, 7, 9, 11],
        KeyMode::Minor => [0, 2, 3, 5, 7, 8, 10],
    }
}

/// Semitones above the tonic for a degree id such as "3", "b6" or "#4".
pub fn parse_degree(id: &str) -> Option<i32> {
    let t = id.trim();
    let mut acc: i32 = 0;
    let mut rest = t;
    for (i, c) in t.char_indices() {
        match c {
            'b' | '♭' => acc -= 1,
            '#' | '♯' => acc += 1,
            _ => {
                rest = &t[i..];
                break;
            }
        }
    }
    let n: usize = rest.parse().ok()?;
    if !(1..=7).contains(&n) || acc.abs() > 1 {
        return None;
    }
    Some(([0, 2, 4, 5, 7, 9, 11][n - 1] + acc).rem_euclid(12))
}

/// The key of every trial: one for the block, or a new one per trial.
fn key_sequence(
    keys: &ClassSet,
    modes: &[KeyMode],
    change: KeyChange,
    n: usize,
    rng: &mut Rng,
) -> Result<Vec<(PitchClass, KeyMode)>, GenError> {
    let tonics = keys.classes();
    if tonics.is_empty() {
        return err("keys must not be empty");
    }
    let modes: Vec<KeyMode> = if modes.is_empty() {
        vec![KeyMode::Major]
    } else {
        modes.to_vec()
    };
    Ok(match change {
        KeyChange::PerBlock => {
            let k = (*rng.choose(&tonics).unwrap(), *rng.choose(&modes).unwrap());
            vec![k; n]
        }
        KeyChange::PerTrial => {
            let mut bag = BalancedBag::new(tonics);
            (0..n)
                .map(|_| (bag.draw(rng).unwrap(), *rng.choose(&modes).unwrap()))
                .collect()
        }
    })
}

/// A note followed by the scale steps to the tonic (up, down or to the nearer one).
pub fn route_to_tonic(note: i32, tonic_pc: i32, mode: KeyMode, resolution: Resolution) -> Vec<i32> {
    let steps = mode_steps(mode);
    let in_scale = |m: i32| steps.contains(&(m - tonic_pc).rem_euclid(12));
    let rel = (note - tonic_pc).rem_euclid(12);
    let up = match resolution {
        Resolution::None => return vec![note],
        Resolution::Ascending => true,
        Resolution::Descending => false,
        // 2, 3 and 4 (and their alterations) step down; ♯4 and above step up.
        Resolution::Nearest => rel > 5,
    };
    let mut route = vec![note];
    if rel == 0 {
        return route;
    }
    let mut cur = note;
    for _ in 0..12 {
        cur = if up {
            (cur + 1..cur + 13).find(|m| in_scale(*m)).unwrap()
        } else {
            (cur - 12..cur).rev().find(|m| in_scale(*m)).unwrap()
        };
        route.push(cur);
        if (cur - tonic_pc).rem_euclid(12) == 0 {
            break;
        }
    }
    route
}

fn gen_scale_degree(
    p: &ScaleDegreeParams,
    n: usize,
    rng: &mut Rng,
    ctx: &GenContext,
) -> Result<Vec<TrialSpec>, GenError> {
    if !(40.0..=220.0).contains(&p.tempo_bpm) {
        return err("tempo_bpm must be 40–220");
    }
    if p.octaves.is_empty() || p.octaves.iter().any(|o| o.abs() > 2) {
        return err("octaves must be between -2 and 2");
    }
    let explicit: Option<Vec<i32>> = if p.degrees.is_empty() {
        None
    } else {
        let mut v = Vec::new();
        for d in &p.degrees {
            // 8: the tonic above, asked on its own (a top-half level without the lower 1).
            if d.trim() == "8" {
                if !v.contains(&12) {
                    v.push(12);
                }
                continue;
            }
            match parse_degree(d) {
                Some(s) if !v.contains(&s) => v.push(s),
                Some(_) => {}
                None => {
                    return err(format!(
                        "unknown degree `{d}` (use 1–7 with an optional b or #, e.g. b3, #4)"
                    ));
                }
            }
        }
        v.sort();
        Some(v)
    };
    let cadence_bpm = p.cadence_bpm.unwrap_or(p.tempo_bpm);
    if !(40.0..=220.0).contains(&cadence_bpm) {
        return err("cadence_bpm must be 40–220");
    }
    let plan = p
        .cadence
        .unwrap_or(match (p.cadence_each_trial, p.key_change) {
            (Some(true), _) => CadencePlan::Every,
            (_, KeyChange::PerTrial) => CadencePlan::KeyChange,
            (_, KeyChange::PerBlock) => CadencePlan::Start,
        });
    let keys = key_sequence(&p.keys, &p.modes, p.key_change, n, rng)?;
    let beat = 60_000.0 / p.tempo_bpm as f64;
    let cadence_beat = 60_000.0 / cadence_bpm as f64;
    let step = beat * 0.6;
    let mut bags: Vec<(KeyMode, BalancedBag<i32>)> = Vec::new();
    let mut prev_key = None;
    let mut last_note: Option<i32> = None;
    let mut out = Vec::with_capacity(n);
    for (i, (tonic, mode)) in keys.into_iter().enumerate() {
        let mut degrees: Vec<i32> = explicit
            .clone()
            .unwrap_or_else(|| mode_steps(mode).to_vec());
        if degrees.len() < 2 {
            return err("degrees needs at least two degrees");
        }
        // 8: the tonic an octave above the cadence's, so answers name the octave too.
        if p.upper_tonic && degrees.contains(&0) && !degrees.contains(&12) {
            degrees.push(12);
        }
        let upper_tonic = p.upper_tonic || degrees.contains(&12);
        let bag = match bags.iter().position(|(m, _)| *m == mode) {
            Some(k) => &mut bags[k].1,
            None => {
                // Degrees recently missed come up more often: once more at 20% wrong, twice at 40%.
                let mut weighted = Vec::new();
                for d in &degrees {
                    let id = if *d == 12 {
                        "8"
                    } else {
                        DEGREE_IDS[*d as usize]
                    };
                    let e = if p.adaptive {
                        ctx.weak
                            .get(&format!("scale_degree_id:{id}"))
                            .copied()
                            .unwrap_or(0.0)
                    } else {
                        0.0
                    };
                    let copies = 1 + usize::from(e >= 0.2) + usize::from(e >= 0.4);
                    weighted.extend(std::iter::repeat_n(*d, copies));
                }
                bags.push((mode, BalancedBag::allowing_repeats(weighted)));
                &mut bags.last_mut().unwrap().1
            }
        };
        let semis = bag.draw(rng).unwrap();
        let timbre = pick_timbre(&p.timbres, rng)?;
        let r = cadence_chords(tonic, mode)[0].1[0];
        let mut octave = if semis == 12 {
            0
        } else {
            *rng.choose(&p.octaves).unwrap()
        };
        // A repeated degree is played in another octave when there is one.
        if semis != 12 && last_note == Some(r + semis + 12 * octave) {
            let others: Vec<i32> = p.octaves.iter().copied().filter(|o| *o != octave).collect();
            if let Some(&o) = rng.choose(&others) {
                octave = o;
            }
        }
        let note = r + semis + 12 * octave;
        last_note = Some(note);
        // The answer a note stands for: its degree, with the tonic above the cadence's as 8.
        let id_of = |m: i32| -> &'static str {
            let rel = (m - r).rem_euclid(12);
            if rel == 0 && upper_tonic && m > r {
                "8"
            } else {
                DEGREE_IDS[rel as usize]
            }
        };
        let cadence = match plan {
            CadencePlan::Every => true,
            CadencePlan::Start => i == 0,
            CadencePlan::KeyChange => prev_key != Some((tonic, mode)),
        };
        prev_key = Some((tonic, mode));
        let mut events = if cadence {
            cadence_events(tonic, mode, cadence_bpm, &timbre, 0.0)
        } else {
            Vec::new()
        };
        let t0 = if cadence { 6.0 * cadence_beat } else { 0.0 };
        events.push(ev(note, t0, beat * 1.5, &timbre, -6.0));
        // The answers open with the note, unless a pause to audiate it is asked for.
        let think = match p.think_ms {
            Some(0) | None => 0.0,
            Some(ms) => beat * 1.5 + ms as f64,
        };
        let route = route_to_tonic(note, tonic.index() as i32, mode, p.resolution);
        let walk = |from: usize, start: f64| -> Vec<SoundEvent> {
            route[from..]
                .iter()
                .enumerate()
                .map(|(k, m)| {
                    let last = from + k + 1 == route.len();
                    ev(
                        *m,
                        start + k as f64 * step,
                        if last { step * 2.5 } else { step * 0.95 },
                        &timbre,
                        -6.0,
                    )
                })
                .collect()
        };
        if p.resolve_before_answer && route.len() > 1 {
            events.extend(walk(1, t0 + beat * 1.5));
        }
        let label = |s: i32| -> String {
            let rel = s.rem_euclid(12);
            match p.labels {
                DegreeLabels::Numbers if s == 12 => "8".into(),
                DegreeLabels::Numbers => DEGREE_NUMBERS[rel as usize].into(),
                DegreeLabels::Solfege => DEGREE_SOLFEGE[rel as usize].into(),
                DegreeLabels::FixedDo => {
                    fixed_do(tonic, mode, rel, degree_number(DEGREE_IDS[rel as usize]))
                }
            }
        };
        // Each answer in this key, walking to the tonic: 8 from the upper tonic, the rest in
        // this question's octave.
        let degree_sound = |s: i32| -> Vec<SoundEvent> {
            let m = if s == 12 { r + 12 } else { r + s + 12 * octave };
            let route = route_to_tonic(m, tonic.index() as i32, mode, p.resolution);
            let mut e = vec![ev(m, 0.0, beat * 1.5, &timbre, -6.0)];
            for (k, x) in route.iter().enumerate().skip(1) {
                let dur = if k + 1 == route.len() {
                    step * 2.5
                } else {
                    step * 0.95
                };
                e.push(ev(
                    *x,
                    beat * 1.5 + (k - 1) as f64 * step,
                    dur,
                    &timbre,
                    -6.0,
                ));
            }
            e
        };
        let choice_id =
            |s: i32| -> &'static str { if s == 12 { "8" } else { DEGREE_IDS[s as usize] } };
        let choice_sounds: Vec<ChoiceSound> = degrees
            .iter()
            .map(|s| ChoiceSound {
                id: choice_id(*s).into(),
                events: degree_sound(*s),
            })
            .collect();
        let after_answer = (p.resolution != Resolution::None).then(|| walk(0, 0.0));
        // Every note these trials play that is one of the answers, so it lights up as it sounds.
        let ids: Vec<&str> = degrees.iter().map(|s| choice_id(*s)).collect();
        let mut note_choices: Vec<(i32, String)> = Vec::new();
        for m in choice_sounds
            .iter()
            .flat_map(|c| c.events.iter())
            .chain(after_answer.iter().flatten())
            .map(|e| e.midi)
        {
            let id = id_of(m);
            if ids.contains(&id) && !note_choices.iter().any(|(x, _)| *x == m) {
                note_choices.push((m, id.to_string()));
            }
        }
        out.push(TrialSpec::Identify(IdentifyTrial {
            exercise: ExerciseKind::ScaleDegreeId,
            prompt: format!(
                "Which degree? ({} key)",
                if mode == KeyMode::Major {
                    "major"
                } else {
                    "minor"
                }
            ),
            events,
            mask_before: Mask::None,
            choices: degrees
                .iter()
                .map(|s| Choice::new(choice_id(*s), label(*s)))
                .collect(),
            correct: id_of(note).into(),
            deadline_ms: None,
            reference: None,
            stimulus: json!({
                "key": tonic.name(),
                "mode": mode,
                "degree": id_of(note),
                "semitones": semis,
                "midi": note,
                "octave": octave,
                "cadence": cadence,
                "resolution": p.resolution,
                "timbre": timbre,
            }),
            cue: None,
            pace_ms: None,
            answer_after_ms: Some(t0 + think),
            after_answer,
            note_choices,
            choice_sounds,
            ..IdentifyTrial::skeleton()
        }));
    }
    Ok(out)
}

/// The degree number (1–7) in a degree id such as "b3" or "#4".
fn degree_number(id: &str) -> i32 {
    id.trim_start_matches(['b', '#']).parse().unwrap_or(1)
}

/// The chromatic fixed-do name (C is do) of the note `semis` above `tonic`, spelt as the key
/// writes it: degree `number` picks the letter, then sharps take an i and flats an e (di ri fi si
/// li; ra me se le te), so in G major 7 is fi and in F major 4 is te; B is ti and G♯ si. A
/// spelling with no syllable of its own (E♯, F♭, B♯, C♭, double sharps and flats) is named as
/// the note it sounds.
fn fixed_do(tonic: PitchClass, mode: KeyMode, semis: i32, number: i32) -> String {
    const NATURAL: [i32; 7] = [0, 2, 4, 5, 7, 9, 11];
    // Per letter C–B: flat, natural, sharp; empty where the spelling is another note's.
    const NAMES: [[&str; 3]; 7] = [
        ["", "do", "di"],
        ["ra", "re", "ri"],
        ["me", "mi", ""],
        ["", "fa", "fi"],
        ["se", "sol", "si"],
        ["le", "la", "li"],
        ["te", "ti", ""],
    ];
    // By pitch class: the natural, sharp or flat name of the note that sounds.
    const NATURALS: [&str; 12] = [
        "do", "", "re", "", "mi", "fa", "", "sol", "", "la", "", "ti",
    ];
    const SHARPS: [&str; 12] = ["", "di", "", "ri", "", "", "fi", "", "si", "", "li", ""];
    const FLATS: [&str; 12] = ["", "ra", "", "me", "", "", "se", "", "le", "", "te", ""];
    // The tonic's letter as the key is usually written (D♭, E♭, F♯, A♭, B♭ major; C♯, E♭, F♯,
    // G♯, B♭ minor).
    let letters = match mode {
        KeyMode::Major => [0, 1, 1, 2, 2, 3, 3, 4, 5, 5, 6, 6],
        KeyMode::Minor => [0, 0, 1, 2, 2, 3, 3, 4, 4, 5, 6, 6],
    };
    let letter = (letters[tonic.index() as usize] + number - 1).rem_euclid(7) as usize;
    let target = (tonic.index() as i32 + semis).rem_euclid(12) as usize;
    let accidental = (target as i32 - NATURAL[letter] + 6).rem_euclid(12) - 6;
    if (-1..=1).contains(&accidental) {
        let name = NAMES[letter][(accidental + 1) as usize];
        if !name.is_empty() {
            return name.into();
        }
    }
    let name = [
        NATURALS[target],
        if accidental > 0 {
            SHARPS[target]
        } else {
            FLATS[target]
        },
    ]
    .into_iter()
    .find(|n| !n.is_empty())
    .unwrap_or(SHARPS[target]);
    name.into()
}

fn gen_interval_comparison(
    p: &IntervalComparisonParams,
    n: usize,
    rng: &mut Rng,
) -> Result<Vec<TrialSpec>, GenError> {
    if p.intervals.is_empty() || p.directions.is_empty() || p.differences.is_empty() {
        return err("intervals, directions and differences must not be empty");
    }
    if p.differences.iter().any(|d| *d > 6) {
        return err("differences must be 0–6 semitones");
    }
    let range = p.root_range.unwrap_or(NoteRange::new(48, 72));
    let note = p.note_ms.max(200) as f64;
    let same = !p.pairs_from_pool && p.differences.contains(&0);
    if p.pairs_from_pool
        && p.intervals
            .iter()
            .map(|i| i.semitones())
            .collect::<std::collections::BTreeSet<_>>()
            .len()
            < 2
    {
        return err("pairs_from_pool needs at least two different intervals");
    }
    let mut choices = vec![
        Choice::new("first", "First is larger"),
        Choice::new("second", "Second is larger"),
    ];
    if same {
        choices.push(Choice::new("same", "Same"));
    }
    let mut bag = BalancedBag::allowing_repeats(p.intervals.clone());
    let mut diffs = BalancedBag::new(p.differences.clone());
    let mut last = None;
    let mut out = Vec::with_capacity(n);
    for _ in 0..n {
        let a = bag.draw(rng).unwrap().semitones();
        let b = if p.pairs_from_pool {
            let others: Vec<i32> = p
                .intervals
                .iter()
                .map(|i| i.semitones())
                .filter(|s| *s != a)
                .collect();
            *rng.choose(&others).unwrap()
        } else {
            let d = diffs.draw(rng).unwrap() as i32;
            let options: Vec<i32> = [a + d, a - d]
                .into_iter()
                .filter(|b| (1..=24).contains(b))
                .collect();
            *rng.choose(&options).unwrap_or(&a)
        };
        let (first, second) = if rng.chance(0.5) { (a, b) } else { (b, a) };
        let dir = *rng.choose(&p.directions).unwrap();
        let timbre = pick_timbre(&p.timbres, rng)?;
        if p.shared.is_empty() {
            return err("shared must not be empty");
        }
        let r1 = pick_note(range, &mut last, rng);
        // The second interval's first note heard, or its last, tied to the first interval's.
        let shared = *rng.choose(&p.shared).unwrap();
        let descending = dir == IntervalDirection::Descending;
        let (f1, l1) = if descending {
            (r1 + first, r1)
        } else {
            (r1, r1 + first)
        };
        let r2 = match shared {
            SharedNote::None => pick_note(range, &mut last, rng),
            SharedNote::First => from_first(f1, second, dir).0,
            SharedNote::Second => {
                if descending {
                    l1
                } else {
                    l1 - second
                }
            }
            SharedNote::Near => {
                let d = rng.range_i32(1, 3) * if rng.chance(0.5) { 1 } else { -1 };
                from_first(f1 + d, second, dir).0
            }
        };
        let span = if dir == IntervalDirection::Harmonic {
            note * 1.6
        } else {
            note * 2.0
        };
        let mut events = interval_events(r1, r1 + first, dir, note, 0.0, &timbre);
        events.extend(interval_events(
            r2,
            r2 + second,
            dir,
            note,
            span + 600.0,
            &timbre,
        ));
        let events_both = events.clone();
        let name = |s: i32| Interval::from_semitones(s).map(|i| i.id()).unwrap_or("?");
        let correct = if first == second {
            "same"
        } else if first > second {
            "first"
        } else {
            "second"
        };
        out.push(TrialSpec::Identify(IdentifyTrial {
            exercise: ExerciseKind::IntervalComparison,
            prompt: "Which interval is larger?".into(),
            events,
            mask_before: Mask::None,
            choices: choices.clone(),
            correct: correct.into(),
            deadline_ms: None,
            reference: None,
            stimulus: json!({
                "first": name(first),
                "second": name(second),
                "difference": (first - second).abs(),
                "direction": dir,
                "timbre": timbre,
                "shared": shared,
            }),
            cue: None,
            pace_ms: None,
            answer_after_ms: Some(span + 600.0),
            after_answer: None,
            choice_sounds: vec![
                ChoiceSound {
                    id: "first".into(),
                    events: interval_events(r1, r1 + first, dir, note, 0.0, &timbre),
                },
                ChoiceSound {
                    id: "second".into(),
                    events: interval_events(r2, r2 + second, dir, note, 0.0, &timbre),
                },
                ChoiceSound {
                    id: "same".into(),
                    events: events_both.clone(),
                },
            ],

            ..IdentifyTrial::skeleton()
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
    let styles: Vec<ChordStyle> = if p.styles.is_empty() {
        vec![p.style]
    } else {
        p.styles.clone()
    };
    // The inversions a quality is asked in: none to name for the augmented triad and the
    // diminished seventh, whose inversions are the same notes.
    let inversions_of = |q: ChordQuality| -> Vec<u8> {
        if p.answer_inversion && q.symmetric() {
            return vec![0];
        }
        inversions
            .iter()
            .copied()
            .filter(|&i| (i as usize) < q.intervals().len())
            .collect()
    };
    let answer_id = |q: ChordQuality, inv: u8| -> String {
        if p.answer_inversion {
            format!("{}/{}", q.id(), inv)
        } else {
            q.id().to_string()
        }
    };
    let range = p.root_range.unwrap_or(NoteRange::new(48, 67));
    let mut choices = Vec::new();
    for q in &p.qualities {
        if p.answer_inversion {
            for inv in inversions_of(*q) {
                let label = if q.symmetric() {
                    q.long_name().to_string()
                } else {
                    format!("{}, {}", q.long_name(), inversion_label(inv))
                };
                if !choices.iter().any(|c: &Choice| c.id == answer_id(*q, inv)) {
                    choices.push(Choice::new(answer_id(*q, inv), label));
                }
            }
        } else if !choices.iter().any(|c: &Choice| c.id == q.id()) {
            choices.push(Choice::new(q.id(), q.long_name()));
        }
    }
    if p.answer_inversion && choices.len() < 2 {
        return err("the chords and inversions leave only one answer");
    }
    // With inversions asked, every (chord, inversion) answer comes up about equally often.
    let answers: Vec<(ChordQuality, u8)> = p
        .qualities
        .iter()
        .flat_map(|q| {
            let invs = if p.answer_inversion {
                inversions_of(*q)
            } else {
                vec![u8::MAX]
            };
            invs.into_iter().map(move |i| (*q, i))
        })
        .collect();
    let mut bag = BalancedBag::allowing_repeats(answers);
    let dur = p.duration_ms.max(300) as f64;
    let mut out = Vec::with_capacity(n);
    let mut last = None;
    for _ in 0..n {
        let (q, asked): (ChordQuality, u8) = bag.draw(rng).unwrap();
        let inv = if asked != u8::MAX && !q.symmetric() {
            asked
        } else {
            let valid: Vec<u8> = inversions
                .iter()
                .copied()
                .filter(|&i| (i as usize) < q.intervals().len())
                .collect();
            *rng.choose(&valid).unwrap_or(&0)
        };
        let style = *rng.choose(&styles).unwrap();
        let root = pick_note(range, &mut last, rng);
        let timbre = pick_timbre(&p.timbres, rng)?;
        let events = chord_events(&q.voicing(root, inv as usize), style, dur, &timbre);
        // Every answer on the same root (in the inversion heard when inversions are not asked).
        let mut choice_sounds: Vec<ChoiceSound> = Vec::new();
        for cq in &p.qualities {
            let size = cq.intervals().len();
            if p.answer_inversion {
                for ci in inversions_of(*cq) {
                    let id = answer_id(*cq, ci);
                    if !choice_sounds.iter().any(|c| c.id == id) {
                        choice_sounds.push(ChoiceSound {
                            id,
                            events: chord_events(
                                &cq.voicing(root, ci as usize),
                                style,
                                dur,
                                &timbre,
                            ),
                        });
                    }
                }
            } else if !choice_sounds.iter().any(|c| c.id == cq.id()) {
                let ci = if (inv as usize) < size {
                    inv as usize
                } else {
                    0
                };
                choice_sounds.push(ChoiceSound {
                    id: cq.id().into(),
                    events: chord_events(&cq.voicing(root, ci), style, dur, &timbre),
                });
            }
        }
        let correct = answer_id(q, if q.symmetric() { 0 } else { inv });
        out.push(TrialSpec::Identify(IdentifyTrial {
            answer_after_ms: None,
            after_answer: None,
            cue: None,
            pace_ms: None,
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
                "style": style,
            }),
            choice_sounds,

            ..IdentifyTrial::skeleton()
        }));
    }
    Ok(out)
}

/// A chord's notes played together, one after another, or both.
fn chord_events(notes: &[i32], style: ChordStyle, dur: f64, timbre: &str) -> Vec<SoundEvent> {
    let step = 380.0;
    let mut events = Vec::new();
    match style {
        ChordStyle::Block => {
            for &m in notes {
                events.push(ev(m, 0.0, dur, timbre, -9.0));
            }
        }
        ChordStyle::Arpeggio => {
            for (i, &m) in notes.iter().enumerate() {
                events.push(ev(m, i as f64 * step, dur, timbre, -9.0));
            }
        }
        ChordStyle::ArpeggioDown => {
            for (i, &m) in notes.iter().rev().enumerate() {
                events.push(ev(m, i as f64 * step, dur, timbre, -9.0));
            }
        }
        ChordStyle::BlockThenArpeggio => {
            for &m in notes {
                events.push(ev(m, 0.0, dur * 0.8, timbre, -9.0));
            }
            let t0 = dur * 0.8 + 250.0;
            for (i, &m) in notes.iter().enumerate() {
                events.push(ev(m, t0 + i as f64 * step, step * 1.1, timbre, -9.0));
            }
        }
    }
    events
}

/// A scale from `root` in the given direction, one note per `step` ms.
fn scale_events(
    kind: ScaleKind,
    root: i32,
    direction: ScaleDirection,
    step: f64,
    timbre: &str,
) -> Vec<SoundEvent> {
    let up = kind.octave_notes(root);
    let notes: Vec<i32> = match direction {
        ScaleDirection::Ascending => up.clone(),
        ScaleDirection::Descending => up.iter().rev().copied().collect(),
        ScaleDirection::UpDown => up
            .iter()
            .copied()
            .chain(up.iter().rev().skip(1).copied())
            .collect(),
    };
    notes
        .iter()
        .enumerate()
        .map(|(i, &m)| {
            let dur = if i + 1 == notes.len() {
                step * 2.0
            } else {
                step * 0.95
            };
            ev(m, i as f64 * step, dur, timbre, -6.0)
        })
        .collect()
}

fn gen_scale_id(p: &ScaleIdParams, n: usize, rng: &mut Rng) -> Result<Vec<TrialSpec>, GenError> {
    if p.scales.is_empty() {
        return err("scales must not be empty");
    }
    let range = p.root_range.unwrap_or(NoteRange::new(48, 62));
    let mut bag = BalancedBag::allowing_repeats(p.scales.clone());
    let mut kinds: Vec<ScaleKind> = Vec::new();
    for s in &p.scales {
        if !kinds.contains(s) {
            kinds.push(*s);
        }
    }
    let choices: Vec<Choice> = kinds
        .iter()
        .map(|s| Choice::new(s.id(), s.long_name()))
        .collect();
    let step = 60_000.0 / p.tempo_bpm.clamp(40.0, 400.0) as f64;
    let mut out = Vec::with_capacity(n);
    let mut last = None;
    for _ in 0..n {
        let kind: ScaleKind = bag.draw(rng).unwrap();
        let root = pick_note(range, &mut last, rng);
        let timbre = pick_timbre(&p.timbres, rng)?;
        let events = scale_events(kind, root, p.direction, step, &timbre);
        out.push(TrialSpec::Identify(IdentifyTrial {
            answer_after_ms: None,
            after_answer: None,
            cue: None,
            pace_ms: None,
            exercise: ExerciseKind::ScaleId,
            prompt: "Which scale?".into(),
            events,
            mask_before: Mask::None,
            choices: choices.clone(),
            correct: kind.id().into(),
            deadline_ms: None,
            reference: None,
            stimulus: json!({ "scale": kind.id(), "root_midi": root, "direction": p.direction, "timbre": timbre }),
            // Every scale from the same root.
            choice_sounds: kinds
                .iter()
                .map(|k| ChoiceSound { id: k.id().into(), events: scale_events(*k, root, p.direction, step, &timbre) })
                .collect(),

            ..IdentifyTrial::skeleton()}));
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
            accompaniment: Vec::new(),
            just_cents: Vec::new(),
            silent: Vec::new(),
            reveal: None,
            free: None,
            score_until_beat: None,
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
            accompaniment: Vec::new(),
            just_cents: Vec::new(),
            silent: Vec::new(),
            reveal: None,
            free: None,
            score_until_beat: None,
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
    let mut bag = BalancedBag::allowing_repeats(p.intervals.clone());
    let keys = if p.in_key {
        key_sequence(&p.keys, &p.modes, p.key_change, n, rng)?
    } else {
        Vec::new()
    };
    let plan = cadence_plan(p.cadence, None, p.key_change);
    let mut prev_key = None;
    let mut out = Vec::with_capacity(n);
    let mut last = None;
    for i in 0..n {
        let iv: Interval = bag.draw(rng).unwrap();
        let mut dir = *rng.choose(&p.directions).unwrap();
        let s = iv.semitones();
        if s > range.span() {
            return err(format!(
                "interval {} does not fit in the range {}–{}",
                iv.id(),
                range.low,
                range.high
            ));
        }
        let mut signed = if dir == SingDirection::Up { s } else { -s };
        // A stepping stone for a hard interval, as an offset from the first note: an anchor
        // (additive) or the inversion the other way (complementary).
        let via_off = |sign: i32| -> Option<i32> {
            match p.scaffold {
                Scaffold::None => None,
                Scaffold::Additive => match s {
                    6 => Some(5),
                    8 | 9 => Some(7),
                    10 | 11 => Some(12),
                    _ => None,
                }
                .map(|a| sign * a),
                Scaffold::Complementary => (5..12).contains(&s).then(|| -sign * (12 - s)),
            }
        };
        let mut key_cadence: Option<(PitchClass, KeyMode, bool)> = None;
        let first = if p.in_key {
            // In the singer's range: the tonic, or any scale note, with the sung note in the
            // scale too unless chromatic.
            let (tonic, mode) = keys[i];
            let steps = mode_steps(mode);
            let rel = |m: i32| (m - tonic.index() as i32).rem_euclid(12);
            let fits = |range: NoteRange, signed: i32| -> Vec<i32> {
                (range.low.0..=range.high.0)
                    .filter(|m| range.contains(*m + signed))
                    .filter(|m| via_off(signed.signum()).is_none_or(|v| range.contains(*m + v)))
                    .filter(|m| match p.start {
                        IntervalStart::Tonic => rel(*m) == 0,
                        _ => steps.contains(&rel(*m)),
                    })
                    .filter(|m| p.chromatic || steps.contains(&rel(*m + signed)))
                    .collect()
            };
            // When no octave of this key fits (an octave from A in a C3–G4 range), stretch the
            // range by up to a fourth each way rather than fail. An interval that leaves the key
            // one way is sung the other way when that is allowed (a major second up from do, a
            // minor seventh down: complementary intervals).
            let mut order = vec![dir];
            order.extend(p.directions.iter().copied().filter(|d| *d != dir));
            let mut cands = Vec::new();
            for d in order {
                let sg = if d == SingDirection::Up { s } else { -s };
                cands = fits(range, sg);
                if cands.is_empty() {
                    cands = fits(NoteRange::new(range.low.0 - 5, range.high.0 + 5), sg);
                }
                if !cands.is_empty() {
                    dir = d;
                    signed = sg;
                    break;
                }
            }
            let fresh: Vec<i32> = cands.iter().copied().filter(|m| Some(*m) != last).collect();
            let Some(&f) = rng.choose(if fresh.is_empty() { &cands } else { &fresh }) else {
                return err(format!(
                    "{} {} does not fit {} in the range; widen the range or set chromatic",
                    iv.long_name().to_lowercase(),
                    if dir == SingDirection::Up {
                        "up"
                    } else {
                        "down"
                    },
                    if p.start == IntervalStart::Tonic {
                        "from the tonic"
                    } else {
                        "in the scale"
                    }
                ));
            };
            last = Some(f);
            let changed = prev_key != Some((tonic, mode));
            prev_key = Some((tonic, mode));
            key_cadence = Some((tonic, mode, cadence_due(plan, i, changed)));
            f
        } else {
            // Every note, the stepping stone included, inside the range.
            let v = via_off(signed.signum()).unwrap_or(0);
            let lo = 0.min(signed).min(v);
            let hi = 0.max(signed).max(v);
            pick_note(
                NoteRange::new(range.low.0 - lo, (range.high.0 - hi).max(range.low.0 - lo)),
                &mut last,
                rng,
            )
        };
        let second = first + signed;
        let sign = signed.signum();
        let via: Option<(i32, String)> = via_off(sign).map(|v| {
            let how = match p.scaffold {
                Scaffold::Complementary => format!(
                    "a {} {} first, then the octave {}",
                    Interval::from_semitones(12 - s)
                        .map(|i| i.long_name().to_lowercase())
                        .unwrap_or_default(),
                    if sign < 0 { "up" } else { "down" },
                    if sign < 0 { "down" } else { "up" }
                ),
                _ => match s {
                    6 => "a 4th, then a semitone".into(),
                    8 | 9 => "a 5th, then a step".into(),
                    _ => "the octave, then back a step".into(),
                },
            };
            (first + v, how)
        });
        let mut notes = vec![TargetNote {
            midi: first,
            start_beat: 0.0,
            beats: 2.0,
        }];
        if let Some((m, _)) = &via {
            notes.push(TargetNote {
                midi: *m,
                start_beat: 2.0,
                beats: 2.0,
            });
        }
        notes.push(TargetNote {
            midi: second,
            start_beat: notes.len() as f32 * 2.0,
            beats: 2.0,
        });
        let mut tl = timeline(&notes, &p.sung, reference);
        // A cadence before the first note when one is due: four chords, then a beat's rest.
        if let Some((tonic, mode, true)) = key_cadence {
            const CADENCE_BEATS: f32 = 6.0;
            for r in &mut tl.reference {
                r.start_beat += CADENCE_BEATS;
            }
            tl.sing_start += CADENCE_BEATS;
            tl.total += CADENCE_BEATS;
            let mut chords = Vec::new();
            for (k, (bass, upper)) in cadence_chords(tonic, mode).iter().enumerate() {
                let beats = if k == 3 { 2.0 } else { 0.95 };
                for &m in std::iter::once(bass).chain(upper.iter()) {
                    chords.push(TargetNote {
                        midi: m,
                        start_beat: k as f32,
                        beats,
                    });
                }
            }
            chords.append(&mut tl.reference);
            tl.reference = chords;
        }
        out.push(TrialSpec::Sung(SungTrial {
            exercise: ExerciseKind::SingInterval,
            prompt: match key_cadence {
                Some((tonic, mode, _)) => format!(
                    "From {}, sing {} (a {} {})",
                    note_in_key(p.labels, tonic, mode, first - tonic.index() as i32),
                    note_in_key(p.labels, tonic, mode, second - tonic.index() as i32),
                    iv.long_name().to_lowercase(),
                    if dir == SingDirection::Up {
                        "up"
                    } else {
                        "down"
                    }
                ),
                None => format!(
                    "Sing a {} {} from {}",
                    iv.long_name().to_lowercase(),
                    if dir == SingDirection::Up {
                        "up"
                    } else {
                        "down"
                    },
                    Note(first).display_name()
                ),
            } + &via
                .as_ref()
                .map(|(_, how)| format!(", via {how}"))
                .unwrap_or_default(),
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
            accompaniment: Vec::new(),
            just_cents: Vec::new(),
            silent: Vec::new(),
            reveal: None,
            free: None,
            score_until_beat: None,
            stimulus: json!({
                "interval": iv.id(),
                "direction": dir,
                "first_midi": first,
                "reference": reference,
                "backing": p.sung.backing,
                "key": key_cadence.map(|k| k.0.name()),
                "mode": key_cadence.map(|k| k.1),
                "cadence": key_cadence.map(|k| k.2),
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
            accompaniment: Vec::new(),
            just_cents: Vec::new(),
            silent: Vec::new(),
            reveal: None,
            free: None,
            score_until_beat: None,
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

/// The melodies of a block: generated one by one, or a list (the coach's or the library's) used
/// in turn.
enum Melodies<'a> {
    Generate(&'a MelodyParams),
    /// Melodies with the library piece each came from.
    List(Vec<(CompactMelody, Option<String>)>),
}

impl<'a> Melodies<'a> {
    fn new(source: &'a MelodySource, ctx: &GenContext, rng: &mut Rng) -> Result<Self, GenError> {
        Ok(match source {
            MelodySource::Generate(m) => Melodies::Generate(m),
            MelodySource::Given(list) => {
                if list.is_empty() {
                    return err("melody.given must contain at least one melody");
                }
                Melodies::List(list.iter().map(|m| (m.clone(), None)).collect())
            }
            MelodySource::Library(sel) => {
                let pieces = library::select(&ctx.library, sel, 1)
                    .map_err(|e| GenError(format!("melody.library: {e}")))?;
                let mut list: Vec<(CompactMelody, Option<String>)> = pieces
                    .iter()
                    .map(|p| {
                        let mut m = p.melody(sel.part);
                        if sel.fit_range {
                            library::fit_octaves(&mut m, ctx.comfortable);
                        }
                        if !sel.piece_tempo {
                            m.tempo_bpm = None;
                        }
                        (m, Some(p.id.clone()))
                    })
                    .collect();
                if sel.shuffle {
                    rng.shuffle(&mut list);
                }
                Melodies::List(list)
            }
        })
    }

    /// The melody for trial `i`, and the library piece it came from.
    fn score(
        &self,
        i: usize,
        tempo_bpm: f32,
        rng: &mut Rng,
        ctx: &GenContext,
    ) -> Result<(Score, Option<String>), GenError> {
        match self {
            Melodies::Generate(m) => generate_melody(m, ctx.comfortable, tempo_bpm, rng)
                .map(|s| (s, None))
                .map_err(|e| GenError(format!("melody: {e}"))),
            Melodies::List(list) => {
                let k = i % list.len();
                let (m, piece) = &list[k];
                Score::from_compact(m, tempo_bpm)
                    .map(|s| (s, piece.clone()))
                    .map_err(|e| {
                        GenError(match piece {
                            Some(id) => format!("library piece `{id}`: {e}"),
                            None => format!("melody.given[{k}]: {e}"),
                        })
                    })
            }
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
        return err(
            "backing `melody` would sing the answer; use `click`, `chords`, `drone` or `none` (Sing along is offered after each attempt)",
        );
    }
    let melodies = Melodies::new(&p.melody, ctx, rng)?;
    let mut out = Vec::with_capacity(n);
    for i in 0..n {
        let (score, piece) = melodies.score(i, p.tempo_bpm, rng, ctx)?;
        let mut t = sight_trial(score, p);
        if let Some(id) = piece {
            t.stimulus["piece"] = json!(id);
        }
        out.push(TrialSpec::Sung(t));
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
            reference.push(TargetNote {
                midi: *bass,
                start_beat: t + k as f32,
                beats,
            });
            for m in upper {
                reference.push(TargetNote {
                    midi: *m,
                    start_beat: t + k as f32,
                    beats,
                });
            }
        }
        sections.push(TimelineSection {
            label: "Establish key".into(),
            start_beat: 0.0,
            end_beat: 5.0,
        });
        t = 6.0;
    }
    if o.first_note {
        if let Some(first) = targets.first() {
            reference.push(TargetNote {
                midi: first.midi,
                start_beat: t,
                beats: 2.0,
            });
            sections.push(TimelineSection {
                label: "First note".into(),
                start_beat: t,
                end_beat: t + 2.0,
            });
            t += 3.0;
        }
    }
    let count_in = t.ceil();
    let sing_start = count_in + (o.count_in_bars * bpb) as f32;
    if o.count_in_bars > 0 {
        sections.push(TimelineSection {
            label: "Count-in".into(),
            start_beat: count_in,
            end_beat: sing_start,
        });
    }
    let length = score.total_beats();
    sections.push(TimelineSection {
        label: o.main_label.into(),
        start_beat: sing_start,
        end_beat: sing_start + length,
    });
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
        prompt: score
            .title
            .clone()
            .unwrap_or_else(|| "Sing the melody".into()),
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
        accompaniment: Vec::new(),
        just_cents: Vec::new(),
        silent: Vec::new(),
        reveal: None,
        free: None,
        score_until_beat: None,
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
    t.stimulus["mode"] = json!(p.mode);
    let bpb = t.beats_per_bar.max(1) as f32;
    // Moves the count-in, the singing and everything after it later by `by` beats.
    let delay = |t: &mut SungTrial, by: f32| {
        let from = t.count_in_beat.unwrap_or(t.sing_start_beat);
        for s in &mut t.sections {
            if s.start_beat >= from - 1e-3 {
                s.start_beat += by;
                s.end_beat += by;
            }
        }
        t.count_in_beat = t.count_in_beat.map(|c| c + by);
        t.sing_start_beat += by;
        t.total_beats += by;
    };
    match p.mode {
        SightMode::Read => {}
        SightMode::Singback => {
            // After the key the melody plays, then it is sung back with the score hidden.
            let at = t.count_in_beat.unwrap_or(t.sing_start_beat);
            let length = t.score.as_ref().map(|s| s.total_beats()).unwrap_or(0.0);
            let by = ((length + 1.0) / bpb).ceil() * bpb;
            delay(&mut t, by);
            let melody: Vec<TargetNote> = t
                .notes
                .iter()
                .map(|n| TargetNote {
                    midi: n.midi,
                    start_beat: at + n.start_beat,
                    beats: n.beats * 0.95,
                })
                .collect();
            t.reference_beats.extend(melody);
            t.sections.push(TimelineSection {
                label: "Listen".into(),
                start_beat: at,
                end_beat: at + length,
            });
            t.sections
                .sort_by(|a, b| a.start_beat.total_cmp(&b.start_beat));
            t.score = None;
            t.prompt = "Listen, then sing it back".into();
        }
        SightMode::Flash => {
            // Only the tonic sounds; the score shows for a while, then goes.
            let tempo = t.tempo_bpm;
            let flash = (p.flash_s.clamp(3.0, 60.0) * tempo / 60.0 / bpb).ceil() * bpb;
            let tonic = t.score.as_ref().map(|s| s.key.tonic_pc()).unwrap_or(0);
            let low = t.notes.iter().map(|n| n.midi).min().unwrap_or(60);
            let mut tm = PitchClass::new(tonic).in_octave(3);
            while tm + 12 <= low {
                tm += 12;
            }
            let at = t.count_in_beat.unwrap_or(t.sing_start_beat);
            t.reference_beats.clear();
            t.sections.retain(|s| s.start_beat >= at - 1e-3);
            let by = flash - at;
            delay(&mut t, by);
            t.reference_beats.push(TargetNote {
                midi: tm,
                start_beat: 0.0,
                beats: 2.0,
            });
            t.sections.insert(
                0,
                TimelineSection {
                    label: "Read it silently".into(),
                    start_beat: 0.0,
                    end_beat: flash,
                },
            );
            t.score_until_beat = Some(flash);
            t.prompt = "Read and audiate it; then sing it from memory".into();
        }
    }
    // Kodály's silent singing: some notes are sung in the head only.
    let silent: Vec<usize> = match p.silent {
        SilentMode::None => Vec::new(),
        SilentMode::Bars => {
            let bars = (t
                .notes
                .iter()
                .map(|n| n.start_beat + n.beats)
                .fold(0.0f32, f32::max)
                / bpb)
                .ceil() as u32;
            let chosen: Vec<u32> = if p.silent_bars.is_empty() {
                (2..bars).collect()
            } else {
                p.silent_bars.clone()
            };
            t.notes
                .iter()
                .enumerate()
                .filter(|(_, n)| chosen.contains(&((n.start_beat / bpb).floor() as u32 + 1)))
                .map(|(i, _)| i)
                .collect()
        }
        SilentMode::StrongBeats | SilentMode::WeakBeats => t
            .notes
            .iter()
            .enumerate()
            .filter(|(_, n)| {
                ((n.start_beat % bpb).abs() < 1e-3) == (p.silent == SilentMode::StrongBeats)
            })
            .map(|(i, _)| i)
            .collect(),
    };
    if !silent.is_empty() && silent.len() < t.notes.len() {
        // Show when to sing aloud and when silently.
        if p.silent == SilentMode::Bars {
            let sing = t.sing_start_beat;
            let mut runs: Vec<(bool, f32, f32)> = Vec::new();
            for (i, n) in t.notes.iter().enumerate() {
                let quiet = silent.contains(&i);
                let (a, b) = (sing + n.start_beat, sing + n.start_beat + n.beats);
                match runs.last_mut() {
                    Some(r) if r.0 == quiet => r.2 = b,
                    _ => runs.push((quiet, a, b)),
                }
            }
            t.sections.retain(|s| s.label != "Sing");
            for (quiet, a, b) in runs {
                t.sections.push(TimelineSection {
                    label: if quiet {
                        "Sing silently".into()
                    } else {
                        "Sing aloud".into()
                    },
                    start_beat: a,
                    end_beat: b,
                });
            }
        }
        t.stimulus["silent"] = json!(silent);
        t.silent = silent;
    }
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
    let melodies = Melodies::new(&p.melody, ctx, rng)?;
    let mut out = Vec::with_capacity(n);
    for i in 0..n {
        let (score, piece) = melodies.score(i, p.tempo_bpm, rng, ctx)?;
        // The editor starts with the first note (and any rest before it) when it is given.
        let mut given = score.clone();
        given.title = None;
        given.notes = if p.give_first_note {
            let first = score
                .notes
                .iter()
                .position(|n| n.pitch.is_some())
                .unwrap_or(0);
            let mut lead: Vec<ScoreNote> = score.notes[..=first].to_vec();
            if let Some(f) = lead.first() {
                if f.start > 0 {
                    lead.insert(
                        0,
                        ScoreNote {
                            pitch: None,
                            start: 0,
                            dur: f.start,
                        },
                    );
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
                backing: if p.click {
                    Backing::Click
                } else {
                    Backing::None
                },
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
        if let Some(id) = piece {
            stimulus["piece"] = json!(id);
        }
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

// ---------------------------------------------------------------------------------------------
// Harmony singing
// ---------------------------------------------------------------------------------------------

/// How far a just interval above a note lies from equal temperament, in cents (5/4 is 13.7
/// cents flat, 3/2 two cents sharp, 16/9 for the minor seventh).
pub fn just_deviation(semitones: i32) -> f32 {
    const DEV: [f32; 12] = [
        0.0, 11.7, 3.9, 15.6, -13.7, -2.0, -9.8, 2.0, 13.7, -15.6, -3.9, -11.7,
    ];
    DEV[semitones.rem_euclid(12) as usize]
}

/// Each sung note's offset in just intonation against the lowest other voice sounding at its
/// onset (0 when the others rest).
fn just_offsets(notes: &[TargetNote], others: &[TargetNote]) -> Vec<f32> {
    notes
        .iter()
        .map(|n| {
            let t = n.start_beat + 1e-3;
            match others
                .iter()
                .filter(|o| o.start_beat <= t && o.start_beat + o.beats > t)
                .min_by_key(|o| o.midi)
            {
                Some(o) if n.midi >= o.midi => just_deviation(n.midi - o.midi),
                Some(o) => -just_deviation(o.midi - n.midi),
                None => 0.0,
            }
        })
        .collect()
}

/// A pitch moved `steps` scale steps in `key`, spelled from the key signature.
fn diatonic_shift(key: &KeySig, p: Pitch, steps: i32) -> Pitch {
    let q = Pitch::from_diatonic(p.diatonic() + steps, 0);
    Pitch::new(q.step, key.signature_alter(q.step), q.octave)
}

/// The note a perfect fourth above or below `p`, on the letter three steps away (B♭ above F,
/// F♯ below B).
fn perfect_fourth(p: Pitch, up: bool) -> Pitch {
    let q = Pitch::from_diatonic(p.diatonic() + if up { 3 } else { -3 }, 0);
    let target = p.midi() + if up { 5 } else { -5 };
    Pitch::new(q.step, (target - q.midi()) as i8, q.octave)
}

/// A melody and the voice that follows it a diatonic interval away, as (sung, played).
fn parallel_voices(melody: Score, pp: &ParallelHarmony, ctx: &GenContext) -> (Score, Score) {
    let steps = pp.voice.steps();
    let key = melody.key.clone();
    let tonic_step = key.degree(0, 4).step;
    let ends_on_tonic = melody
        .pitched()
        .last()
        .is_some_and(|(_, p)| (p.midi() - key.tonic_pc()).rem_euclid(12) == 0);
    let mut voice = melody.clone();
    let last = voice.notes.iter().rposition(|n| n.pitch.is_some());
    for (i, n) in voice.notes.iter_mut().enumerate() {
        let Some(p) = n.pitch else { continue };
        let mut h = diatonic_shift(&key, p, steps);
        // Over a closing tonic the voice ends on the tonic triad: a note outside it moves a step.
        if ends_on_tonic && Some(i) == last {
            let in_triad = |d: i32| [0, 2, 4].contains(&d.rem_euclid(7));
            let degree = h.step as i32 - tonic_step as i32;
            if !in_triad(degree) {
                h = diatonic_shift(&key, h, if in_triad(degree - 1) { -1 } else { 1 });
            }
        }
        // A tritone against the melody (fa against ti) becomes a perfect fourth.
        if pp.avoid_tritone && (h.midi() - p.midi()).rem_euclid(12) == 6 {
            h = perfect_fourth(p, steps > 0);
        }
        n.pitch = Some(h);
    }
    let (mut sung, mut played) = match pp.sing {
        HarmonyLine::Harmony => (voice, melody),
        HarmonyLine::Melody => (melody, voice),
    };
    // The sung line goes into the comfortable range; the other voice moves with it.
    if let Some((lo, hi)) = sung.range() {
        let k = library::best_octave(lo, hi, ctx.comfortable);
        for s in [&mut sung, &mut played] {
            for n in &mut s.notes {
                if let Some(p) = &mut n.pitch {
                    p.octave += k as i8;
                }
            }
        }
        if let Some((lo, hi)) = sung.range() {
            sung.clef = ClefChoice::Auto.resolve(lo, hi);
        }
    }
    (sung, played)
}

fn gen_harmony(
    p: &HarmonyParams,
    n: usize,
    rng: &mut Rng,
    ctx: &GenContext,
) -> Result<Vec<TrialSpec>, GenError> {
    check_timbres(std::slice::from_ref(&p.timbre))?;
    if !(30.0..=220.0).contains(&p.tempo_bpm) {
        return err("tempo_bpm must be 30–220");
    }
    // Melodies for parallel harmony, or written pieces (given or from the library) in block order
    // with the part sung.
    enum Voices<'a> {
        Parallel(&'a ParallelHarmony, Melodies<'a>),
        Written {
            source: &'static str,
            pieces: Vec<(LibraryPiece, usize)>,
            fit_range: bool,
            piece_tempo: bool,
        },
    }
    let mut voices = match &p.source {
        HarmonySource::Parallel(pp) => Voices::Parallel(pp, Melodies::new(&pp.melody, ctx, rng)?),
        HarmonySource::Given(g) => {
            if g.pieces.is_empty() {
                return err("source.given.pieces must contain at least one piece");
            }
            let mut pieces = Vec::with_capacity(g.pieces.len());
            for (k, v) in g.pieces.iter().enumerate() {
                let piece = library::piece_from_value(v, &format!("given#{}", k + 1))
                    .map_err(|e| GenError(format!("source.given.pieces[{k}]: {e}")))?;
                if piece.parts.len() < 2 {
                    return err(format!("source.given.pieces[{k}] needs two to four parts"));
                }
                let part = g.part.min(piece.parts.len() - 1);
                pieces.push((piece, part));
            }
            if g.shuffle {
                rng.shuffle(&mut pieces);
            }
            Voices::Written {
                source: "given",
                pieces,
                fit_range: g.fit_range,
                piece_tempo: g.piece_tempo,
            }
        }
        HarmonySource::Library(sel) => {
            let mut pieces: Vec<(LibraryPiece, usize)> = library::select(&ctx.library, sel, 2)
                .map_err(|e| GenError(format!("source.library: {e}")))?
                .into_iter()
                .map(|piece| (piece.clone(), sel.part.min(piece.parts.len() - 1)))
                .collect();
            if sel.shuffle {
                rng.shuffle(&mut pieces);
            }
            Voices::Written {
                source: "library",
                pieces,
                fit_range: sel.fit_range,
                piece_tempo: sel.piece_tempo,
            }
        }
    };
    let mut out = Vec::with_capacity(n);
    for i in 0..n {
        let (sung, played, prompt, extra) = match &mut voices {
            Voices::Parallel(pp, melodies) => {
                let (melody, piece) = melodies.score(i, p.tempo_bpm, rng, ctx)?;
                let (sung, played) = parallel_voices(melody, pp, ctx);
                let prompt = match pp.sing {
                    HarmonyLine::Harmony => format!("Sing {} the melody", pp.voice.describe()),
                    HarmonyLine::Melody => {
                        format!("Sing the melody over a voice {}", pp.voice.describe())
                    }
                };
                let mut extra = json!({ "source": "parallel", "voice": pp.voice, "sing": pp.sing });
                if let Some(id) = piece {
                    extra["piece"] = json!(id);
                }
                (sung, vec![played], prompt, extra)
            }
            Voices::Written {
                source,
                pieces,
                fit_range,
                piece_tempo,
            } => {
                let (piece, part) = &pieces[i % pieces.len()];
                let part = *part;
                let mut melodies: Vec<CompactMelody> =
                    (0..piece.parts.len()).map(|k| piece.melody(k)).collect();
                if !*piece_tempo {
                    for m in &mut melodies {
                        m.tempo_bpm = None;
                    }
                }
                if *fit_range {
                    if let Some((lo, hi)) = library::melody_range(&melodies[part]) {
                        let k = library::best_octave(lo, hi, ctx.comfortable);
                        for m in &mut melodies {
                            library::shift_octaves(m, k);
                        }
                    }
                }
                let score = |k: usize| {
                    Score::from_compact(&melodies[k], p.tempo_bpm)
                        .map_err(|e| GenError(format!("piece `{}`: {e}", piece.id)))
                };
                let sung = score(part)?;
                let plays: Vec<usize> = (0..melodies.len())
                    .filter(|k| *k != part && (p.play_parts.is_empty() || p.play_parts.contains(k)))
                    .collect();
                let played = plays
                    .iter()
                    .map(|k| score(*k))
                    .collect::<Result<Vec<_>, _>>()?;
                let name = piece.parts[part].name.clone().unwrap_or_else(|| {
                    match (part, piece.parts.len()) {
                        (0, _) => "the top part".into(),
                        (k, n) if k + 1 == n => "the bottom part".into(),
                        (k, n) => format!("part {} of {n}", k + 1),
                    }
                });
                let prompt = format!("{}: sing {name}", piece.title);
                let extra = json!({
                    "source": source,
                    "piece": piece.id,
                    "part": part,
                    "parts": piece.parts.len(),
                    "played_parts": plays,
                });
                (sung, played, prompt, extra)
            }
        };
        let notation = p
            .notation
            .unwrap_or(!matches!(p.source, HarmonySource::Parallel(_)));
        let mut t = score_timeline(
            sung,
            &ScoreTimeline {
                exercise: ExerciseKind::Harmony,
                key: p.start.plays_key(),
                first_note: p.start.plays_first_note(),
                count_in_bars: p.count_in_bars,
                backing: if p.click {
                    Backing::Click
                } else {
                    Backing::None
                },
                backing_volume: 0.7,
                timbre: &p.timbre,
                guide: false,
                follow_cursor: p.follow_cursor,
                main_label: "Sing",
            },
        );
        t.prompt = prompt;
        t.accompaniment = played.iter().flat_map(|s| s.targets()).collect();
        t.just_cents = just_offsets(&t.notes, &t.accompaniment);
        if !notation {
            t.score = None;
        }
        // Practice: your part plays along and its notes are shown.
        if p.practice {
            t.guide = true;
            t.lane_targets = LaneTargets::Shown;
        }
        if let (Some(a), Some(b)) = (t.stimulus.as_object_mut(), extra.as_object()) {
            a.extend(b.clone());
            a.insert("notation".into(), json!(notation));
            a.insert("start".into(), json!(p.start));
            a.insert("practice".into(), json!(p.practice));
        }
        out.push(TrialSpec::Sung(t));
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::{BTreeMap, BTreeSet};

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
    fn every_answer_can_be_heard_after_answering() {
        let ctx = GenContext::default();
        let pitches = |events: &[SoundEvent]| {
            let mut v: Vec<i32> = events.iter().map(|e| e.midi).collect();
            v.sort();
            v
        };
        for (json, same_as_played) in [
            (
                r#"{"type": "pitch_id", "count": 6, "params": {"answer": "note_with_octave", "octaves": [3, 4]}}"#,
                true,
            ),
            (
                r#"{"type": "interval_id", "count": 6, "params": {"directions": ["ascending", "descending", "harmonic"]}}"#,
                true,
            ),
            (
                r#"{"type": "interval_id", "count": 6, "params": {"in_key": true}}"#,
                false,
            ),
            (
                r#"{"type": "chord_id", "count": 6, "params": {"inversions": [0, 1, 2], "answer_inversion": true}}"#,
                true,
            ),
            (
                r#"{"type": "chord_id", "count": 6, "params": {"inversions": [0, 1]}}"#,
                true,
            ),
            (r#"{"type": "scale_id", "count": 6}"#, true),
            (r#"{"type": "tonal_center_id", "count": 6}"#, true),
            (r#"{"type": "scale_degree_id", "count": 6}"#, false),
            (r#"{"type": "interval_comparison", "count": 6}"#, true),
        ] {
            for t in generate_block(&block(json), 3, &ctx).unwrap() {
                let TrialSpec::Identify(t) = t else { panic!() };
                for c in &t.choices {
                    assert!(
                        t.choice_sound(&c.id).is_some_and(|e| !e.is_empty()),
                        "{json}: no sound for {}",
                        c.id
                    );
                }
                // The right answer sounds like what was played.
                if same_as_played && t.exercise != ExerciseKind::IntervalComparison {
                    assert_eq!(
                        pitches(t.choice_sound(&t.correct).unwrap()),
                        pitches(&t.events),
                        "{json}"
                    );
                }
            }
        }
        // The colour task has nothing to hear.
        let colours = block(
            r#"{"type": "incidental_pairing", "count": 2, "params": {"classes": ["C", "G"]}}"#,
        );
        let TrialSpec::Identify(t) = generate_block(&colours, 1, &ctx).unwrap().remove(0) else {
            panic!()
        };
        assert!(t.choice_sounds.is_empty());
    }

    #[test]
    fn octave_steps_say_nothing_about_the_note() {
        let ctx = GenContext::default();
        let b = block(
            r#"{"type": "pitch_id", "count": 1000, "params": {"timbres": ["gm:0"], "octaves": [3, 4, 5, 6], "min_jump_semitones": 13, "min_octave_step": 2}}"#,
        );
        let trials = generate_block(&b, 3, &ctx).unwrap();
        let notes: Vec<(i32, i32, String)> = trials
            .iter()
            .map(|t| {
                let TrialSpec::Identify(t) = t else { panic!() };
                (
                    t.stimulus["midi"].as_i64().unwrap() as i32,
                    t.stimulus["octave"].as_i64().unwrap() as i32,
                    t.correct.clone(),
                )
            })
            .collect();
        for w in notes.windows(2) {
            assert!((w[1].1 - w[0].1).abs() >= 2, "{w:?}");
            assert!((w[1].0 - w[0].0).abs() >= 13, "{w:?}");
        }
        // Every octave is used, and the octave a tone lands in does not depend on its name:
        // after an octave-3 tone the next is in octave 5 or 6, whatever the note.
        let mut after3: BTreeMap<String, BTreeSet<i32>> = BTreeMap::new();
        for w in notes.windows(2).filter(|w| w[0].1 == 3) {
            after3.entry(w[1].2.clone()).or_default().insert(w[1].1);
        }
        assert_eq!(after3.len(), 12);
        let expected: BTreeSet<i32> = [5, 6].into_iter().collect();
        assert!(after3.values().all(|o| *o == expected), "{after3:?}");
        let octaves: BTreeSet<i32> = notes.iter().map(|n| n.1).collect();
        assert_eq!(octaves, [3, 4, 5, 6].into_iter().collect());
    }

    #[test]
    fn two_classes_do_not_just_alternate() {
        let ctx = GenContext::default();
        let b = block(
            r#"{"type": "pitch_id", "count": 400, "params": {"classes": ["D#", "F#"], "octaves": [3]}}"#,
        );
        let answers: Vec<String> = generate_block(&b, 9, &ctx)
            .unwrap()
            .into_iter()
            .map(|t| {
                let TrialSpec::Identify(t) = t else { panic!() };
                t.correct
            })
            .collect();
        let repeats = answers.windows(2).filter(|w| w[0] == w[1]).count();
        // Chance is about half; strict alternation would give none.
        assert!(repeats > 120, "{repeats} repeats in 400");
        // Still even, give or take the last, partly drawn cycle of twelve.
        let d = answers.iter().filter(|a| *a == "D#").count() as i32;
        assert!((d - 200).abs() <= 6, "{d} of 400");
    }

    #[test]
    fn jump_rule_keeps_the_notes_even() {
        let ctx = GenContext::default();
        let b = block(
            r#"{"type": "pitch_id", "count": 60, "params": {"timbres": ["gm:0"], "octaves": [3, 4, 5], "min_jump_semitones": 13}}"#,
        );
        for seed in 0..40 {
            let mut counts: BTreeMap<String, u32> = BTreeMap::new();
            for t in generate_block(&b, seed, &ctx).unwrap() {
                let TrialSpec::Identify(t) = t else { panic!() };
                *counts.entry(t.correct.clone()).or_default() += 1;
            }
            assert_eq!(counts.len(), 12);
            let (lo, hi) = (
                counts.values().min().unwrap(),
                counts.values().max().unwrap(),
            );
            // Independent draws would typically spread by five or more.
            assert!(hi - lo <= 2, "seed {seed}: {counts:?}");
        }
    }

    #[test]
    fn soundfont_timbres_stay_in_range_and_installed() {
        let ctx = GenContext {
            soundfonts: Some(vec!["Cello".into()]),
            ..Default::default()
        };
        let b = block(
            r#"{"type": "pitch_id", "count": 200, "params": {"pool": "any", "timbres": ["sf:Cello:42", "sf:Trombone:57", "gm:0"], "octaves": [3, 4, 5, 6]}}"#,
        );
        let mut seen = BTreeSet::new();
        for t in generate_block(&b, 1, &ctx).unwrap() {
            let TrialSpec::Identify(t) = t else { panic!() };
            let timbre = t.stimulus["timbre"].as_str().unwrap().to_string();
            let oct = t.stimulus["octave"].as_i64().unwrap();
            assert_ne!(timbre, "sf:Trombone:57", "not installed");
            if timbre == "sf:Cello:42" {
                assert!((3..=4).contains(&oct), "cello in octave {oct}");
            }
            seen.insert(timbre);
        }
        assert_eq!(seen.len(), 2);
        let none = block(
            r#"{"type": "pitch_id", "count": 5, "params": {"pool": "any", "timbres": ["sf:Trombone:57"]}}"#,
        );
        let e = generate_block(&none, 1, &ctx).unwrap_err();
        assert!(e.0.contains("not installed"), "{}", e.0);
    }

    #[test]
    fn pitch_id_respects_pools_and_classes() {
        let ctx = GenContext::default();
        let b = block(
            r#"{"type": "pitch_id", "count": 120, "params": {"classes": ["C", "E", "G#"], "octaves": [2, 3, 4, 5, 6]}}"#,
        );
        let trials = generate_block(&b, 5, &ctx).unwrap();
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
            // Only the stage's notes are played and offered: no "other".
            let ids: Vec<&str> = t.choices.iter().map(|c| c.id.as_str()).collect();
            assert_eq!(ids, ["C", "E", "G#"]);
            assert!(ids.contains(&t.correct.as_str()));
            assert_eq!(t.correct, t.stimulus["pitch_class"]);
            // Each answer can be heard near the note played, in the same timbre.
            let played = &t.events[0];
            for id in &ids {
                let e = &t.choice_sound(id).unwrap()[0];
                assert_eq!(PitchClass::of_midi(e.midi).name(), *id);
                assert!((e.midi - played.midi).abs() <= 6);
                assert_eq!(e.timbre, played.timbre);
            }
            assert_eq!(t.choice_sound(&t.correct).unwrap()[0].midi, played.midi);
        }

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
            assert_eq!(default_mode(&TrialSpec::Sung(t)), PitchRefMode::AnyOctave);
        }
    }

    #[test]
    fn chord_drills_mix_styles_and_skip_symmetric_inversions() {
        let ctx = GenContext::default();
        let b = block(
            r#"{"type": "chord_id", "count": 60, "params": {"qualities": ["major", "augmented", "diminished7"],
                "answer_inversion": true, "inversions": [0, 1, 2, 3], "styles": ["arpeggio", "arpeggio_down", "block"]}}"#,
        );
        let mut styles = BTreeSet::new();
        for t in generate_block(&b, 4, &ctx).unwrap() {
            let TrialSpec::Identify(t) = t else { panic!() };
            // Major in three positions; the augmented triad and diminished seventh only once.
            assert_eq!(t.choices.len(), 5, "{:?}", t.choices);
            assert!(t.choices.iter().any(|c| c.id == t.correct));
            if t.correct.starts_with("augmented") || t.correct.starts_with("diminished7") {
                assert!(t.correct.ends_with("/0"));
            }
            styles.insert(t.stimulus["style"].as_str().unwrap().to_string());
            // A descending arpeggio starts on its top note.
            if t.stimulus["style"] == "arpeggio_down" {
                let first = t
                    .events
                    .iter()
                    .min_by(|a, b| a.start_ms.total_cmp(&b.start_ms))
                    .unwrap();
                assert_eq!(first.midi, t.events.iter().map(|e| e.midi).max().unwrap());
            }
        }
        assert_eq!(styles.len(), 3);
    }

    #[test]
    fn comparison_pairs_come_from_the_pool() {
        let ctx = GenContext::default();
        let b = block(
            r#"{"type": "interval_comparison", "count": 40, "params": {"intervals": ["P4", "P5", "P8"], "pairs_from_pool": true}}"#,
        );
        for t in generate_block(&b, 2, &ctx).unwrap() {
            let TrialSpec::Identify(t) = t else { panic!() };
            assert!(!t.choices.iter().any(|c| c.id == "same"));
            let (a, b) = (
                t.stimulus["first"].as_str().unwrap(),
                t.stimulus["second"].as_str().unwrap(),
            );
            assert_ne!(a, b);
            assert!(["P4", "P5", "P8"].contains(&a) && ["P4", "P5", "P8"].contains(&b));
        }
    }

    #[test]
    fn the_top_half_of_the_scale_asks_eight_without_one() {
        let ctx = GenContext::default();
        let b = block(
            r#"{"type": "scale_degree_id", "count": 24, "params": {"degrees": ["5", "6", "7", "8"]}}"#,
        );
        let mut seen = BTreeSet::new();
        for t in generate_block(&b, 5, &ctx).unwrap() {
            let TrialSpec::Identify(t) = t else { panic!() };
            let ids: Vec<&str> = t.choices.iter().map(|c| c.id.as_str()).collect();
            assert_eq!(ids, ["5", "6", "7", "8"]);
            seen.insert(t.correct.clone());
        }
        assert_eq!(seen.len(), 4);
        let low = block(
            r#"{"type": "scale_degree_id", "count": 8, "params": {"degrees": ["1", "2", "3", "4"], "upper_tonic": false}}"#,
        );
        let TrialSpec::Identify(t) = &generate_block(&low, 5, &ctx).unwrap()[0] else {
            panic!()
        };
        assert_eq!(t.choices.len(), 4);
    }

    #[test]
    fn complementary_intervals_are_sung_the_way_that_stays_in_the_key() {
        let ctx = GenContext::default();
        let b = block(
            r#"{"type": "sing_interval", "count": 30, "params": {"in_key": true, "start": "tonic",
                "intervals": ["M2", "m7"], "directions": ["up", "down"]}}"#,
        );
        for t in generate_block(&b, 6, &ctx).unwrap() {
            let TrialSpec::Sung(t) = t else { panic!() };
            let way = (
                t.stimulus["interval"].as_str().unwrap(),
                t.stimulus["direction"].as_str().unwrap(),
            );
            assert!(way == ("M2", "up") || way == ("m7", "down"), "{way:?}");
            // Re, either way.
            assert_eq!((t.notes[1].midi - t.notes[0].midi).rem_euclid(12), 2);
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
        let b =
            block(r#"{"type": "sight_sing", "count": 5, "seed": 7, "params": {"tempo_bpm": 90}}"#);
        let trials = generate_block(&b, 7, &GenContext::default()).unwrap();
        assert_eq!(trials.len(), 5);
        for t in &trials {
            let TrialSpec::Sung(t) = t else {
                panic!("not sung")
            };
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
            assert_eq!(
                default_mode(&TrialSpec::Sung(t.clone())),
                PitchRefMode::AnyOctave
            );
        }
    }

    #[test]
    fn sight_singing_uses_coach_melodies_and_relative_grading_without_a_start() {
        let b = block(
            r#"{"type": "sight_sing", "count": 3, "params": {"start": "none", "melody": {"given": [
            {"key": "F major", "time": "3/4", "notes": [{"note": "C4", "beats": 1}, {"note": "F4", "beats": 2}, {"note": "E4", "beats": 1}, {"note": "F4", "beats": 2}]}
        ]}}}"#,
        );
        let trials = generate_block(&b, 1, &GenContext::default()).unwrap();
        let TrialSpec::Sung(t) = &trials[2] else {
            panic!()
        };
        assert!(t.reference_beats.is_empty());
        assert_eq!(t.count_in_beat, Some(0.0));
        assert_eq!(t.sing_start_beat, 3.0);
        assert_eq!(
            t.notes.iter().map(|n| n.midi).collect::<Vec<_>>(),
            [60, 65, 64, 65]
        );
        assert_eq!(default_mode(&trials[2]), PitchRefMode::RelativeToFirst);
        let bad = block(r#"{"type": "sight_sing", "count": 1, "params": {"backing": "melody"}}"#);
        assert!(generate_block(&bad, 1, &GenContext::default()).is_err());
    }

    #[test]
    fn dictation_trials_give_the_first_note_and_play_the_melody() {
        let b = block(r#"{"type": "melodic_dictation", "count": 4, "params": {"max_plays": 3}}"#);
        let trials = generate_block(&b, 3, &GenContext::default()).unwrap();
        for t in &trials {
            let TrialSpec::Dictation(t) = t else {
                panic!("not dictation")
            };
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
        let b = block(
            r#"{"type": "melodic_dictation", "count": 1, "params": {"give_first_note": false, "play_key": false}}"#,
        );
        let TrialSpec::Dictation(t) = &generate_block(&b, 1, &GenContext::default()).unwrap()[0]
        else {
            panic!()
        };
        assert!(t.given.notes.is_empty());
        assert!(t.playback.reference_beats.is_empty());
    }

    fn library_context() -> GenContext {
        let mut library = Vec::new();
        for (file, text) in [
            (
                "Book/ex1.json",
                r#"{"key": "G major", "tempo_bpm": 66, "notes": "D5:2 B4:2 | G4:4"}"#,
            ),
            (
                "Book/ex2.json",
                r#"{"key": "F major", "notes": "F4 G4 A4 F4 | C5:4"}"#,
            ),
            (
                "Duets/d1.json",
                r#"{"key": "C", "parts": [{"notes": "E4:2 F4:2 | E4:4"}, {"notes": "C4:2 D4:2 | C4:4"}]}"#,
            ),
        ] {
            library.extend(library::read_file(file, text.as_bytes()).unwrap());
        }
        GenContext {
            comfortable: NoteRange::new(43, 64),
            library,
            ..GenContext::default()
        }
    }

    #[test]
    fn library_melodies_fit_the_range_and_name_their_piece() {
        let ctx = library_context();
        let b = block(
            r#"{"type": "sight_sing", "count": 3, "params": {"melody": {"library": {"collection": "book"}}}}"#,
        );
        let trials = generate_block(&b, 1, &ctx).unwrap();
        let pieces: Vec<&str> = trials
            .iter()
            .map(|t| match t {
                TrialSpec::Sung(t) => t.stimulus["piece"].as_str().unwrap(),
                _ => panic!(),
            })
            .collect();
        assert_eq!(pieces, ["Book/ex1", "Book/ex2", "Book/ex1"]);
        let TrialSpec::Sung(t) = &trials[0] else {
            panic!()
        };
        // D5–G4 moved down an octave into G2–E4; the piece's own tempo is kept.
        assert_eq!(
            t.notes.iter().map(|n| n.midi).collect::<Vec<_>>(),
            [62, 59, 55]
        );
        assert_eq!(t.tempo_bpm, 66.0);

        let b = block(
            r#"{"type": "melodic_dictation", "count": 1, "params": {"melody": {"library": {"ids": ["Duets/d1"], "part": 1, "fit_range": false}}}}"#,
        );
        let TrialSpec::Dictation(t) = &generate_block(&b, 1, &ctx).unwrap()[0] else {
            panic!()
        };
        assert_eq!(t.score.pitch_names(), "C4 D4 C4");
        assert_eq!(t.stimulus["piece"], "Duets/d1");

        let b = block(
            r#"{"type": "sight_sing", "count": 1, "params": {"melody": {"library": {"tags": ["nothing"]}}}}"#,
        );
        let e = generate_block(&b, 1, &ctx).unwrap_err();
        assert!(
            e.to_string().contains("no library pieces tagged nothing"),
            "{e}"
        );
    }

    #[test]
    fn parallel_harmony_follows_the_melody_in_thirds_and_ends_on_the_triad() {
        let b = block(
            r#"{"type": "harmony", "count": 4, "params": {"source": {"parallel": {"voice": "third_below"}}}}"#,
        );
        let ctx = GenContext::default();
        for t in generate_block(&b, 11, &ctx).unwrap() {
            let TrialSpec::Sung(t) = t else { panic!() };
            assert_eq!(t.exercise, ExerciseKind::Harmony);
            assert!(
                t.score.is_none(),
                "parallel harmony is sung by ear by default"
            );
            assert_eq!(t.notes.len(), t.accompaniment.len());
            let key = KeySig::parse(t.stimulus["key"].as_str().unwrap()).unwrap();
            for (k, (sung, played)) in t.notes.iter().zip(&t.accompaniment).enumerate() {
                assert_eq!(sung.start_beat, played.start_beat);
                let gap = played.midi - sung.midi;
                if k + 1 < t.notes.len() {
                    assert!(gap == 3 || gap == 4, "a third below: {gap}");
                } else {
                    // The last note is in the tonic triad.
                    let rel = (sung.midi - key.tonic_pc()).rem_euclid(12);
                    assert!([0, 3, 4, 7].contains(&rel), "{rel}");
                }
            }
            // Thirds below are pure when 13.7 (major) or 15.6 (minor) cents off equal temperament.
            assert!(
                t.just_cents[0] == 13.7 || t.just_cents[0] == -15.6,
                "{:?}",
                t.just_cents
            );
            assert!(
                t.notes.iter().all(|n| ctx.comfortable.contains(n.midi)
                    || (n.midi - ctx.comfortable.low.0).abs() < 5)
            );
            assert_eq!(
                default_mode(&TrialSpec::Sung(t.clone())),
                PitchRefMode::Absolute
            );
        }
    }

    #[test]
    fn tritones_in_fourths_and_fifths_become_perfect_fourths() {
        // C, F, B and D in C major: F and B are the tritone pair.
        let melody = json!({"given": [{"key": "C major", "notes": [
            {"note": "C4", "beats": 1}, {"note": "F4", "beats": 1}, {"note": "B4", "beats": 1}, {"note": "D5", "beats": 1}]}]});
        let harmony = |voice: &str, avoid: bool| -> (Vec<i32>, Vec<String>) {
            let b: BlockSpec = serde_json::from_value(json!({"type": "harmony", "count": 1, "params": {
                "source": {"parallel": {"voice": voice, "melody": melody, "avoid_tritone": avoid}}, "notation": true}}))
            .unwrap();
            let TrialSpec::Sung(t) = generate_block(&b, 1, &GenContext::default())
                .unwrap()
                .remove(0)
            else {
                panic!()
            };
            let gaps = t
                .notes
                .iter()
                .zip(&t.accompaniment)
                .map(|(s, m)| s.midi - m.midi)
                .collect();
            let names = t.score.unwrap().pitched().map(|(_, p)| p.name()).collect();
            (gaps, names)
        };
        // Fourth above: F gets B♭ (a perfect fourth), not B.
        let (gaps, names) = harmony("fourth_above", true);
        assert_eq!(gaps, [5, 5, 5, 5]);
        assert!(names[1].starts_with("Bb"), "{names:?}");
        assert_eq!(harmony("fourth_above", false).0, [5, 6, 5, 5]);
        // Fourth below: B gets F♯.
        let (gaps, names) = harmony("fourth_below", true);
        assert_eq!(gaps, [-5, -5, -5, -5]);
        assert!(names[2].starts_with("F#"), "{names:?}");
        // Fifths keep perfect fifths and turn the diminished fifth into a fourth, in the key.
        assert_eq!(harmony("fifth_below", true).0, [-7, -5, -7, -7]);
        assert_eq!(harmony("fifth_below", false).0, [-7, -6, -7, -7]);
        let (gaps, names) = harmony("fifth_above", true);
        assert_eq!(gaps, [7, 7, 5, 7]);
        assert!(names[2].starts_with('E'), "{names:?}");
    }

    #[test]
    fn written_harmony_is_sung_as_written_with_any_interval() {
        // Two voices with a tritone (F over B) in bar 1.
        let duet = json!({"title": "Duet", "key": "C major", "parts": [
            {"name": "Upper", "notes": "F4:2 E4:2 | C5:4"},
            {"name": "Lower", "notes": "B3:2 C4:2 | C4:4"}]});
        let b: BlockSpec =
            serde_json::from_value(json!({"type": "harmony", "count": 1, "params": {
            "source": {"given": {"pieces": [duet], "part": 1, "fit_range": false}}}}))
            .unwrap();
        let TrialSpec::Sung(t) = generate_block(&b, 1, &GenContext::default())
            .unwrap()
            .remove(0)
        else {
            panic!()
        };
        let gaps: Vec<i32> = t
            .notes
            .iter()
            .zip(&t.accompaniment)
            .map(|(s, a)| a.midi - s.midi)
            .collect();
        assert_eq!(gaps, [6, 4, 12], "the tritone stays");
        assert!(t.score.is_some(), "written parts are read from the staff");
        assert_eq!(t.prompt, "Duet: sing Lower");
        assert_eq!(t.stimulus["source"], "given");

        // SATB: sing the tenor against the bass alone.
        let satb = json!({"key": "G major", "time": "3/4", "parts": [
            {"name": "Soprano", "notes": "B4:3"}, {"name": "Alto", "notes": "G4:3"},
            {"name": "Tenor", "notes": "D4:3"}, {"name": "Bass", "notes": "G3:3"}]});
        let b: BlockSpec = serde_json::from_value(json!({"type": "harmony", "count": 1, "params": {
            "play_parts": [3], "source": {"given": {"pieces": [satb], "part": 2, "fit_range": false}}}})).unwrap();
        let TrialSpec::Sung(t) = generate_block(&b, 1, &GenContext::default())
            .unwrap()
            .remove(0)
        else {
            panic!()
        };
        assert_eq!(t.notes.iter().map(|n| n.midi).collect::<Vec<_>>(), [62]);
        assert_eq!(
            t.accompaniment.iter().map(|n| n.midi).collect::<Vec<_>>(),
            [55]
        );
        assert_eq!(t.stimulus["played_parts"], json!([3]));
        assert!(t.prompt.ends_with("sing Tenor"));

        // A piece needs two parts, and errors name the piece.
        let solo = json!({"key": "C", "notes": "C4:4"});
        let b: BlockSpec =
            serde_json::from_value(json!({"type": "harmony", "count": 1, "params": {
            "source": {"given": {"pieces": [solo]}}}}))
            .unwrap();
        assert!(
            generate_block(&b, 1, &GenContext::default())
                .unwrap_err()
                .to_string()
                .contains("pieces[0] needs two to four parts")
        );
        let bad = json!({"key": "C", "parts": [{"notes": "C4:4"}, {"notes": "C4:3 | D4:5"}]});
        let b: BlockSpec =
            serde_json::from_value(json!({"type": "harmony", "count": 1, "params": {
            "source": {"given": {"pieces": [bad]}}}}))
            .unwrap();
        let e = generate_block(&b, 1, &GenContext::default())
            .unwrap_err()
            .to_string();
        assert!(
            e.contains("source.given.pieces[0]: parts[1]: notes: the last bar has 5 beats"),
            "{e}"
        );
    }

    #[test]
    fn harmony_practice_plays_your_part_and_counts_as_assisted() {
        let b = block(
            r#"{"type": "harmony", "count": 2, "params": {"practice": true, "source": {"parallel": {"voice": "fifth_below", "melody": {"bars": 2}}}}}"#,
        );
        for t in generate_block(&b, 4, &GenContext::default()).unwrap() {
            assert!(t.is_practice());
            let TrialSpec::Sung(t) = t else { panic!() };
            assert!(t.guide);
            assert_eq!(t.lane_targets, LaneTargets::Shown);
            assert_eq!(t.stimulus["practice"], json!(true));
            assert_eq!(t.prompt, "Sing a fifth below the melody");
        }
        let b = block(r#"{"type": "harmony", "count": 1}"#);
        assert!(!generate_block(&b, 4, &GenContext::default()).unwrap()[0].is_practice());
    }

    #[test]
    fn parallel_melody_accepts_a_bare_generator_or_a_source() {
        let parse = |v: serde_json::Value| serde_json::from_value::<ParallelHarmony>(v);
        let bare = parse(json!({"melody": {"bars": 2}})).unwrap();
        assert!(matches!(bare.melody, MelodySource::Generate(ref m) if m.bars == 2));
        let tagged = parse(json!({"melody": {"library": {"collection": "Book"}}})).unwrap();
        assert!(matches!(tagged.melody, MelodySource::Library(_)));
        assert!(parse(json!({"melody": {"generate": {"bars": "two"}}})).is_err());
        assert!(matches!(
            parse(json!({})).unwrap().melody,
            MelodySource::Generate(_)
        ));
        assert!(parse(json!({})).unwrap().avoid_tritone);
    }

    #[test]
    fn library_harmony_sings_one_part_over_the_others() {
        let ctx = library_context();
        let b = block(
            r#"{"type": "harmony", "count": 1, "params": {"source": {"library": {"part": 1}}}}"#,
        );
        let TrialSpec::Sung(t) = &generate_block(&b, 1, &ctx).unwrap()[0] else {
            panic!()
        };
        assert_eq!(t.stimulus["piece"], "Duets/d1");
        assert!(t.score.is_some(), "library harmony shows the part");
        // Both voices move together, so the sixths and thirds stay.
        assert_eq!(
            t.notes.iter().map(|n| n.midi).collect::<Vec<_>>(),
            [60, 62, 60]
        );
        assert_eq!(
            t.accompaniment.iter().map(|n| n.midi).collect::<Vec<_>>(),
            [64, 65, 64]
        );
        assert_eq!(t.prompt, "d1: sing the bottom part");
        let one_part = block(
            r#"{"type": "harmony", "count": 1, "params": {"source": {"library": {"collection": "Book"}}}}"#,
        );
        assert!(
            generate_block(&one_part, 1, &ctx)
                .unwrap_err()
                .to_string()
                .contains("2 or more parts")
        );
    }

    fn anchor(title: &str, first: &str, melody: bool) -> Anchor {
        serde_json::from_value(json!({
            "id": title.to_lowercase(),
            "title": title,
            "first_note": first,
            "melody": if melody { json!({"key": "C major", "notes": [{"note": first, "beats": 1}, {"note": "D4", "beats": 1}, {"note": "E4", "beats": 2}]}) } else { serde_json::Value::Null },
        }))
        .unwrap()
    }

    #[test]
    fn anchor_songs_need_saved_anchors() {
        let b = block(r#"{"type": "anchor_song", "count": 6}"#);
        assert!(generate_block(&b, 1, &GenContext::default()).is_err());
        let ctx = GenContext {
            anchors: vec![
                anchor("Yesterday", "G4", false),
                anchor("Hey Jude", "C4", true),
            ],
            ..GenContext::default()
        };
        let trials = generate_block(&b, 1, &ctx).unwrap();
        assert_eq!(trials.len(), 6);
        for t in &trials {
            let TrialSpec::SingPitch(t) = t else {
                panic!("not sing_pitch")
            };
            assert_eq!(t.exercise, ExerciseKind::AnchorSong);
            assert!(t.prompt.starts_with("Sing the first note of"));
            assert!(t.target_midi.is_none());
        }
        // Key checks use only anchors with a melody and balance original against shifted.
        let b = block(r#"{"type": "anchor_song", "count": 40, "params": {"mode": "key_check"}}"#);
        let trials = generate_block(&b, 2, &ctx).unwrap();
        let mut shifted = 0;
        for t in &trials {
            let TrialSpec::Identify(t) = t else {
                panic!("not identify")
            };
            assert_eq!(t.stimulus["title"], "Hey Jude");
            let shift = t.stimulus["shift"].as_i64().unwrap() as i32;
            assert_eq!(t.events[0].midi, 60 + shift);
            assert_eq!(t.correct, if shift == 0 { "original" } else { "shifted" });
            if shift != 0 {
                shifted += 1;
            }
        }
        assert!((10..=30).contains(&shifted), "{shifted} of 40 shifted");
    }

    #[test]
    fn incidental_pairing_pairs_each_colour_with_one_class() {
        let b = block(
            r#"{"type": "incidental_pairing", "count": 200, "params": {"mismatch_rate": 0.1}}"#,
        );
        let trials = generate_block(&b, 9, &GenContext::default()).unwrap();
        let mut mismatched = 0;
        for t in &trials {
            let TrialSpec::Identify(t) = t else { panic!() };
            let cue = t.cue.as_ref().expect("cue");
            assert_eq!(cue.color, t.correct);
            assert_eq!(t.choices.len(), 4);
            let pc: PitchClass = t.stimulus["pitch_class"].as_str().unwrap().parse().unwrap();
            let matched = t.stimulus["matched"].as_bool().unwrap();
            assert_eq!(matched, PAIRING_COLORS[pc.index() as usize].0 == cue.color);
            if !matched {
                mismatched += 1;
            }
        }
        assert!((8..=35).contains(&mismatched), "{mismatched} mismatched");
    }

    #[test]
    fn degrees_parse_and_routes_walk_to_the_tonic() {
        assert_eq!(parse_degree("1"), Some(0));
        assert_eq!(parse_degree("b3"), Some(3));
        assert_eq!(parse_degree("♯4"), Some(6));
        assert_eq!(parse_degree("7"), Some(11));
        assert_eq!(parse_degree("8"), None);
        assert_eq!(parse_degree("bb3"), None);
        // C major, from C4 (60): 3 goes down, 6 goes up, 4 down, 5 up.
        assert_eq!(
            route_to_tonic(64, 0, KeyMode::Major, Resolution::Nearest),
            [64, 62, 60]
        );
        assert_eq!(
            route_to_tonic(69, 0, KeyMode::Major, Resolution::Nearest),
            [69, 71, 72]
        );
        assert_eq!(
            route_to_tonic(65, 0, KeyMode::Major, Resolution::Nearest),
            [65, 64, 62, 60]
        );
        assert_eq!(
            route_to_tonic(67, 0, KeyMode::Major, Resolution::Nearest),
            [67, 69, 71, 72]
        );
        assert_eq!(
            route_to_tonic(64, 0, KeyMode::Major, Resolution::Ascending),
            [64, 65, 67, 69, 71, 72]
        );
        assert_eq!(
            route_to_tonic(69, 0, KeyMode::Major, Resolution::Descending),
            [69, 67, 65, 64, 62, 60]
        );
        assert_eq!(
            route_to_tonic(60, 0, KeyMode::Major, Resolution::Nearest),
            [60]
        );
        // Chromatic ♯4 steps up into 5 and on; A minor ♭6 steps up through ♭7.
        assert_eq!(
            route_to_tonic(66, 0, KeyMode::Major, Resolution::Nearest),
            [66, 67, 69, 71, 72]
        );
        assert_eq!(
            route_to_tonic(65, 9, KeyMode::Minor, Resolution::Nearest),
            [65, 67, 69]
        );
    }

    #[test]
    fn scale_degree_trials_follow_the_key_and_resolve() {
        let b = block(
            r#"{"type": "scale_degree_id", "count": 70, "params": {"degrees": ["1", "2", "3"]}}"#,
        );
        let trials = generate_block(&b, 4, &GenContext::default()).unwrap();
        let mut seen = std::collections::HashSet::new();
        for (i, t) in trials.iter().enumerate() {
            let TrialSpec::Identify(t) = t else { panic!() };
            // 8 is the tonic an octave above the cadence's.
            assert_eq!(
                t.choices.iter().map(|c| c.id.as_str()).collect::<Vec<_>>(),
                ["1", "2", "3", "8"]
            );
            let note = t.events.last().unwrap().midi;
            let semis = if t.correct == "8" {
                0
            } else {
                parse_degree(&t.correct).unwrap()
            };
            assert_eq!((note - 0).rem_euclid(12), semis, "key of C");
            // In one key the cadence (16 notes) plays once, before the first question.
            let cadence = if i == 0 { 16 } else { 0 };
            assert_eq!(t.events.len(), cadence + 1, "trial {i}");
            // Answers open after the note and a beat's breath (2.5 beats at 100 bpm).
            assert_eq!(
                t.answer_after_ms,
                Some(if i == 0 { 6.0 * 600.0 } else { 0.0 })
            );
            // Every note of the walk to the tonic lights its answer.
            for e in t.after_answer.as_ref().unwrap() {
                assert!(t.note_choices.iter().any(|(m, _)| *m == e.midi));
            }
            let after = t.after_answer.as_ref().unwrap();
            assert_eq!(after[0].midi, note);
            assert_eq!(after.last().unwrap().midi.rem_euclid(12), 0);
            seen.insert(t.correct.clone());
        }
        assert_eq!(seen.len(), 4);
        // A new key per question, minor labels in solfege, the cadence only when due.
        let b = block(
            r#"{"type": "scale_degree_id", "count": 12, "params": {"keys": "all", "key_change": "per_trial", "modes": ["minor"], "labels": "solfege", "cadence_each_trial": false}}"#,
        );
        let trials = generate_block(&b, 5, &GenContext::default()).unwrap();
        let TrialSpec::Identify(t) = &trials[0] else {
            panic!()
        };
        assert_eq!(
            t.choices
                .iter()
                .map(|c| c.label.as_str())
                .collect::<Vec<_>>(),
            ["do", "re", "me", "fa", "sol", "le", "te", "do"]
        );
        let keys: std::collections::HashSet<_> =
            trials.iter().map(|t| t.stimulus()["key"].clone()).collect();
        assert!(keys.len() > 6);
        assert!(
            generate_block(
                &block(r#"{"type": "scale_degree_id", "params": {"degrees": ["9"]}}"#),
                1,
                &GenContext::default()
            )
            .is_err()
        );
    }

    #[test]
    fn fixed_do_names_the_keys_notes_and_cadences_follow_the_plan() {
        assert_eq!(
            (0..12)
                .filter(|s| mode_steps(KeyMode::Major).contains(s))
                .map(|s| fixed_do(
                    PitchClass::new(7),
                    KeyMode::Major,
                    s,
                    degree_number(DEGREE_IDS[s as usize])
                ))
                .collect::<Vec<_>>(),
            ["sol", "la", "ti", "do", "re", "mi", "fi"]
        );
        assert_eq!(fixed_do(PitchClass::new(5), KeyMode::Major, 5, 4), "te");
        assert_eq!(fixed_do(PitchClass::new(3), KeyMode::Minor, 3, 3), "se");
        // G♯ is si; D♭ ra; E♯ and B♯ are named as the notes they sound, fa and do.
        assert_eq!(fixed_do(PitchClass::new(9), KeyMode::Major, 11, 7), "si");
        assert_eq!(fixed_do(PitchClass::new(1), KeyMode::Major, 0, 1), "ra");
        assert_eq!(fixed_do(PitchClass::new(6), KeyMode::Major, 11, 7), "fa");
        assert_eq!(fixed_do(PitchClass::new(1), KeyMode::Minor, 11, 7), "do");
        // A double sharp (F𝄪 in G♯ minor) is the note it sounds: sol.
        assert_eq!(fixed_do(PitchClass::new(8), KeyMode::Minor, 11, 7), "sol");
        // Fixed do is the default label; a key per question cadences on each change.
        let b = block(
            r#"{"type": "scale_degree_id", "count": 6, "params": {"keys": "all", "key_change": "per_trial"}}"#,
        );
        for t in generate_block(&b, 2, &GenContext::default()).unwrap() {
            let TrialSpec::Identify(t) = t else { panic!() };
            assert_eq!(t.stimulus["cadence"], true);
            assert!(t.choices.iter().any(|c| c.label.starts_with("do")
                || c.label.starts_with("sol")
                || c.label.starts_with("re")
                || c.label.starts_with("mi")
                || c.label.starts_with("fa")
                || c.label.starts_with("la")
                || c.label.starts_with("ti")));
        }
        let every = block(
            r#"{"type": "scale_degree_id", "count": 4, "params": {"cadence": "every", "cadence_bpm": 60}}"#,
        );
        for t in generate_block(&every, 2, &GenContext::default()).unwrap() {
            let TrialSpec::Identify(t) = t else { panic!() };
            assert_eq!(t.answer_after_ms, Some(6.0 * 1000.0), "six beats at 60 bpm");
        }
    }

    #[test]
    fn intervals_from_the_tonic_diatonic_or_chromatic() {
        let ctx = GenContext::default();
        // From the tonic of G major: m3 is not in the scale unless chromatic.
        let diatonic = block(
            r#"{"type": "interval_id", "count": 20, "params": {"in_key": true, "keys": ["G"], "start": "tonic", "intervals": ["M3", "m3"], "directions": ["ascending"]}}"#,
        );
        assert!(generate_block(&diatonic, 1, &ctx).is_err());
        let chromatic = block(
            r#"{"type": "interval_id", "count": 40, "params": {"in_key": true, "keys": ["G"], "start": "tonic", "chromatic": true, "start_octaves": [0, 1], "intervals": ["M3", "m3"], "directions": ["ascending"]}}"#,
        );
        let trials = generate_block(&chromatic, 1, &ctx).unwrap();
        let mut seen_m3 = false;
        for (i, t) in trials.iter().enumerate() {
            let TrialSpec::Identify(t) = t else { panic!() };
            // The cadence plays once, before the first question.
            let lead = if i == 0 { 16 } else { 0 };
            assert_eq!(t.events.len(), lead + 2);
            assert_eq!(t.events[lead].midi.rem_euclid(12), 7, "starts on G");
            let x = t.explain.as_deref().unwrap();
            assert!(x.starts_with("sol → "), "{x}");
            if t.correct == "m3" {
                assert_eq!(x, "sol → te");
                assert_eq!(t.stimulus["diatonic"], false);
                seen_m3 = true;
            }
        }
        assert!(seen_m3);
    }

    #[test]
    fn one_interval_with_other() {
        let ctx = GenContext::default();
        let alone =
            block(r#"{"type": "interval_id", "count": 5, "params": {"intervals": ["M3"]}}"#);
        assert!(
            generate_block(&alone, 1, &ctx).is_err(),
            "one answer is not a question"
        );
        let b = block(
            r#"{"type": "interval_id", "count": 200, "params": {"intervals": ["M3"], "other_rate": 0.3, "start": "fixed", "start_note": "D4", "start_octaves": [-1, 0, 1]}}"#,
        );
        let trials = generate_block(&b, 3, &ctx).unwrap();
        let mut others = 0;
        for t in &trials {
            let TrialSpec::Identify(t) = t else { panic!() };
            assert_eq!(t.choices.len(), 2);
            let first = t.events[0].midi;
            assert_eq!(first.rem_euclid(12), 2, "every interval starts on a D");
            if t.correct == OTHER {
                others += 1;
                assert_ne!(t.stimulus["interval"], "M3");
            }
        }
        assert!((40..=80).contains(&others), "{others}");
    }

    #[test]
    fn compared_intervals_share_the_note_asked_for() {
        let ctx = GenContext::default();
        for (shared, check) in [("first", 0usize), ("second", 1)] {
            let b = block(&format!(
                r#"{{"type": "interval_comparison", "count": 30, "params": {{"directions": ["ascending"], "shared": ["{shared}"]}}}}"#
            ));
            for t in generate_block(&b, 2, &ctx).unwrap() {
                let TrialSpec::Identify(t) = t else { panic!() };
                let e = &t.events;
                // Ascending: notes 0,1 then 2,3.
                assert_eq!(e[check].midi, e[2 + check].midi, "{shared}");
            }
        }
    }

    #[test]
    fn singing_in_a_key_starts_with_a_cadence_and_names_the_notes() {
        let ctx = GenContext::default();
        let b = block(
            r#"{"type": "sing_interval", "count": 6, "params": {"in_key": true, "keys": ["G"], "start": "tonic", "intervals": ["M3", "P5"], "directions": ["up"]}}"#,
        );
        let trials = generate_block(&b, 1, &ctx).unwrap();
        for (i, t) in trials.iter().enumerate() {
            let TrialSpec::Sung(t) = t else { panic!() };
            assert!(t.prompt.starts_with("From sol, sing "), "{}", t.prompt);
            assert_eq!(t.notes[0].midi.rem_euclid(12), 7);
            // Four chords of four notes, then the first note, on the first question only.
            let expected = if i == 0 { 17 } else { 1 };
            assert_eq!(t.reference_beats.len(), expected, "trial {i}");
        }
    }

    #[test]
    fn intervals_in_a_key_stay_diatonic() {
        let b = block(
            r#"{"type": "interval_id", "count": 60, "params": {"in_key": true, "keys": ["D"], "intervals": ["m2", "M2", "m3", "M3", "P4", "TT", "P5", "m6", "M6", "m7", "M7", "P8"]}}"#,
        );
        let trials = generate_block(&b, 6, &GenContext::default()).unwrap();
        let d_major = [2, 4, 6, 7, 9, 11, 1];
        for (i, t) in trials.iter().enumerate() {
            let TrialSpec::Identify(t) = t else { panic!() };
            assert_eq!(t.stimulus["key"], "D");
            // One key: the cadence plays before the first question only.
            let lead = if i == 0 { 16 } else { 0 };
            let notes: Vec<i32> = t.events.iter().skip(lead).map(|e| e.midi).collect();
            assert_eq!(notes.len(), 2);
            for m in &notes {
                assert!(d_major.contains(&m.rem_euclid(12)), "{m} not in D major");
            }
            assert_eq!(t.answer_after_ms.is_some(), i == 0);
        }
        let minor_tt = block(
            r#"{"type": "interval_id", "count": 4, "params": {"in_key": true, "modes": ["minor"], "intervals": ["TT", "P4"]}}"#,
        );
        assert!(generate_block(&minor_tt, 1, &GenContext::default()).is_ok());
        // Free-standing intervals have no cadence.
        let free = block(r#"{"type": "interval_id", "count": 4}"#);
        let TrialSpec::Identify(t) = &generate_block(&free, 1, &GenContext::default()).unwrap()[0]
        else {
            panic!()
        };
        assert_eq!(t.events.len(), 2);
        assert!(t.answer_after_ms.is_none());
    }

    #[test]
    fn interval_comparison_answers_match_the_sizes() {
        let b = block(r#"{"type": "interval_comparison", "count": 80}"#);
        let trials = generate_block(&b, 8, &GenContext::default()).unwrap();
        let mut answers = std::collections::HashMap::new();
        for t in &trials {
            let TrialSpec::Identify(t) = t else { panic!() };
            let size = |k: &str| {
                Interval::ALL
                    .iter()
                    .find(|i| i.id() == t.stimulus[k])
                    .unwrap()
                    .semitones()
            };
            let (a, b) = (size("first"), size("second"));
            let expected = if a == b {
                "same"
            } else if a > b {
                "first"
            } else {
                "second"
            };
            assert_eq!(t.correct, expected);
            assert!((a - b).abs() <= 2);
            *answers.entry(t.correct.clone()).or_insert(0) += 1;
        }
        assert_eq!(answers.len(), 3);
        let no_same = block(
            r#"{"type": "interval_comparison", "count": 10, "params": {"differences": [1]}}"#,
        );
        let TrialSpec::Identify(t) =
            &generate_block(&no_same, 1, &GenContext::default()).unwrap()[0]
        else {
            panic!()
        };
        assert_eq!(t.choices.len(), 2);
    }

    fn pitch_trials(json: &str, seed: u64) -> Vec<IdentifyTrial> {
        generate_block(&block(json), seed, &GenContext::default())
            .unwrap()
            .into_iter()
            .map(|t| match t {
                TrialSpec::Identify(t) => t,
                _ => panic!("not an identification trial"),
            })
            .collect()
    }

    #[test]
    fn jump_rule_keeps_consecutive_tones_apart() {
        for seed in 0..10 {
            let trials = pitch_trials(
                r#"{"type": "pitch_id", "count": 1000, "params": {"octaves": [3, 4, 5], "min_jump_semitones": 13}}"#,
                seed,
            );
            for w in trials.windows(2) {
                let (a, b) = (w[0].events[0].midi, w[1].events[0].midi);
                assert!(
                    w[1].stimulus.get("constraint_relaxed").is_none(),
                    "seed {seed}: relaxed"
                );
                assert!((a - b).abs() >= 13, "seed {seed}: {a} then {b}");
            }
            // Every class still comes up often.
            for pc in PitchClass::ALL {
                let n = trials.iter().filter(|t| t.correct == pc.name()).count();
                assert!(n > 50, "seed {seed}: {pc:?} only {n} times");
            }
        }
    }

    #[test]
    fn subsets_mix_in_other_at_the_edges() {
        let trials = pitch_trials(
            r#"{"type": "pitch_id", "count": 1000, "params": {"classes": ["C", "E", "G#"], "out_of_set_rate": 0.2, "octaves": [3, 4, 5], "min_jump_semitones": 13}}"#,
            3,
        );
        let others: Vec<&IdentifyTrial> = trials.iter().filter(|t| t.correct == OTHER).collect();
        let share = others.len() as f64 / trials.len() as f64;
        assert!((share - 0.2).abs() < 0.03, "{share}");
        for t in &trials {
            assert_eq!(t.choices.len(), 4);
            assert_eq!(t.choices.last().unwrap().id, OTHER);
            let pc = t.stimulus["pitch_class"].as_str().unwrap();
            let inside = ["C", "E", "G#"].contains(&pc);
            assert_eq!(t.correct == OTHER, !inside);
            assert_eq!(t.stimulus["out_of_set"], json!(!inside));
            if !inside {
                // "Other" plays the tone that was heard.
                assert_eq!(t.choice_sound(OTHER).unwrap()[0].midi, t.events[0].midi);
            }
        }
        // Out-of-set tones sit at the edges of the range unless the jump rule moved them.
        let edge = others
            .iter()
            .filter(|t| matches!(t.stimulus["octave"].as_i64(), Some(3) | Some(5)))
            .count();
        assert!(
            edge as f64 / others.len() as f64 > 0.75,
            "{edge} of {}",
            others.len()
        );
        // "Other" is a pitch-class answer.
        let bad = block(
            r#"{"type": "pitch_id", "count": 5, "params": {"classes": ["C", "E"], "out_of_set_rate": 0.2, "answer": "note_with_octave"}}"#,
        );
        assert!(generate_block(&bad, 1, &GenContext::default()).is_err());
    }

    #[test]
    fn class_weights_set_the_anchor_share() {
        let trials = pitch_trials(
            r#"{"type": "pitch_id", "count": 1000, "params": {"class_weights": {"C": 0.4}}}"#,
            9,
        );
        let share = |pc: &str| {
            trials.iter().filter(|t| t.correct == pc).count() as f64 / trials.len() as f64
        };
        assert!((share("C") - 0.4).abs() < 0.02, "{}", share("C"));
        for pc in PitchClass::ALL.iter().skip(1) {
            assert!(
                (share(pc.name()) - 0.6 / 11.0).abs() < 0.02,
                "{pc:?}: {}",
                share(pc.name())
            );
        }
        let ctx = GenContext::default();
        for bad in [
            r#"{"type": "pitch_id", "params": {"class_weights": {"C": 1.2}}}"#,
            r#"{"type": "pitch_id", "params": {"classes": ["C", "E"], "class_weights": {"D": 0.5}}}"#,
            r#"{"type": "pitch_id", "params": {"class_weights": {"H": 0.5}}}"#,
        ] {
            assert!(generate_block(&block(bad), 1, &ctx).is_err(), "{bad}");
        }
    }

    #[test]
    fn naming_shows_note_colours_only_when_answers_show() {
        let taneda = crate::labels::builtin_set(crate::labels::TANEDA_COLORS).unwrap();
        for t in pitch_trials(
            r#"{"type": "pitch_id", "count": 6, "feedback": "after_trial"}"#,
            3,
        ) {
            // Letter names stay; every key takes its note's colour.
            for c in t.choices.iter().filter(|c| c.id != OTHER) {
                assert_eq!(c.color, taneda.entries[&c.id].color, "{}", c.id);
                assert_ne!(c.word.as_deref(), Some(c.label.as_str()));
            }
            let pc = t.stimulus["pitch_class"].as_str().unwrap();
            assert_eq!(t.answer_color, taneda.entries[pc].color);
            assert!(t.flash_color.is_none());
        }
        for t in pitch_trials(r#"{"type": "pitch_id", "count": 6, "assessment": true}"#, 3) {
            assert!(t.choices.iter().all(|c| c.color.is_none()));
            assert!(t.answer_color.is_none());
        }
        // A trial with its own colours keeps them.
        let b = block(
            r#"{"type": "pitch_id", "count": 4, "params": {"labels": "gervain_names", "color_flash": true, "hide_note_names": true}}"#,
        );
        for t in generate_block(&b, 1, &GenContext::default()).unwrap() {
            let TrialSpec::Identify(t) = t else { panic!() };
            assert!(t.flash_color.is_some() && t.answer_color.is_none());
        }
        let b = block(r#"{"type": "sing_pitch", "count": 3}"#);
        for t in generate_block(&b, 1, &GenContext::default()).unwrap() {
            let TrialSpec::SingPitch(t) = t else { panic!() };
            assert_eq!(t.answer_color, taneda.entries[t.target_class.name()].color);
        }
    }

    #[test]
    fn streams_are_paced_without_masks() {
        let trials = pitch_trials(
            r#"{"type": "pitch_id", "count": 5, "params": {"pacing": {"stream": {"ioi_ms": 1400}}, "mask": "noise"}}"#,
            1,
        );
        for t in &trials {
            assert_eq!(t.pace_ms, Some(1400));
            assert_eq!(t.mask_before, Mask::None);
        }
        // A tone must fit in its slot.
        let tight = block(
            r#"{"type": "pitch_id", "count": 5, "params": {"pacing": {"stream": {"ioi_ms": 1100}}}}"#,
        );
        assert!(generate_block(&tight, 1, &GenContext::default()).is_err());
    }

    #[test]
    fn detection_streams_keep_the_target_share_and_the_jump() {
        let trials = generate_block(
            &block(r#"{"type":"pitch_detect","count":30,"params":{"octaves":[3,4,5],"min_jump_semitones":13}}"#),
            2,
            &GenContext::default(),
        )
        .unwrap();
        assert_eq!(trials.len(), 30);
        for t in &trials {
            let TrialSpec::Detect(t) = t else {
                panic!("not a stream")
            };
            assert_eq!(t.tones.len(), 16);
            assert_eq!(t.tones.iter().filter(|n| n.target).count(), 4);
            let relaxed = t
                .stimulus
                .get("constraint_relaxed")
                .and_then(|v| v.as_bool())
                .unwrap_or(false);
            if !relaxed {
                for w in t.tones.windows(2) {
                    assert!((w[0].midi - w[1].midi).abs() >= 13);
                }
            }
            assert!(
                t.prompt_events[0].timbre.ends_with("52")
                    || t.prompt_events[0].timbre.contains("52")
            );
        }
    }

    #[test]
    fn chord_colours_use_the_voicing_and_extract_a_named_note() {
        let trials = generate_block(
            &block(r#"{"type":"chord_color_id","count":6,"params":{"chords":["red"],"single_note_after":true}}"#),
            4,
            &GenContext::default(),
        )
        .unwrap();
        assert_eq!(trials.len(), 12);
        let TrialSpec::Identify(chord) = &trials[0] else {
            panic!()
        };
        assert_eq!(chord.exercise, ExerciseKind::ChordColorId);
        assert_eq!(
            chord.events.iter().map(|e| e.midi).collect::<Vec<_>>(),
            vec![60, 64, 67]
        );
        assert_eq!(chord.correct, "red");
        let TrialSpec::Identify(note) = &trials[1] else {
            panic!()
        };
        assert_eq!(note.exercise, ExerciseKind::PitchId);
        assert_eq!(note.stimulus["context"], json!("chord_extraction"));
        assert!(
            note.choices
                .iter()
                .any(|c| c.word.as_deref() == Some("Red") || c.label == "Red" || c.color.is_some())
        );
    }

    #[test]
    fn requeue_moves_a_missed_note_to_another_octave() {
        let trial = generate_block(
            &block(r#"{"type":"pitch_id","count":1,"params":{"classes":["C","E","G"],"octaves":[3,4,5],"requeue_misses":true}}"#),
            8,
            &GenContext::default(),
        )
        .unwrap()
        .remove(0);
        let again = requeue_trial(&trial).unwrap();
        let TrialSpec::Identify(a) = &trial else {
            panic!()
        };
        let TrialSpec::Identify(b) = &again else {
            panic!()
        };
        assert!(a.requeue_misses);
        assert_ne!(a.stimulus["octave"], b.stimulus["octave"]);
        assert_eq!(a.stimulus["pitch_class"], b.stimulus["pitch_class"]);
    }

    #[test]
    fn label_sets_colour_the_buttons_and_can_hide_the_letter() {
        let TrialSpec::Identify(t) = &generate_block(
            &block(r#"{"type":"pitch_id","count":4,"params":{"classes":["C","E","G"],"labels":"taneda_colors","hide_note_names":true,"color_flash":true}}"#),
            1,
            &GenContext::default(),
        )
        .unwrap()[0]
        else {
            panic!()
        };
        let c = t.choices.iter().find(|c| c.id == "C").unwrap();
        assert_eq!(c.label, "Red");
        assert_eq!(c.color.as_deref(), Some("#d64545"));
        assert!(t.flash_color.is_some());
        assert_eq!(t.stimulus["label_set"], json!("taneda_colors"));
        assert_eq!(t.stimulus["pitch_class"].as_str().is_some(), true);
    }
}
