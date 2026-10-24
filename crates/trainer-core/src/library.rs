//! The melody library: pieces read from JSON and MIDI files in the library folder.
//!
//! Every `.json`, `.mid` and `.midi` file in the folder and its subfolders is imported, and a
//! subfolder is a collection. A changed file is read again and a deleted file takes its pieces
//! with it: the folder is the library, and the database only indexes it.

use std::cmp::Ordering;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::UNIX_EPOCH;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use ts_rs::TS;

use crate::schema::{KeyMode, LibrarySelect, NoteRange};
use crate::score::{ClefChoice, CompactMelody, CompactNote, KeySig, Pitch, Score, TimeSig};

/// At most this many parts per piece.
pub const MAX_PARTS: usize = 4;
pub const EXTENSIONS: [&str; 3] = ["json", "mid", "midi"];
/// Tempo used to check pieces that do not give one.
const CHECK_TEMPO: f32 = 90.0;

/// A piece: one or more single-line parts in the same key and metre.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema, TS)]
pub struct LibraryPiece {
    pub id: String,
    pub title: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub collection: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tags: Vec<String>,
    /// E.g. `Eb major`.
    pub key: String,
    pub time: TimeSig,
    /// Beats per minute in the time signature's unit.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tempo_bpm: Option<f32>,
    /// Top part first.
    pub parts: Vec<LibraryPart>,
    /// The source file, relative to the library folder.
    pub file: String,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema, TS)]
pub struct LibraryPart {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(default)]
    pub clef: ClefChoice,
    pub notes: Vec<CompactNote>,
}

impl LibraryPiece {
    /// Part `k` (the lowest part when there are fewer) as a melody.
    pub fn melody(&self, k: usize) -> CompactMelody {
        let part = &self.parts[k.min(self.parts.len().saturating_sub(1))];
        CompactMelody {
            key: self.key.clone(),
            time: self.time,
            clef: part.clef,
            tempo_bpm: self.tempo_bpm,
            notes: part.notes.clone(),
            title: Some(self.title.clone()),
        }
    }

    /// Every part as a score.
    pub fn scores(&self) -> Result<Vec<Score>, String> {
        (0..self.parts.len())
            .map(|k| {
                Score::from_compact(&self.melody(k), self.tempo_bpm.unwrap_or(CHECK_TEMPO))
                    .map_err(|e| if self.parts.len() > 1 { format!("parts[{k}]: {e}") } else { e })
            })
            .collect()
    }

    /// Normalises the key, checks every part and pads shorter parts with a rest to the length of
    /// the longest.
    fn finish(&mut self) -> Result<(), String> {
        self.key = KeySig::parse(&self.key).map_err(|e| format!("key: {e}"))?.to_string();
        if self.parts.is_empty() {
            return Err("a piece needs at least one part".into());
        }
        if self.parts.len() > MAX_PARTS {
            return Err(format!("a piece can have at most {MAX_PARTS} parts"));
        }
        if let Some(t) = self.tempo_bpm {
            if !(20.0..=300.0).contains(&t) {
                return Err(format!("tempo_bpm {t} is outside 20–300"));
            }
        }
        let lengths: Vec<f32> = self.parts.iter().map(|p| p.notes.iter().map(note_beats).sum()).collect();
        let longest = lengths.iter().copied().fold(0.0, f32::max);
        for (part, len) in self.parts.iter_mut().zip(lengths) {
            if longest - len > 1e-4 {
                part.notes.push(CompactNote::Rest { rest: longest - len });
            }
        }
        self.scores()?;
        Ok(())
    }

    /// One line per piece for lists: id, title, key, metre, bars and parts.
    pub fn summary(&self) -> Value {
        let scores = self.scores().unwrap_or_default();
        let range = scores.iter().filter_map(|s| s.range()).fold(None, |acc: Option<(i32, i32)>, (lo, hi)| {
            Some(acc.map_or((lo, hi), |(a, b)| (a.min(lo), b.max(hi))))
        });
        json!({
            "id": self.id,
            "title": self.title,
            "collection": self.collection,
            "tags": self.tags,
            "key": self.key,
            "time": self.time,
            "tempo_bpm": self.tempo_bpm,
            "bars": scores.first().map(|s| s.bars),
            "parts": self.parts.iter().map(|p| p.name.clone().unwrap_or_default()).collect::<Vec<_>>(),
            "range": range.map(|(lo, hi)| format!("{}–{}", crate::theory::Note(lo).name(), crate::theory::Note(hi).name())),
            "file": self.file,
        })
    }
}

fn note_beats(n: &CompactNote) -> f32 {
    match n {
        CompactNote::Note { beats, .. } => *beats,
        CompactNote::Rest { rest } => *rest,
    }
}

// ---------------------------------------------------------------------------------------------
// JSON
// ---------------------------------------------------------------------------------------------

/// Reads the pieces in a library JSON file: one piece, a list of pieces, or a collection
/// `{"collection": "…", "tags": […], "pieces": […]}`.
///
/// A piece is `{"title", "key", "time", "tempo_bpm", "pickup", "notes"}` for one part, or has
/// `"parts": [{"name", "clef", "notes"}, …]` instead of `notes`. Notes are a list
/// (`[{"note": "Eb4", "beats": 1}, {"rest": 0.5}]`) or text (`"Eb4:1 F4:.5 G4 | Ab4:2~ | Ab4:1 R:1"`).
pub fn parse_json(text: &str) -> Result<Vec<LibraryPiece>, String> {
    let v: Value =
        serde_json::from_str(text.trim_start_matches('\u{feff}')).map_err(|e| format!("invalid JSON: {e}"))?;
    let (items, collection, tags): (Vec<&Value>, Option<String>, Vec<String>) = match &v {
        Value::Array(a) => (a.iter().collect(), None, Vec::new()),
        Value::Object(o) if o.contains_key("pieces") || o.contains_key("melodies") => {
            let list = o.get("pieces").or_else(|| o.get("melodies")).unwrap();
            let a = list.as_array().ok_or("`pieces` must be a list")?;
            (a.iter().collect(), opt_string(o.get("collection"), "collection")?, strings(o.get("tags"), "tags")?)
        }
        Value::Object(_) => (vec![&v], None, Vec::new()),
        _ => return Err("expected a piece, a list of pieces or {\"pieces\": […]}".into()),
    };
    if items.is_empty() {
        return Err("the file has no pieces".into());
    }
    let many = items.len() > 1;
    items
        .iter()
        .enumerate()
        .map(|(k, item)| {
            let mut p = parse_piece(item).map_err(|e| if many { format!("pieces[{k}]: {e}") } else { e })?;
            if p.collection.is_none() {
                p.collection = collection.clone();
            }
            for t in &tags {
                if !p.tags.contains(t) {
                    p.tags.push(t.clone());
                }
            }
            Ok(p)
        })
        .collect()
}

fn opt_string(v: Option<&Value>, field: &str) -> Result<Option<String>, String> {
    match v {
        None | Some(Value::Null) => Ok(None),
        Some(Value::String(s)) if s.trim().is_empty() => Ok(None),
        Some(Value::String(s)) => Ok(Some(s.trim().to_string())),
        Some(_) => Err(format!("`{field}` must be text")),
    }
}

fn opt_number(v: Option<&Value>, field: &str) -> Result<Option<f64>, String> {
    match v {
        None | Some(Value::Null) => Ok(None),
        Some(x) => x.as_f64().map(Some).ok_or_else(|| format!("`{field}` must be a number")),
    }
}

fn strings(v: Option<&Value>, field: &str) -> Result<Vec<String>, String> {
    match v {
        None | Some(Value::Null) => Ok(Vec::new()),
        Some(Value::String(s)) => Ok(vec![s.trim().to_string()]),
        Some(Value::Array(a)) => a
            .iter()
            .map(|x| x.as_str().map(|s| s.trim().to_string()).ok_or_else(|| format!("`{field}` must be a list of text")))
            .collect(),
        Some(_) => Err(format!("`{field}` must be a list of text")),
    }
}

fn parse_clef(v: Option<&Value>) -> Result<ClefChoice, String> {
    match v {
        None | Some(Value::Null) => Ok(ClefChoice::Auto),
        Some(x) => serde_json::from_value(x.clone()).map_err(|_| "`clef` must be auto, treble, treble8vb or bass".into()),
    }
}

fn parse_piece(v: &Value) -> Result<LibraryPiece, String> {
    let o = v.as_object().ok_or("expected an object")?;
    let key = opt_string(o.get("key"), "key")?.ok_or("missing `key`, e.g. \"Eb major\"")?;
    let time: TimeSig = match opt_string(o.get("time"), "time")? {
        Some(t) => t.parse().map_err(|e| format!("time: {e}"))?,
        None => TimeSig::COMMON,
    };
    let tempo_bpm = opt_number(o.get("tempo_bpm"), "tempo_bpm")?.map(|t| t as f32);
    let pickup = opt_number(o.get("pickup"), "pickup")?.map(|b| b as f32);
    let parts = match (o.get("parts"), o.get("notes")) {
        (Some(p), _) => p
            .as_array()
            .ok_or("`parts` must be a list")?
            .iter()
            .enumerate()
            .map(|(k, part)| parse_part(part, time, pickup).map_err(|e| format!("parts[{k}]: {e}")))
            .collect::<Result<Vec<_>, _>>()?,
        (None, Some(n)) => vec![LibraryPart {
            name: None,
            clef: parse_clef(o.get("clef"))?,
            notes: parse_notes(n, time, pickup).map_err(|e| format!("notes: {e}"))?,
        }],
        (None, None) => return Err("needs `notes` (one part) or `parts`".into()),
    };
    Ok(LibraryPiece {
        id: opt_string(o.get("id"), "id")?.unwrap_or_default(),
        title: opt_string(o.get("title"), "title")?.unwrap_or_default(),
        collection: opt_string(o.get("collection"), "collection")?,
        tags: strings(o.get("tags"), "tags")?,
        key,
        time,
        tempo_bpm,
        parts,
        file: String::new(),
    })
}

fn parse_part(v: &Value, time: TimeSig, pickup: Option<f32>) -> Result<LibraryPart, String> {
    let o = v.as_object().ok_or("expected {\"name\": …, \"notes\": …}")?;
    Ok(LibraryPart {
        name: opt_string(o.get("name"), "name")?,
        clef: parse_clef(o.get("clef"))?,
        notes: parse_notes(o.get("notes").ok_or("missing `notes`")?, time, pickup)
            .map_err(|e| format!("notes: {e}"))?,
    })
}

/// Notes as a list or as text. A pickup (given, or found from the first barline of the text)
/// becomes a rest that fills the rest of the first bar.
fn parse_notes(v: &Value, time: TimeSig, pickup: Option<f32>) -> Result<Vec<CompactNote>, String> {
    let (mut notes, found_pickup) = match v {
        Value::String(s) => parse_note_text(s, time)?,
        Value::Array(a) => (
            a.iter()
                .enumerate()
                .map(|(i, n)| parse_note_object(n).map_err(|e| format!("[{i}]: {e}")))
                .collect::<Result<Vec<_>, _>>()?,
            None,
        ),
        _ => return Err("expected a list of notes or text like \"C4:1 D4:1 E4:2\"".into()),
    };
    if notes.is_empty() {
        return Err("no notes".into());
    }
    if let Some(p) = pickup.or(found_pickup) {
        let bar = time.beats as f32;
        if p <= 0.0 || p >= bar {
            return Err(format!("a pickup of {p} beats must be shorter than a bar of {time}"));
        }
        notes.insert(0, CompactNote::Rest { rest: bar - p });
    }
    Ok(notes)
}

fn parse_note_object(v: &Value) -> Result<CompactNote, String> {
    const SHAPE: &str = "expected {\"note\": \"C4\", \"beats\": 1} or {\"rest\": 1}";
    let o = v.as_object().ok_or(SHAPE)?;
    if let Some(r) = o.get("rest") {
        let rest = r.as_f64().ok_or("`rest` must be a number of beats")? as f32;
        return Ok(CompactNote::Rest { rest });
    }
    let note = o.get("note").or_else(|| o.get("pitch")).ok_or(SHAPE)?;
    let beats = o.get("beats").and_then(Value::as_f64).ok_or("missing `beats`")? as f32;
    match note {
        Value::Null => Ok(CompactNote::Rest { rest: beats }),
        Value::String(s) => Ok(CompactNote::Note { note: s.parse()?, beats }),
        _ => Err("`note` must be a pitch like \"Eb4\"".into()),
    }
}

fn fmt_beats(b: f32) -> String {
    let s = format!("{b:.3}");
    s.trim_end_matches('0').trim_end_matches('.').to_string()
}

/// `C4:1 D4:.5 E4 | F4:2~ | F4:1 R:1`: pitch and beats, a duration carries over to the next
/// note, `R` is a rest, `~` ties to the next note and each `|` is a barline that is checked.
/// Returns the notes and, when the first bar is short, the pickup.
fn parse_note_text(text: &str, time: TimeSig) -> Result<(Vec<CompactNote>, Option<f32>), String> {
    let bar = time.beats as f32;
    let mut notes: Vec<CompactNote> = Vec::new();
    let mut dur = 1.0f32;
    let mut tie = false;
    let mut in_bar = 0.0f32;
    let mut barlines = 0;
    let mut pickup = None;
    for token in text.split(|c: char| c.is_whitespace() || c == ',').filter(|t| !t.is_empty()) {
        if token.chars().all(|c| c == '|') {
            if in_bar == 0.0 && notes.is_empty() {
                continue;
            }
            let short = in_bar < bar - 1e-4;
            if barlines == 0 && short {
                pickup = Some(in_bar);
            } else if (in_bar - bar).abs() > 1e-4 {
                let number = barlines + if pickup.is_some() { 0 } else { 1 };
                return Err(format!(
                    "bar {number} has {} beats; a bar of {time} has {}",
                    fmt_beats(in_bar),
                    fmt_beats(bar)
                ));
            }
            barlines += 1;
            in_bar = 0.0;
            continue;
        }
        let (body, tied) = match token.strip_suffix('~') {
            Some(b) => (b, true),
            None => (token, false),
        };
        let (name, d) = match body.split_once(':') {
            Some((n, d)) => (n, Some(d)),
            None => (body, None),
        };
        if let Some(d) = d {
            dur = d
                .parse::<f32>()
                .ok()
                .filter(|x| *x > 0.0 && x.is_finite())
                .ok_or_else(|| format!("`{token}`: `{d}` is not a number of beats"))?;
        }
        if name.eq_ignore_ascii_case("r") || name.eq_ignore_ascii_case("rest") {
            if tie {
                return Err(format!("`{token}`: a tie joins two notes of the same pitch"));
            }
            notes.push(CompactNote::Rest { rest: dur });
        } else {
            let pitch: Pitch = name.parse().map_err(|e| format!("`{token}`: {e}"))?;
            match (tie, notes.last_mut()) {
                (true, Some(CompactNote::Note { note, beats })) if note.midi() == pitch.midi() => *beats += dur,
                (true, _) => return Err(format!("`{token}`: a tie joins two notes of the same pitch")),
                (false, _) => notes.push(CompactNote::Note { note: pitch, beats: dur }),
            }
        }
        tie = tied;
        in_bar += dur;
    }
    if tie {
        return Err("the last note is tied to nothing".into());
    }
    if barlines > 0 && in_bar > bar + 1e-4 {
        return Err(format!(
            "the last bar has {} beats; a bar of {time} has {}",
            fmt_beats(in_bar),
            fmt_beats(bar)
        ));
    }
    Ok((notes, pickup))
}

// ---------------------------------------------------------------------------------------------
// MIDI
// ---------------------------------------------------------------------------------------------

/// A note in sixteenths: start, end, MIDI note.
type Sixteenths = (i64, i64, i32);

struct Voice {
    track: usize,
    channel: u8,
    name: Option<String>,
    notes: Vec<Sixteenths>,
}

/// Reads a standard MIDI file: one part per track (and channel) with notes, top part first. A
/// part with chords keeps its top line. Times are rounded to sixteenth notes; a short first bar
/// (a pickup) is kept as such, and the key comes from the key signature or is estimated.
pub fn parse_midi(bytes: &[u8]) -> Result<LibraryPiece, String> {
    use midly::{MetaMessage, MidiMessage, Smf, Timing, TrackEventKind};
    let smf = Smf::parse(bytes).map_err(|e| format!("not a MIDI file: {e}"))?;
    let tpq = match smf.header.timing {
        Timing::Metrical(t) if t.as_int() > 0 => t.as_int() as f64,
        Timing::Metrical(_) => return Err("the file has no timing".into()),
        Timing::Timecode(..) => return Err("MIDI files timed in SMPTE frames are not supported".into()),
    };
    let q = |tick: u64| (tick as f64 * 4.0 / tpq).round() as i64;
    let mut tempo: Option<(u64, u32)> = None;
    let mut time_sigs: Vec<(u64, u8, u8)> = Vec::new();
    let mut key_sig: Option<(u64, i8, bool)> = None;
    let mut title = None;
    let mut voices: Vec<Voice> = Vec::new();
    for (ti, track) in smf.tracks.iter().enumerate() {
        let mut t: u64 = 0;
        let mut name: Option<String> = None;
        let mut sounding: HashMap<(u8, u8), Vec<u64>> = HashMap::new();
        let mut finished: Vec<(u8, u64, u64, u8)> = Vec::new();
        for ev in track {
            t += ev.delta.as_int() as u64;
            match ev.kind {
                TrackEventKind::Meta(MetaMessage::Tempo(us)) => {
                    if tempo.is_none_or(|(t0, _)| t < t0) {
                        tempo = Some((t, us.as_int()));
                    }
                }
                TrackEventKind::Meta(MetaMessage::TimeSignature(n, d, _, _)) => time_sigs.push((t, n, d)),
                TrackEventKind::Meta(MetaMessage::KeySignature(sf, minor)) => {
                    if key_sig.is_none_or(|(t0, ..)| t < t0) {
                        key_sig = Some((t, sf, minor));
                    }
                }
                TrackEventKind::Meta(MetaMessage::TrackName(b)) => {
                    if name.is_none() {
                        name = Some(String::from_utf8_lossy(b).trim().to_string()).filter(|s| !s.is_empty());
                    }
                }
                TrackEventKind::Midi { channel, message } => {
                    let ch = channel.as_int();
                    if ch == 9 {
                        continue; // percussion
                    }
                    let (key, on) = match message {
                        MidiMessage::NoteOn { key, vel } => (key.as_int(), vel.as_int() > 0),
                        MidiMessage::NoteOff { key, .. } => (key.as_int(), false),
                        _ => continue,
                    };
                    let starts = sounding.entry((ch, key)).or_default();
                    if on {
                        starts.push(t);
                    } else if !starts.is_empty() {
                        finished.push((ch, starts.remove(0), t, key));
                    }
                }
                _ => {}
            }
        }
        for ((ch, key), starts) in sounding {
            for s in starts {
                finished.push((ch, s, t, key));
            }
        }
        if finished.is_empty() {
            if title.is_none() {
                title = name;
            }
            continue;
        }
        for (ch, s, e, key) in finished {
            let at = match voices.iter().position(|v| v.track == ti && v.channel == ch) {
                Some(i) => i,
                None => {
                    voices.push(Voice { track: ti, channel: ch, name: name.clone(), notes: Vec::new() });
                    voices.len() - 1
                }
            };
            voices[at].notes.push((q(s), q(e), key as i32));
        }
    }
    if voices.is_empty() {
        return Err("the file has no notes".into());
    }

    // Metre, and a pickup written as a short first time signature.
    time_sigs.sort_by_key(|x| x.0);
    time_sigs.dedup_by(|b, a| {
        let same = a.0 == b.0;
        if same {
            *a = *b;
        }
        same
    });
    let sig = |n: u8, d: u8| -> Result<TimeSig, String> {
        let unit = 1u32.checked_shl(d as u32).unwrap_or(0);
        format!("{n}/{unit}").parse::<TimeSig>()
    };
    let (time, shift) = match time_sigs.as_slice() {
        [] => (TimeSig::COMMON, 0),
        [(_, n, d)] => (sig(*n, *d)?, 0),
        [(t0, n0, d0), (t1, n1, d1), ..] => {
            let second = sig(*n1, *d1)?;
            let short = q(*t1) - q(*t0);
            let bar = second.bar_ticks() as i64;
            if *t0 == 0 && short > 0 && short < bar {
                (second, bar - short)
            } else {
                (sig(*n0, *d0)?, 0)
            }
        }
    };
    let bar = time.bar_ticks() as i64;

    // Keep the four busiest voices, top part first.
    voices.sort_by_key(|v| std::cmp::Reverse(v.notes.len()));
    voices.truncate(MAX_PARTS);
    let mean = |v: &Voice| v.notes.iter().map(|n| n.2 as f64).sum::<f64>() / v.notes.len() as f64;
    voices.sort_by(|a, b| mean(b).partial_cmp(&mean(a)).unwrap_or(Ordering::Equal));
    let mut lines: Vec<Vec<Sixteenths>> = voices.iter().map(|v| top_line(&v.notes, shift)).collect();
    let first = lines.iter().filter_map(|l| l.first().map(|n| n.0)).min().unwrap_or(0);
    let drop = first.div_euclid(bar) * bar;
    for l in &mut lines {
        for n in l.iter_mut() {
            n.0 -= drop;
            n.1 -= drop;
        }
    }

    let all: Vec<Sixteenths> = lines.iter().flatten().copied().collect();
    let key = match key_sig {
        Some((_, sf, minor)) => {
            KeySig::from_fifths(sf as i32, if minor { KeyMode::Minor } else { KeyMode::Major })?
        }
        None => {
            let last = lines.first().and_then(|l| l.last()).map(|n| n.2);
            estimate_key(&all.iter().map(|n| (n.2, (n.1 - n.0) as f32)).collect::<Vec<_>>(), last)
        }
    };
    let beat = time.beat_ticks() as f32;
    let parts = voices
        .iter()
        .zip(&lines)
        .map(|(v, line)| {
            let mut notes = Vec::new();
            let mut cursor = 0;
            for &(s, e, m) in line {
                if s > cursor {
                    notes.push(CompactNote::Rest { rest: (s - cursor) as f32 / beat });
                }
                notes.push(CompactNote::Note { note: key.spell(m), beats: (e - s) as f32 / beat });
                cursor = e;
            }
            LibraryPart { name: v.name.clone(), clef: ClefChoice::Auto, notes }
        })
        .collect();
    let tempo_bpm = tempo
        .map(|(_, us)| (60_000_000.0 / us.max(1) as f64 * time.unit as f64 / 4.0) as f32)
        .map(|b| (b * 10.0).round() / 10.0)
        .filter(|b| (20.0..=300.0).contains(b));
    Ok(LibraryPiece {
        id: String::new(),
        title: title.unwrap_or_default(),
        collection: None,
        tags: Vec::new(),
        key: key.to_string(),
        time,
        tempo_bpm,
        parts,
        file: String::new(),
    })
}

/// The top line of some notes in sixteenths, moved by `shift`: one note per onset (the highest),
/// lower notes under a held note dropped, each note cut at the next onset, and a gap of one
/// sixteenth after a long note closed.
fn top_line(notes: &[Sixteenths], shift: i64) -> Vec<Sixteenths> {
    let mut sorted: Vec<Sixteenths> = notes.iter().map(|&(s, e, m)| (s + shift, e.max(s + 1) + shift, m)).collect();
    sorted.sort_by(|a, b| a.0.cmp(&b.0).then(b.2.cmp(&a.2)));
    let mut out: Vec<Sixteenths> = Vec::new();
    for (s, e, m) in sorted {
        if let Some(last) = out.last_mut() {
            if s == last.0 || (s < last.1 && m < last.2) {
                continue;
            }
            if s < last.1 {
                last.1 = s;
            } else if s - last.1 == 1 && last.1 - last.0 >= 4 {
                last.1 = s;
            }
        }
        out.push((s, e, m));
    }
    out
}

/// The likeliest key of some notes (MIDI note, weight) by the Krumhansl–Kessler profiles, with a
/// small bonus for a tonic on the last note.
pub fn estimate_key(notes: &[(i32, f32)], last: Option<i32>) -> KeySig {
    const MAJOR: [f32; 12] = [6.35, 2.23, 3.48, 2.33, 4.38, 4.09, 2.52, 5.19, 2.39, 3.66, 2.29, 2.88];
    const MINOR: [f32; 12] = [6.33, 2.68, 3.52, 5.38, 2.60, 3.53, 2.54, 4.75, 3.98, 2.69, 3.34, 3.17];
    let mut hist = [0f32; 12];
    for (m, w) in notes {
        hist[m.rem_euclid(12) as usize] += w.max(0.0);
    }
    let mut best = (f32::MIN, 0, KeyMode::Major);
    for (mode, profile) in [(KeyMode::Major, MAJOR), (KeyMode::Minor, MINOR)] {
        for tonic in 0..12 {
            let x: Vec<f32> = (0..12).map(|i| hist[(tonic + i) % 12]).collect();
            let r = correlation(&x, &profile)
                + if last.is_some_and(|l| l.rem_euclid(12) as usize == tonic) { 0.1 } else { 0.0 };
            if r > best.0 {
                best = (r, tonic, mode);
            }
        }
    }
    KeySig::for_tonic_pc(best.1 as i32, best.2)
}

fn correlation(x: &[f32], y: &[f32]) -> f32 {
    let n = x.len() as f32;
    let (mx, my) = (x.iter().sum::<f32>() / n, y.iter().sum::<f32>() / n);
    let cov: f32 = x.iter().zip(y).map(|(a, b)| (a - mx) * (b - my)).sum();
    let vx: f32 = x.iter().map(|a| (a - mx).powi(2)).sum();
    let vy: f32 = y.iter().map(|b| (b - my).powi(2)).sum();
    if vx <= 0.0 || vy <= 0.0 { 0.0 } else { cov / (vx * vy).sqrt() }
}

// ---------------------------------------------------------------------------------------------
// Files
// ---------------------------------------------------------------------------------------------

/// `data_dir/library`.
pub fn folder(data_dir: &Path) -> PathBuf {
    data_dir.join("library")
}

pub fn is_library_file(name: &str) -> bool {
    Path::new(name)
        .extension()
        .and_then(|e| e.to_str())
        .is_some_and(|e| EXTENSIONS.iter().any(|x| e.eq_ignore_ascii_case(x)))
}

/// Library files under `dir` as (path relative to `dir` with `/`, size and modification time),
/// skipping hidden files and folders.
pub fn scan(dir: &Path) -> Vec<(String, String)> {
    fn walk(dir: &Path, rel: &str, depth: u32, out: &mut Vec<(String, String)>) {
        let Ok(entries) = std::fs::read_dir(dir) else { return };
        for e in entries.flatten() {
            let name = e.file_name().to_string_lossy().to_string();
            if name.starts_with('.') {
                continue;
            }
            let path = e.path();
            let Ok(meta) = std::fs::metadata(&path) else { continue };
            let rel_name = if rel.is_empty() { name.clone() } else { format!("{rel}/{name}") };
            if meta.is_dir() {
                if depth < 8 {
                    walk(&path, &rel_name, depth + 1, out);
                }
            } else if is_library_file(&name) {
                let mtime = meta
                    .modified()
                    .ok()
                    .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
                    .map(|d| d.as_nanos())
                    .unwrap_or(0);
                out.push((rel_name, format!("{}:{mtime}", meta.len())));
            }
        }
    }
    let mut out = Vec::new();
    walk(dir, "", 0, &mut out);
    out
}

/// The pieces in a library file, with ids, titles and collections filled in: a piece without an
/// id is named after the file (`Ottman/ch03-12`, or `…#2` for the second piece of a file), and
/// its collection is the file's folder.
pub fn read_file(rel: &str, bytes: &[u8]) -> Result<Vec<LibraryPiece>, String> {
    let path = Path::new(rel);
    let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("").to_ascii_lowercase();
    let mut pieces = if ext == "json" {
        parse_json(std::str::from_utf8(bytes).map_err(|_| "the file is not UTF-8 text")?)?
    } else {
        vec![parse_midi(bytes)?]
    };
    let base = rel.rsplit_once('.').map_or(rel, |(s, _)| s);
    let stem = path.file_stem().and_then(|s| s.to_str()).unwrap_or(base);
    let folder = rel.rsplit_once('/').map(|(d, _)| d.to_string());
    let many = pieces.len() > 1;
    for (k, p) in pieces.iter_mut().enumerate() {
        if p.id.is_empty() {
            p.id = if many { format!("{base}#{}", k + 1) } else { base.to_string() };
        }
        if p.title.is_empty() {
            p.title = if many { format!("{stem} {}", k + 1) } else { stem.to_string() };
        }
        if p.collection.is_none() {
            p.collection = folder.clone();
        }
        p.file = rel.to_string();
        p.finish().map_err(|e| if many { format!("pieces[{k}]: {e}") } else { e })?;
    }
    for (k, p) in pieces.iter().enumerate() {
        if pieces[..k].iter().any(|q| q.id == p.id) {
            return Err(format!("the id `{}` is used twice", p.id));
        }
    }
    Ok(pieces)
}

/// A file or folder name with the characters Windows forbids replaced.
fn clean_name(name: &str) -> String {
    let s: String = name
        .chars()
        .map(|c| if c.is_control() || r#"<>:"/\|?*"#.contains(c) { '_' } else { c })
        .collect();
    s.trim().trim_matches('.').trim().to_string()
}

/// The library-relative path for a file `name` in `collection` (a folder path like `Ottman/Ch 3`).
pub fn file_path(collection: Option<&str>, name: &str) -> Result<String, String> {
    let name = clean_name(name);
    if name.is_empty() || !is_library_file(&name) {
        return Err(format!("`{name}` is not a .json, .mid or .midi file name"));
    }
    let mut parts: Vec<String> = collection
        .unwrap_or("")
        .split(['/', '\\'])
        .map(clean_name)
        .filter(|s| !s.is_empty())
        .collect();
    parts.push(name);
    Ok(parts.join("/"))
}

/// Writes a file into the library folder, replacing one of the same name. The data goes to a
/// hidden temporary file first, so a scan never reads half a file.
pub fn write_file(dir: &Path, rel: &str, bytes: &[u8]) -> Result<(), String> {
    let target = resolve(dir, rel)?;
    let parent = target.parent().ok_or("invalid path")?;
    std::fs::create_dir_all(parent).map_err(|e| format!("creating {}: {e}", parent.display()))?;
    let name = target.file_name().and_then(|n| n.to_str()).ok_or("invalid path")?;
    let tmp = parent.join(format!(".{name}.tmp"));
    std::fs::write(&tmp, bytes).map_err(|e| format!("writing {}: {e}", tmp.display()))?;
    std::fs::rename(&tmp, &target).map_err(|e| format!("writing {}: {e}", target.display()))
}

pub fn remove_file(dir: &Path, rel: &str) -> Result<(), String> {
    let target = resolve(dir, rel)?;
    std::fs::remove_file(&target).map_err(|e| format!("deleting {}: {e}", target.display()))
}

/// `dir/rel`, refusing paths that leave the folder.
fn resolve(dir: &Path, rel: &str) -> Result<PathBuf, String> {
    let mut out = dir.to_path_buf();
    for part in rel.split(['/', '\\']) {
        if part.is_empty() || part == "." || part == ".." || part.contains(':') {
            return Err(format!("`{rel}` is not a path inside the library folder"));
        }
        out.push(part);
    }
    Ok(out)
}

/// A JSON document holding `pieces`, in the format `parse_json` reads.
pub fn to_json(collection: Option<&str>, pieces: &[Value]) -> String {
    let doc = match collection {
        Some(c) => json!({ "collection": c, "pieces": pieces }),
        None => json!({ "pieces": pieces }),
    };
    serde_json::to_string_pretty(&doc).unwrap_or_default()
}

// ---------------------------------------------------------------------------------------------
// Choosing pieces for exercises
// ---------------------------------------------------------------------------------------------

/// Orders ids naturally: `ch2` before `ch10`.
pub fn natural_cmp(a: &str, b: &str) -> Ordering {
    let (mut x, mut y) = (a.chars().peekable(), b.chars().peekable());
    loop {
        match (x.peek().copied(), y.peek().copied()) {
            (None, None) => return Ordering::Equal,
            (None, _) => return Ordering::Less,
            (_, None) => return Ordering::Greater,
            (Some(c), Some(d)) if c.is_ascii_digit() && d.is_ascii_digit() => {
                let mut n = String::new();
                while let Some(c) = x.peek().copied().filter(char::is_ascii_digit) {
                    n.push(c);
                    x.next();
                }
                let mut m = String::new();
                while let Some(d) = y.peek().copied().filter(char::is_ascii_digit) {
                    m.push(d);
                    y.next();
                }
                let (n, m) = (n.trim_start_matches('0'), m.trim_start_matches('0'));
                let ord = n.len().cmp(&m.len()).then_with(|| n.cmp(m));
                if ord != Ordering::Equal {
                    return ord;
                }
            }
            (Some(c), Some(d)) => {
                let ord = c.to_lowercase().cmp(d.to_lowercase());
                if ord != Ordering::Equal {
                    return ord;
                }
                x.next();
                y.next();
            }
        }
    }
}

/// Whether a piece is in `collection` or one of its sub-collections.
pub fn in_collection(p: &LibraryPiece, collection: &str) -> bool {
    let c = collection.trim().trim_matches('/').to_lowercase();
    p.collection.as_deref().is_some_and(|pc| {
        let pc = pc.to_lowercase();
        pc == c || pc.starts_with(&format!("{c}/"))
    })
}

/// The pieces `sel` asks for, in library order (or the order of its ids), with at least
/// `min_parts` parts.
pub fn select<'a>(
    library: &'a [LibraryPiece],
    sel: &LibrarySelect,
    min_parts: usize,
) -> Result<Vec<&'a LibraryPiece>, String> {
    if !sel.ids.is_empty() {
        return sel
            .ids
            .iter()
            .map(|id| {
                let p = library.iter().find(|p| p.id == *id).ok_or_else(|| format!("the library has no piece `{id}`"))?;
                if p.parts.len() < min_parts {
                    return Err(format!("`{id}` has {} part(s); this exercise needs {min_parts}", p.parts.len()));
                }
                Ok(p)
            })
            .collect();
    }
    let out: Vec<&LibraryPiece> = library
        .iter()
        .filter(|p| sel.collection.as_deref().is_none_or(|c| in_collection(p, c)))
        .filter(|p| sel.tags.is_empty() || sel.tags.iter().any(|t| p.tags.iter().any(|x| x.eq_ignore_ascii_case(t))))
        .filter(|p| p.parts.len() >= min_parts)
        .collect();
    if out.is_empty() {
        let mut what = Vec::new();
        if let Some(c) = &sel.collection {
            what.push(format!("in collection `{c}`"));
        }
        if !sel.tags.is_empty() {
            what.push(format!("tagged {}", sel.tags.join(" or ")));
        }
        if min_parts > 1 {
            what.push(format!("with {min_parts} or more parts"));
        }
        return Err(if library.is_empty() {
            "the library is empty: add JSON or MIDI files to the library folder".into()
        } else {
            format!("no library pieces {}", if what.is_empty() { "match".into() } else { what.join(", ") })
        });
    }
    Ok(out)
}

/// The whole number of octaves that moves notes spanning `lo`–`hi` best into `range` (the
/// fewest semitones outside it, then the smallest move).
pub fn best_octave(lo: i32, hi: i32, range: NoteRange) -> i32 {
    let cost = |k: i32| (range.low.0 - (lo + 12 * k)).max(0) + ((hi + 12 * k) - range.high.0).max(0);
    (-3..=3).min_by_key(|k: &i32| (cost(*k), k.abs())).unwrap_or(0)
}

/// Moves a melody by `k` octaves; the key and spelling stay, and the clef is chosen again.
pub fn shift_octaves(m: &mut CompactMelody, k: i32) {
    if k == 0 {
        return;
    }
    for n in &mut m.notes {
        if let CompactNote::Note { note, .. } = n {
            note.octave += k as i8;
        }
    }
    m.clef = ClefChoice::Auto;
}

/// Lowest and highest notes of a melody.
pub fn melody_range(m: &CompactMelody) -> Option<(i32, i32)> {
    let midis = m.notes.iter().filter_map(|n| match n {
        CompactNote::Note { note, .. } => Some(note.midi()),
        CompactNote::Rest { .. } => None,
    });
    midis.fold(None, |acc, x| Some(acc.map_or((x, x), |(lo, hi): (i32, i32)| (lo.min(x), hi.max(x)))))
}

/// Moves a melody by whole octaves to sit best in `range`; the key and spelling stay. Returns
/// the number of octaves moved.
pub fn fit_octaves(m: &mut CompactMelody, range: NoteRange) -> i32 {
    let Some((lo, hi)) = melody_range(m) else { return 0 };
    let k = best_octave(lo, hi, range);
    shift_octaves(m, k);
    k
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pitches(part: &LibraryPart) -> Vec<String> {
        part.notes
            .iter()
            .map(|n| match n {
                CompactNote::Note { note, beats } => format!("{}:{}", note.name(), fmt_beats(*beats)),
                CompactNote::Rest { rest } => format!("R:{}", fmt_beats(*rest)),
            })
            .collect()
    }

    #[test]
    fn note_text_carries_durations_ties_and_checks_bars() {
        let v = json!({ "key": "F major", "time": "3/4", "notes": "C4:1 | F4 G4:.5 A4 Bb4:1~ | Bb4:2 R:1" });
        let mut p = parse_piece(&v).unwrap();
        p.id = "x".into();
        p.finish().unwrap();
        assert_eq!(
            pitches(&p.parts[0]),
            ["R:2", "C4:1", "F4:1", "G4:0.5", "A4:0.5", "Bb4:3", "R:1"]
        );
        let bad = json!({ "key": "C", "notes": "C4:1 D4 E4 F4 | G4:2 G4 G4 | C5:4" });
        let e = parse_piece(&bad).unwrap_err();
        assert!(e.contains("bar 2 has 6 beats"), "{e}");
        let tie = json!({ "key": "C", "notes": "C4:1~ D4:1" });
        assert!(parse_piece(&tie).unwrap_err().contains("tie"));
    }

    #[test]
    fn json_files_hold_one_piece_a_list_or_a_collection() {
        let one = r#"{"title": "Air", "key": "Eb", "notes": [{"note": "Eb4", "beats": 2}, {"rest": 2}]}"#;
        let p = read_file("Book/air.json", one.as_bytes()).unwrap();
        assert_eq!(p.len(), 1);
        assert_eq!((p[0].id.as_str(), p[0].title.as_str(), p[0].key.as_str()), ("Book/air", "Air", "Eb major"));
        assert_eq!(p[0].collection.as_deref(), Some("Book"));

        let coll = r#"{"collection": "Duets", "tags": ["two-part"], "pieces": [
            {"key": "G major", "parts": [{"name": "Upper", "notes": "B4:2 C5:2 | D5:4"},
                                         {"name": "Lower", "notes": "G4:2 A4:2"}]},
            {"id": "duet-2", "key": "D major", "notes": "D4 E4 F#4 G4"}]}"#;
        let p = read_file("set.json", coll.as_bytes()).unwrap();
        assert_eq!(p.iter().map(|x| x.id.as_str()).collect::<Vec<_>>(), ["set#1", "duet-2"]);
        assert_eq!(p[0].collection.as_deref(), Some("Duets"));
        assert_eq!(p[0].tags, ["two-part"]);
        // The shorter part is padded to the same length.
        assert_eq!(pitches(&p[0].parts[1]), ["G4:2", "A4:2", "R:4"]);
        assert_eq!(p[1].scores().unwrap()[0].bars, 1);

        let e = read_file("bad.json", br#"{"key": "C", "notes": [{"note": "H4", "beats": 1}]}"#).unwrap_err();
        assert!(e.contains("notes: [0]"), "{e}");
    }

    /// A type-1 MIDI file: a conductor track and one or two note tracks.
    fn midi_file(tracks: &[(&str, &[(u32, u32, u8)])], key: Option<(i8, bool)>, sigs: &[(u32, u8, u8)]) -> Vec<u8> {
        use midly::num::{u4, u7, u15, u24, u28};
        use midly::{Format, Header, MetaMessage, MidiMessage, Smf, Timing, TrackEvent, TrackEventKind};
        let mut conductor: Vec<(u32, TrackEventKind)> = vec![
            (0, TrackEventKind::Meta(MetaMessage::TrackName(b"Exercise 4"))),
            (0, TrackEventKind::Meta(MetaMessage::Tempo(u24::new(1_000_000)))),
        ];
        for &(t, n, d) in sigs {
            conductor.push((t, TrackEventKind::Meta(MetaMessage::TimeSignature(n, d, 24, 8))));
        }
        if let Some((sf, minor)) = key {
            conductor.push((0, TrackEventKind::Meta(MetaMessage::KeySignature(sf, minor))));
        }
        let to_track = |mut evs: Vec<(u32, TrackEventKind<'static>)>| {
            evs.sort_by_key(|e| e.0);
            let mut last = 0;
            let mut out: Vec<TrackEvent> = evs
                .into_iter()
                .map(|(t, kind)| {
                    let delta = t - last;
                    last = t;
                    TrackEvent { delta: u28::new(delta), kind }
                })
                .collect();
            out.push(TrackEvent { delta: u28::new(0), kind: TrackEventKind::Meta(MetaMessage::EndOfTrack) });
            out
        };
        let mut all = vec![to_track(conductor)];
        for (name, notes) in tracks {
            let name: &'static [u8] = Box::leak(name.as_bytes().to_vec().into_boxed_slice());
            let mut evs = vec![(0, TrackEventKind::Meta(MetaMessage::TrackName(name)))];
            for &(s, e, key) in notes.iter() {
                let ch = u4::new(0);
                evs.push((s, TrackEventKind::Midi { channel: ch, message: MidiMessage::NoteOn { key: u7::new(key), vel: u7::new(80) } }));
                evs.push((e, TrackEventKind::Midi { channel: ch, message: MidiMessage::NoteOff { key: u7::new(key), vel: u7::new(0) } }));
            }
            all.push(to_track(evs));
        }
        let smf = Smf { header: Header::new(Format::Parallel, Timing::Metrical(u15::new(480))), tracks: all };
        let mut bytes = Vec::new();
        smf.write_std(&mut bytes).unwrap();
        bytes
    }

    #[test]
    fn midi_parts_metre_pickup_and_key() {
        // A pickup quarter (2/4 bar of 1/4), then 3/4; slightly detached notes; key from notes.
        let upper: &[(u32, u32, u8)] = &[(0, 450, 67), (480, 950, 72), (960, 1430, 74), (1440, 1900, 76), (1920, 3300, 72)];
        let lower: &[(u32, u32, u8)] = &[(480, 1400, 60), (1440, 1900, 64), (1920, 3300, 60)];
        let bytes = midi_file(&[("Alto", lower), ("Soprano", upper)], None, &[(0, 1, 2), (480, 3, 2)]);
        let p = read_file("Folk/ex4.mid", &bytes).unwrap().remove(0);
        assert_eq!((p.title.as_str(), p.key.as_str(), p.time.to_string().as_str()), ("Exercise 4", "C major", "3/4"));
        assert_eq!(p.tempo_bpm, Some(60.0));
        assert_eq!(p.parts[0].name.as_deref(), Some("Soprano"));
        assert_eq!(pitches(&p.parts[0]), ["R:2", "G4:1", "C5:1", "D5:1", "E5:1", "C5:3"]);
        assert_eq!(pitches(&p.parts[1]), ["R:3", "C4:2", "E4:1", "C4:3"]);

        // A key signature wins; chords keep their top note.
        let chords: &[(u32, u32, u8)] = &[(0, 480, 62), (0, 480, 66), (480, 960, 69), (480, 960, 64)];
        let bytes = midi_file(&[("Piano", chords)], Some((2, false)), &[(0, 4, 2)]);
        let p = read_file("x.mid", &bytes).unwrap().remove(0);
        assert_eq!(p.key, "D major");
        assert_eq!(pitches(&p.parts[0]), ["F#4:1", "A4:1"]);
    }

    #[test]
    fn keys_are_estimated_from_notes() {
        let g_minor = [67, 69, 70, 72, 74, 75, 78, 79, 74, 70, 67];
        let notes: Vec<(i32, f32)> = g_minor.iter().map(|m| (*m, 1.0)).collect();
        assert_eq!(estimate_key(&notes, Some(67)).to_string(), "G minor");
        let a_major = [69, 71, 73, 74, 76, 78, 80, 81, 76, 73, 69];
        let notes: Vec<(i32, f32)> = a_major.iter().map(|m| (*m, 1.0)).collect();
        assert_eq!(estimate_key(&notes, Some(69)).to_string(), "A major");
    }

    #[test]
    fn selection_ordering_and_octave_fitting() {
        let lib: Vec<LibraryPiece> = ["B/ex10", "B/ex2", "A/one"]
            .iter()
            .map(|id| {
                let mut p = parse_piece(&json!({ "key": "C", "notes": "C5:4" })).unwrap();
                p.id = id.to_string();
                p.collection = id.rsplit_once('/').map(|(c, _)| c.to_string());
                p
            })
            .collect();
        let mut sorted = lib.clone();
        sorted.sort_by(|a, b| natural_cmp(&a.id, &b.id));
        assert_eq!(sorted.iter().map(|p| p.id.as_str()).collect::<Vec<_>>(), ["A/one", "B/ex2", "B/ex10"]);
        let sel = LibrarySelect { collection: Some("b".into()), ..LibrarySelect::default() };
        assert_eq!(select(&sorted, &sel, 1).unwrap().len(), 2);
        assert!(select(&sorted, &sel, 2).unwrap_err().contains("2 or more parts"));
        let sel = LibrarySelect { ids: vec!["A/one".into(), "nope".into()], ..LibrarySelect::default() };
        assert!(select(&sorted, &sel, 1).unwrap_err().contains("`nope`"));

        let mut m = lib[0].melody(0);
        assert_eq!(fit_octaves(&mut m, NoteRange::new(43, 64)), -1);
        assert!(matches!(m.notes[0], CompactNote::Note { note, .. } if note.name() == "C4"));
    }

    #[test]
    fn paths_stay_inside_the_folder() {
        assert_eq!(file_path(Some("Ottman/Ch 3"), "12.json").unwrap(), "Ottman/Ch 3/12.json");
        assert_eq!(file_path(Some("../x"), "a?.mid").unwrap(), "x/a_.mid");
        assert!(file_path(None, "notes.txt").is_err());
        assert!(resolve(Path::new("/lib"), "../etc/passwd").is_err());
    }
}
