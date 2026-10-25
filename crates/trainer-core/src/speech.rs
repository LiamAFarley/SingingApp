//! The speech folder: spoken digits, letters, note names and label words.
//!
//! Files live in the app's `data\speech` folder. Missing files turn off only the features
//! that speak; naming, detection and chords still run.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use ts_rs::TS;

struct Expected {
    rel: String,
    /// Feature this file belongs to. `None` is listed but does not disable anything.
    feature: Option<&'static str>,
}

fn file(rel: String, feature: Option<&'static str>) -> Expected {
    Expected { rel, feature }
}

fn expected() -> Vec<Expected> {
    let mut out = Vec::new();
    for d in 0..10 {
        out.push(file(format!("digits/{d}.wav"), Some("Spoken digit span")));
    }
    for c in b'A'..=b'Z' {
        out.push(file(
            format!("letters/{}.wav", c as char),
            Some("Spoken letter n-back"),
        ));
    }
    for name in [
        "C", "C#", "D", "D#", "E", "F", "F#", "G", "G#", "A", "A#", "B",
    ] {
        out.push(file(format!("notes/{name}.wav"), Some("Spoken note names")));
    }
    // The other spelling is accepted and not required.
    for name in ["Db", "Eb", "Gb", "Ab", "Bb"] {
        out.push(file(format!("notes/{name}.wav"), None));
    }
    for word in [
        "Red", "Blue", "Yellow", "Orange", "Brown", "Grey", "Green", "Magenta", "Teal", "Gold",
        "Indigo", "Pink",
    ] {
        out.push(file(
            format!("labels/taneda_colors/{word}.wav"),
            Some("Spoken Taneda colours"),
        ));
    }
    for name in [
        "Sarah", "David", "Francine", "Jimmy", "Karen", "Leo", "Nina", "Oscar", "Paula", "Robert",
        "Rita", "Simon",
    ] {
        out.push(file(
            format!("labels/gervain_names/{name}.wav"),
            Some("Spoken Gervain names"),
        ));
    }
    for word in ["sing", "now"] {
        out.push(file(format!("prompts/{word}.wav"), None));
    }
    out
}

/// What the speech folder has, and which speaking features stay off.
#[derive(Clone, Debug, Serialize, Deserialize, TS)]
pub struct SpeechReport {
    pub dir: String,
    /// Required and optional files that are not there yet.
    pub missing: Vec<String>,
    pub present: u32,
    /// Features that need files which are missing. Everything else still runs.
    pub disabled: Vec<String>,
}

pub fn speech_dir(data_dir: &Path) -> PathBuf {
    data_dir.join("speech")
}

const EXTENSIONS: [&str; 3] = ["wav", "flac", "mp3"];

/// The file for a path such as `notes/C.wav`, accepting wav, flac or mp3.
pub fn resolve(dir: &Path, rel: &str) -> Option<PathBuf> {
    let rel = rel.trim_start_matches(['/', '\\']);
    if rel.contains("..") {
        return None;
    }
    let path = dir.join(rel);
    if path.is_file() {
        return Some(path);
    }
    let stem = path.with_extension("");
    for ext in EXTENSIONS {
        let p = stem.with_extension(ext);
        if p.is_file() {
            return Some(p);
        }
    }
    None
}

pub fn inspect(dir: &Path) -> SpeechReport {
    let files = expected();
    let mut missing = Vec::new();
    let mut present = 0u32;
    let mut disabled: Vec<String> = Vec::new();
    let mut groups: Vec<(&str, bool)> = Vec::new();
    for f in &files {
        if resolve(dir, &f.rel).is_some() {
            present += 1;
        } else {
            missing.push(f.rel.clone());
            if let Some(feature) = f.feature {
                if let Some(g) = groups.iter_mut().find(|g| g.0 == feature) {
                    g.1 = false;
                } else {
                    groups.push((feature, false));
                }
            }
        }
        if let Some(feature) = f.feature {
            if !groups.iter().any(|g| g.0 == feature) {
                groups.push((feature, true));
            }
        }
    }
    for (feature, ok) in groups {
        if !ok {
            disabled.push(feature.to_string());
        }
    }
    SpeechReport {
        dir: dir.display().to_string(),
        missing,
        present,
        disabled,
    }
}

/// Every path a generator should produce, one per line, for `tools/speech_file_list.py`.
pub fn expected_paths() -> Vec<String> {
    expected().into_iter().map(|e| e.rel).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn missing_files_are_listed_and_only_those_features_are_disabled() {
        let dir = std::env::temp_dir().join(format!("singing-speech-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("digits")).unwrap();
        // One digit is present; the rest of the digits, and everything else, are missing.
        std::fs::write(dir.join("digits").join("0.flac"), b"not really flac").unwrap();
        let report = inspect(&dir);
        assert!(report.missing.iter().any(|m| m == "digits/1.wav"));
        assert!(
            !report.missing.iter().any(|m| m == "digits/0.wav"),
            "flac stands in for wav"
        );
        assert!(report.disabled.iter().any(|d| d == "Spoken digit span"));
        assert!(report.disabled.iter().any(|d| d == "Spoken note names"));
        assert!(report.present >= 1);
        // A missing file does not invent a path outside the folder.
        assert!(resolve(&dir, "../secret.wav").is_none());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_complete_group_is_not_disabled() {
        let dir = std::env::temp_dir().join(format!("singing-speech-ok-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        for rel in expected_paths() {
            let path = dir.join(&rel);
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(path, b"x").unwrap();
        }
        let report = inspect(&dir);
        assert!(report.missing.is_empty(), "{:?}", report.missing);
        assert!(report.disabled.is_empty(), "{:?}", report.disabled);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
