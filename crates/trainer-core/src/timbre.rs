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
        let sf_holdout = SOUNDFONT_PRESETS
            .iter()
            .filter(|p| p.pool == SfPool::Holdout)
            .map(|p| p.id);
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
            .into_iter()
            .chain(sf_holdout)
            .map(|s| s.to_string())
            .collect(),
            // Every octave is open to training: the naming register is C3–B6 throughout.
            holdout_octaves: Vec::new(),
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

/// Timbres used when a block does not name any: General MIDI and synth timbres, plus the training
/// presets of the extra SoundFonts in `installed` (all of them when `None`, i.e. not known).
pub fn default_training_timbres(installed: Option<&[String]>) -> Vec<TimbreId> {
    let gm = ["gm:0", "gm:48", "gm:73", "gm:24", "gm:52", "synth:triangle"];
    let sf = SOUNDFONT_PRESETS
        .iter()
        .filter(|p| p.pool == SfPool::Training)
        .filter(|p| soundfont_installed(p.id, installed))
        .map(|p| p.id);
    gm.into_iter().chain(sf).map(|s| s.to_string()).collect()
}

/// Whether a timbre can be rendered as itself: `false` only for an `sf:` bank missing from
/// `installed`. (A missing bank would fall back to a synth tone.)
pub fn soundfont_installed(id: &str, installed: Option<&[String]>) -> bool {
    match (Timbre::parse(id), installed) {
        (Some(Timbre::SoundFont { bank, .. }), Some(list)) => {
            list.iter().any(|b| b.eq_ignore_ascii_case(&bank))
        }
        _ => true,
    }
}

/// File stems of the `.sf2` banks in `dirs` (the names `sf:` timbres use).
pub fn installed_soundfonts(dirs: &[&std::path::Path]) -> Vec<String> {
    let mut out = Vec::new();
    for dir in dirs {
        let Ok(entries) = std::fs::read_dir(dir) else {
            continue;
        };
        for p in entries.flatten().map(|e| e.path()) {
            let sf2 = p
                .extension()
                .and_then(|x| x.to_str())
                .map(|x| x.eq_ignore_ascii_case("sf2"))
                .unwrap_or(false);
            if let (true, Some(stem)) = (sf2, p.file_stem().and_then(|s| s.to_str())) {
                out.push(stem.to_string());
            }
        }
    }
    out.sort();
    out
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SfPool {
    Training,
    Holdout,
}

/// An extra SoundFont preset that was measured for pitch accuracy (docs/SOUNDFONTS.md).
pub struct SfPreset {
    pub id: &'static str,
    pub name: &'static str,
    /// Lowest and highest MIDI note that renders in tune.
    pub low: i32,
    pub high: i32,
    pub pool: SfPool,
}

const fn sf(id: &'static str, name: &'static str, low: i32, high: i32, pool: SfPool) -> SfPreset {
    SfPreset {
        id,
        name,
        low,
        high,
        pool,
    }
}

/// The measured presets, with the training/holdout split from docs/SOUNDFONTS.md.
pub const SOUNDFONT_PRESETS: [SfPreset; 19] = [
    sf(
        "sf:UprightPiano:0",
        "Upright piano",
        48,
        83,
        SfPool::Training,
    ),
    sf("sf:TenorSax:0", "Tenor saxophone", 48, 83, SfPool::Training),
    sf(
        "sf:ClassicalGuitar:0",
        "Classical guitar",
        48,
        83,
        SfPool::Training,
    ),
    sf("sf:Viola:41", "Viola", 48, 83, SfPool::Training),
    sf(
        "sf:SoloVoices:0",
        "Male voice (Duncan)",
        48,
        83,
        SfPool::Training,
    ),
    sf(
        "sf:SoftVoice:0",
        "Soft female voice",
        48,
        83,
        SfPool::Training,
    ),
    sf("sf:ConcertHarp:0", "Concert harp", 48, 83, SfPool::Training),
    sf("sf:FrenchHorn:60", "French horn", 48, 83, SfPool::Holdout),
    sf("sf:Trombone:57", "Trombone", 48, 83, SfPool::Holdout),
    sf("sf:Cello:42", "Cello", 48, 72, SfPool::Holdout),
    sf("sf:Violin:100", "Violin", 48, 83, SfPool::Holdout),
    sf("sf:Flute:73", "Flute", 48, 83, SfPool::Holdout),
    sf(
        "sf:DrawbarOrgan:0",
        "Drawbar organ",
        48,
        83,
        SfPool::Holdout,
    ),
    sf(
        "sf:MaleOohs:1",
        "Male \"ooh\" ensemble",
        50,
        83,
        SfPool::Holdout,
    ),
    sf(
        "sf:OperaFemale:0",
        "Operatic female voice",
        49,
        75,
        SfPool::Holdout,
    ),
    sf(
        "sf:SoloVoices:1",
        "Male voice, \"na\"",
        48,
        83,
        SfPool::Holdout,
    ),
    sf(
        "sf:SoloVoices:4",
        "Male bass voice (Buck)",
        48,
        72,
        SfPool::Holdout,
    ),
    sf(
        "sf:SoloVoices:5",
        "Female voice (Dawn)",
        48,
        83,
        SfPool::Holdout,
    ),
    sf("sf:HeavenChoir:1", "Choir", 48, 83, SfPool::Holdout),
];

/// The MIDI notes a timbre renders in tune, when that is limited. Extra SoundFonts were only
/// measured over C3–B5, so an unlisted `sf:` preset is held to that too; General MIDI, synth and
/// Shepard tones cover the whole keyboard.
pub fn usable_range(id: &str) -> Option<(i32, i32)> {
    if let Some(p) = SOUNDFONT_PRESETS.iter().find(|p| p.id == id) {
        return Some((p.low, p.high));
    }
    match Timbre::parse(id) {
        Some(Timbre::SoundFont { .. }) => Some((48, 83)),
        _ => None,
    }
}

/// Whether all twelve notes of `octave` are inside the timbre's usable range.
pub fn octave_usable(id: &str, octave: i32) -> bool {
    match usable_range(id) {
        Some((lo, hi)) => 12 * (octave + 1) >= lo && 12 * (octave + 1) + 11 <= hi,
        None => true,
    }
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
        assert!(p.is_holdout("sf:FrenchHorn:60", 4));
        // No octave is held out: the naming register is C3–B6 throughout.
        assert!(!p.is_holdout("gm:0", 6));
        assert!(!p.is_holdout("gm:0", 2));
        assert!(!p.is_holdout("gm:0", 4));
        for t in default_training_timbres(None) {
            assert!(!p.is_holdout_timbre(&t), "{t} must not be holdout");
        }
        for sf in SOUNDFONT_PRESETS {
            assert!(Timbre::parse(sf.id).is_some(), "{}", sf.id);
            assert_eq!(
                p.is_holdout_timbre(sf.id),
                sf.pool == SfPool::Holdout,
                "{}",
                sf.id
            );
        }
    }

    #[test]
    fn soundfont_ranges_and_installs() {
        // Cello is in tune C3–C5, so only octaves 3 and 4 are whole.
        assert!(octave_usable("sf:Cello:42", 3));
        assert!(octave_usable("sf:Cello:42", 4));
        assert!(!octave_usable("sf:Cello:42", 5));
        // Unmeasured SoundFont presets are held to C3–B5; GM covers the keyboard.
        assert!(!octave_usable("sf:NewBank:3", 6));
        assert!(octave_usable("gm:0", 6));
        let installed = vec!["UprightPiano".to_string()];
        let training = default_training_timbres(Some(installed.as_slice()));
        assert!(training.contains(&"sf:UprightPiano:0".to_string()));
        assert!(!training.contains(&"sf:TenorSax:0".to_string()));
        assert!(training.contains(&"gm:0".to_string()));
        assert!(soundfont_installed("gm:42", Some(installed.as_slice())));
        assert!(!soundfont_installed(
            "sf:Cello:42",
            Some(installed.as_slice())
        ));
        assert!(soundfont_installed("sf:Cello:42", None));
    }
}
