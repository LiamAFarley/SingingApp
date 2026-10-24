//! Monophonic scores for sight singing and dictation: spelled notes on a sixteenth-note grid with a
//! key, time signature and clef. Scores are graded as timed target notes and drawn as staff
//! notation by the UI.

use std::borrow::Cow;
use std::fmt;
use std::str::FromStr;

use schemars::{JsonSchema, Schema, SchemaGenerator, json_schema};
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use ts_rs::TS;

use crate::schema::{KeyMode, SoundEvent, TargetNote};

/// Grid resolution: a quarter note is four ticks (sixteenth notes; no tuplets yet).
pub const TICKS_PER_QUARTER: u32 = 4;

const STEP_SEMITONES: [i32; 7] = [0, 2, 4, 5, 7, 9, 11];
const STEP_LETTERS: [char; 7] = ['C', 'D', 'E', 'F', 'G', 'A', 'B'];
/// Steps that carry sharps in key signatures, in order (F C G D A E B).
const SHARP_ORDER: [u8; 7] = [3, 0, 4, 1, 5, 2, 6];
/// Steps that carry flats in key signatures, in order (B E A D G C F).
const FLAT_ORDER: [u8; 7] = [6, 2, 5, 1, 4, 0, 3];
/// Major-key tonics by number of fifths, -7 (Cb) to 7 (C#).
const MAJOR_TONICS: [&str; 15] = [
    "Cb", "Gb", "Db", "Ab", "Eb", "Bb", "F", "C", "G", "D", "A", "E", "B", "F#", "C#",
];
/// Minor-key tonics by number of fifths, -7 (Ab) to 7 (A#).
const MINOR_TONICS: [&str; 15] = [
    "Ab", "Eb", "Bb", "F", "C", "G", "D", "A", "E", "B", "F#", "C#", "G#", "D#", "A#",
];

fn accidental_text(alter: i8) -> &'static str {
    match alter {
        -2 => "bb",
        -1 => "b",
        1 => "#",
        2 => "##",
        _ => "",
    }
}

/// Parses a letter with accidentals (`Eb`, `F#`, `Bbb`) into (step, alter) and the remainder.
fn parse_step(s: &str) -> Option<(u8, i8, &str)> {
    let mut chars = s.char_indices();
    let (_, letter) = chars.next()?;
    let step = STEP_LETTERS
        .iter()
        .position(|c| *c == letter.to_ascii_uppercase())? as u8;
    let mut alter: i8 = 0;
    let mut rest = &s[letter.len_utf8()..];
    for (i, c) in chars {
        match c {
            '#' | '♯' => alter += 1,
            'b' | '♭' => alter -= 1,
            'x' | '𝄪' => alter += 2,
            _ => {
                rest = &s[i..];
                break;
            }
        }
        rest = &s[i + c.len_utf8()..];
    }
    if !(-2..=2).contains(&alter) {
        return None;
    }
    Some((step, alter, rest))
}

// ---------------------------------------------------------------------------------------------
// Pitch
// ---------------------------------------------------------------------------------------------

/// A spelled pitch: letter, alteration and octave, written like `Eb4`, `F#3` or `Bbb2`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, TS)]
#[ts(type = "string")]
pub struct Pitch {
    /// 0 = C … 6 = B.
    pub step: u8,
    /// Semitones, -2 to 2.
    pub alter: i8,
    pub octave: i8,
}

impl Pitch {
    pub fn new(step: u8, alter: i8, octave: i8) -> Self {
        Pitch {
            step: step % 7,
            alter,
            octave,
        }
    }

    pub fn midi(self) -> i32 {
        (self.octave as i32 + 1) * 12 + STEP_SEMITONES[self.step as usize] + self.alter as i32
    }

    /// Position on the staff in diatonic steps, seven per octave (C4 = 28).
    pub fn diatonic(self) -> i32 {
        self.octave as i32 * 7 + self.step as i32
    }

    pub fn from_diatonic(d: i32, alter: i8) -> Self {
        Pitch::new(d.rem_euclid(7) as u8, alter, d.div_euclid(7) as i8)
    }

    pub fn letter(self) -> char {
        STEP_LETTERS[self.step as usize]
    }

    /// Letter and accidentals without the octave, e.g. `Eb`.
    pub fn class_name(self) -> String {
        format!("{}{}", self.letter(), accidental_text(self.alter))
    }

    pub fn name(self) -> String {
        format!("{}{}", self.class_name(), self.octave)
    }
}

impl fmt::Display for Pitch {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.name())
    }
}

impl FromStr for Pitch {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let t = s.trim();
        let (step, alter, rest) = parse_step(t).ok_or_else(|| format!("invalid pitch `{s}`"))?;
        let octave: i8 = rest
            .parse()
            .map_err(|_| format!("invalid pitch `{s}` (expected a name like Eb4)"))?;
        if !(-1..=9).contains(&octave) {
            return Err(format!("octave of `{s}` is out of range"));
        }
        Ok(Pitch::new(step, alter, octave))
    }
}

impl Serialize for Pitch {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&self.name())
    }
}

impl<'de> Deserialize<'de> for Pitch {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let s = String::deserialize(deserializer)?;
        s.parse().map_err(serde::de::Error::custom)
    }
}

impl JsonSchema for Pitch {
    fn schema_name() -> Cow<'static, str> {
        "Pitch".into()
    }

    fn json_schema(_: &mut SchemaGenerator) -> Schema {
        json_schema!({
            "type": "string",
            "description": "Spelled pitch with octave, e.g. Eb4, F#3, C5. Spelling is kept (Eb4 and D#4 are written differently).",
            "pattern": "^[A-Ga-g](#{1,2}|b{1,2}|x)?-?[0-9]$"
        })
    }
}

// ---------------------------------------------------------------------------------------------
// Key, time signature, clef
// ---------------------------------------------------------------------------------------------

/// A key: tonic spelling and mode, written `Eb major` or `F# minor`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, TS)]
pub struct KeySig {
    /// Tonic letter and accidentals, e.g. `Eb`.
    pub tonic: String,
    pub mode: KeyMode,
    /// Sharps (positive) or flats (negative) in the key signature.
    pub fifths: i32,
}

impl KeySig {
    pub fn new(tonic: &str, mode: KeyMode) -> Result<Self, String> {
        let (step, alter, rest) = parse_step(tonic.trim()).ok_or_else(|| format!("invalid key tonic `{tonic}`"))?;
        if !rest.trim().is_empty() {
            return Err(format!("invalid key tonic `{tonic}`"));
        }
        let name = format!("{}{}", STEP_LETTERS[step as usize], accidental_text(alter));
        let table = match mode {
            KeyMode::Major => &MAJOR_TONICS,
            KeyMode::Minor => &MINOR_TONICS,
        };
        let idx = table
            .iter()
            .position(|t| *t == name)
            .ok_or_else(|| format!("`{name} {}` has no standard key signature", mode_name(mode)))?;
        Ok(KeySig {
            tonic: name,
            mode,
            fifths: idx as i32 - 7,
        })
    }

    /// Parses `Eb major`, `F# minor`, `c minor`, `Bb` (major) or `F#m`.
    pub fn parse(s: &str) -> Result<Self, String> {
        let t = s.trim();
        let lower = t.to_ascii_lowercase();
        let (tonic, mode) = if let Some(x) = lower.strip_suffix(" minor") {
            (&t[..x.len()], KeyMode::Minor)
        } else if let Some(x) = lower.strip_suffix(" major") {
            (&t[..x.len()], KeyMode::Major)
        } else if t.len() > 1 && t.ends_with('m') {
            (&t[..t.len() - 1], KeyMode::Minor)
        } else {
            (t, KeyMode::Major)
        };
        KeySig::new(tonic, mode)
    }

    fn tonic_step(&self) -> (u8, i8) {
        let (step, alter, _) = parse_step(&self.tonic).expect("tonic validated on construction");
        (step, alter)
    }

    /// Alteration the key signature gives a step.
    pub fn signature_alter(&self, step: u8) -> i8 {
        let n = self.fifths.unsigned_abs().min(7) as usize;
        if self.fifths > 0 && SHARP_ORDER[..n].contains(&step) {
            1
        } else if self.fifths < 0 && FLAT_ORDER[..n].contains(&step) {
            -1
        } else {
            0
        }
    }

    /// The tonic as a pitch class (0–11).
    pub fn tonic_pc(&self) -> i32 {
        let (step, alter) = self.tonic_step();
        (STEP_SEMITONES[step as usize] + alter as i32).rem_euclid(12)
    }

    /// Scale degree `degree` (0 = tonic, may exceed 6 or be negative) relative to the tonic in
    /// `octave`, spelled from the key signature (natural minor in minor keys).
    pub fn degree(&self, degree: i32, tonic_octave: i32) -> Pitch {
        let (step, _) = self.tonic_step();
        let d = tonic_octave * 7 + step as i32 + degree;
        let p = Pitch::from_diatonic(d, 0);
        Pitch::new(p.step, self.signature_alter(p.step), p.octave)
    }

    /// The raised seventh degree (leading tone) of a minor key, spelled correctly.
    pub fn leading_tone(&self, tonic_octave: i32) -> Pitch {
        let p = self.degree(-1, tonic_octave);
        match self.mode {
            KeyMode::Minor => Pitch::new(p.step, p.alter + 1, p.octave),
            KeyMode::Major => p,
        }
    }

    /// Spells a MIDI note in this key: diatonic notes from the signature, the raised sixth and
    /// seventh of minor keys as such, other chromatic notes with the alteration nearest to natural
    /// (sharps in sharp keys, flats in flat keys on a tie).
    pub fn spell(&self, midi: i32) -> Pitch {
        let tonic = self.tonic_pc();
        let rel = (midi - tonic).rem_euclid(12);
        if self.mode == KeyMode::Minor && (rel == 11 || rel == 9) {
            // Raised seventh (leading tone) or raised sixth.
            let degree = if rel == 11 { 6 } else { 5 };
            return self.spell_degree_altered(midi, degree, 1);
        }
        let mut best: Option<(i32, Pitch)> = None;
        for step in 0..7u8 {
            let base = STEP_SEMITONES[step as usize];
            for alter in -2i8..=2 {
                let pc = (base + alter as i32).rem_euclid(12);
                if pc != midi.rem_euclid(12) {
                    continue;
                }
                let sig = self.signature_alter(step);
                // Cost: 0 for the key's own note, then prefer naturals, then the key's direction.
                let cost = if alter == sig {
                    0
                } else {
                    let direction_penalty = if (alter > 0) == (self.fifths >= 0) { 0 } else { 1 };
                    10 * (alter as i32).abs() + direction_penalty + 1
                };
                let octave = ((midi - base - alter as i32) / 12 - 1) as i8;
                let p = Pitch::new(step, alter, octave);
                if p.midi() != midi {
                    continue;
                }
                if best.as_ref().map(|(c, _)| cost < *c).unwrap_or(true) {
                    best = Some((cost, p));
                }
            }
        }
        best.map(|(_, p)| p).unwrap_or_else(|| Pitch::from_diatonic(0, 0))
    }

    fn spell_degree_altered(&self, midi: i32, degree: i32, raise: i8) -> Pitch {
        let (step, _) = self.tonic_step();
        let s = ((step as i32 + degree).rem_euclid(7)) as u8;
        let alter = self.signature_alter(s) + raise;
        let octave = ((midi - STEP_SEMITONES[s as usize] - alter as i32) / 12 - 1) as i8;
        Pitch::new(s, alter, octave)
    }

    /// The key name as VexFlow writes it, e.g. `Eb` or `Cm`.
    pub fn vexflow_spec(&self) -> String {
        match self.mode {
            KeyMode::Major => self.tonic.clone(),
            KeyMode::Minor => format!("{}m", self.tonic),
        }
    }
}

fn mode_name(m: KeyMode) -> &'static str {
    match m {
        KeyMode::Major => "major",
        KeyMode::Minor => "minor",
    }
}

impl fmt::Display for KeySig {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} {}", self.tonic, mode_name(self.mode))
    }
}

impl<'de> Deserialize<'de> for KeySig {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(untagged)]
        enum Raw {
            Text(String),
            Parts { tonic: String, mode: KeyMode },
        }
        match Raw::deserialize(deserializer)? {
            Raw::Text(s) => KeySig::parse(&s),
            Raw::Parts { tonic, mode } => KeySig::new(&tonic, mode),
        }
        .map_err(serde::de::Error::custom)
    }
}

impl JsonSchema for KeySig {
    fn schema_name() -> Cow<'static, str> {
        "KeySig".into()
    }

    fn json_schema(_: &mut SchemaGenerator) -> Schema {
        json_schema!({
            "type": "string",
            "description": "Key as tonic and mode, e.g. \"C major\", \"Eb major\", \"F# minor\"."
        })
    }
}

/// A simple meter: `4/4`, `3/4`, `2/4`, `6/8` …
#[derive(Clone, Copy, Debug, PartialEq, Eq, TS)]
#[ts(type = "string")]
pub struct TimeSig {
    pub beats: u32,
    pub unit: u32,
}

impl TimeSig {
    pub const COMMON: TimeSig = TimeSig { beats: 4, unit: 4 };

    /// Ticks per beat of the signature's unit.
    pub fn beat_ticks(self) -> u32 {
        TICKS_PER_QUARTER * 4 / self.unit
    }

    pub fn bar_ticks(self) -> u32 {
        self.beat_ticks() * self.beats
    }
}

impl fmt::Display for TimeSig {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}/{}", self.beats, self.unit)
    }
}

impl FromStr for TimeSig {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let (a, b) = s
            .trim()
            .split_once('/')
            .ok_or_else(|| format!("invalid time signature `{s}`"))?;
        let beats: u32 = a.trim().parse().map_err(|_| format!("invalid time signature `{s}`"))?;
        let unit: u32 = b.trim().parse().map_err(|_| format!("invalid time signature `{s}`"))?;
        if !(1..=12).contains(&beats) || ![2, 4, 8].contains(&unit) {
            return Err(format!("unsupported time signature `{s}` (use 1–12 beats of 2, 4 or 8)"));
        }
        Ok(TimeSig { beats, unit })
    }
}

impl Serialize for TimeSig {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&self.to_string())
    }
}

impl<'de> Deserialize<'de> for TimeSig {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let s = String::deserialize(deserializer)?;
        s.parse().map_err(serde::de::Error::custom)
    }
}

impl JsonSchema for TimeSig {
    fn schema_name() -> Cow<'static, str> {
        "TimeSig".into()
    }

    fn json_schema(_: &mut SchemaGenerator) -> Schema {
        json_schema!({
            "type": "string",
            "description": "Time signature, e.g. \"4/4\", \"3/4\", \"2/4\", \"6/8\".",
            "pattern": "^[0-9]{1,2}/(2|4|8)$"
        })
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "snake_case")]
pub enum Clef {
    Treble,
    /// Treble clef sounding an octave lower (tenor parts).
    Treble8vb,
    Bass,
}

/// A clef, or `auto` to choose one from the melody's range.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "snake_case")]
pub enum ClefChoice {
    #[default]
    Auto,
    Treble,
    Treble8vb,
    Bass,
}

impl ClefChoice {
    /// Bass clef for low melodies, octave-treble for the tenor range, treble above.
    pub fn resolve(self, low: i32, high: i32) -> Clef {
        match self {
            ClefChoice::Treble => Clef::Treble,
            ClefChoice::Treble8vb => Clef::Treble8vb,
            ClefChoice::Bass => Clef::Bass,
            ClefChoice::Auto => {
                let mid = (low + high) as f32 / 2.0;
                if mid < 55.5 {
                    Clef::Bass
                } else if mid < 62.0 {
                    Clef::Treble8vb
                } else {
                    Clef::Treble
                }
            }
        }
    }
}

// ---------------------------------------------------------------------------------------------
// Score
// ---------------------------------------------------------------------------------------------

/// A note or rest on the tick grid.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema, TS)]
pub struct ScoreNote {
    /// `null` for a rest.
    pub pitch: Option<Pitch>,
    /// Ticks from the start of bar 1 (a quarter note is four ticks).
    pub start: u32,
    pub dur: u32,
}

/// A single-line melody with its notation context.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema, TS)]
pub struct Score {
    pub key: KeySig,
    pub time: TimeSig,
    pub clef: Clef,
    pub tempo_bpm: f32,
    pub bars: u32,
    /// Notes and rests in order, without overlaps. Gaps are read as rests.
    pub notes: Vec<ScoreNote>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
}

impl Score {
    pub fn beat_ticks(&self) -> u32 {
        self.time.beat_ticks()
    }

    pub fn bar_ticks(&self) -> u32 {
        self.time.bar_ticks()
    }

    pub fn total_ticks(&self) -> u32 {
        self.bars * self.bar_ticks()
    }

    /// Length in beats of the time signature's unit.
    pub fn total_beats(&self) -> f32 {
        self.total_ticks() as f32 / self.beat_ticks() as f32
    }

    pub fn pitched(&self) -> impl Iterator<Item = (&ScoreNote, Pitch)> {
        self.notes.iter().filter_map(|n| n.pitch.map(|p| (n, p)))
    }

    /// Lowest and highest sounding MIDI notes.
    pub fn range(&self) -> Option<(i32, i32)> {
        let mut it = self.pitched().map(|(_, p)| p.midi());
        let first = it.next()?;
        Some(it.fold((first, first), |(lo, hi), m| (lo.min(m), hi.max(m))))
    }

    /// The sounding notes as grading targets, in beats from the start of bar 1.
    pub fn targets(&self) -> Vec<TargetNote> {
        let bt = self.beat_ticks() as f32;
        self.pitched()
            .map(|(n, p)| TargetNote {
                midi: p.midi(),
                start_beat: n.start as f32 / bt,
                beats: n.dur as f32 / bt,
            })
            .collect()
    }

    pub fn validate(&self) -> Result<(), String> {
        if self.bars == 0 || self.bars > 64 {
            return Err("a score needs 1–64 bars".into());
        }
        if !(20.0..=300.0).contains(&self.tempo_bpm) {
            return Err(format!("tempo {} is outside 20–300 bpm", self.tempo_bpm));
        }
        let mut end = 0;
        for (i, n) in self.notes.iter().enumerate() {
            if n.dur == 0 {
                return Err(format!("notes[{i}] has no duration"));
            }
            if n.start < end {
                return Err(format!("notes[{i}] overlaps the previous note"));
            }
            end = n.start + n.dur;
            if let Some(p) = n.pitch {
                if !(21..=108).contains(&p.midi()) {
                    return Err(format!("notes[{i}] ({p}) is outside the piano range"));
                }
            }
        }
        if end > self.total_ticks() {
            return Err("the notes run past the last bar".into());
        }
        if self.pitched().next().is_none() {
            return Err("the score has no notes".into());
        }
        Ok(())
    }

    /// Builds a score from coach-written notes.
    pub fn from_compact(m: &CompactMelody, default_tempo: f32) -> Result<Score, String> {
        let key = KeySig::parse(&m.key)?;
        let time = m.time;
        let bt = time.beat_ticks() as f32;
        let mut t: u32 = 0;
        let mut notes = Vec::with_capacity(m.notes.len());
        for (i, n) in m.notes.iter().enumerate() {
            let (pitch, beats) = match n {
                CompactNote::Note { note, beats } => (Some(*note), *beats),
                CompactNote::Rest { rest } => (None, *rest),
            };
            let ticks = beats * bt;
            if beats <= 0.0 || (ticks - ticks.round()).abs() > 1e-3 {
                return Err(format!(
                    "notes[{i}]: {beats} beats is not a whole number of sixteenth notes in {time}"
                ));
            }
            let ticks = ticks.round() as u32;
            notes.push(ScoreNote {
                pitch,
                start: t,
                dur: ticks,
            });
            t += ticks;
        }
        let bars = t.div_ceil(time.bar_ticks()).max(1);
        let mut score = Score {
            key,
            time,
            clef: Clef::Treble,
            tempo_bpm: m.tempo_bpm.unwrap_or(default_tempo),
            bars,
            notes,
            title: m.title.clone(),
        };
        let (lo, hi) = score.range().ok_or("the melody has no notes")?;
        score.clef = m.clef.resolve(lo, hi);
        score.validate()?;
        Ok(score)
    }

    /// Note names in order, for summaries (`D4 E4 F#4 …`).
    pub fn pitch_names(&self) -> String {
        self.pitched()
            .map(|(_, p)| p.name())
            .collect::<Vec<_>>()
            .join(" ")
    }
}

/// Sound events for notes on the tick grid (to play back a written melody).
pub fn note_events(notes: &[ScoreNote], time: TimeSig, tempo_bpm: f32, timbre: &str) -> Vec<SoundEvent> {
    let beat_ms = 60_000.0 / tempo_bpm.max(20.0) as f64;
    let tick_ms = beat_ms / time.beat_ticks() as f64;
    notes
        .iter()
        .filter_map(|n| {
            n.pitch.map(|p| SoundEvent {
                midi: p.midi(),
                start_ms: n.start as f64 * tick_ms,
                dur_ms: n.dur as f64 * tick_ms * 0.95,
                timbre: timbre.to_string(),
                gain_db: -6.0,
                detune_cents: 0.0,
            })
        })
        .collect()
}

/// A melody written by the coach.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct CompactMelody {
    /// Key, e.g. "C major", "Eb major", "F# minor".
    pub key: String,
    #[serde(default = "d_common_time")]
    pub time: TimeSig,
    #[serde(default)]
    pub clef: ClefChoice,
    #[serde(default)]
    pub tempo_bpm: Option<f32>,
    /// Notes and rests in order, e.g. `{"note": "Eb4", "beats": 1}` or `{"rest": 0.5}`. Beats are
    /// counted in the time signature's unit (a quarter note in 4/4). Notes may cross barlines;
    /// they are tied in the notation.
    pub notes: Vec<CompactNote>,
    #[serde(default)]
    pub title: Option<String>,
}

fn d_common_time() -> TimeSig {
    TimeSig::COMMON
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(untagged)]
pub enum CompactNote {
    Note { note: Pitch, beats: f32 },
    Rest { rest: f32 },
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pitches_parse_and_print() {
        for (s, midi) in [("Eb4", 63), ("F#3", 54), ("Cb4", 59), ("B#3", 60), ("C-1", 0), ("Bbb2", 45)] {
            let p: Pitch = s.parse().unwrap();
            assert_eq!(p.midi(), midi, "{s}");
            assert_eq!(p.name(), s);
        }
        assert!("H4".parse::<Pitch>().is_err());
        assert!("E".parse::<Pitch>().is_err());
        assert_eq!("C4".parse::<Pitch>().unwrap().diatonic(), 28);
    }

    #[test]
    fn key_signatures() {
        assert_eq!(KeySig::parse("Eb major").unwrap().fifths, -3);
        assert_eq!(KeySig::parse("F# minor").unwrap().fifths, 3);
        assert_eq!(KeySig::parse("c minor").unwrap().fifths, -3);
        assert_eq!(KeySig::parse("Bb").unwrap().fifths, -2);
        assert_eq!(KeySig::parse("C#m").unwrap().fifths, 4);
        assert!(KeySig::parse("D# major").is_err());
        let g = KeySig::parse("G major").unwrap();
        assert_eq!(g.signature_alter(3), 1);
        assert_eq!(g.signature_alter(0), 0);
        assert_eq!(KeySig::parse("Eb major").unwrap().vexflow_spec(), "Eb");
        assert_eq!(KeySig::parse("C minor").unwrap().vexflow_spec(), "Cm");
    }

    #[test]
    fn degrees_and_spelling_follow_the_key() {
        let eb = KeySig::parse("Eb major").unwrap();
        assert_eq!(eb.degree(0, 4).name(), "Eb4");
        assert_eq!(eb.degree(3, 4).name(), "Ab4");
        assert_eq!(eb.degree(-1, 4).name(), "D4");
        assert_eq!(eb.spell(63).name(), "Eb4");
        assert_eq!(eb.spell(66).name(), "Gb4");
        let cm = KeySig::parse("C minor").unwrap();
        assert_eq!(cm.spell(71).name(), "B4");
        assert_eq!(cm.leading_tone(5).name(), "B4");
        let dm = KeySig::parse("D minor").unwrap();
        assert_eq!(dm.spell(61).name(), "C#4");
        assert_eq!(KeySig::parse("C major").unwrap().spell(61).name(), "C#4");
        assert_eq!(KeySig::parse("F major").unwrap().spell(61).name(), "Db4");
        assert_eq!(KeySig::parse("A minor").unwrap().spell(68).name(), "G#4");
        assert_eq!(KeySig::parse("F# major").unwrap().spell(65).name(), "E#4");
    }

    #[test]
    fn compact_melodies_become_scores() {
        let m: CompactMelody = serde_json::from_value(serde_json::json!({
            "key": "D major",
            "time": "3/4",
            "notes": [
                {"note": "D4", "beats": 1}, {"note": "F#4", "beats": 0.5}, {"note": "E4", "beats": 0.5},
                {"note": "A4", "beats": 1}, {"rest": 1}, {"note": "D5", "beats": 2}
            ]
        }))
        .unwrap();
        let s = Score::from_compact(&m, 80.0).unwrap();
        assert_eq!(s.bars, 2);
        assert_eq!(s.key.fifths, 2);
        assert_eq!(s.notes[2].start, 6);
        assert_eq!(s.notes[5].start, 16);
        assert_eq!(s.notes[5].dur, 8);
        let t = s.targets();
        assert_eq!(t.len(), 5);
        assert_eq!(t[4].midi, 74);
        assert_eq!(t[4].start_beat, 4.0);
        assert_eq!(s.clef, Clef::Treble);
        let bad: CompactMelody = serde_json::from_value(serde_json::json!({
            "key": "C major", "notes": [{"note": "C4", "beats": 0.3}]
        }))
        .unwrap();
        assert!(Score::from_compact(&bad, 80.0).is_err());
    }

    #[test]
    fn clef_choice_follows_range() {
        assert_eq!(ClefChoice::Auto.resolve(43, 60), Clef::Bass);
        assert_eq!(ClefChoice::Auto.resolve(50, 67), Clef::Treble8vb);
        assert_eq!(ClefChoice::Auto.resolve(60, 76), Clef::Treble);
    }
}
