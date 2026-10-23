//! Timbre ids and the training/holdout stimulus pools.

use serde::{Deserialize, Serialize};

/// Timbre ids:
/// - `gm:<0-127>` General MIDI program from the default SoundFont
/// - `sf:<bank file stem>:<program>` program from an extra SoundFont in the library
/// - `synth:<sine|triangle|saw|square|pulse|soft_piano>` built-in oscillators
/// - `shepard` octave-ambiguous Shepard tone
pub type TimbreId = String;

pub const SYNTH_WAVES: [&str; 6] = ["sine", "triangle", "saw", "square", "pulse", "soft_piano"];

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Timbre {
    Gm(u8),
    SoundFont { bank: String, program: u8 },
    Synth(String),
    Shepard,
}

impl Timbre {
    pub fn parse(id: &str) -> Option<Timbre> {
        let id = id.trim();
        if id == "shepard" {
            return Some(Timbre::Shepard);
        }
        if let Some(p) = id.strip_prefix("gm:") {
            return p.parse::<u8>().ok().filter(|p| *p < 128).map(Timbre::Gm);
        }
        if let Some(w) = id.strip_prefix("synth:") {
            return SYNTH_WAVES
                .contains(&w)
                .then(|| Timbre::Synth(w.to_string()));
        }
        if let Some(rest) = id.strip_prefix("sf:") {
            let (bank, program) = rest.rsplit_once(':')?;
            let program = program.parse::<u8>().ok().filter(|p| *p < 128)?;
            if bank.is_empty() {
                return None;
            }
            return Some(Timbre::SoundFont {
                bank: bank.to_string(),
                program,
            });
        }
        None
    }

    pub fn display_name(&self) -> String {
        match self {
            Timbre::Gm(p) => GM_PROGRAM_NAMES[*p as usize].to_string(),
            Timbre::SoundFont { bank, program } => format!("{bank} #{program}"),
            Timbre::Synth(w) => match w.as_str() {
                "soft_piano" => "Soft piano (synth)".to_string(),
                other => format!("{}{} wave", other[..1].to_uppercase(), &other[1..]),
            },
            Timbre::Shepard => "Shepard tone".to_string(),
        }
    }
}

pub fn is_valid_timbre(id: &str) -> bool {
    Timbre::parse(id).is_some()
}

/// Which stimuli are reserved for assessment. A stimulus is holdout if its timbre or its octave
/// is in the holdout lists.
#[derive(Clone, Debug, Serialize, Deserialize, schemars::JsonSchema)]
pub struct PoolConfig {
    pub holdout_timbres: Vec<TimbreId>,
    pub holdout_octaves: Vec<i32>,
}

impl Default for PoolConfig {
    fn default() -> Self {
        PoolConfig {
            holdout_timbres: [
                "gm:1",
                "gm:11",
                "gm:19",
                "gm:42",
                "gm:56",
                "gm:71",
                "synth:pulse",
                "shepard",
            ]
            .iter()
            .map(|s| s.to_string())
            .collect(),
            holdout_octaves: vec![2, 6],
        }
    }
}

impl PoolConfig {
    pub fn is_holdout_timbre(&self, timbre: &str) -> bool {
        self.holdout_timbres.iter().any(|t| t == timbre)
    }

    pub fn is_holdout_octave(&self, octave: i32) -> bool {
        self.holdout_octaves.contains(&octave)
    }

    pub fn is_holdout(&self, timbre: &str, octave: i32) -> bool {
        self.is_holdout_timbre(timbre) || self.is_holdout_octave(octave)
    }
}

/// Timbres used when a block does not name any.
pub fn default_training_timbres() -> Vec<TimbreId> {
    ["gm:0", "gm:48", "gm:73", "gm:24", "gm:52", "synth:triangle"]
        .iter()
        .map(|s| s.to_string())
        .collect()
}

pub const GM_PROGRAM_NAMES: [&str; 128] = [
    "Acoustic Grand Piano",
    "Bright Acoustic Piano",
    "Electric Grand Piano",
    "Honky-tonk Piano",
    "Electric Piano 1",
    "Electric Piano 2",
    "Harpsichord",
    "Clavinet",
    "Celesta",
    "Glockenspiel",
    "Music Box",
    "Vibraphone",
    "Marimba",
    "Xylophone",
    "Tubular Bells",
    "Dulcimer",
    "Drawbar Organ",
    "Percussive Organ",
    "Rock Organ",
    "Church Organ",
    "Reed Organ",
    "Accordion",
    "Harmonica",
    "Tango Accordion",
    "Acoustic Guitar (nylon)",
    "Acoustic Guitar (steel)",
    "Electric Guitar (jazz)",
    "Electric Guitar (clean)",
    "Electric Guitar (muted)",
    "Overdriven Guitar",
    "Distortion Guitar",
    "Guitar Harmonics",
    "Acoustic Bass",
    "Electric Bass (finger)",
    "Electric Bass (pick)",
    "Fretless Bass",
    "Slap Bass 1",
    "Slap Bass 2",
    "Synth Bass 1",
    "Synth Bass 2",
    "Violin",
    "Viola",
    "Cello",
    "Contrabass",
    "Tremolo Strings",
    "Pizzicato Strings",
    "Orchestral Harp",
    "Timpani",
    "String Ensemble 1",
    "String Ensemble 2",
    "Synth Strings 1",
    "Synth Strings 2",
    "Choir Aahs",
    "Voice Oohs",
    "Synth Voice",
    "Orchestra Hit",
    "Trumpet",
    "Trombone",
    "Tuba",
    "Muted Trumpet",
    "French Horn",
    "Brass Section",
    "Synth Brass 1",
    "Synth Brass 2",
    "Soprano Sax",
    "Alto Sax",
    "Tenor Sax",
    "Baritone Sax",
    "Oboe",
    "English Horn",
    "Bassoon",
    "Clarinet",
    "Piccolo",
    "Flute",
    "Recorder",
    "Pan Flute",
    "Blown Bottle",
    "Shakuhachi",
    "Whistle",
    "Ocarina",
    "Lead 1 (square)",
    "Lead 2 (sawtooth)",
    "Lead 3 (calliope)",
    "Lead 4 (chiff)",
    "Lead 5 (charang)",
    "Lead 6 (voice)",
    "Lead 7 (fifths)",
    "Lead 8 (bass + lead)",
    "Pad 1 (new age)",
    "Pad 2 (warm)",
    "Pad 3 (polysynth)",
    "Pad 4 (choir)",
    "Pad 5 (bowed)",
    "Pad 6 (metallic)",
    "Pad 7 (halo)",
    "Pad 8 (sweep)",
    "FX 1 (rain)",
    "FX 2 (soundtrack)",
    "FX 3 (crystal)",
    "FX 4 (atmosphere)",
    "FX 5 (brightness)",
    "FX 6 (goblins)",
    "FX 7 (echoes)",
    "FX 8 (sci-fi)",
    "Sitar",
    "Banjo",
    "Shamisen",
    "Koto",
    "Kalimba",
    "Bagpipe",
    "Fiddle",
    "Shanai",
    "Tinkle Bell",
    "Agogo",
    "Steel Drums",
    "Woodblock",
    "Taiko Drum",
    "Melodic Tom",
    "Synth Drum",
    "Reverse Cymbal",
    "Guitar Fret Noise",
    "Breath Noise",
    "Seashore",
    "Bird Tweet",
    "Telephone Ring",
    "Helicopter",
    "Applause",
    "Gunshot",
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_timbres() {
        assert_eq!(Timbre::parse("gm:0"), Some(Timbre::Gm(0)));
        assert_eq!(Timbre::parse("gm:128"), None);
        assert_eq!(
            Timbre::parse("synth:sine"),
            Some(Timbre::Synth("sine".into()))
        );
        assert_eq!(Timbre::parse("synth:noise"), None);
        assert_eq!(Timbre::parse("shepard"), Some(Timbre::Shepard));
        assert_eq!(
            Timbre::parse("sf:Salamander:0"),
            Some(Timbre::SoundFont {
                bank: "Salamander".into(),
                program: 0
            })
        );
        assert_eq!(Timbre::parse("gm:52").unwrap().display_name(), "Choir Aahs");
    }

    #[test]
    fn pools() {
        let p = PoolConfig::default();
        assert!(p.is_holdout("gm:1", 4));
        assert!(p.is_holdout("gm:0", 6));
        assert!(!p.is_holdout("gm:0", 4));
        for t in default_training_timbres() {
            assert!(!p.is_holdout_timbre(&t), "{t} must not be holdout");
        }
    }
}
