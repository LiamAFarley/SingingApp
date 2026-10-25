//! Session and exercise definitions (written by the LLM, presets or the user) and the concrete
//! trial specs the app runs.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::melody::MelodyParams;
use crate::score::{CompactMelody, Pitch, Score};
use crate::theory::{ChordQuality, Interval, Note, PitchClass, ScaleKind};
use crate::timbre::TimbreId;

// ---------------------------------------------------------------------------------------------
// Shared enums
// ---------------------------------------------------------------------------------------------

/// When the singer or listener sees whether they were right.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "snake_case")]
pub enum FeedbackMode {
    /// Live pitch line visible during sung attempts, result after each trial.
    Continuous,
    /// Result after each trial.
    #[default]
    AfterTrial,
    /// Results only at the end of the block.
    AfterBlock,
    /// Results only at the end of the session.
    EndOfSession,
    /// No results shown (assessment).
    None,
    /// A wrong answer is not marked wrong. The right answer lights up and is played, and the
    /// stored result is still the true one.
    Errorless,
}

impl FeedbackMode {
    pub fn shows_trial_result(self) -> bool {
        matches!(
            self,
            FeedbackMode::Continuous | FeedbackMode::AfterTrial | FeedbackMode::Errorless
        )
    }

    pub fn is_errorless(self) -> bool {
        matches!(self, FeedbackMode::Errorless)
    }

    pub fn as_str(self) -> &'static str {
        match self {
            FeedbackMode::Continuous => "continuous",
            FeedbackMode::AfterTrial => "after_trial",
            FeedbackMode::AfterBlock => "after_block",
            FeedbackMode::EndOfSession => "end_of_session",
            FeedbackMode::None => "none",
            FeedbackMode::Errorless => "errorless",
        }
    }
}

/// Which stimuli a block may draw. Holdout stimuli are reserved for assessment.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "snake_case")]
pub enum Pool {
    #[default]
    Training,
    Holdout,
    Any,
}

impl Pool {
    pub fn as_str(self) -> &'static str {
        match self {
            Pool::Training => "training",
            Pool::Holdout => "holdout",
            Pool::Any => "any",
        }
    }
}

/// Sound played between trials so the next trial cannot be solved by relative pitch. Off unless
/// asked for: a 13-semitone jump rule and an answer limit guard against that without the wait.
/// A burst of random tones still interferes with pitch memory (Deutsch 1970), so it stays where
/// nothing else can, such as notes in one octave or Shepard tones.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "snake_case")]
pub enum Mask {
    #[default]
    None,
    Noise,
    /// Six quick random tones, under a second.
    RandomTones,
    ChromaticCluster,
    /// A 20-second, endlessly descending Shepard–Risset glissando that clears echoic and tonal
    /// memory before a block answered without help.
    ShepardGlissando,
}

impl Mask {
    pub fn as_str(self) -> &'static str {
        match self {
            Mask::None => "none",
            Mask::Noise => "noise",
            Mask::RandomTones => "random_tones",
            Mask::ChromaticCluster => "chromatic_cluster",
            Mask::ShepardGlissando => "shepard_glissando",
        }
    }

    /// About how long the mask lasts, in seconds.
    pub fn seconds(self) -> f32 {
        match self {
            Mask::None => 0.0,
            Mask::ShepardGlissando => SHEPARD_GLISSANDO_S,
            Mask::RandomTones => 0.9,
            _ => 1.5,
        }
    }
}

pub const SHEPARD_GLISSANDO_S: f32 = 20.0;

/// When tones arrive in a naming block.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "snake_case")]
pub enum Pacing {
    /// The next tone follows your answer.
    #[default]
    Respond,
    /// A tone every `ioi_ms` whether or not you answered (Brady's paced streams). An answer is
    /// accepted until shortly before the next tone.
    Stream { ioi_ms: u32 },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "snake_case")]
pub enum AllKeyword {
    All,
}

/// `"all"` or a list of pitch classes such as `["C", "E", "G#"]`.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(untagged)]
pub enum ClassSet {
    Keyword(AllKeyword),
    List(Vec<PitchClass>),
}

impl Default for ClassSet {
    fn default() -> Self {
        ClassSet::Keyword(AllKeyword::All)
    }
}

impl ClassSet {
    pub fn classes(&self) -> Vec<PitchClass> {
        match self {
            ClassSet::Keyword(AllKeyword::All) => PitchClass::ALL.to_vec(),
            ClassSet::List(v) => {
                let mut out: Vec<PitchClass> = Vec::new();
                for pc in v {
                    if !out.contains(pc) {
                        out.push(*pc);
                    }
                }
                out
            }
        }
    }

    pub fn is_all(&self) -> bool {
        self.classes().len() == 12
    }
}

/// Inclusive note range, e.g. `{"low": "C3", "high": "G4"}`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema, TS)]
pub struct NoteRange {
    pub low: Note,
    pub high: Note,
}

impl NoteRange {
    pub fn new(low: i32, high: i32) -> Self {
        NoteRange {
            low: Note(low.min(high)),
            high: Note(low.max(high)),
        }
    }

    pub fn contains(&self, midi: i32) -> bool {
        midi >= self.low.0 && midi <= self.high.0
    }

    pub fn span(&self) -> i32 {
        self.high.0 - self.low.0
    }
}

/// How sung pitches are compared with the targets.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "snake_case")]
pub enum PitchRefMode {
    /// Error against the exact target pitch.
    Absolute,
    /// Against the exact pitches, but the whole line may be sung a whole number of octaves
    /// higher or lower (a man reading treble clef an octave down).
    AnyOctave,
    /// Absolute, octave ignored.
    PitchClass,
    /// Targets re-anchored to the singer's first note.
    RelativeToFirst,
    /// A global offset and drift are fitted and reported separately from note errors.
    RelativeDriftTolerant,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "snake_case")]
pub enum Tuning {
    #[default]
    Equal,
    Just,
}

/// Per-block grading overrides. Unset values use the exercise defaults.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize, JsonSchema, TS)]
pub struct GradingOverrides {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mode: Option<PitchRefMode>,
    /// A sung note within this many cents of the target counts as the right note (default 50).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub category_tol_cents: Option<f32>,
    /// Within this many cents counts as in tune (default 20).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub in_tune_tol_cents: Option<f32>,
    /// Band the note centre must stay inside to count as settled (default 25).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stable_tol_cents: Option<f32>,
    /// How long the centre must stay inside the band to count as settled (default 400 ms).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub hold_ms: Option<u32>,
    /// Onset tolerance for timed exercises (default 120 ms).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub onset_tol_ms: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tuning: Option<Tuning>,
}

/// What plays alongside a sung multi-note exercise.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "snake_case")]
pub enum Backing {
    None,
    #[default]
    Click,
    Drone,
    Chords,
    Melody,
    MelodyAndChords,
}

impl Backing {
    pub fn has_click(self) -> bool {
        !matches!(self, Backing::None | Backing::Drone)
    }

    pub fn has_melody(self) -> bool {
        matches!(self, Backing::Melody | Backing::MelodyAndChords)
    }

    pub fn has_chords(self) -> bool {
        matches!(self, Backing::Chords | Backing::MelodyAndChords)
    }
}

/// How the target is presented before or during singing.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "snake_case")]
pub enum ReferenceMode {
    /// The target is played first, then the singer repeats it.
    #[default]
    Echo,
    /// The target plays along while the singer sings.
    Guided,
    /// Only the first note is played as a starting pitch.
    FirstNote,
    /// Nothing is played (no-reference singing).
    None,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "snake_case")]
pub enum LaneTargets {
    #[default]
    Shown,
    HiddenUntilDone,
}

fn d_tempo() -> f32 {
    80.0
}
fn d_beats_per_bar() -> u32 {
    4
}
fn d_one() -> u32 {
    1
}
fn d_backing_volume() -> f32 {
    0.7
}
fn d_ref_timbre() -> TimbreId {
    "gm:0".to_string()
}

/// Shared options for sung exercises with more than one note.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct SungOptions {
    #[serde(default = "d_tempo")]
    pub tempo_bpm: f32,
    #[serde(default = "d_beats_per_bar")]
    pub beats_per_bar: u32,
    #[serde(default = "d_one")]
    pub count_in_bars: u32,
    #[serde(default)]
    pub backing: Backing,
    #[serde(default = "d_backing_volume")]
    pub backing_volume: f32,
    /// Defaults to `echo` for patterns and `first_note` for interval singing.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reference: Option<ReferenceMode>,
    #[serde(default)]
    pub lane_targets: LaneTargets,
    /// Timbre of the reference, guide melody and chords.
    #[serde(default = "d_ref_timbre")]
    pub timbre: TimbreId,
}

impl Default for SungOptions {
    fn default() -> Self {
        SungOptions {
            tempo_bpm: d_tempo(),
            beats_per_bar: d_beats_per_bar(),
            count_in_bars: d_one(),
            backing: Backing::default(),
            backing_volume: d_backing_volume(),
            reference: None,
            lane_targets: LaneTargets::default(),
            timbre: d_ref_timbre(),
        }
    }
}

// ---------------------------------------------------------------------------------------------
// Exercise parameters
// ---------------------------------------------------------------------------------------------

fn d_octaves() -> Vec<i32> {
    vec![3, 4, 5]
}
fn d_ms_1000() -> u32 {
    1000
}
fn d_ms_1500() -> u32 {
    1500
}
fn d_ms_4000() -> u32 {
    4000
}
fn d_true() -> bool {
    true
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "snake_case")]
pub enum PitchAnswer {
    #[default]
    PitchClass,
    NoteWithOctave,
}

/// Name a single tone with no reference (absolute pitch identification).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct PitchIdParams {
    /// The notes played and offered as answers: all twelve, or a subset for category training.
    #[serde(default)]
    pub classes: ClassSet,
    #[serde(default = "d_octaves")]
    pub octaves: Vec<i32>,
    /// Empty uses the default training (or holdout) timbres.
    #[serde(default)]
    pub timbres: Vec<TimbreId>,
    #[serde(default)]
    pub pool: Pool,
    #[serde(default = "d_ms_1000")]
    pub duration_ms: u32,
    /// Random loudness range in dB relative to full level, e.g. [-12, -3].
    #[serde(default)]
    pub loudness_db: Option<[f32; 2]>,
    #[serde(default)]
    pub deadline_ms: Option<u32>,
    #[serde(default)]
    pub mask: Mask,
    #[serde(default)]
    pub answer: PitchAnswer,
    #[serde(default)]
    pub detune_cents: f32,
    /// Each tone is at least this many semitones from the previous one, so it cannot be named by
    /// its interval from the last (labelled) tone. 13 needs at least three octaves.
    #[serde(default)]
    pub min_jump_semitones: u32,
    /// Each tone is at least this many octaves (by octave number) from the previous one. At 2 every
    /// note clears 13 semitones whatever its name, so the octave a tone lands in says nothing
    /// about its name, as it can under `min_jump_semitones` alone. Needs four or more octaves;
    /// where a timbre cannot step that far, `min_jump_semitones` decides.
    #[serde(default)]
    pub min_octave_step: u32,
    /// For a subset: this share of tones comes from outside it, at the edges of the range, and is
    /// answered "Other".
    #[serde(default)]
    pub out_of_set_rate: f32,
    /// Relative weights per pitch class, e.g. `{"C": 0.4}` (Brady's anchor). Classes not listed
    /// share what is left equally.
    #[serde(default, skip_serializing_if = "std::collections::BTreeMap::is_empty")]
    pub class_weights: std::collections::BTreeMap<String, f32>,
    #[serde(default)]
    pub pacing: Pacing,
    /// Label set id (`note_names`, `taneda_colors`, `gervain_names`, `my_colors`, or a saved set).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub labels: Option<String>,
    /// Tint the screen with the played note's colour after the answer.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub color_flash: bool,
    /// Buttons show the colour and word only, with no letter name.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub hide_note_names: bool,
    /// A missed class is asked again later in the block, in another octave when the block has one.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub requeue_misses: bool,
}

impl Default for PitchIdParams {
    fn default() -> Self {
        serde_json::from_str("{}").unwrap()
    }
}

/// "Sing E-flat" with no reference tone (absolute pitch production).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct SingPitchParams {
    #[serde(default)]
    pub classes: ClassSet,
    /// When set, the target is that exact octave and is graded absolute; otherwise octave is ignored.
    #[serde(default)]
    pub octave: Option<i32>,
    #[serde(default = "d_ms_4000")]
    pub response_ms: u32,
    #[serde(default)]
    pub delay_ms: u32,
    #[serde(default)]
    pub pre_mask: Mask,
    #[serde(default = "d_true")]
    pub reveal_target_after: bool,
    #[serde(default = "d_ref_timbre")]
    pub reveal_timbre: TimbreId,
}

impl Default for SingPitchParams {
    fn default() -> Self {
        serde_json::from_str("{}").unwrap()
    }
}

/// Passive exposure: tone, pause, label. No response; never counted as AP evidence.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct PitchPairingParams {
    #[serde(default)]
    pub classes: ClassSet,
    #[serde(default = "d_octaves")]
    pub octaves: Vec<i32>,
    #[serde(default)]
    pub timbres: Vec<TimbreId>,
    #[serde(default)]
    pub pool: Pool,
    #[serde(default = "d_ms_1000")]
    pub duration_ms: u32,
    #[serde(default = "d_ms_1500")]
    pub gap_ms: u32,
    /// Label set id. The letter name is used when this is omitted.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub labels: Option<String>,
    /// Tint the screen with the note's colour while its label is shown.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub color_flash: bool,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub hide_note_names: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "snake_case")]
pub enum KeyMode {
    Major,
    Minor,
}

fn d_major() -> Vec<KeyMode> {
    vec![KeyMode::Major]
}

/// Hear a cadence in a random key and name the tonic. Without a reference this is absolute
/// pitch in context; with `reference: true` a labelled A4 is played first (relative tonic finding).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct TonalCenterIdParams {
    #[serde(default)]
    pub reference: bool,
    #[serde(default)]
    pub keys: ClassSet,
    #[serde(default = "d_major")]
    pub modes: Vec<KeyMode>,
    #[serde(default)]
    pub timbres: Vec<TimbreId>,
    #[serde(default)]
    pub mask: Mask,
    #[serde(default = "d_tempo")]
    pub tempo_bpm: f32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "snake_case")]
pub enum IntervalDirection {
    Ascending,
    Descending,
    Harmonic,
}

fn d_simple_intervals() -> Vec<Interval> {
    Interval::SIMPLE.to_vec()
}
fn d_melodic_dirs() -> Vec<IntervalDirection> {
    vec![IntervalDirection::Ascending, IntervalDirection::Descending]
}
fn d_ms_900() -> u32 {
    900
}

/// When the key of a key-based exercise changes.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "snake_case")]
pub enum KeyChange {
    /// One key for the whole block (picked from `keys`).
    #[default]
    PerBlock,
    /// A new key for every question.
    PerTrial,
}

fn d_tonic_c() -> ClassSet {
    ClassSet::List(vec![PitchClass::new(0)])
}

/// Interval identification. Without `in_key` intervals stand alone on random roots; with it a
/// cadence sets a key and both notes come from its scale (diatonic intervals).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct IntervalIdParams {
    #[serde(default = "d_simple_intervals")]
    pub intervals: Vec<Interval>,
    #[serde(default = "d_melodic_dirs")]
    pub directions: Vec<IntervalDirection>,
    /// Range for the lower note. Default C3–C5 (ignored with `in_key`).
    #[serde(default)]
    pub root_range: Option<NoteRange>,
    #[serde(default)]
    pub timbres: Vec<TimbreId>,
    #[serde(default = "d_ms_900")]
    pub note_ms: u32,
    /// Play the intervals inside a key, after a cadence, with both notes from its scale.
    #[serde(default)]
    pub in_key: bool,
    /// Tonics for `in_key` (default C).
    #[serde(default = "d_tonic_c")]
    pub keys: ClassSet,
    #[serde(default = "d_major")]
    pub modes: Vec<KeyMode>,
    #[serde(default)]
    pub key_change: KeyChange,
    /// When the cadence plays (with `in_key`). Default: once at the start in one key, on every
    /// key change when the key changes.
    #[serde(default)]
    pub cadence: Option<CadencePlan>,
    /// Older form of `cadence: every` (true).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cadence_each_trial: Option<bool>,
    /// Cadence tempo (default 100).
    #[serde(default)]
    pub cadence_bpm: Option<f32>,
    /// Where the first note heard comes from: anywhere (`random`), the key's tonic (`tonic`,
    /// "from do"; `start_note` without a key) or `start_note` (`fixed`).
    #[serde(default)]
    pub start: IntervalStart,
    /// The fixed first note (default C4).
    #[serde(default)]
    pub start_note: Option<Note>,
    /// Octaves the first note may move by, chosen per question (default [0]).
    #[serde(default = "d_zero_octaves")]
    pub start_octaves: Vec<i32>,
    /// In a key: the second note may leave the scale, for non-diatonic intervals (a minor third
    /// above the tonic of a major key).
    #[serde(default)]
    pub chromatic: bool,
    /// Share of questions (0–0.5) playing an interval from outside the set, answered Other, so a
    /// set of one interval is still a question.
    #[serde(default)]
    pub other_rate: f32,
    /// Intervals Other is drawn from (default: every interval up to an octave not in the set).
    #[serde(default)]
    pub other_intervals: Vec<Interval>,
    /// How the two notes are named after answering, in a key.
    #[serde(default)]
    pub labels: DegreeLabels,
}

/// Where an interval's first note comes from.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "snake_case")]
pub enum IntervalStart {
    #[default]
    Random,
    Tonic,
    Fixed,
}

fn d_zero_octaves() -> Vec<i32> {
    vec![0]
}

/// Which note two compared intervals share.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "snake_case")]
pub enum SharedNote {
    /// Unrelated starting notes.
    #[default]
    None,
    /// Both start on the same note.
    First,
    /// Both end on the same note.
    Second,
    /// They start a semitone to a minor third apart.
    Near,
}

fn d_compare_diffs() -> Vec<u32> {
    vec![0, 1, 2]
}

fn d_ms_700() -> u32 {
    700
}

/// Interval comparison: two intervals play; say which is larger, or that they are the same.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct IntervalComparisonParams {
    /// Intervals the first one is drawn from.
    #[serde(default = "d_simple_intervals")]
    pub intervals: Vec<Interval>,
    /// Size differences in semitones between the two (0 means the same interval).
    #[serde(default = "d_compare_diffs")]
    pub differences: Vec<u32>,
    #[serde(default = "d_melodic_dirs")]
    pub directions: Vec<IntervalDirection>,
    /// Range for the lower notes. Default C3–C5.
    #[serde(default)]
    pub root_range: Option<NoteRange>,
    #[serde(default)]
    pub timbres: Vec<TimbreId>,
    #[serde(default = "d_ms_700")]
    pub note_ms: u32,
    /// Which note the two intervals share, picked per question from this list. Sharing a note
    /// makes the comparison easier; `none` is the hardest.
    #[serde(default = "d_shared_none")]
    pub shared: Vec<SharedNote>,
}

fn d_shared_none() -> Vec<SharedNote> {
    vec![SharedNote::None]
}

/// How a scale degree moves to the tonic after it is answered.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "snake_case")]
pub enum Resolution {
    /// To the nearer tonic: 2, 3 and 4 step down, 5, 6 and 7 step up.
    #[default]
    Nearest,
    Ascending,
    Descending,
    None,
}

/// Answer labels for scale degrees.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "snake_case")]
pub enum DegreeLabels {
    /// 1, 2, ♭3 … numbered against the major scale.
    Numbers,
    /// Movable do with do-based minor: the tonic is always do (do re me fa sol le te).
    Solfege,
    /// Fixed do: C is always do, so the buttons name the key's actual notes (in G major: sol la
    /// si do re mi fa♯ sol), with the degree number beside each.
    #[default]
    FixedDo,
}

/// When a scale-degree exercise plays its key-setting cadence.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "snake_case")]
pub enum CadencePlan {
    /// Once, before the first question.
    Start,
    /// Before the first question and whenever the key changes.
    KeyChange,
    /// Before every question.
    Every,
}

fn d_degree_octaves() -> Vec<i32> {
    vec![0]
}

fn d_degree_tempo() -> f32 {
    100.0
}

/// Scale degrees, as in Functional Ear Trainer: a cadence sets the key, a note plays, name its
/// degree; then the note walks diatonically to the tonic so its function is heard.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct ScaleDegreeParams {
    /// Tonics (default C, a fixed key).
    #[serde(default = "d_tonic_c")]
    pub keys: ClassSet,
    #[serde(default = "d_major")]
    pub modes: Vec<KeyMode>,
    #[serde(default)]
    pub key_change: KeyChange,
    /// Degrees asked, numbered against the major scale: "1", "2", "b3", "#4", "7" …
    /// Default: the seven degrees of the mode. Fewer degrees make early levels.
    #[serde(default)]
    pub degrees: Vec<String>,
    /// When the cadence plays. Default: once at the start in one key, on every key change when
    /// the key changes.
    #[serde(default)]
    pub cadence: Option<CadencePlan>,
    /// Older form of `cadence: every` (true) or the default (false).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cadence_each_trial: Option<bool>,
    /// Cadence tempo; the question's own tempo (`tempo_bpm`) when not set.
    #[serde(default)]
    pub cadence_bpm: Option<f32>,
    /// Octaves of the note: 0 is the octave from the tonic up, -1 the one below.
    #[serde(default = "d_degree_octaves")]
    pub octaves: Vec<i32>,
    /// Answers run 1–8: the tonic above the cadence's tonic is 8, so the octave is named too.
    #[serde(default = "d_true")]
    pub upper_tonic: bool,
    #[serde(default)]
    pub resolution: Resolution,
    /// Also play the resolution before answering (a learning aid for new degrees).
    #[serde(default)]
    pub resolve_before_answer: bool,
    #[serde(default)]
    pub labels: DegreeLabels,
    #[serde(default)]
    pub timbres: Vec<TimbreId>,
    #[serde(default = "d_degree_tempo")]
    pub tempo_bpm: f32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "snake_case")]
pub enum ChordStyle {
    #[default]
    Block,
    Arpeggio,
    BlockThenArpeggio,
}

fn d_triads() -> Vec<ChordQuality> {
    ChordQuality::TRIADS.to_vec()
}
fn d_root_position() -> Vec<u8> {
    vec![0]
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct ChordIdParams {
    #[serde(default = "d_triads")]
    pub qualities: Vec<ChordQuality>,
    /// Inversions to use (0 = root position).
    #[serde(default = "d_root_position")]
    pub inversions: Vec<u8>,
    /// Also ask for the inversion.
    #[serde(default)]
    pub answer_inversion: bool,
    #[serde(default)]
    pub root_range: Option<NoteRange>,
    #[serde(default)]
    pub timbres: Vec<TimbreId>,
    #[serde(default)]
    pub style: ChordStyle,
    #[serde(default = "d_ms_1500")]
    pub duration_ms: u32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "snake_case")]
pub enum ScaleDirection {
    #[default]
    Ascending,
    Descending,
    UpDown,
}

fn d_scales() -> Vec<ScaleKind> {
    vec![
        ScaleKind::Major,
        ScaleKind::NaturalMinor,
        ScaleKind::HarmonicMinor,
        ScaleKind::MelodicMinor,
    ]
}
fn d_scale_tempo() -> f32 {
    160.0
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct ScaleIdParams {
    #[serde(default = "d_scales")]
    pub scales: Vec<ScaleKind>,
    #[serde(default)]
    pub direction: ScaleDirection,
    #[serde(default = "d_scale_tempo")]
    pub tempo_bpm: f32,
    #[serde(default)]
    pub root_range: Option<NoteRange>,
    #[serde(default)]
    pub timbres: Vec<TimbreId>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "snake_case")]
pub enum MatchMode {
    /// Reference first, then sing.
    #[default]
    After,
    /// Sing together with the reference.
    Unison,
}

fn d_ms_1200() -> u32 {
    1200
}
fn d_ms_3000() -> u32 {
    3000
}

/// Hear a note, sing it back.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct PitchMatchParams {
    /// Default: the comfortable range from the range profile.
    #[serde(default)]
    pub range: Option<NoteRange>,
    #[serde(default)]
    pub timbres: Vec<TimbreId>,
    #[serde(default = "d_ms_1200")]
    pub reference_ms: u32,
    #[serde(default = "d_ms_3000")]
    pub response_ms: u32,
    #[serde(default)]
    pub mode: MatchMode,
}

/// Hold a note steadily; reports centre error, drift, stability and vibrato.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct SustainParams {
    #[serde(default)]
    pub range: Option<NoteRange>,
    #[serde(default = "d_ms_4000")]
    pub hold_ms: u32,
    /// Play the target as a drone while holding.
    #[serde(default = "d_true")]
    pub drone: bool,
    #[serde(default = "d_ref_timbre")]
    pub timbre: TimbreId,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "snake_case")]
pub enum SingDirection {
    Up,
    Down,
}

fn d_sing_intervals() -> Vec<Interval> {
    vec![
        Interval::Maj2,
        Interval::Maj3,
        Interval::P4,
        Interval::P5,
        Interval::Maj6,
        Interval::P8,
    ]
}
fn d_sing_dirs() -> Vec<SingDirection> {
    vec![SingDirection::Up, SingDirection::Down]
}

/// The first note is given; sing both notes of the interval in time.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct SingIntervalParams {
    #[serde(default = "d_sing_intervals")]
    pub intervals: Vec<Interval>,
    #[serde(default = "d_sing_dirs")]
    pub directions: Vec<SingDirection>,
    #[serde(default)]
    pub range: Option<NoteRange>,
    /// Sing inside a key: a cadence first, then the first note, named by `labels`.
    #[serde(default)]
    pub in_key: bool,
    #[serde(default = "d_tonic_c")]
    pub keys: ClassSet,
    #[serde(default = "d_major")]
    pub modes: Vec<KeyMode>,
    #[serde(default)]
    pub key_change: KeyChange,
    #[serde(default)]
    pub cadence: Option<CadencePlan>,
    /// In a key: start from the tonic (`tonic`) or any note of the scale (`random`).
    #[serde(default)]
    pub start: IntervalStart,
    /// In a key: the sung note may leave the scale.
    #[serde(default)]
    pub chromatic: bool,
    #[serde(default)]
    pub labels: DegreeLabels,
    #[serde(flatten)]
    pub sung: SungOptions,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "snake_case")]
pub enum Shape {
    #[default]
    Up,
    Down,
    UpDown,
}

fn d_one_u8() -> u8 {
    1
}
fn d_major_scale() -> ScaleKind {
    ScaleKind::Major
}

/// A sung pattern, transposed for each trial.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum PatternSpec {
    Scale {
        kind: ScaleKind,
        #[serde(default)]
        shape: Shape,
        #[serde(default = "d_one_u8")]
        octaves: u8,
    },
    Arpeggio {
        quality: ChordQuality,
        #[serde(default)]
        shape: Shape,
    },
    /// Scale degrees, 1 = tonic; 8 is the octave and 0 or negatives go below.
    Degrees {
        degrees: Vec<i32>,
        #[serde(default = "d_major_scale")]
        scale: ScaleKind,
    },
    /// Exact notes; transposed so the first note is the trial's tonic unless `keys` is `fixed`.
    Notes { notes: Vec<Note> },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "snake_case")]
pub enum KeyPlan {
    /// A random tonic that fits the range for every trial.
    #[default]
    Random,
    /// Start low and move up a semitone per trial (warm-up style).
    Ascending,
    /// Start high and move down a semitone per trial.
    Descending,
    /// Always start on `start` (or the pattern's own notes).
    Fixed,
}

fn d_beats_per_note() -> f32 {
    1.0
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct PatternParams {
    pub pattern: PatternSpec,
    #[serde(default)]
    pub range: Option<NoteRange>,
    #[serde(default)]
    pub keys: KeyPlan,
    /// Tonic of the first trial (or of every trial when `keys` is `fixed`).
    #[serde(default)]
    pub start: Option<Note>,
    #[serde(default = "d_beats_per_note")]
    pub beats_per_note: f32,
    #[serde(flatten)]
    pub sung: SungOptions,
}

/// What is played before a sight-singing attempt.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "snake_case")]
pub enum SightStart {
    /// A cadence establishes the key, then the first note sounds.
    #[default]
    KeyAndFirstNote,
    /// Only the cadence: find the first note from the key.
    KeyOnly,
    /// Only the first note.
    FirstNote,
    /// Nothing: absolute-pitch sight singing (or relative grading from the first sung note).
    None,
}

impl SightStart {
    pub fn plays_key(self) -> bool {
        matches!(self, SightStart::KeyAndFirstNote | SightStart::KeyOnly)
    }

    pub fn plays_first_note(self) -> bool {
        matches!(self, SightStart::KeyAndFirstNote | SightStart::FirstNote)
    }
}

/// Where sight-singing and dictation melodies come from.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum MelodySource {
    /// A new generated melody for every trial.
    Generate(MelodyParams),
    /// Melodies written by the coach; trial i uses melody i modulo their number.
    Given(Vec<CompactMelody>),
    /// Pieces from the melody library (JSON and MIDI files in the library folder), in turn.
    Library(LibrarySelect),
}

/// Which library pieces an exercise uses: by id, or every piece in a collection or with a tag.
/// With nothing set, the whole library.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct LibrarySelect {
    /// Piece ids (see list_library), used in this order.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub ids: Vec<String>,
    /// A collection (a folder of the library); its sub-collections are included.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub collection: Option<String>,
    /// Pieces with any of these tags.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tags: Vec<String>,
    /// Shuffle instead of library order (natural order of ids).
    #[serde(default)]
    pub shuffle: bool,
    /// The part of multi-part pieces to sing or write (0 = top).
    #[serde(default)]
    pub part: usize,
    /// Move each melody by whole octaves to fit the comfortable range (the key stays).
    #[serde(default = "d_true")]
    pub fit_range: bool,
    /// Use a piece's own tempo when it has one; otherwise the exercise's tempo_bpm.
    #[serde(default = "d_true")]
    pub piece_tempo: bool,
}

impl Default for LibrarySelect {
    fn default() -> Self {
        serde_json::from_str("{}").unwrap()
    }
}

impl Default for MelodySource {
    fn default() -> Self {
        MelodySource::Generate(MelodyParams::default())
    }
}

fn d_sight_tempo() -> f32 {
    72.0
}

/// Sight singing from staff notation: the score is shown, the key is established, a bar is
/// counted in and the melody is sung in time, then graded note by note.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct SightSingParams {
    #[serde(default)]
    pub melody: MelodySource,
    /// Beats per minute in the time signature's unit (a coach melody's own tempo wins).
    #[serde(default = "d_sight_tempo")]
    pub tempo_bpm: f32,
    #[serde(default)]
    pub start: SightStart,
    #[serde(default = "d_one")]
    pub count_in_bars: u32,
    /// What plays while singing: `click` (default), `none`, `drone` or `chords`. The count-in
    /// always clicks.
    #[serde(default)]
    pub backing: Backing,
    #[serde(default = "d_backing_volume")]
    pub backing_volume: f32,
    /// Timbre of the cadence, first note and the melody when listening back.
    #[serde(default = "d_ref_timbre")]
    pub timbre: TimbreId,
    /// Move a cursor over the score while singing.
    #[serde(default)]
    pub follow_cursor: bool,
}

impl Default for SightSingParams {
    fn default() -> Self {
        serde_json::from_str("{}").unwrap()
    }
}

fn d_dictation_melody() -> MelodySource {
    MelodySource::Generate(MelodyParams {
        bars: 2,
        max_leap: 2,
        ..MelodyParams::default()
    })
}

/// The voice that follows the melody in parallel harmony, a diatonic third, fourth, fifth or
/// sixth away.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "snake_case")]
pub enum HarmonyVoice {
    #[default]
    ThirdBelow,
    ThirdAbove,
    FourthBelow,
    FourthAbove,
    FifthBelow,
    FifthAbove,
    SixthBelow,
    SixthAbove,
}

impl HarmonyVoice {
    /// Diatonic steps from the melody.
    pub fn steps(self) -> i32 {
        match self {
            HarmonyVoice::ThirdBelow => -2,
            HarmonyVoice::ThirdAbove => 2,
            HarmonyVoice::FourthBelow => -3,
            HarmonyVoice::FourthAbove => 3,
            HarmonyVoice::FifthBelow => -4,
            HarmonyVoice::FifthAbove => 4,
            HarmonyVoice::SixthBelow => -5,
            HarmonyVoice::SixthAbove => 5,
        }
    }

    /// `a third below`, `a fifth above` …
    pub fn describe(self) -> &'static str {
        match self {
            HarmonyVoice::ThirdBelow => "a third below",
            HarmonyVoice::ThirdAbove => "a third above",
            HarmonyVoice::FourthBelow => "a fourth below",
            HarmonyVoice::FourthAbove => "a fourth above",
            HarmonyVoice::FifthBelow => "a fifth below",
            HarmonyVoice::FifthAbove => "a fifth above",
            HarmonyVoice::SixthBelow => "a sixth below",
            HarmonyVoice::SixthAbove => "a sixth above",
        }
    }
}

/// Which line of parallel harmony you sing; the app plays the other.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "snake_case")]
pub enum HarmonyLine {
    /// The added voice, against the melody.
    #[default]
    Harmony,
    /// The melody, against the added voice.
    Melody,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct ParallelHarmony {
    /// The melody the voice follows: generated (default), written by the coach (`given`) or from
    /// the library. A bare generator object such as `{"bars": 2}` means `{"generate": {…}}`.
    #[serde(default, deserialize_with = "melody_or_generator")]
    pub melody: MelodySource,
    #[serde(default)]
    pub voice: HarmonyVoice,
    #[serde(default)]
    pub sing: HarmonyLine,
    /// A harmony note a tritone from the melody becomes a perfect fourth instead (F–B♭ for
    /// fourths, F over C for fifths).
    #[serde(default = "d_true")]
    pub avoid_tritone: bool,
}

/// A melody source, or a bare generator object for `generate`.
fn melody_or_generator<'de, D: serde::Deserializer<'de>>(d: D) -> Result<MelodySource, D::Error> {
    let v = serde_json::Value::deserialize(d)?;
    let tagged = v.as_object().is_some_and(|o| {
        o.len() == 1
            && ["generate", "given", "library"]
                .iter()
                .any(|k| o.contains_key(*k))
    });
    if tagged {
        serde_json::from_value(v).map_err(serde::de::Error::custom)
    } else {
        serde_json::from_value::<MelodyParams>(v)
            .map(MelodySource::Generate)
            .map_err(serde::de::Error::custom)
    }
}

/// Two- to four-part pieces written into the session (two voices, SATB …), sung as written.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct GivenHarmony {
    /// Pieces in the melody library's JSON form, each with two to four parts, top part first.
    #[schemars(schema_with = "given_pieces_schema")]
    pub pieces: Vec<serde_json::Value>,
    /// The part you sing: 0 is the top part (soprano), 3 the bass of a four-part piece; a piece
    /// with fewer parts gives its lowest.
    #[serde(default)]
    pub part: usize,
    #[serde(default)]
    pub shuffle: bool,
    /// Move every part by the same octaves so yours fits the comfortable range.
    #[serde(default = "d_true")]
    pub fit_range: bool,
    /// Use a piece's own tempo when it has one; otherwise the exercise's tempo_bpm.
    #[serde(default = "d_true")]
    pub piece_tempo: bool,
}

fn given_pieces_schema(_: &mut schemars::SchemaGenerator) -> schemars::Schema {
    schemars::json_schema!({
        "type": "array",
        "minItems": 1,
        "items": {
            "type": "object",
            "required": ["key", "parts"],
            "properties": {
                "title": { "type": "string" },
                "key": { "type": "string", "description": "e.g. \"Eb major\", \"F# minor\"" },
                "time": { "type": "string", "description": "e.g. \"3/4\"; default 4/4" },
                "tempo_bpm": { "type": "number" },
                "pickup": { "type": "number", "description": "beats before the first barline" },
                "parts": {
                    "type": "array",
                    "minItems": 2,
                    "maxItems": 4,
                    "description": "top part first; any intervals between them",
                    "items": {
                        "type": "object",
                        "required": ["notes"],
                        "properties": {
                            "name": { "type": "string", "description": "e.g. Soprano, Alto, Tenor, Bass" },
                            "clef": { "enum": ["auto", "treble", "treble8vb", "bass"] },
                            "notes": {
                                "description": "text \"Eb4:1 F4:.5 G4 | Ab4:2~ | Ab4:1 R:1\" (pitch:beats, a length carries over, R rest, ~ tie, | barline checked) or a list of {note, beats} | {rest}",
                                "oneOf": [{ "type": "string" }, { "type": "array" }]
                            }
                        }
                    }
                }
            }
        }
    })
}

/// Where the voices of a harmony exercise come from.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum HarmonySource {
    /// A melody and a voice a fixed diatonic interval away from it (third, fourth, fifth, sixth).
    Parallel(ParallelHarmony),
    /// Pieces written into the session with two to four parts, sung as written: free two-voice
    /// or SATB harmony with any intervals, tritones included.
    Given(GivenHarmony),
    /// Pieces with two or more parts from the melody library, sung as written. `part` is the one
    /// you sing; the app plays the others.
    Library(LibrarySelect),
}

impl Default for HarmonySource {
    fn default() -> Self {
        HarmonySource::Parallel(serde_json::from_str("{}").unwrap())
    }
}

fn d_harmony_tempo() -> f32 {
    72.0
}

/// Harmony singing: the app plays one voice while you sing another, graded note by note. With
/// `notation` your part is shown on the staff; otherwise you sing it by ear.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct HarmonyParams {
    #[serde(default)]
    pub source: HarmonySource,
    /// Practice: your part plays along and its notes are shown, so you hear what to sing. These
    /// attempts are stored as assisted and left out of accuracy.
    #[serde(default)]
    pub practice: bool,
    /// With written pieces (given, library): the parts the app plays with you, by index (0 =
    /// top). Default: all the others. Indices a piece does not have are ignored.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub play_parts: Vec<usize>,
    /// Show your part on the staff. Default: shown for written pieces (given, library), by ear
    /// for parallel harmony.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub notation: Option<bool>,
    /// Beats per minute in the time signature's unit (a library piece's own tempo wins unless
    /// `piece_tempo` is off).
    #[serde(default = "d_harmony_tempo")]
    pub tempo_bpm: f32,
    /// What is played first: by default the key, then the first note of your part.
    #[serde(default)]
    pub start: SightStart,
    #[serde(default = "d_one")]
    pub count_in_bars: u32,
    /// A click under the singing.
    #[serde(default = "d_true")]
    pub click: bool,
    /// Timbre of the cadence, first note and the other voice.
    #[serde(default = "d_ref_timbre")]
    pub timbre: TimbreId,
    #[serde(default)]
    pub follow_cursor: bool,
}

impl Default for HarmonyParams {
    fn default() -> Self {
        serde_json::from_str("{}").unwrap()
    }
}

fn d_dictation_tempo() -> f32 {
    80.0
}

/// Melodic dictation: after the key and a count-in the melody is played, and the user writes it
/// on the staff in the score editor. Graded note by note for pitch and (optionally) rhythm.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct DictationParams {
    /// Defaults to generated two-bar melodies, mostly stepwise.
    #[serde(default = "d_dictation_melody")]
    pub melody: MelodySource,
    #[serde(default = "d_dictation_tempo")]
    pub tempo_bpm: f32,
    /// Plays allowed, the first included; 0 for no limit.
    #[serde(default)]
    pub max_plays: u32,
    /// Write the first note into the answer.
    #[serde(default = "d_true")]
    pub give_first_note: bool,
    /// Play a cadence in the key first.
    #[serde(default = "d_true")]
    pub play_key: bool,
    /// Click while the melody plays (the count-in bar always clicks).
    #[serde(default = "d_true")]
    pub click: bool,
    /// Grade note positions and lengths as well as pitches.
    #[serde(default = "d_true")]
    pub grade_rhythm: bool,
    #[serde(default = "d_ref_timbre")]
    pub timbre: TimbreId,
}

impl Default for DictationParams {
    fn default() -> Self {
        serde_json::from_str("{}").unwrap()
    }
}

/// A song the user knows well in its original key: an absolute-pitch anchor.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema, TS)]
pub struct Anchor {
    #[serde(default)]
    pub id: String,
    /// The song as prompts name it, e.g. "Yesterday (The Beatles)".
    pub title: String,
    /// The first sung note of the original recording, e.g. "G4".
    pub first_note: Pitch,
    /// The opening phrase in the original key, for "original key or shifted?" trials.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub melody: Option<CompactMelody>,
    #[serde(default)]
    pub created_at: String,
    #[serde(default)]
    pub source: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "snake_case")]
pub enum AnchorMode {
    /// "Sing the first note of …", graded by pitch class (octave reported).
    #[default]
    SingFirstNote,
    /// The opening phrase plays in its original key or shifted; answer which.
    KeyCheck,
}

fn d_anchor_shifts() -> Vec<i32> {
    vec![-2, -1, 1, 2]
}

fn d_half() -> f32 {
    0.5
}

/// Anchor songs: sing a known song's first note from memory, or tell whether its opening is
/// played in the original key. Uses the anchors saved in the app (or with `save_anchor`).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct AnchorSongParams {
    #[serde(default)]
    pub mode: AnchorMode,
    /// Anchor ids or titles to use; empty uses every saved anchor (those with a melody for
    /// `key_check`).
    #[serde(default)]
    pub anchors: Vec<String>,
    /// Semitone shifts used when the phrase is not in the original key (`key_check`).
    #[serde(default = "d_anchor_shifts")]
    pub shifts: Vec<i32>,
    /// Fraction of `key_check` trials played in the original key.
    #[serde(default = "d_half")]
    pub original_rate: f32,
    #[serde(default = "d_ms_4000")]
    pub response_ms: u32,
    #[serde(default)]
    pub pre_mask: Mask,
    /// Play the first note after a sung attempt.
    #[serde(default = "d_true")]
    pub reveal_target_after: bool,
    #[serde(default = "d_ref_timbre")]
    pub timbre: TimbreId,
}

fn d_incidental_classes() -> ClassSet {
    ClassSet::List(vec![
        PitchClass::new(0),
        PitchClass::new(4),
        PitchClass::new(8),
        PitchClass::new(2),
    ])
}

fn d_ms_300() -> u32 {
    300
}

fn d_mismatch() -> f32 {
    0.1
}

/// The colour each pitch class is always paired with in incidental pairing.
pub const PAIRING_COLORS: [(&str, &str); 12] = [
    ("red", "Red"),
    ("orange", "Orange"),
    ("yellow", "Yellow"),
    ("olive", "Olive"),
    ("green", "Green"),
    ("teal", "Teal"),
    ("sky", "Sky"),
    ("blue", "Blue"),
    ("indigo", "Indigo"),
    ("purple", "Purple"),
    ("pink", "Pink"),
    ("brown", "Brown"),
];

/// Incidental pairing: a colour task in which each colour always comes with the same pitch class.
/// Press the colour shown; the tone is never mentioned. Logged as exposure, never as evidence of
/// absolute pitch. A few mismatched trials show, through response times, whether the pairing is
/// being learnt.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct IncidentalPairingParams {
    /// Pitch classes, each with its fixed colour (C red, E green, G♯ indigo, D yellow …).
    #[serde(default = "d_incidental_classes")]
    pub classes: ClassSet,
    #[serde(default = "d_octaves")]
    pub octaves: Vec<i32>,
    #[serde(default)]
    pub timbres: Vec<TimbreId>,
    #[serde(default)]
    pub pool: Pool,
    #[serde(default = "d_ms_1000")]
    pub duration_ms: u32,
    /// Delay from the tone's onset to the colour appearing.
    #[serde(default = "d_ms_300")]
    pub cue_delay_ms: u32,
    /// Fraction of trials whose tone belongs to another colour.
    #[serde(default = "d_mismatch")]
    pub mismatch_rate: f32,
}

fn d_jump_13() -> u32 {
    13
}
fn d_tones_16() -> u32 {
    16
}
fn d_quarter() -> f32 {
    0.25
}
fn d_ms_1750() -> u32 {
    1750
}
fn d_choir() -> TimbreId {
    "gm:52".into()
}
fn d_taneda() -> String {
    "taneda_colors".into()
}

/// How a detection stream marks each tone.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "snake_case")]
pub enum DetectMarks {
    /// A summary of d′ and latency after the stream.
    #[default]
    Summary,
    /// A mark after each tone, as well as the summary.
    Each,
    /// No marks during the stream. The block's feedback mode still decides the summary.
    None,
}

/// Fast detection (Simple Speed): one stream of tones, press on the target class.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct PitchDetectParams {
    #[serde(default)]
    pub classes: ClassSet,
    #[serde(default = "d_octaves")]
    pub octaves: Vec<i32>,
    #[serde(default)]
    pub timbres: Vec<TimbreId>,
    #[serde(default)]
    pub pool: Pool,
    #[serde(default = "d_jump_13")]
    pub min_jump_semitones: u32,
    /// Tones in the stream.
    #[serde(default = "d_tones_16")]
    pub tones: u32,
    /// Share of tones that are the target class.
    #[serde(default = "d_quarter")]
    pub target_rate: f32,
    /// How long each tone can be answered, and the time until the next one.
    #[serde(default = "d_ms_1750")]
    pub window_ms: u32,
    #[serde(default = "d_ms_1500")]
    pub prompt_ms: u32,
    #[serde(default = "d_ms_300")]
    pub tone_ms: u32,
    /// The target is sung on this timbre (General MIDI choir "aahs").
    #[serde(default = "d_choir")]
    pub prompt_timbre: TimbreId,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub labels: Option<String>,
    #[serde(default)]
    pub marks: DetectMarks,
    /// Say the target's name when that speech file exists.
    #[serde(default = "d_true")]
    pub speak: bool,
}

impl Default for PitchDetectParams {
    fn default() -> Self {
        serde_json::from_str("{}").unwrap()
    }
}

/// Eguchi chord colours: name the colour of a fixed voicing.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct ChordColorParams {
    /// Chord ids from the Eguchi catalogue, in catalogue order. Empty means the nine white chords.
    #[serde(default)]
    pub chords: Vec<String>,
    #[serde(default = "d_ms_1500")]
    pub duration_ms: u32,
    #[serde(default = "d_ref_timbre")]
    pub timbre: TimbreId,
    /// Shift the fixed voicing by this many octaves.
    #[serde(default)]
    pub register_shift: i32,
    /// After each chord, one of its notes (or a white-key note) is named on its own.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub single_note_after: bool,
    /// Label set for that note.
    #[serde(default = "d_taneda")]
    pub note_labels: String,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub requeue_misses: bool,
}

impl Default for ChordColorParams {
    fn default() -> Self {
        serde_json::from_str("{}").unwrap()
    }
}

// ---------------------------------------------------------------------------------------------
// Blocks and sessions
// ---------------------------------------------------------------------------------------------

/// The exercise type and its parameters. `params` may be `{}` to use every default.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "type", content = "params", rename_all = "snake_case")]
pub enum Exercise {
    PitchId(PitchIdParams),
    SingPitch(SingPitchParams),
    PitchPairing(PitchPairingParams),
    TonalCenterId(TonalCenterIdParams),
    IntervalId(IntervalIdParams),
    ChordId(ChordIdParams),
    ScaleId(ScaleIdParams),
    PitchMatch(PitchMatchParams),
    Sustain(SustainParams),
    SingInterval(SingIntervalParams),
    Pattern(PatternParams),
    SightSing(SightSingParams),
    MelodicDictation(DictationParams),
    AnchorSong(AnchorSongParams),
    IncidentalPairing(IncidentalPairingParams),
    ScaleDegreeId(ScaleDegreeParams),
    IntervalComparison(IntervalComparisonParams),
    Harmony(HarmonyParams),
    PitchDetect(PitchDetectParams),
    ChordColorId(ChordColorParams),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "snake_case")]
pub enum ExerciseKind {
    PitchId,
    SingPitch,
    PitchPairing,
    TonalCenterId,
    IntervalId,
    ChordId,
    ScaleId,
    PitchMatch,
    Sustain,
    SingInterval,
    Pattern,
    SightSing,
    MelodicDictation,
    AnchorSong,
    IncidentalPairing,
    ScaleDegreeId,
    IntervalComparison,
    Harmony,
    PitchDetect,
    ChordColorId,
}

impl ExerciseKind {
    pub const ALL: [ExerciseKind; 20] = [
        ExerciseKind::PitchId,
        ExerciseKind::SingPitch,
        ExerciseKind::PitchPairing,
        ExerciseKind::TonalCenterId,
        ExerciseKind::IntervalId,
        ExerciseKind::ChordId,
        ExerciseKind::ScaleId,
        ExerciseKind::PitchMatch,
        ExerciseKind::Sustain,
        ExerciseKind::SingInterval,
        ExerciseKind::Pattern,
        ExerciseKind::SightSing,
        ExerciseKind::MelodicDictation,
        ExerciseKind::AnchorSong,
        ExerciseKind::IncidentalPairing,
        ExerciseKind::ScaleDegreeId,
        ExerciseKind::IntervalComparison,
        ExerciseKind::Harmony,
        ExerciseKind::PitchDetect,
        ExerciseKind::ChordColorId,
    ];

    pub fn as_str(self) -> &'static str {
        match self {
            ExerciseKind::PitchId => "pitch_id",
            ExerciseKind::SingPitch => "sing_pitch",
            ExerciseKind::PitchPairing => "pitch_pairing",
            ExerciseKind::TonalCenterId => "tonal_center_id",
            ExerciseKind::IntervalId => "interval_id",
            ExerciseKind::ChordId => "chord_id",
            ExerciseKind::ScaleId => "scale_id",
            ExerciseKind::PitchMatch => "pitch_match",
            ExerciseKind::Sustain => "sustain",
            ExerciseKind::SingInterval => "sing_interval",
            ExerciseKind::Pattern => "pattern",
            ExerciseKind::SightSing => "sight_sing",
            ExerciseKind::MelodicDictation => "melodic_dictation",
            ExerciseKind::AnchorSong => "anchor_song",
            ExerciseKind::IncidentalPairing => "incidental_pairing",
            ExerciseKind::ScaleDegreeId => "scale_degree_id",
            ExerciseKind::IntervalComparison => "interval_comparison",
            ExerciseKind::Harmony => "harmony",
            ExerciseKind::PitchDetect => "pitch_detect",
            ExerciseKind::ChordColorId => "chord_color_id",
        }
    }

    pub fn from_str(s: &str) -> Option<Self> {
        Self::ALL.iter().copied().find(|k| k.as_str() == s)
    }

    pub fn title(self) -> &'static str {
        match self {
            ExerciseKind::PitchId => "Name the note",
            ExerciseKind::SingPitch => "Sing a named note",
            ExerciseKind::PitchPairing => "Note pairing (listening)",
            ExerciseKind::TonalCenterId => "Find the key",
            ExerciseKind::IntervalId => "Interval identification",
            ExerciseKind::ChordId => "Chord identification",
            ExerciseKind::ScaleId => "Scale identification",
            ExerciseKind::PitchMatch => "Pitch matching",
            ExerciseKind::Sustain => "Sustained note",
            ExerciseKind::SingInterval => "Interval singing",
            ExerciseKind::Pattern => "Sung patterns",
            ExerciseKind::SightSing => "Sight singing",
            ExerciseKind::MelodicDictation => "Melodic dictation",
            ExerciseKind::AnchorSong => "Anchor songs",
            ExerciseKind::IncidentalPairing => "Colour task (incidental pairing)",
            ExerciseKind::ScaleDegreeId => "Scale degrees",
            ExerciseKind::IntervalComparison => "Interval comparison",
            ExerciseKind::Harmony => "Harmony singing",
            ExerciseKind::PitchDetect => "Find the note",
            ExerciseKind::ChordColorId => "Chord colours",
        }
    }

    pub fn default_tags(self) -> &'static [&'static str] {
        match self {
            ExerciseKind::PitchId => &["ap.id"],
            ExerciseKind::SingPitch => &["ap.production"],
            ExerciseKind::PitchPairing => &["ap.exposure"],
            ExerciseKind::TonalCenterId => &["ap.context"],
            ExerciseKind::IntervalId => &["rp.interval"],
            ExerciseKind::ChordId => &["rp.chord"],
            ExerciseKind::ScaleId => &["rp.scale"],
            ExerciseKind::PitchMatch => &["voice.pitch_match"],
            ExerciseKind::Sustain => &["voice.sustain"],
            ExerciseKind::SingInterval => &["voice.interval"],
            ExerciseKind::Pattern => &["voice.pattern"],
            ExerciseKind::SightSing => &["voice.sight_singing"],
            ExerciseKind::MelodicDictation => &["ear.dictation"],
            ExerciseKind::AnchorSong => &["ap.anchor"],
            ExerciseKind::IncidentalPairing => &["ap.exposure"],
            ExerciseKind::ScaleDegreeId => &["rp.degree"],
            ExerciseKind::IntervalComparison => &["rp.interval_compare"],
            ExerciseKind::Harmony => &["voice.harmony"],
            ExerciseKind::PitchDetect => &["ap.detect"],
            ExerciseKind::ChordColorId => &["ap.chord_color"],
        }
    }

    pub fn is_sung(self) -> bool {
        matches!(
            self,
            ExerciseKind::SingPitch
                | ExerciseKind::PitchMatch
                | ExerciseKind::Sustain
                | ExerciseKind::SingInterval
                | ExerciseKind::Pattern
                | ExerciseKind::SightSing
                | ExerciseKind::Harmony
        )
    }
}

impl Exercise {
    pub fn kind(&self) -> ExerciseKind {
        match self {
            Exercise::PitchId(_) => ExerciseKind::PitchId,
            Exercise::SingPitch(_) => ExerciseKind::SingPitch,
            Exercise::PitchPairing(_) => ExerciseKind::PitchPairing,
            Exercise::TonalCenterId(_) => ExerciseKind::TonalCenterId,
            Exercise::IntervalId(_) => ExerciseKind::IntervalId,
            Exercise::ChordId(_) => ExerciseKind::ChordId,
            Exercise::ScaleId(_) => ExerciseKind::ScaleId,
            Exercise::PitchMatch(_) => ExerciseKind::PitchMatch,
            Exercise::Sustain(_) => ExerciseKind::Sustain,
            Exercise::SingInterval(_) => ExerciseKind::SingInterval,
            Exercise::Pattern(_) => ExerciseKind::Pattern,
            Exercise::SightSing(_) => ExerciseKind::SightSing,
            Exercise::MelodicDictation(_) => ExerciseKind::MelodicDictation,
            Exercise::AnchorSong(_) => ExerciseKind::AnchorSong,
            Exercise::IncidentalPairing(_) => ExerciseKind::IncidentalPairing,
            Exercise::ScaleDegreeId(_) => ExerciseKind::ScaleDegreeId,
            Exercise::IntervalComparison(_) => ExerciseKind::IntervalComparison,
            Exercise::Harmony(_) => ExerciseKind::Harmony,
            Exercise::PitchDetect(_) => ExerciseKind::PitchDetect,
            Exercise::ChordColorId(_) => ExerciseKind::ChordColorId,
        }
    }

    pub fn pool(&self) -> Option<Pool> {
        match self {
            Exercise::PitchId(p) => Some(p.pool),
            Exercise::PitchPairing(p) => Some(p.pool),
            Exercise::IncidentalPairing(p) => Some(p.pool),
            Exercise::PitchDetect(p) => Some(p.pool),
            _ => None,
        }
    }
}

fn d_count() -> u32 {
    20
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct BlockSpec {
    #[serde(flatten)]
    pub exercise: Exercise,
    /// Number of trials.
    #[serde(default = "d_count")]
    pub count: u32,
    /// Seed for reproducible trials; derived from the session when omitted.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub seed: Option<u64>,
    #[serde(default)]
    pub feedback: FeedbackMode,
    #[serde(default, skip_serializing_if = "is_default_grading")]
    pub grading: GradingOverrides,
    /// Extra tags added to every trial, e.g. `ap.id.timbre_transfer`.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tags: Vec<String>,
    /// Fraction correct needed to pass the block (shown in results; nothing auto-advances).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pass_mark: Option<f32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    /// Shown before the block starts.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub instructions: Option<String>,
    /// Marks an assessment block: feedback is forced off and holdout rules are enforced.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub assessment: bool,
    /// Played once before the first trial, e.g. `shepard_glissando` to clear pitch memory before
    /// a test. When omitted, a block without feedback starts with the glissando anyway if a named
    /// tone was heard in the last few minutes; `none` turns that off.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pre_mask: Option<Mask>,
    /// The course level this block belongs to, for course progress.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub course: Option<CourseRef>,
}

/// A block's place in a course: which course and level, and whether it is the level's check.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema, TS)]
pub struct CourseRef {
    pub course: String,
    /// 1-based.
    pub level: u32,
    /// The level's verification block (no feedback), which its pass rule counts.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub verify: bool,
}

impl CourseRef {
    /// The trial tag for this course level, e.g. `course.ap_naming.03`.
    pub fn tag(&self) -> String {
        format!("course.{}.{:02}", self.course, self.level)
    }
}

fn is_default_grading(g: &GradingOverrides) -> bool {
    *g == GradingOverrides::default()
}

impl BlockSpec {
    pub fn new(exercise: Exercise, count: u32) -> Self {
        BlockSpec {
            exercise,
            count,
            seed: None,
            feedback: FeedbackMode::AfterTrial,
            grading: GradingOverrides::default(),
            tags: Vec::new(),
            pass_mark: None,
            title: None,
            instructions: None,
            assessment: false,
            pre_mask: None,
            course: None,
        }
    }

    pub fn effective_feedback(&self) -> FeedbackMode {
        if self.assessment {
            FeedbackMode::None
        } else {
            self.feedback
        }
    }

    pub fn display_title(&self) -> String {
        self.title
            .clone()
            .unwrap_or_else(|| self.exercise.kind().title().to_string())
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct SessionSpec {
    pub title: String,
    /// Label used to compare protocols later, e.g. `ap_progressive`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub protocol: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub planned_minutes: Option<f32>,
    /// Note from whoever wrote the session, shown to the user.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub notes: Option<String>,
    /// Base seed; each block derives its own unless it sets `seed`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub seed: Option<u64>,
    pub blocks: Vec<BlockSpec>,
}

#[derive(Debug, thiserror::Error)]
#[error("{path}: {message}")]
pub struct SpecParseError {
    pub path: String,
    pub message: String,
}

impl SessionSpec {
    /// Parses a session from JSON, filling in missing `params` objects and reporting the JSON
    /// path of the first error.
    pub fn from_value(mut value: serde_json::Value) -> Result<SessionSpec, SpecParseError> {
        if let Some(blocks) = value.get_mut("blocks").and_then(|b| b.as_array_mut()) {
            for block in blocks {
                if let Some(obj) = block.as_object_mut() {
                    obj.entry("params").or_insert_with(|| serde_json::json!({}));
                }
            }
        }
        // Report errors per block so the message points at the offending block.
        if let Some(blocks) = value.get("blocks").and_then(|b| b.as_array()) {
            for (i, block) in blocks.iter().enumerate() {
                if let Err(e) = serde_json::from_value::<BlockSpec>(block.clone()) {
                    return Err(SpecParseError {
                        path: format!("blocks[{i}]"),
                        message: e.to_string(),
                    });
                }
            }
        }
        serde_json::from_value(value).map_err(|e| SpecParseError {
            path: "$".into(),
            message: e.to_string(),
        })
    }

    pub fn from_json(json: &str) -> Result<SessionSpec, SpecParseError> {
        let value: serde_json::Value = serde_json::from_str(json).map_err(|e| SpecParseError {
            path: "$".into(),
            message: e.to_string(),
        })?;
        Self::from_value(value)
    }

    pub fn total_trials(&self) -> u32 {
        self.blocks.iter().map(|b| b.count).sum()
    }
}

// ---------------------------------------------------------------------------------------------
// Concrete trials
// ---------------------------------------------------------------------------------------------

/// A sound to play, relative to the start of its group.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema, TS)]
pub struct SoundEvent {
    pub midi: i32,
    pub start_ms: f64,
    pub dur_ms: f64,
    pub timbre: TimbreId,
    #[serde(default)]
    pub gain_db: f32,
    #[serde(default)]
    pub detune_cents: f32,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema, TS)]
pub struct Choice {
    pub id: String,
    pub label: String,
    /// CSS colour for a swatch on the button.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub color: Option<String>,
    /// A word shown with the label ("Sarah", "Red"), or instead of it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub word: Option<String>,
}

impl Choice {
    pub fn new(id: impl AsRef<str>, label: impl AsRef<str>) -> Self {
        Self {
            id: id.as_ref().to_string(),
            label: label.as_ref().to_string(),
            color: None,
            word: None,
        }
    }

    pub fn swatch(mut self, color: impl Into<String>, word: impl Into<String>) -> Self {
        self.color = Some(color.into());
        let word = word.into();
        if !word.is_empty() {
            self.word = Some(word);
        }
        self
    }
}

/// Play sounds, then pick an answer (pitch, interval, chord, scale and key identification).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema, TS)]
pub struct IdentifyTrial {
    pub exercise: ExerciseKind,
    pub prompt: String,
    pub events: Vec<SoundEvent>,
    pub mask_before: Mask,
    pub choices: Vec<Choice>,
    pub correct: String,
    pub deadline_ms: Option<u32>,
    /// What the Reference button plays, if the exercise allows one.
    pub reference: Option<Vec<SoundEvent>>,
    /// The stimulus description used for statistics, e.g. `{"pitch_class": "F#", "octave": 4}`.
    #[ts(type = "Record<string, unknown>")]
    pub stimulus: serde_json::Value,
    /// A colour shown after the sound starts (incidental pairing); response times count from it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cue: Option<VisualCue>,
    /// Answers are taken and timed from this point in the sound (after a key-setting cadence).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub answer_after_ms: Option<f64>,
    /// Played as soon as the result is shown, e.g. a scale degree walking to the tonic.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub after_answer: Option<Vec<SoundEvent>>,
    /// Added to the result, e.g. the two notes of an interval in a key ("sol → si").
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub explain: Option<String>,
    /// Which answer each MIDI note stands for, so the answer lights up while it sounds (a
    /// degree walking to the tonic).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub note_choices: Vec<(i32, String)>,
    /// What each answer sounds like in this trial's register and timbre, played when it is
    /// clicked after answering. Made with the trial and not stored.
    #[serde(skip)]
    pub choice_sounds: Vec<ChoiceSound>,
    /// Paced stream: a tone every this many ms, answered before the next one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pace_ms: Option<u32>,
    /// Screen tint for the played note, shown with the result or during pairing.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub flash_color: Option<String>,
    /// Label set id stored with the trial.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub label_set: Option<String>,
    /// This class comes back later in the block if it is missed.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub requeue_misses: bool,
    /// Octaves a requeued note may move to. Empty keeps the same octave.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub requeue_octaves: Vec<i32>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ChoiceSound {
    pub id: String,
    pub events: Vec<SoundEvent>,
}

impl IdentifyTrial {
    /// The sound of answer `id`, if the trial has one.
    pub fn choice_sound(&self, id: &str) -> Option<&[SoundEvent]> {
        self.choice_sounds
            .iter()
            .find(|c| c.id == id)
            .map(|c| c.events.as_slice())
    }

    /// Empty optional fields, spread into a constructor: `IdentifyTrial { .., ..IdentifyTrial::skeleton() }`.
    pub fn skeleton() -> Self {
        Self {
            exercise: ExerciseKind::PitchId,
            prompt: String::new(),
            events: Vec::new(),
            mask_before: Mask::None,
            choices: Vec::new(),
            correct: String::new(),
            deadline_ms: None,
            reference: None,
            stimulus: serde_json::Value::Null,
            cue: None,
            answer_after_ms: None,
            after_answer: None,
            explain: None,
            note_choices: Vec::new(),
            choice_sounds: Vec::new(),
            pace_ms: None,
            flash_color: None,
            label_set: None,
            requeue_misses: false,
            requeue_octaves: Vec::new(),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema, TS)]
pub struct VisualCue {
    /// A colour id from `PAIRING_COLORS`.
    pub color: String,
    pub delay_ms: u32,
}

fn d_sing_pitch_kind() -> ExerciseKind {
    ExerciseKind::SingPitch
}

/// Sing a named pitch with no reference.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema, TS)]
pub struct SingPitchTrial {
    /// `sing_pitch`, or `anchor_song` for "sing the first note of …".
    #[serde(default = "d_sing_pitch_kind")]
    pub exercise: ExerciseKind,
    pub prompt: String,
    pub target_class: PitchClass,
    pub target_midi: Option<i32>,
    pub response_ms: u32,
    pub delay_ms: u32,
    pub pre_mask: Mask,
    pub reveal: Option<Vec<SoundEvent>>,
    #[ts(type = "Record<string, unknown>")]
    pub stimulus: serde_json::Value,
}

/// Passive pairing of a tone with its name.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema, TS)]
pub struct PairingTrial {
    pub events: Vec<SoundEvent>,
    pub label: String,
    pub gap_ms: u32,
    #[ts(type = "Record<string, unknown>")]
    pub stimulus: serde_json::Value,
    /// Screen tint while the label is showing.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub flash_color: Option<String>,
    /// Speech file for the label, relative to the speech folder.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub speech: Option<String>,
}

/// One tone in a detection stream.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema, TS)]
pub struct DetectTone {
    pub events: Vec<SoundEvent>,
    pub target: bool,
    /// More than an octave from the sung prompt.
    pub far: bool,
    pub midi: i32,
}

/// One detection stream: a named target, then a run of tones to catch.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema, TS)]
pub struct DetectTrial {
    pub prompt: String,
    pub target_class: PitchClass,
    pub target_label: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub target_color: Option<String>,
    pub prompt_events: Vec<SoundEvent>,
    pub prompt_ms: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub speech: Option<String>,
    pub tones: Vec<DetectTone>,
    pub window_ms: u32,
    pub marks: DetectMarks,
    #[ts(type = "Record<string, unknown>")]
    pub stimulus: serde_json::Value,
}

/// A target note on the sung timeline, in beats from the moment singing starts.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema, TS)]
pub struct TargetNote {
    pub midi: i32,
    pub start_beat: f32,
    pub beats: f32,
}

/// Reference, count-in and singing on a metronome timeline.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema, TS)]
pub struct SungTrial {
    pub exercise: ExerciseKind,
    pub prompt: String,
    pub notes: Vec<TargetNote>,
    pub tempo_bpm: f32,
    pub beats_per_bar: u32,
    /// Beat (from the start of the trial audio) at which singing starts.
    pub sing_start_beat: f32,
    /// Total length of the trial timeline in beats, including any tail.
    pub total_beats: f32,
    /// Played before singing (echo reference or starting pitch), beats from trial start.
    pub reference_beats: Vec<TargetNote>,
    pub reference_timbre: TimbreId,
    pub backing: Backing,
    pub backing_volume: f32,
    /// Play the target melody while singing (guided mode or "sing along").
    pub guide: bool,
    pub drone_midi: Option<i32>,
    pub lane_targets: LaneTargets,
    /// Whether onsets are graded against the beat grid.
    pub timed: bool,
    #[ts(type = "Record<string, unknown>")]
    pub stimulus: serde_json::Value,
    /// The notated melody for sight singing; drawn as a staff instead of the pitch lane.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub score: Option<Score>,
    /// Beat at which a count-in starts that always clicks, whatever the backing. Without it the
    /// click (if the backing has one) runs from the start.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub count_in_beat: Option<f32>,
    /// Labelled parts of the timeline (establish key, first note, count-in, sing).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub sections: Vec<TimelineSection>,
    #[serde(default)]
    pub follow_cursor: bool,
    /// Another voice played while singing (harmony), in beats from the start of singing like
    /// `notes`.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub accompaniment: Vec<TargetNote>,
    /// Per-note offsets in cents from equal temperament for just intonation against the
    /// accompaniment, used when grading with tuning `just`.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub just_cents: Vec<f32>,
}

/// Hear a melody and write it in the score editor.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema, TS)]
pub struct DictationTrial {
    pub prompt: String,
    /// The melody to write; not shown until the answer is checked.
    pub score: Score,
    /// What the editor starts with: the melody's key, meter, clef and bars, with the first note
    /// (and any rest before it) when it is given.
    pub given: Score,
    /// Cadence, count-in and melody on a timeline (rendered like a sung trial with the guide on).
    pub playback: SungTrial,
    /// Plays allowed, the first included; 0 for no limit.
    pub max_plays: u32,
    pub grade_rhythm: bool,
    #[ts(type = "Record<string, unknown>")]
    pub stimulus: serde_json::Value,
}

/// A labelled stretch of a sung trial's timeline, in beats from its start.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema, TS)]
pub struct TimelineSection {
    pub label: String,
    pub start_beat: f32,
    pub end_beat: f32,
}

impl SungTrial {
    pub fn beat_ms(&self) -> f64 {
        60_000.0 / self.tempo_bpm as f64
    }

    pub fn sing_start_ms(&self) -> f64 {
        self.sing_start_beat as f64 * self.beat_ms()
    }

    pub fn total_ms(&self) -> f64 {
        self.total_beats as f64 * self.beat_ms()
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum TrialSpec {
    Identify(IdentifyTrial),
    SingPitch(SingPitchTrial),
    Pairing(PairingTrial),
    Sung(SungTrial),
    Dictation(DictationTrial),
    Detect(DetectTrial),
}

impl TrialSpec {
    /// A practice trial plays the answer along (harmony practice); it is stored as assisted.
    pub fn is_practice(&self) -> bool {
        matches!(self, TrialSpec::Sung(t) if t.exercise == ExerciseKind::Harmony && t.guide)
    }

    pub fn stimulus(&self) -> &serde_json::Value {
        match self {
            TrialSpec::Identify(t) => &t.stimulus,
            TrialSpec::SingPitch(t) => &t.stimulus,
            TrialSpec::Pairing(t) => &t.stimulus,
            TrialSpec::Sung(t) => &t.stimulus,
            TrialSpec::Dictation(t) => &t.stimulus,
            TrialSpec::Detect(t) => &t.stimulus,
        }
    }

    pub fn prompt(&self) -> &str {
        match self {
            TrialSpec::Identify(t) => &t.prompt,
            TrialSpec::SingPitch(t) => &t.prompt,
            TrialSpec::Pairing(t) => &t.label,
            TrialSpec::Sung(t) => &t.prompt,
            TrialSpec::Dictation(t) => &t.prompt,
            TrialSpec::Detect(t) => &t.prompt,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_session_with_defaults_and_missing_params() {
        let json = r#"{
          "title": "AP stage 2",
          "protocol": "ap_progressive",
          "blocks": [
            {"type": "pitch_id", "count": 60, "seed": 1842,
             "params": {"classes": ["C", "E", "G#"], "timbres": ["gm:0", "synth:triangle"]}},
            {"type": "sing_pitch", "count": 12, "grading": {"mode": "pitch_class", "category_tol_cents": 50}},
            {"type": "pattern", "count": 4, "params": {"pattern": {"scale": {"kind": "major", "shape": "up_down"}}, "tempo_bpm": 90, "backing": "melody"}}
          ]
        }"#;
        let s = SessionSpec::from_json(json).unwrap();
        assert_eq!(s.blocks.len(), 3);
        assert_eq!(s.total_trials(), 76);
        match &s.blocks[0].exercise {
            Exercise::PitchId(p) => {
                assert_eq!(p.classes.classes().len(), 3);
                assert_eq!(p.octaves, vec![3, 4, 5]);
                // Masks are off unless asked for.
                assert_eq!(p.mask, Mask::None);
            }
            other => panic!("unexpected {other:?}"),
        }
        match &s.blocks[1].exercise {
            Exercise::SingPitch(p) => {
                assert!(p.classes.is_all());
                assert!(p.reveal_target_after);
                assert_eq!(p.response_ms, 4000);
            }
            other => panic!("unexpected {other:?}"),
        }
        assert_eq!(s.blocks[1].grading.mode, Some(PitchRefMode::PitchClass));
        match &s.blocks[2].exercise {
            Exercise::Pattern(p) => {
                assert_eq!(p.sung.tempo_bpm, 90.0);
                assert_eq!(p.sung.backing, Backing::Melody);
                assert_eq!(p.sung.reference, None);
            }
            other => panic!("unexpected {other:?}"),
        }
        // Round trip keeps the shape.
        let back = serde_json::to_value(&s).unwrap();
        let again = SessionSpec::from_value(back).unwrap();
        assert_eq!(again, s);
    }

    #[test]
    fn reports_bad_block_path() {
        let json =
            r#"{"title": "x", "blocks": [{"type": "pitch_id"}, {"type": "nope", "params": {}}]}"#;
        let err = SessionSpec::from_json(json).unwrap_err();
        assert_eq!(err.path, "blocks[1]");
    }

    #[test]
    fn sing_interval_parses_with_no_params() {
        let s =
            SessionSpec::from_json(r#"{"title":"x","blocks":[{"type":"sing_interval"}]}"#).unwrap();
        match &s.blocks[0].exercise {
            Exercise::SingInterval(p) => assert_eq!(p.sung.reference, None),
            other => panic!("unexpected {other:?}"),
        }
    }
}
