//! Pitch, note-name, interval, chord and scale theory.

use std::borrow::Cow;
use std::fmt;
use std::str::FromStr;

use schemars::{JsonSchema, Schema, SchemaGenerator, json_schema};
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use ts_rs::TS;

pub const DEFAULT_A4_HZ: f64 = 440.0;

pub fn midi_to_hz(midi: f64, a4_hz: f64) -> f64 {
    a4_hz * 2f64.powf((midi - 69.0) / 12.0)
}

pub fn hz_to_midi(hz: f64, a4_hz: f64) -> f64 {
    69.0 + 12.0 * (hz / a4_hz).log2()
}

/// Wraps a semitone difference into [-6, 6).
pub fn wrap_semitones(d: f64) -> f64 {
    (d + 6.0).rem_euclid(12.0) - 6.0
}

const SHARP_NAMES: [&str; 12] = [
    "C", "C#", "D", "D#", "E", "F", "F#", "G", "G#", "A", "A#", "B",
];
const FLAT_NAMES: [&str; 12] = [
    "C", "Db", "D", "Eb", "E", "F", "Gb", "G", "Ab", "A", "Bb", "B",
];
/// Names used when speaking or displaying a pitch class without a key context.
const DISPLAY_NAMES: [&str; 12] = [
    "C", "C♯", "D", "E♭", "E", "F", "F♯", "G", "A♭", "A", "B♭", "B",
];

/// One of the 12 pitch classes, C = 0.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, TS)]
#[ts(type = "string")]
pub struct PitchClass(u8);

impl PitchClass {
    pub const ALL: [PitchClass; 12] = [
        PitchClass(0),
        PitchClass(1),
        PitchClass(2),
        PitchClass(3),
        PitchClass(4),
        PitchClass(5),
        PitchClass(6),
        PitchClass(7),
        PitchClass(8),
        PitchClass(9),
        PitchClass(10),
        PitchClass(11),
    ];

    pub fn new(semitones: i32) -> Self {
        PitchClass(semitones.rem_euclid(12) as u8)
    }

    pub fn of_midi(midi: i32) -> Self {
        Self::new(midi)
    }

    pub fn index(self) -> u8 {
        self.0
    }

    /// Canonical name with sharps, e.g. `C#`. Used in data and APIs.
    pub fn name(self) -> &'static str {
        SHARP_NAMES[self.0 as usize]
    }

    pub fn flat_name(self) -> &'static str {
        FLAT_NAMES[self.0 as usize]
    }

    /// Name for display and speech without key context, e.g. `E♭`, `F♯`.
    pub fn display_name(self) -> &'static str {
        DISPLAY_NAMES[self.0 as usize]
    }

    /// The MIDI note of this pitch class in the given octave (scientific numbering, C4 = 60).
    pub fn in_octave(self, octave: i32) -> i32 {
        (octave + 1) * 12 + self.0 as i32
    }

    /// Signed distance `self - from` in semitones, wrapped to -6..=5.
    pub fn signed_distance(self, from: PitchClass) -> i32 {
        (self.0 as i32 - from.0 as i32 + 6).rem_euclid(12) - 6
    }

    pub fn transpose(self, semitones: i32) -> Self {
        Self::new(self.0 as i32 + semitones)
    }
}

impl fmt::Display for PitchClass {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.name())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("invalid note name `{0}`")]
pub struct NoteParseError(pub String);

/// Parses a letter plus accidentals, returning the semitone offset from C (unwrapped, so `Cb` is -1)
/// and the unparsed remainder.
fn parse_letter_and_accidentals(s: &str) -> Option<(i32, &str)> {
    let mut chars = s.char_indices();
    let (_, letter) = chars.next()?;
    let base = match letter.to_ascii_uppercase() {
        'C' => 0,
        'D' => 2,
        'E' => 4,
        'F' => 5,
        'G' => 7,
        'A' => 9,
        'B' => 11,
        _ => return None,
    };
    let mut offset = 0;
    let mut rest_start = letter.len_utf8();
    for (i, c) in chars {
        match c {
            '#' | '♯' => offset += 1,
            'b' | '♭' => offset -= 1,
            'x' | '𝄪' => offset += 2,
            _ => {
                rest_start = i;
                break;
            }
        }
        rest_start = i + c.len_utf8();
    }
    Some((base + offset, &s[rest_start..]))
}

impl FromStr for PitchClass {
    type Err = NoteParseError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let t = s.trim();
        if let Ok(n) = t.parse::<i32>() {
            if (0..12).contains(&n) {
                return Ok(PitchClass::new(n));
            }
        }
        match parse_letter_and_accidentals(t) {
            Some((semis, rest)) if rest.is_empty() => Ok(PitchClass::new(semis)),
            _ => Err(NoteParseError(s.to_string())),
        }
    }
}

impl Serialize for PitchClass {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(self.name())
    }
}

impl<'de> Deserialize<'de> for PitchClass {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(untagged)]
        enum Raw {
            Name(String),
            Index(i64),
        }
        match Raw::deserialize(deserializer)? {
            Raw::Name(s) => s.parse().map_err(serde::de::Error::custom),
            Raw::Index(i) if (0..12).contains(&i) => Ok(PitchClass::new(i as i32)),
            Raw::Index(i) => Err(serde::de::Error::custom(format!(
                "pitch class index {i} out of range"
            ))),
        }
    }
}

impl JsonSchema for PitchClass {
    fn schema_name() -> Cow<'static, str> {
        "PitchClass".into()
    }

    fn json_schema(_: &mut SchemaGenerator) -> Schema {
        json_schema!({
            "type": "string",
            "description": "Pitch class name: C, C#, D, D#, E, F, F#, G, G#, A, A#, B (flats such as Eb are also accepted).",
            "pattern": "^[A-Ga-g](#|b|♯|♭|x)*$"
        })
    }
}

/// A concrete note as a MIDI number (C4 = 60), serialized as a name like `F#4`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, TS)]
#[ts(type = "string")]
pub struct Note(pub i32);

impl Note {
    pub fn midi(self) -> i32 {
        self.0
    }

    pub fn pitch_class(self) -> PitchClass {
        PitchClass::of_midi(self.0)
    }

    pub fn octave(self) -> i32 {
        self.0.div_euclid(12) - 1
    }

    pub fn name(self) -> String {
        format!("{}{}", self.pitch_class().name(), self.octave())
    }

    /// Display name with ♯/♭ glyphs and octave, e.g. `E♭4`.
    pub fn display_name(self) -> String {
        format!("{}{}", self.pitch_class().display_name(), self.octave())
    }

    pub fn hz(self, a4_hz: f64) -> f64 {
        midi_to_hz(self.0 as f64, a4_hz)
    }
}

impl fmt::Display for Note {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.name())
    }
}

impl FromStr for Note {
    type Err = NoteParseError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let t = s.trim();
        if let Ok(n) = t.parse::<i32>() {
            return Ok(Note(n));
        }
        let (semis, rest) =
            parse_letter_and_accidentals(t).ok_or_else(|| NoteParseError(s.to_string()))?;
        let octave: i32 = rest.parse().map_err(|_| NoteParseError(s.to_string()))?;
        Ok(Note((octave + 1) * 12 + semis))
    }
}

impl Serialize for Note {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&self.name())
    }
}

impl<'de> Deserialize<'de> for Note {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(untagged)]
        enum Raw {
            Name(String),
            Midi(i64),
        }
        match Raw::deserialize(deserializer)? {
            Raw::Name(s) => s.parse().map_err(serde::de::Error::custom),
            Raw::Midi(m) => Ok(Note(m as i32)),
        }
    }
}

impl JsonSchema for Note {
    fn schema_name() -> Cow<'static, str> {
        "Note".into()
    }

    fn json_schema(_: &mut SchemaGenerator) -> Schema {
        json_schema!({
            "type": "string",
            "description": "Note in scientific pitch notation, e.g. C4 (middle C), F#3, Eb5.",
            "pattern": "^[A-Ga-g](#|b|♯|♭|x)*-?[0-9]$"
        })
    }
}

/// Intervals up to two octaves, named the way exercises and answers use them.
#[derive(
    Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize, JsonSchema, TS,
)]
pub enum Interval {
    P1,
    #[serde(rename = "m2")]
    Min2,
    #[serde(rename = "M2")]
    Maj2,
    #[serde(rename = "m3")]
    Min3,
    #[serde(rename = "M3")]
    Maj3,
    P4,
    TT,
    P5,
    #[serde(rename = "m6")]
    Min6,
    #[serde(rename = "M6")]
    Maj6,
    #[serde(rename = "m7")]
    Min7,
    #[serde(rename = "M7")]
    Maj7,
    P8,
    #[serde(rename = "m9")]
    Min9,
    #[serde(rename = "M9")]
    Maj9,
    #[serde(rename = "m10")]
    Min10,
    #[serde(rename = "M10")]
    Maj10,
    P11,
    #[serde(rename = "A11")]
    Aug11,
    P12,
    #[serde(rename = "m13")]
    Min13,
    #[serde(rename = "M13")]
    Maj13,
    #[serde(rename = "m14")]
    Min14,
    #[serde(rename = "M14")]
    Maj14,
    P15,
}

impl Interval {
    pub const ALL: [Interval; 25] = [
        Interval::P1,
        Interval::Min2,
        Interval::Maj2,
        Interval::Min3,
        Interval::Maj3,
        Interval::P4,
        Interval::TT,
        Interval::P5,
        Interval::Min6,
        Interval::Maj6,
        Interval::Min7,
        Interval::Maj7,
        Interval::P8,
        Interval::Min9,
        Interval::Maj9,
        Interval::Min10,
        Interval::Maj10,
        Interval::P11,
        Interval::Aug11,
        Interval::P12,
        Interval::Min13,
        Interval::Maj13,
        Interval::Min14,
        Interval::Maj14,
        Interval::P15,
    ];

    /// The simple intervals m2 through P8.
    pub const SIMPLE: [Interval; 12] = [
        Interval::Min2,
        Interval::Maj2,
        Interval::Min3,
        Interval::Maj3,
        Interval::P4,
        Interval::TT,
        Interval::P5,
        Interval::Min6,
        Interval::Maj6,
        Interval::Min7,
        Interval::Maj7,
        Interval::P8,
    ];

    pub fn semitones(self) -> i32 {
        Self::ALL.iter().position(|&i| i == self).unwrap() as i32
    }

    pub fn from_semitones(s: i32) -> Option<Self> {
        Self::ALL.get(usize::try_from(s).ok()?).copied()
    }

    /// Short id used in data, e.g. `m3`.
    pub fn id(self) -> &'static str {
        const IDS: [&str; 25] = [
            "P1", "m2", "M2", "m3", "M3", "P4", "TT", "P5", "m6", "M6", "m7", "M7", "P8", "m9",
            "M9", "m10", "M10", "P11", "A11", "P12", "m13", "M13", "m14", "M14", "P15",
        ];
        IDS[self.semitones() as usize]
    }

    pub fn long_name(self) -> &'static str {
        const NAMES: [&str; 25] = [
            "Unison",
            "Minor 2nd",
            "Major 2nd",
            "Minor 3rd",
            "Major 3rd",
            "Perfect 4th",
            "Tritone",
            "Perfect 5th",
            "Minor 6th",
            "Major 6th",
            "Minor 7th",
            "Major 7th",
            "Octave",
            "Minor 9th",
            "Major 9th",
            "Minor 10th",
            "Major 10th",
            "Perfect 11th",
            "Augmented 11th",
            "Perfect 12th",
            "Minor 13th",
            "Major 13th",
            "Minor 14th",
            "Major 14th",
            "Two octaves",
        ];
        NAMES[self.semitones() as usize]
    }
}

impl FromStr for Interval {
    type Err = NoteParseError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Interval::ALL
            .iter()
            .copied()
            .find(|i| i.id() == s.trim())
            .ok_or_else(|| NoteParseError(s.to_string()))
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "snake_case")]
pub enum ChordQuality {
    Major,
    Minor,
    Diminished,
    Augmented,
    Sus2,
    Sus4,
    Dominant7,
    Major7,
    Minor7,
    HalfDiminished7,
    Diminished7,
    MinorMajor7,
    Major6,
    Minor6,
}

impl ChordQuality {
    pub const TRIADS: [ChordQuality; 4] = [
        ChordQuality::Major,
        ChordQuality::Minor,
        ChordQuality::Diminished,
        ChordQuality::Augmented,
    ];

    pub const ALL: [ChordQuality; 14] = [
        ChordQuality::Major,
        ChordQuality::Minor,
        ChordQuality::Diminished,
        ChordQuality::Augmented,
        ChordQuality::Sus2,
        ChordQuality::Sus4,
        ChordQuality::Dominant7,
        ChordQuality::Major7,
        ChordQuality::Minor7,
        ChordQuality::HalfDiminished7,
        ChordQuality::Diminished7,
        ChordQuality::MinorMajor7,
        ChordQuality::Major6,
        ChordQuality::Minor6,
    ];

    /// Semitones above the root, in root position.
    pub fn intervals(self) -> &'static [i32] {
        match self {
            ChordQuality::Major => &[0, 4, 7],
            ChordQuality::Minor => &[0, 3, 7],
            ChordQuality::Diminished => &[0, 3, 6],
            ChordQuality::Augmented => &[0, 4, 8],
            ChordQuality::Sus2 => &[0, 2, 7],
            ChordQuality::Sus4 => &[0, 5, 7],
            ChordQuality::Dominant7 => &[0, 4, 7, 10],
            ChordQuality::Major7 => &[0, 4, 7, 11],
            ChordQuality::Minor7 => &[0, 3, 7, 10],
            ChordQuality::HalfDiminished7 => &[0, 3, 6, 10],
            ChordQuality::Diminished7 => &[0, 3, 6, 9],
            ChordQuality::MinorMajor7 => &[0, 3, 7, 11],
            ChordQuality::Major6 => &[0, 4, 7, 9],
            ChordQuality::Minor6 => &[0, 3, 7, 9],
        }
    }

    pub fn id(self) -> &'static str {
        match self {
            ChordQuality::Major => "major",
            ChordQuality::Minor => "minor",
            ChordQuality::Diminished => "diminished",
            ChordQuality::Augmented => "augmented",
            ChordQuality::Sus2 => "sus2",
            ChordQuality::Sus4 => "sus4",
            ChordQuality::Dominant7 => "dominant7",
            ChordQuality::Major7 => "major7",
            ChordQuality::Minor7 => "minor7",
            ChordQuality::HalfDiminished7 => "half_diminished7",
            ChordQuality::Diminished7 => "diminished7",
            ChordQuality::MinorMajor7 => "minor_major7",
            ChordQuality::Major6 => "major6",
            ChordQuality::Minor6 => "minor6",
        }
    }

    pub fn long_name(self) -> &'static str {
        match self {
            ChordQuality::Major => "Major",
            ChordQuality::Minor => "Minor",
            ChordQuality::Diminished => "Diminished",
            ChordQuality::Augmented => "Augmented",
            ChordQuality::Sus2 => "Sus 2",
            ChordQuality::Sus4 => "Sus 4",
            ChordQuality::Dominant7 => "Dominant 7th",
            ChordQuality::Major7 => "Major 7th",
            ChordQuality::Minor7 => "Minor 7th",
            ChordQuality::HalfDiminished7 => "Half-diminished 7th",
            ChordQuality::Diminished7 => "Diminished 7th",
            ChordQuality::MinorMajor7 => "Minor-major 7th",
            ChordQuality::Major6 => "Major 6th",
            ChordQuality::Minor6 => "Minor 6th",
        }
    }

    pub fn from_id(id: &str) -> Option<Self> {
        Self::ALL.iter().copied().find(|q| q.id() == id)
    }

    /// MIDI notes of the chord in close position with the given inversion (0 = root position).
    pub fn voicing(self, root_midi: i32, inversion: usize) -> Vec<i32> {
        let mut notes: Vec<i32> = self.intervals().iter().map(|i| root_midi + i).collect();
        for _ in 0..inversion.min(notes.len() - 1) {
            let lowest = notes.remove(0);
            notes.push(lowest + 12);
        }
        notes
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "snake_case")]
pub enum ScaleKind {
    Major,
    NaturalMinor,
    HarmonicMinor,
    MelodicMinor,
    Dorian,
    Phrygian,
    Lydian,
    Mixolydian,
    Locrian,
    MajorPentatonic,
    MinorPentatonic,
    Blues,
    WholeTone,
    Chromatic,
}

impl ScaleKind {
    pub const ALL: [ScaleKind; 14] = [
        ScaleKind::Major,
        ScaleKind::NaturalMinor,
        ScaleKind::HarmonicMinor,
        ScaleKind::MelodicMinor,
        ScaleKind::Dorian,
        ScaleKind::Phrygian,
        ScaleKind::Lydian,
        ScaleKind::Mixolydian,
        ScaleKind::Locrian,
        ScaleKind::MajorPentatonic,
        ScaleKind::MinorPentatonic,
        ScaleKind::Blues,
        ScaleKind::WholeTone,
        ScaleKind::Chromatic,
    ];

    /// Ascending semitone offsets within one octave, starting at 0.
    pub fn steps(self) -> &'static [i32] {
        match self {
            ScaleKind::Major => &[0, 2, 4, 5, 7, 9, 11],
            ScaleKind::NaturalMinor => &[0, 2, 3, 5, 7, 8, 10],
            ScaleKind::HarmonicMinor => &[0, 2, 3, 5, 7, 8, 11],
            ScaleKind::MelodicMinor => &[0, 2, 3, 5, 7, 9, 11],
            ScaleKind::Dorian => &[0, 2, 3, 5, 7, 9, 10],
            ScaleKind::Phrygian => &[0, 1, 3, 5, 7, 8, 10],
            ScaleKind::Lydian => &[0, 2, 4, 6, 7, 9, 11],
            ScaleKind::Mixolydian => &[0, 2, 4, 5, 7, 9, 10],
            ScaleKind::Locrian => &[0, 1, 3, 5, 6, 8, 10],
            ScaleKind::MajorPentatonic => &[0, 2, 4, 7, 9],
            ScaleKind::MinorPentatonic => &[0, 3, 5, 7, 10],
            ScaleKind::Blues => &[0, 3, 5, 6, 7, 10],
            ScaleKind::WholeTone => &[0, 2, 4, 6, 8, 10],
            ScaleKind::Chromatic => &[0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11],
        }
    }

    pub fn id(self) -> &'static str {
        match self {
            ScaleKind::Major => "major",
            ScaleKind::NaturalMinor => "natural_minor",
            ScaleKind::HarmonicMinor => "harmonic_minor",
            ScaleKind::MelodicMinor => "melodic_minor",
            ScaleKind::Dorian => "dorian",
            ScaleKind::Phrygian => "phrygian",
            ScaleKind::Lydian => "lydian",
            ScaleKind::Mixolydian => "mixolydian",
            ScaleKind::Locrian => "locrian",
            ScaleKind::MajorPentatonic => "major_pentatonic",
            ScaleKind::MinorPentatonic => "minor_pentatonic",
            ScaleKind::Blues => "blues",
            ScaleKind::WholeTone => "whole_tone",
            ScaleKind::Chromatic => "chromatic",
        }
    }

    pub fn long_name(self) -> &'static str {
        match self {
            ScaleKind::Major => "Major",
            ScaleKind::NaturalMinor => "Natural minor",
            ScaleKind::HarmonicMinor => "Harmonic minor",
            ScaleKind::MelodicMinor => "Melodic minor",
            ScaleKind::Dorian => "Dorian",
            ScaleKind::Phrygian => "Phrygian",
            ScaleKind::Lydian => "Lydian",
            ScaleKind::Mixolydian => "Mixolydian",
            ScaleKind::Locrian => "Locrian",
            ScaleKind::MajorPentatonic => "Major pentatonic",
            ScaleKind::MinorPentatonic => "Minor pentatonic",
            ScaleKind::Blues => "Blues",
            ScaleKind::WholeTone => "Whole tone",
            ScaleKind::Chromatic => "Chromatic",
        }
    }

    pub fn from_id(id: &str) -> Option<Self> {
        Self::ALL.iter().copied().find(|k| k.id() == id)
    }

    /// One octave of the scale from `root_midi`, including the octave on top.
    pub fn octave_notes(self, root_midi: i32) -> Vec<i32> {
        let mut v: Vec<i32> = self.steps().iter().map(|s| root_midi + s).collect();
        v.push(root_midi + 12);
        v
    }

    /// MIDI note of a (1-based, possibly >7 or <1) scale degree.
    pub fn degree_midi(self, tonic_midi: i32, degree: i32) -> i32 {
        let steps = self.steps();
        let n = steps.len() as i32;
        let idx = degree - 1;
        let octave = idx.div_euclid(n);
        let step = steps[idx.rem_euclid(n) as usize];
        tonic_midi + octave * 12 + step
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hz_midi_round_trip() {
        assert!((midi_to_hz(69.0, 440.0) - 440.0).abs() < 1e-9);
        assert!((hz_to_midi(261.625_565, 440.0) - 60.0).abs() < 1e-5);
        let m = hz_to_midi(444.01, 440.0);
        assert!(((m - 69.0) * 100.0 - 15.70).abs() < 0.05, "{m}");
    }

    #[test]
    fn parses_note_names() {
        assert_eq!("C4".parse::<Note>().unwrap(), Note(60));
        assert_eq!("F#3".parse::<Note>().unwrap(), Note(54));
        assert_eq!("Eb5".parse::<Note>().unwrap(), Note(75));
        assert_eq!("B#3".parse::<Note>().unwrap(), Note(60));
        assert_eq!("Cb4".parse::<Note>().unwrap(), Note(59));
        assert_eq!("A-1".parse::<Note>().unwrap(), Note(9));
        assert_eq!("E♭4".parse::<Note>().unwrap(), Note(63));
        assert!("H4".parse::<Note>().is_err());
        assert!("C".parse::<Note>().is_err());
        assert_eq!(Note(61).name(), "C#4");
        assert_eq!(Note(63).display_name(), "E♭4");
    }

    #[test]
    fn parses_pitch_classes() {
        assert_eq!("Eb".parse::<PitchClass>().unwrap(), PitchClass::new(3));
        assert_eq!("D#".parse::<PitchClass>().unwrap(), PitchClass::new(3));
        assert_eq!("Cb".parse::<PitchClass>().unwrap(), PitchClass::new(11));
        assert_eq!("7".parse::<PitchClass>().unwrap(), PitchClass::new(7));
        assert!("C4".parse::<PitchClass>().is_err());
        let json = serde_json::to_string(&PitchClass::new(6)).unwrap();
        assert_eq!(json, "\"F#\"");
        let back: PitchClass = serde_json::from_str("\"Gb\"").unwrap();
        assert_eq!(back, PitchClass::new(6));
    }

    #[test]
    fn signed_distance_wraps() {
        let c = PitchClass::new(0);
        assert_eq!(PitchClass::new(1).signed_distance(c), 1);
        assert_eq!(PitchClass::new(11).signed_distance(c), -1);
        assert_eq!(PitchClass::new(6).signed_distance(c), -6);
        assert_eq!(PitchClass::new(5).signed_distance(c), 5);
        assert!((wrap_semitones(11.6) - -0.4).abs() < 1e-9);
    }

    #[test]
    fn intervals_and_chords() {
        assert_eq!(Interval::Min6.semitones(), 8);
        assert_eq!("m6".parse::<Interval>().unwrap(), Interval::Min6);
        assert_eq!(serde_json::to_string(&Interval::Maj3).unwrap(), "\"M3\"");
        assert_eq!(ChordQuality::Major.voicing(60, 1), vec![64, 67, 72]);
        assert_eq!(ChordQuality::Dominant7.voicing(55, 3), vec![65, 67, 71, 74]);
        assert_eq!(ScaleKind::Major.degree_midi(60, 8), 72);
        assert_eq!(ScaleKind::Major.degree_midi(60, 0), 59);
        assert_eq!(ScaleKind::NaturalMinor.degree_midi(57, 3), 60);
    }
}
