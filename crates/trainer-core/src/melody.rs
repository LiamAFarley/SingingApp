//! Deterministic melody generator for sight singing and dictation.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::rng::Rng;
use crate::schema::{KeyMode, NoteRange};
use crate::score::{ClefChoice, KeySig, Pitch, Score, ScoreNote, TICKS_PER_QUARTER, TimeSig};

/// Note values a generated melody may use.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "snake_case")]
pub enum NoteValue {
    Whole,
    DottedHalf,
    Half,
    DottedQuarter,
    Quarter,
    Eighth,
    Sixteenth,
}

impl NoteValue {
    pub fn ticks(self) -> u32 {
        let q = TICKS_PER_QUARTER;
        match self {
            NoteValue::Whole => 4 * q,
            NoteValue::DottedHalf => 3 * q,
            NoteValue::Half => 2 * q,
            NoteValue::DottedQuarter => q * 3 / 2,
            NoteValue::Quarter => q,
            NoteValue::Eighth => q / 2,
            NoteValue::Sixteenth => q / 4,
        }
    }
}

/// How a generated melody is built. Defaults give a four-bar diatonic tune in 4/4 with quarter and
/// half notes, mostly stepwise, inside the singer's comfortable range, ending on the tonic.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct MelodyParams {
    /// Tonics to choose from, e.g. ["C", "G", "F", "Bb"]. Empty: keys with up to two sharps or flats.
    #[serde(default)]
    pub keys: Vec<String>,
    #[serde(default = "d_major")]
    pub modes: Vec<KeyMode>,
    #[serde(default = "d_meters")]
    pub meters: Vec<TimeSig>,
    #[serde(default = "d_bars")]
    pub bars: u32,
    /// Note values to use; quarter notes fill any gap these cannot.
    #[serde(default = "d_rhythms")]
    pub rhythms: Vec<NoteValue>,
    /// Largest leap in scale steps (2 = a third, 4 = a fifth, 7 = an octave).
    #[serde(default = "d_max_leap")]
    pub max_leap: u8,
    /// Probability that a move is a step rather than a leap.
    #[serde(default = "d_step_rate")]
    pub step_rate: f32,
    /// Probability that an inner note becomes a rest.
    #[serde(default)]
    pub rest_rate: f32,
    /// Scale degrees (1–7) the melody may start on.
    #[serde(default = "d_start_degrees")]
    pub start_degrees: Vec<u8>,
    #[serde(default = "d_true")]
    pub end_on_tonic: bool,
    /// Defaults to the singer's comfortable range.
    #[serde(default)]
    pub range: Option<NoteRange>,
    #[serde(default)]
    pub clef: ClefChoice,
}

impl Default for MelodyParams {
    fn default() -> Self {
        serde_json::from_str("{}").unwrap()
    }
}

fn d_major() -> Vec<KeyMode> {
    vec![KeyMode::Major]
}
fn d_meters() -> Vec<TimeSig> {
    vec![TimeSig::COMMON]
}
fn d_bars() -> u32 {
    4
}
fn d_rhythms() -> Vec<NoteValue> {
    vec![NoteValue::Half, NoteValue::Quarter]
}
fn d_max_leap() -> u8 {
    4
}
fn d_step_rate() -> f32 {
    0.7
}
fn d_start_degrees() -> Vec<u8> {
    vec![1, 3, 5]
}
fn d_true() -> bool {
    true
}

const DEFAULT_MAJOR: [&str; 5] = ["C", "G", "F", "D", "Bb"];
const DEFAULT_MINOR: [&str; 5] = ["A", "E", "D", "B", "G"];

/// Rhythm cells: groups of note values that fill `len` beats starting on a beat.
struct Cell {
    values: &'static [NoteValue],
    weight: u32,
}

fn cells_for(beat_ticks: u32) -> Vec<Cell> {
    use NoteValue::*;
    if beat_ticks == TICKS_PER_QUARTER {
        vec![
            Cell { values: &[Quarter], weight: 6 },
            Cell { values: &[Eighth, Eighth], weight: 4 },
            Cell { values: &[Sixteenth, Sixteenth, Sixteenth, Sixteenth], weight: 1 },
            Cell { values: &[Eighth, Sixteenth, Sixteenth], weight: 1 },
            Cell { values: &[Sixteenth, Sixteenth, Eighth], weight: 1 },
            Cell { values: &[Half], weight: 4 },
            Cell { values: &[DottedQuarter, Eighth], weight: 2 },
            Cell { values: &[DottedHalf], weight: 1 },
            Cell { values: &[Whole], weight: 1 },
        ]
    } else {
        // Eighth-note beats (x/8): groups of a dotted quarter.
        vec![
            Cell { values: &[DottedQuarter], weight: 4 },
            Cell { values: &[Quarter, Eighth], weight: 4 },
            Cell { values: &[Eighth, Eighth, Eighth], weight: 3 },
            Cell { values: &[DottedHalf], weight: 1 },
        ]
    }
}

fn cell_ticks(c: &Cell) -> u32 {
    c.values.iter().map(|v| v.ticks()).sum()
}

/// Fills `ticks` starting at bar position `pos` with allowed cells; returns note durations.
fn fill(
    ticks: u32,
    pos: u32,
    meter: TimeSig,
    allowed: &[NoteValue],
    rng: &mut Rng,
) -> Vec<u32> {
    let bt = meter.beat_ticks();
    let group = if bt == TICKS_PER_QUARTER { bt } else { bt * 3 };
    let cells = cells_for(bt);
    let mut out = Vec::new();
    let mut at = pos;
    let end = pos + ticks;
    while at < end {
        let left = end - at;
        let fits: Vec<&Cell> = cells
            .iter()
            .filter(|c| {
                let len = cell_ticks(c);
                len <= left
                    && c.values.iter().all(|v| allowed.contains(v))
                    // Long cells start on strong positions: halves on even beats in 4/4 and 2/4.
                    && (len <= group || meter.beats == 3 || at % (2 * group).min(meter.bar_ticks()) == 0)
                    && (len < meter.bar_ticks() || at == 0)
            })
            .collect();
        if fits.is_empty() {
            // Quarter notes (or a single beat group) fill what the vocabulary cannot.
            let step = group.min(left);
            out.push(step);
            at += step;
            continue;
        }
        let total: u32 = fits.iter().map(|c| c.weight).sum();
        let mut pick = rng.below(total as usize) as u32;
        let mut chosen = fits[0];
        for c in &fits {
            if pick < c.weight {
                chosen = c;
                break;
            }
            pick -= c.weight;
        }
        for v in chosen.values {
            out.push(v.ticks());
        }
        at += cell_ticks(chosen);
    }
    out
}

/// Rhythm for the whole melody: durations per bar, with a long final note.
fn rhythm(bars: u32, meter: TimeSig, allowed: &[NoteValue], rng: &mut Rng) -> Vec<Vec<u32>> {
    let bar = meter.bar_ticks();
    let mut out = Vec::with_capacity(bars as usize);
    for b in 0..bars {
        if b + 1 < bars {
            out.push(fill(bar, 0, meter, allowed, rng));
            continue;
        }
        // Final bar: end on a long note, the longest allowed value or the next one down.
        let bt = meter.beat_ticks();
        let group = if bt == TICKS_PER_QUARTER { bt } else { bt * 3 };
        let long: Vec<u32> = [
            NoteValue::Whole,
            NoteValue::DottedHalf,
            NoteValue::Half,
            NoteValue::DottedQuarter,
            NoteValue::Quarter,
        ]
        .into_iter()
        .filter(|v| allowed.contains(v) && v.ticks() <= bar && v.ticks() >= group)
        // The final note starts on a beat (group) boundary.
        .filter(|v| (bar - v.ticks()) % group == 0)
        .map(|v| v.ticks())
        .collect();
        // At least two beats (or groups) long when the vocabulary and the bar allow it.
        let min_len = (2 * group).min(bar);
        let longer: Vec<u32> = long.iter().copied().filter(|t| *t >= min_len).collect();
        let pool = if longer.is_empty() { &long } else { &longer };
        let last = match pool.as_slice() {
            [] => group.min(bar),
            [only] => *only,
            [first, second, ..] => {
                if rng.chance(0.6) {
                    *first
                } else {
                    *second
                }
            }
        };
        let mut v = fill(bar - last, 0, meter, allowed, rng);
        v.push(last);
        out.push(v);
    }
    out
}

/// The tonic octave that leaves the most usable scale degrees around the tonic inside `range`.
fn tonic_octave(key: &KeySig, range: NoteRange) -> i32 {
    let mut best = (i32::MIN, 4);
    for o in 1..=6 {
        let usable = (-4..=7)
            .filter(|d| {
                let m = key.degree(*d, o).midi();
                m >= range.low.0 && m <= range.high.0
            })
            .count() as i32;
        let tonic = key.degree(0, o).midi();
        let tonic_inside = tonic >= range.low.0 && tonic <= range.high.0;
        let score = usable * 2 + if tonic_inside { 3 } else { 0 };
        if score > best.0 {
            best = (score, o);
        }
    }
    best.1
}

/// A generated melody, or an error when the parameters cannot produce one.
pub fn generate_melody(
    p: &MelodyParams,
    default_range: NoteRange,
    tempo_bpm: f32,
    rng: &mut Rng,
) -> Result<Score, String> {
    if p.bars == 0 || p.bars > 32 {
        return Err("bars must be 1–32".into());
    }
    if p.rhythms.is_empty() {
        return Err("rhythms must not be empty".into());
    }
    if p.max_leap == 0 || p.max_leap > 7 {
        return Err("max_leap must be 1–7 scale steps".into());
    }
    if p.start_degrees.iter().any(|d| !(1..=7).contains(d)) {
        return Err("start_degrees must be 1–7".into());
    }
    let modes = if p.modes.is_empty() { d_major() } else { p.modes.clone() };
    let mode = *rng.choose(&modes).unwrap();
    let tonic = if p.keys.is_empty() {
        let list: &[&str] = match mode {
            KeyMode::Major => &DEFAULT_MAJOR,
            KeyMode::Minor => &DEFAULT_MINOR,
        };
        rng.choose(list).unwrap().to_string()
    } else {
        rng.choose(&p.keys).unwrap().clone()
    };
    let key = KeySig::new(&tonic, mode)?;
    let meters = if p.meters.is_empty() { d_meters() } else { p.meters.clone() };
    let meter = *rng.choose(&meters).unwrap();
    if meter.unit == 2 {
        return Err("generated melodies support x/4 and x/8 meters".into());
    }
    if meter.unit == 8 && meter.beats % 3 != 0 {
        return Err("x/8 meters must be compound (3/8, 6/8, 9/8, 12/8)".into());
    }
    let range = p.range.unwrap_or(default_range);
    if range.high.0 - range.low.0 < 7 {
        return Err("the range must span at least a fifth".into());
    }
    let octave = tonic_octave(&key, range);
    let allowed: Vec<i32> = (-14..=21)
        .filter(|d| {
            let m = key.degree(*d, octave).midi();
            m >= range.low.0 && m <= range.high.0
        })
        .collect();
    if allowed.len() < 5 {
        return Err(format!("the range {}–{} leaves too few notes in {key}", range.low, range.high));
    }

    let bars = rhythm(p.bars, meter, &p.rhythms, rng);
    let durations: Vec<u32> = bars.iter().flatten().copied().collect();
    let n = durations.len();

    // Rests replace some inner notes (never two in a row, never the first or last note).
    let mut rest = vec![false; n];
    if p.rest_rate > 0.0 {
        for i in 1..n.saturating_sub(1) {
            if !rest[i - 1] && rng.chance(p.rest_rate as f64) {
                rest[i] = true;
            }
        }
    }
    let sung: Vec<usize> = (0..n).filter(|i| !rest[*i]).collect();
    let degrees = walk(p, &allowed, sung.len(), rng, |d| key.degree(d, octave).midi())?;

    let mut notes = Vec::with_capacity(n);
    let mut t = 0u32;
    let mut k = 0usize;
    for (i, dur) in durations.iter().enumerate() {
        let pitch = if rest[i] {
            None
        } else {
            let d = degrees[k];
            let next = degrees.get(k + 1).copied();
            let prev = if k > 0 { Some(degrees[k - 1]) } else { None };
            k += 1;
            Some(spell_degree(&key, octave, d, prev, next))
        };
        notes.push(ScoreNote {
            pitch,
            start: t,
            dur: *dur,
        });
        t += dur;
    }
    let mut score = Score {
        key,
        time: meter,
        clef: crate::score::Clef::Treble,
        tempo_bpm,
        bars: p.bars,
        notes,
        title: None,
    };
    let (lo, hi) = score.range().ok_or("no notes were generated")?;
    score.clef = p.clef.resolve(lo, hi);
    score.validate()?;
    Ok(score)
}

/// Pitch of scale degree `d`; in minor keys the seventh is raised when it leads to the tonic
/// (unless it comes from the sixth, which would make an augmented second).
fn spell_degree(key: &KeySig, octave: i32, d: i32, prev: Option<i32>, next: Option<i32>) -> Pitch {
    let p = key.degree(d, octave);
    if key.mode == KeyMode::Minor && d.rem_euclid(7) == 6 && next == Some(d + 1) {
        let raised = Pitch::new(p.step, p.alter + 1, p.octave);
        // Only when approached by a plain (major, minor or perfect) interval.
        if prev.is_none_or(|x| plain_interval(key.degree(x, octave), raised)) {
            return raised;
        }
    }
    p
}

/// Whether two spelled pitches form a major, minor or perfect interval (not augmented or
/// diminished), e.g. B–G# (a minor third) but not D–G# or F–G#.
fn plain_interval(a: Pitch, b: Pitch) -> bool {
    let (lo, hi) = if a.midi() <= b.midi() { (a, b) } else { (b, a) };
    let steps = (hi.diatonic() - lo.diatonic()).rem_euclid(7);
    let semis = (hi.midi() - lo.midi()).rem_euclid(12);
    matches!(
        (steps, semis),
        (0, 0) | (1, 1) | (1, 2) | (2, 3) | (2, 4) | (3, 5) | (4, 7) | (5, 8) | (5, 9) | (6, 10) | (6, 11)
    )
}

/// The sequence of scale degrees: a random walk over `allowed`, mostly by step, recovering after
/// leaps, avoiding tritone leaps and (optionally) ending on the tonic, approached by step.
fn walk(
    p: &MelodyParams,
    allowed: &[i32],
    n: usize,
    rng: &mut Rng,
    midi_of: impl Fn(i32) -> i32,
) -> Result<Vec<i32>, String> {
    if n == 0 {
        return Err("the rhythm left no notes".into());
    }
    let lo = *allowed.first().unwrap();
    let hi = *allowed.last().unwrap();
    let mid = (lo + hi) / 2;
    let inside = |d: i32| d >= lo && d <= hi;
    let max_leap = p.max_leap as i32;
    let tritone = |a: i32, b: i32| (midi_of(a) - midi_of(b)).abs() == 6;
    let leap_ok = |a: i32, b: i32| (b - a).abs() <= max_leap && !((b - a).abs() > 1 && tritone(a, b));
    let tonics: Vec<i32> = allowed.iter().copied().filter(|d| d.rem_euclid(7) == 0).collect();
    if p.end_on_tonic && tonics.is_empty() {
        return Err("the range contains no tonic".into());
    }
    let starts: Vec<i32> = allowed
        .iter()
        .copied()
        .filter(|d| p.start_degrees.contains(&((d.rem_euclid(7) + 1) as u8)))
        .collect();
    if starts.is_empty() {
        return Err("no start degree fits the range".into());
    }
    let mut ranked = starts.clone();
    ranked.sort_by_key(|d| (d - mid).abs());
    if n == 1 && p.end_on_tonic {
        let t = *tonics.iter().min_by_key(|t| (*t - mid).abs()).unwrap();
        return Ok(vec![t]);
    }

    // Notes before the ending: all of them, or all but the approach and the final tonic.
    let body = if p.end_on_tonic { n.saturating_sub(2).max(1) } else { n };
    for _attempt in 0..200 {
        let first = ranked[rng.below(ranked.len().min(3))];
        let mut out = vec![first];
        let mut last_leap = 0i32;
        let mut ok = true;
        while out.len() < body {
            let cur = *out.last().unwrap();
            let mut placed = false;
            for _ in 0..20 {
                let mut step = if last_leap.abs() >= 3 && rng.chance(0.8) {
                    // Recover from a large leap by a step the other way.
                    -last_leap.signum()
                } else if rng.chance(p.step_rate as f64) {
                    if rng.chance(0.05) {
                        0
                    } else if rng.chance(0.5) {
                        1
                    } else {
                        -1
                    }
                } else {
                    let size = rng.range_i32(2, max_leap.max(2));
                    if rng.chance(0.5) { size } else { -size }
                };
                // Reflect off the edges of the range.
                if !inside(cur + step) {
                    step = -step;
                }
                let next = cur + step;
                if !inside(next) || !leap_ok(cur, next) {
                    continue;
                }
                if step == 0 && out.len() >= 2 && out[out.len() - 2] == cur {
                    continue;
                }
                out.push(next);
                last_leap = if step.abs() > 1 { step } else { 0 };
                placed = true;
                break;
            }
            if !placed {
                ok = false;
                break;
            }
        }
        if !ok {
            continue;
        }
        if !p.end_on_tonic {
            return Ok(out);
        }
        let prev = *out.last().unwrap();
        if n == 2 {
            // Two notes: the start, then the nearest reachable tonic.
            if let Some(&t) = tonics
                .iter()
                .filter(|t| **t != prev && leap_ok(prev, **t))
                .min_by_key(|t| (*t - prev).abs())
            {
                out.push(t);
                return Ok(out);
            }
            continue;
        }
        // Approach the final tonic by step from above or below.
        let tonic = *tonics.iter().min_by_key(|t| (*t - prev).abs()).unwrap();
        let options: Vec<i32> = [tonic + 1, tonic - 1]
            .into_iter()
            .filter(|d| inside(*d) && leap_ok(prev, *d))
            .collect();
        if options.is_empty() {
            continue;
        }
        // Usually the option nearer the previous note, so the approach is a step when it can be.
        let nearest = *options.iter().min_by_key(|d| (*d - prev).abs()).unwrap();
        let pen = if rng.chance(0.75) {
            nearest
        } else {
            *rng.choose(&options).unwrap()
        };
        out.push(pen);
        out.push(tonic);
        return Ok(out);
    }
    Err("could not build a melody with these constraints; widen the range or allow larger leaps".into())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make(p: &MelodyParams, seed: u64) -> Score {
        let mut rng = Rng::new(seed);
        generate_melody(p, NoteRange::new(48, 67), 80.0, &mut rng).unwrap()
    }

    #[test]
    fn default_melodies_fill_bars_and_end_on_the_tonic() {
        let p = MelodyParams::default();
        for seed in 0..200 {
            let s = make(&p, seed);
            assert_eq!(s.bars, 4);
            let end = s.notes.last().map(|n| n.start + n.dur).unwrap();
            assert_eq!(end, s.total_ticks(), "seed {seed}");
            let (lo, hi) = s.range().unwrap();
            assert!(lo >= 48 && hi <= 67, "seed {seed}: {lo}–{hi}");
            let last = s.notes.last().unwrap().pitch.unwrap();
            assert_eq!(last.midi().rem_euclid(12), s.key.tonic_pc(), "seed {seed}");
            for n in &s.notes {
                assert!([4, 8].contains(&n.dur), "seed {seed}: duration {}", n.dur);
            }
            assert_eq!(s.notes.last().unwrap().dur, 8, "seed {seed}: the last note is a half note");
            let first = s.notes[0].pitch.unwrap();
            let rel = (first.midi() - s.key.tonic_pc()).rem_euclid(12);
            assert!([0, 4, 7].contains(&rel), "seed {seed}: starts {rel} semitones above the tonic");
        }
    }

    #[test]
    fn leaps_stay_within_the_limit_and_avoid_tritones() {
        let p = MelodyParams {
            max_leap: 4,
            step_rate: 0.4,
            ..MelodyParams::default()
        };
        for seed in 0..200 {
            let s = make(&p, seed);
            let ps: Vec<Pitch> = s.pitched().map(|(_, p)| p).collect();
            for w in ps.windows(2) {
                let steps = (w[1].diatonic() - w[0].diatonic()).abs();
                let semis = (w[1].midi() - w[0].midi()).abs();
                assert!(steps <= 4, "seed {seed}: leap of {steps} steps");
                assert!(!(steps > 1 && semis == 6), "seed {seed}: tritone leap");
            }
        }
    }

    #[test]
    fn same_seed_same_melody() {
        let p = MelodyParams {
            rhythms: vec![NoteValue::Quarter, NoteValue::Eighth, NoteValue::Half, NoteValue::DottedQuarter],
            meters: vec!["3/4".parse().unwrap(), TimeSig::COMMON],
            modes: vec![KeyMode::Major, KeyMode::Minor],
            rest_rate: 0.1,
            ..MelodyParams::default()
        };
        for seed in 0..50 {
            let a = make(&p, seed);
            let b = make(&p, seed);
            assert_eq!(a, b);
            let end = a.notes.last().map(|n| n.start + n.dur).unwrap();
            assert_eq!(end, a.total_ticks());
        }
    }

    #[test]
    fn minor_keys_raise_the_leading_tone_into_the_tonic() {
        let p = MelodyParams {
            modes: vec![KeyMode::Minor],
            keys: vec!["A".into()],
            ..MelodyParams::default()
        };
        let mut saw_raised = false;
        for seed in 0..200 {
            let s = make(&p, seed);
            let ps: Vec<Pitch> = s.pitched().map(|(_, p)| p).collect();
            for w in ps.windows(2) {
                if w[0].class_name() == "G#" {
                    saw_raised = true;
                    assert_eq!(w[1].class_name(), "A", "seed {seed}");
                }
            }
        }
        assert!(saw_raised);
    }

    #[test]
    fn leading_tones_are_approached_by_plain_intervals() {
        let p = MelodyParams {
            modes: vec![KeyMode::Minor],
            max_leap: 5,
            step_rate: 0.4,
            ..MelodyParams::default()
        };
        for seed in 0..300 {
            let s = make(&p, seed);
            let ps: Vec<Pitch> = s.pitched().map(|(_, p)| p).collect();
            for w in ps.windows(2) {
                assert!(plain_interval(w[0], w[1]), "seed {seed}: {} to {}", w[0], w[1]);
            }
        }
    }

    #[test]
    fn compound_meters_and_eighths() {
        let p = MelodyParams {
            meters: vec!["6/8".parse().unwrap()],
            rhythms: vec![NoteValue::DottedQuarter, NoteValue::Quarter, NoteValue::Eighth, NoteValue::DottedHalf],
            ..MelodyParams::default()
        };
        for seed in 0..50 {
            let s = make(&p, seed);
            assert_eq!(s.bar_ticks(), 12);
            let end = s.notes.last().map(|n| n.start + n.dur).unwrap();
            assert_eq!(end, s.total_ticks());
        }
    }

    #[test]
    fn impossible_ranges_are_reported() {
        let p = MelodyParams {
            range: Some(NoteRange::new(60, 63)),
            ..MelodyParams::default()
        };
        let mut rng = Rng::new(1);
        assert!(generate_melody(&p, NoteRange::new(48, 67), 80.0, &mut rng).is_err());
    }
}

