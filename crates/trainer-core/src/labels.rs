//! Label and colour sets for absolute-pitch answers.
//!
//! A set maps each pitch class (or Eguchi chord) to a name, a colour, a word and an optional
//! speech file. Trials still store the pitch class, so every set feeds the same mastery numbers.

use std::collections::BTreeMap;

use anyhow::{Result, anyhow};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::storage::Db;
use crate::theory::PitchClass;

/// One label in a set.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema, TS)]
pub struct LabelEntry {
    /// Shown instead of the letter name when set.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    /// CSS colour, usually `#rrggbb`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub color: Option<String>,
    /// A word on the button: "Red", "Sarah".
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub word: Option<String>,
    /// File under the speech folder, e.g. `notes/C.wav` or `labels/gervain_names/Sarah.wav`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub speech: Option<String>,
}

/// A named mapping used by identification, pairing, detection and chord colours.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema, TS)]
pub struct LabelSet {
    pub id: String,
    pub title: String,
    pub description: String,
    /// `pitch_class` or `chord`.
    pub kind: String,
    pub entries: BTreeMap<String, LabelEntry>,
}

pub const NOTE_NAMES: &str = "note_names";
pub const TANEDA_COLORS: &str = "taneda_colors";
pub const GERVAIN_NAMES: &str = "gervain_names";
pub const MY_COLORS: &str = "my_colors";
pub const EGUCHI_CHORDS: &str = "eguchi_chords";

fn entry(color: &str, word: &str, speech: &str) -> LabelEntry {
    LabelEntry {
        name: None,
        color: Some(color.into()),
        word: Some(word.into()),
        speech: Some(speech.into()),
    }
}

fn note_speech(pc: PitchClass) -> String {
    format!("notes/{}.wav", pc.name())
}

/// Letter names, with spoken note names when those files exist.
fn note_names() -> LabelSet {
    let mut entries = BTreeMap::new();
    for pc in PitchClass::ALL {
        entries.insert(
            pc.name().to_string(),
            LabelEntry {
                name: Some(pc.display_name().to_string()),
                color: None,
                word: None,
                speech: Some(note_speech(pc)),
            },
        );
    }
    LabelSet {
        id: NOTE_NAMES.into(),
        title: "Note names".into(),
        description: "The letter name of each note. This is the default.".into(),
        kind: "pitch_class".into(),
        entries,
    }
}

/// Taneda's colours: seven white keys, then black keys as the course adds them.
fn taneda_colors() -> LabelSet {
    // C–B white keys, then the five black keys.
    let rows: [(&str, &str, &str); 12] = [
        ("C", "#d64545", "Red"),
        ("C#", "#c43d8a", "Magenta"),
        ("D", "#3a6fd8", "Blue"),
        ("D#", "#2a9d8f", "Teal"),
        ("E", "#e6c229", "Yellow"),
        ("F", "#e07a2f", "Orange"),
        ("F#", "#c9a227", "Gold"),
        ("G", "#8a5a32", "Brown"),
        ("G#", "#3d3d8f", "Indigo"),
        ("A", "#8d8d8d", "Grey"),
        ("A#", "#e06aa0", "Pink"),
        ("B", "#3d9a4a", "Green"),
    ];
    let mut entries = BTreeMap::new();
    for (pc, color, word) in rows {
        entries.insert(
            pc.into(),
            entry(color, word, &format!("labels/taneda_colors/{word}.wav")),
        );
    }
    LabelSet {
        id: TANEDA_COLORS.into(),
        title: "Taneda colours".into(),
        description: "White keys are red, blue, yellow, orange, brown, grey and green from C to B. Black keys have their own colours.".into(),
        kind: "pitch_class".into(),
        entries,
    }
}

/// Gervain's names: one whole-tone set, then the other.
fn gervain_names() -> LabelSet {
    // Even whole-tone set, then the odd one. D# is E♭, A# is B♭.
    let rows: [(&str, &str, &str, &str); 12] = [
        ("C", "#d64545", "Sarah", "Sarah"),
        ("D", "#3a6fd8", "David", "David"),
        ("E", "#e6c229", "Francine", "Francine"),
        ("F#", "#c9a227", "Jimmy", "Jimmy"),
        ("G#", "#3d3d8f", "Karen", "Karen"),
        ("A#", "#e06aa0", "Leo", "Leo"),
        ("C#", "#c43d8a", "Nina", "Nina"),
        ("D#", "#2a9d8f", "Oscar", "Oscar"),
        ("F", "#e07a2f", "Paula", "Paula"),
        ("G", "#8a5a32", "Robert", "Robert"),
        ("A", "#8d8d8d", "Rita", "Rita"),
        ("B", "#3d9a4a", "Simon", "Simon"),
    ];
    let mut entries = BTreeMap::new();
    for (pc, color, word, file) in rows {
        entries.insert(
            pc.into(),
            entry(color, word, &format!("labels/gervain_names/{file}.wav")),
        );
    }
    LabelSet {
        id: GERVAIN_NAMES.into(),
        title: "Gervain names".into(),
        description: "Sarah, David, Francine, Jimmy, Karen and Leo for one whole-tone set, and six more names for the other.".into(),
        kind: "pitch_class".into(),
        entries,
    }
}

fn my_colors() -> LabelSet {
    LabelSet {
        id: MY_COLORS.into(),
        title: "My colours".into(),
        description: "Your own colour or word for each note. Empty until you fill it in.".into(),
        kind: "pitch_class".into(),
        entries: BTreeMap::new(),
    }
}

/// One Eguchi chord colour: a fixed voicing and its label.
#[derive(Clone, Copy, Debug)]
pub struct EguchiChord {
    pub id: &'static str,
    pub word: &'static str,
    pub color: &'static str,
    /// Chord name. White-phase inversions share a name on purpose; the colour is the answer.
    pub name: &'static str,
    /// MIDI notes, bottom to top, in the fixed register.
    pub notes: &'static [i32],
    pub white: bool,
}

/// The 14-chord sequence. White phase first, then black. Inversions are different colours.
pub const EGUCHI: &[EguchiChord] = &[
    EguchiChord {
        id: "red",
        word: "Red",
        color: "#d64545",
        name: "C major",
        notes: &[60, 64, 67],
        white: true,
    },
    EguchiChord {
        id: "yellow",
        word: "Yellow",
        color: "#e6b325",
        name: "F major",
        notes: &[60, 65, 69],
        white: true,
    },
    EguchiChord {
        id: "blue",
        word: "Blue",
        color: "#3a6fd8",
        name: "G major",
        notes: &[59, 62, 67],
        white: true,
    },
    EguchiChord {
        id: "black",
        word: "Black",
        color: "#2c2c2c",
        name: "F major",
        notes: &[57, 60, 65],
        white: true,
    },
    EguchiChord {
        id: "green",
        word: "Green",
        color: "#3d9a4a",
        name: "G major",
        notes: &[62, 67, 71],
        white: true,
    },
    EguchiChord {
        id: "orange",
        word: "Orange",
        color: "#e07a2f",
        name: "C major",
        notes: &[64, 67, 72],
        white: true,
    },
    EguchiChord {
        id: "purple",
        word: "Purple",
        color: "#7a4ea3",
        name: "F major",
        notes: &[65, 69, 72],
        white: true,
    },
    EguchiChord {
        id: "pink",
        word: "Pink",
        color: "#e06aa0",
        name: "G major",
        notes: &[67, 71, 74],
        white: true,
    },
    EguchiChord {
        id: "brown",
        word: "Brown",
        color: "#8a5a32",
        name: "C major",
        notes: &[55, 60, 64],
        white: true,
    },
    EguchiChord {
        id: "gray",
        word: "Gray",
        color: "#8d8d8d",
        name: "A major",
        notes: &[57, 61, 64],
        white: false,
    },
    EguchiChord {
        id: "tan",
        word: "Tan",
        color: "#c4a574",
        name: "D major",
        notes: &[62, 66, 69],
        white: false,
    },
    EguchiChord {
        id: "light_green",
        word: "Light green",
        color: "#8fce6a",
        name: "E major",
        notes: &[64, 68, 71],
        white: false,
    },
    EguchiChord {
        id: "light_purple",
        word: "Light purple",
        color: "#b9a0d6",
        name: "B♭ major",
        notes: &[58, 62, 65],
        white: false,
    },
    EguchiChord {
        id: "sky_blue",
        word: "Sky blue",
        color: "#7ec8e3",
        name: "E♭ major",
        notes: &[63, 67, 70],
        white: false,
    },
];

fn eguchi_set() -> LabelSet {
    let mut entries = BTreeMap::new();
    for c in EGUCHI {
        entries.insert(
            c.id.into(),
            entry(
                c.color,
                c.word,
                &format!("labels/eguchi_chords/{}.wav", c.word.replace(' ', "_")),
            ),
        );
    }
    LabelSet {
        id: EGUCHI_CHORDS.into(),
        title: "Eguchi chord colours".into(),
        description: "Fourteen chords, each with its own colour. Inversions of the same chord are different colours.".into(),
        kind: "chord".into(),
        entries,
    }
}

pub fn builtin_sets() -> Vec<LabelSet> {
    vec![
        note_names(),
        taneda_colors(),
        gervain_names(),
        my_colors(),
        eguchi_set(),
    ]
}

pub fn builtin_set(id: &str) -> Option<LabelSet> {
    builtin_sets().into_iter().find(|s| s.id == id)
}

pub fn eguchi(id: &str) -> Option<&'static EguchiChord> {
    EGUCHI.iter().find(|c| c.id == id)
}

fn setting_key(id: &str) -> String {
    format!("label_set:{id}")
}

/// Built-in sets with any saved edits written over them, then any extra sets.
pub fn sets_from_db(db: &Db) -> Result<Vec<LabelSet>> {
    let mut sets = builtin_sets();
    for s in &mut sets {
        if let Some(over) = db.get_setting::<LabelSet>(&setting_key(&s.id))? {
            if over.id == s.id {
                *s = over;
            }
        }
    }
    if let Some(extra) = db.get_setting::<Vec<String>>("label_set_ids")? {
        for id in extra {
            if sets.iter().any(|s| s.id == id) {
                continue;
            }
            if let Some(s) = db.get_setting::<LabelSet>(&setting_key(&id))? {
                sets.push(s);
            }
        }
    }
    Ok(sets)
}

/// Saves a set. Built-in ids replace that set; a new id is added.
pub fn save_set(db: &Db, set: &LabelSet) -> Result<LabelSet> {
    validate_set(set)?;
    let known = builtin_set(&set.id).is_some();
    if !known {
        let mut ids: Vec<String> = db.get_setting("label_set_ids")?.unwrap_or_default();
        if !ids.iter().any(|i| i == &set.id) {
            ids.push(set.id.clone());
            db.set_setting("label_set_ids", &ids)?;
        }
    }
    db.set_setting(&setting_key(&set.id), set)?;
    Ok(set.clone())
}

fn validate_set(set: &LabelSet) -> Result<()> {
    if set.id.is_empty()
        || !set
            .id
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_')
    {
        return Err(anyhow!(
            "a label set id uses letters, numbers and underscores"
        ));
    }
    if set.kind != "pitch_class" && set.kind != "chord" {
        return Err(anyhow!("label set kind is pitch_class or chord"));
    }
    for (key, e) in &set.entries {
        if set.kind == "pitch_class" && key.parse::<PitchClass>().is_err() {
            return Err(anyhow!("`{key}` is not a pitch class"));
        }
        if set.kind == "chord" && eguchi(key).is_none() && builtin_set(&set.id).is_some() {
            return Err(anyhow!("`{key}` is not an Eguchi chord"));
        }
        if let Some(c) = &e.color {
            if c.is_empty() || c.len() > 40 || c.chars().any(|ch| ch.is_whitespace()) {
                return Err(anyhow!(
                    "the colour for {key} should be a CSS colour such as #d64545"
                ));
            }
        }
        if let Some(path) = &e.speech {
            if path.contains("..") || path.starts_with('/') || path.starts_with('\\') {
                return Err(anyhow!(
                    "speech for {key} must be a path inside the speech folder"
                ));
            }
        }
        if let Some(w) = &e.word {
            if w.len() > 40 {
                return Err(anyhow!("the word for {key} is too long"));
            }
        }
    }
    Ok(())
}

/// The entry for a pitch class, if this set has one.
pub fn class_entry<'a>(set: &'a LabelSet, pc: PitchClass) -> Option<&'a LabelEntry> {
    set.entries.get(pc.name())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shipped_sets_cover_the_twelve_notes() {
        for id in [TANEDA_COLORS, GERVAIN_NAMES, NOTE_NAMES] {
            let set = builtin_set(id).unwrap();
            assert_eq!(set.entries.len(), 12, "{id}");
            for pc in PitchClass::ALL {
                assert!(set.entries.contains_key(pc.name()), "{id} {}", pc.name());
            }
        }
        assert!(builtin_set(MY_COLORS).unwrap().entries.is_empty());
        assert_eq!(EGUCHI.len(), 14);
        assert_eq!(EGUCHI.iter().filter(|c| c.white).count(), 9);
    }

    #[test]
    fn eguchi_voicings_match_the_listed_pitch_classes() {
        let expect = [
            ("red", vec![0, 4, 7]),
            ("yellow", vec![0, 5, 9]),
            ("blue", vec![11, 2, 7]),
            ("black", vec![9, 0, 5]),
            ("green", vec![2, 7, 11]),
            ("orange", vec![4, 7, 0]),
            ("purple", vec![5, 9, 0]),
            ("pink", vec![7, 11, 2]),
            ("brown", vec![7, 0, 4]),
            ("gray", vec![9, 1, 4]),
            ("tan", vec![2, 6, 9]),
            ("light_green", vec![4, 8, 11]),
            ("light_purple", vec![10, 2, 5]),
            ("sky_blue", vec![3, 7, 10]),
        ];
        for (id, pcs) in expect {
            let c = eguchi(id).unwrap();
            let got: Vec<i32> = c.notes.iter().map(|m| m.rem_euclid(12)).collect();
            assert_eq!(got, pcs, "{id}");
            let mut sorted = c.notes.to_vec();
            sorted.sort();
            assert_eq!(sorted, c.notes.to_vec(), "{id} is listed bottom to top");
        }
    }
}
