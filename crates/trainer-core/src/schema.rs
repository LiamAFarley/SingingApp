//! Session and exercise definitions (written by the LLM, presets or the user) and the concrete
//! trial specs the app runs.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use ts_rs::TS;

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
}

impl FeedbackMode {
    pub fn shows_trial_result(self) -> bool {
        matches!(self, FeedbackMode::Continuous | FeedbackMode::AfterTrial)
    }

    pub fn as_str(self) -> &'static str {
        match self {
            FeedbackMode::Continuous => "continuous",
            FeedbackMode::AfterTrial => "after_trial",
            FeedbackMode::AfterBlock => "after_block",
            FeedbackMode::EndOfSession => "end_of_session",
            FeedbackMode::None => "none",
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

/// Sound played between trials so the next trial cannot be solved by relative pitch.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "snake_case")]
pub enum Mask {
    None,
    Noise,
    #[default]
    RandomTones,
    ChromaticCluster,
}

impl Mask {
    pub fn as_str(self) -> &'static str {
        match self {
            Mask::None => "none",
            Mask::Noise => "noise",
            Mask::RandomTones => "random_tones",
            Mask::ChromaticCluster => "chromatic_cluster",
        }
    }
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
    #[serde(default)]
    pub classes: ClassSet,
    /// Fraction of trials that use a pitch class outside `classes`; the right answer is then "other".
    #[serde(default)]
    pub out_of_set_rate: f32,
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

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct IntervalIdParams {
    #[serde(default = "d_simple_intervals")]
    pub intervals: Vec<Interval>,
    #[serde(default = "d_melodic_dirs")]
    pub directions: Vec<IntervalDirection>,
    /// Range for the lower note. Default C3–C5.
    #[serde(default)]
    pub root_range: Option<NoteRange>,
    #[serde(default)]
    pub timbres: Vec<TimbreId>,
    #[serde(default = "d_ms_900")]
    pub note_ms: u32,
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
}

impl ExerciseKind {
    pub const ALL: [ExerciseKind; 11] = [
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
        }
    }

    pub fn pool(&self) -> Option<Pool> {
        match self {
            Exercise::PitchId(p) => Some(p.pool),
            Exercise::PitchPairing(p) => Some(p.pool),
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
}

/// Sing a named pitch with no reference.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema, TS)]
pub struct SingPitchTrial {
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
}

impl TrialSpec {
    pub fn stimulus(&self) -> &serde_json::Value {
        match self {
            TrialSpec::Identify(t) => &t.stimulus,
            TrialSpec::SingPitch(t) => &t.stimulus,
            TrialSpec::Pairing(t) => &t.stimulus,
            TrialSpec::Sung(t) => &t.stimulus,
        }
    }

    pub fn prompt(&self) -> &str {
        match self {
            TrialSpec::Identify(t) => &t.prompt,
            TrialSpec::SingPitch(t) => &t.prompt,
            TrialSpec::Pairing(t) => &t.label,
            TrialSpec::Sung(t) => &t.prompt,
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
             "params": {"classes": ["C", "E", "G#"], "out_of_set_rate": 0.15, "timbres": ["gm:0", "synth:triangle"]}},
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
                assert_eq!(p.mask, Mask::RandomTones);
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
