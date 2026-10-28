//! Types exchanged with the UI. TypeScript versions are generated into `ui/src/bindings` with
//! `cargo test -p trainer-app export_bindings`.

use serde::{Deserialize, Serialize};
use trainer_audio::{EngineInfo, EngineSettings, EngineStatus};
use trainer_core::grade::NoteResult;
use trainer_core::reports::{BlockReport, SessionReport};
use trainer_core::schema::{Choice, ExerciseKind, FeedbackMode, PitchRefMode, WalkCue};
use trainer_core::score::{Score, ScoreNote};
use trainer_core::storage::CalibrationRow;
use ts_rs::TS;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum Phase {
    Idle,
    /// Block title and instructions, waiting for Begin.
    BlockIntro,
    /// The pitch-memory clearing sound before a block (`scrub_end_s` says when it ends).
    Scrubbing,
    /// Rendering audio for the next trial.
    Preparing,
    /// Stimulus or mask playing; answers are not accepted yet.
    Listening,
    /// Waiting for an answer.
    Answering,
    /// Microphone open on a sung trial.
    Recording,
    /// Analysing the take.
    Grading,
    Feedback,
    BlockSummary,
    SessionSummary,
    Error,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum Interaction {
    Choice,
    SingPitch,
    Pairing,
    Sung,
    /// Write a heard melody in the score editor.
    Dictation,
    /// Press when the target note appears in a stream.
    Detect,
    /// Find a remembered tone again with the arrow keys (implicit note memory).
    Match,
    /// Hear a sequence, then type it back or play it on the pad (span).
    Span,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum ChoiceLayout {
    /// Twelve pitch classes laid out like a piano octave.
    PitchClasses,
    /// Notes with octaves across a keyboard range.
    Notes,
    /// Named answers in a grid.
    List,
    /// Colour swatches (incidental pairing).
    Colors,
    /// Scale degrees in a row, in pitch order (number keys pick the degree with that number).
    Degrees,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct LaneNote {
    pub midi: i32,
    /// Seconds from the start of the trial timeline.
    pub start_s: f32,
    pub dur_s: f32,
}

/// What the follow-along pitch lane draws for a sung trial.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct LaneSpec {
    pub notes: Vec<LaneNote>,
    /// Notes played as the reference before singing (echo / starting pitch).
    pub reference: Vec<LaneNote>,
    /// The other voice of a harmony exercise, played while singing.
    pub accompaniment: Vec<LaneNote>,
    pub beat_s: f32,
    pub beats_per_bar: u32,
    pub sing_start_s: f32,
    pub total_s: f32,
    pub midi_low: i32,
    pub midi_high: i32,
    /// Hide target bars until the attempt is graded (sight-singing style).
    pub targets_hidden: bool,
    /// Whether beats are clicked (draw the beat grid and count-in).
    pub metronome: bool,
    /// Labelled parts of the timeline (establish key, first note, count-in, sing).
    pub sections: Vec<LaneSection>,
    /// Move a cursor over the score while singing (sight singing).
    pub cursor: bool,
    /// A flash score is hidden from this time on (seconds on the timeline) while singing.
    pub score_until_s: Option<f32>,
    /// Indexes of notes sung silently (inner hearing).
    pub silent: Vec<u32>,
    /// Delayed pitch matching: from here (seconds on the timeline) to the singing, the note is
    /// held in the head.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub hold_from_s: Option<f32>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct LaneSection {
    pub label: String,
    /// Seconds from the start of the trial timeline.
    pub start_s: f32,
    pub end_s: f32,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct TrialView {
    pub kind: ExerciseKind,
    pub interaction: Interaction,
    pub prompt: String,
    pub choices: Vec<Choice>,
    pub layout: ChoiceLayout,
    pub deadline_ms: Option<u32>,
    pub can_replay: bool,
    pub can_reference: bool,
    pub lane: Option<LaneSpec>,
    /// Show the live pitch line: with continuous feedback, when singing along, and on a staff
    /// whenever the block shows results.
    pub show_live_pitch: bool,
    /// Sung trials: how pitches are compared with the targets. On a staff the trace is tinted by
    /// its distance from the written notes, unless the grading is relative to the singer's own
    /// start, and drawn in the written octave when the octave does not count.
    pub grading_mode: Option<PitchRefMode>,
    /// Pairing label, shown once `label_at_s` has passed on the master clock.
    pub label: Option<String>,
    pub label_at_s: Option<f64>,
    /// Master-clock time (s) at which this trial's timeline starts (sung trials).
    pub timeline_start_s: Option<f64>,
    /// Master-clock times (s) of the response window.
    pub response_start_s: Option<f64>,
    pub response_end_s: Option<f64>,
    pub attempt: u32,
    pub assisted: bool,
    /// Staff notation for sight singing (drawn instead of the pitch lane), or the editor's
    /// starting score for dictation.
    pub score: Option<Score>,
    /// Fixed-do name per `score.notes` entry (None for rests), in the app's chromatic fixed-do
    /// style. Used by the optional sight-singing label aid.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub solfege: Option<Vec<Option<String>>>,
    /// Dictation: hearings so far and the limit (0 for none).
    pub plays: u32,
    pub max_plays: u32,
    /// Screen tint for the played note (pairing, or the detection target).
    pub flash_color: Option<String>,
    /// Detection: how many tones are in the stream.
    pub detect_total: Option<u32>,
    /// Detection: the tone playing now, 1-based; 0 during the prompt.
    pub detect_index: Option<u32>,
    /// A mark during a detection stream: `hit`, `miss` or `false`.
    pub detect_mark: Option<String>,
    /// Note memory: arrow presses so far (the position itself is not shown).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub match_moves: Option<u32>,
    /// Span: `digits` or `tones`, the current length, and which sequence of the ladder this is.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub span_mode: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub span_length: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub span_item: Option<u32>,
    /// Sing by colour: the swatch to sing.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub prompt_color: Option<String>,
    /// Hold and compare: the target fades in under the voice until this master-clock time, and
    /// the tuner compares the voice with `compare_midi`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub compare_until_s: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub compare_midi: Option<i32>,
    /// Pitch walk: the start and the arrows, each shown from `walk_start_s + at_ms / 1000` on
    /// the master clock.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub walk: Option<Vec<WalkCue>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub walk_start_s: Option<f64>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct FeedbackView {
    /// Answers lit while their note sounds, e.g. each degree as it walks to the tonic.
    pub lit: Vec<LitMark>,
    pub correct: Option<bool>,
    pub summary: String,
    pub correct_choice: Option<String>,
    pub answered: Option<String>,
    pub cents: Option<f32>,
    pub notes: Vec<NoteResult>,
    /// Graded pitch trace: (seconds on the trial timeline, MIDI or null when unvoiced).
    pub trace: Vec<(f32, Option<f32>)>,
    pub can_retry: bool,
    pub can_sing_along: bool,
    pub can_listen: bool,
    pub can_show_answer: bool,
    /// The sung attempt can be played back.
    pub can_play_answer: bool,
    /// Answers can be clicked to hear them.
    pub can_hear_choices: bool,
    /// A single sung note: the target for the live tuner, to practise locking on.
    pub tuner: Option<TunerTarget>,
    /// Moves on by itself after this many ms unless the user interacts.
    pub auto_advance_ms: Option<u32>,
    /// Results are withheld by the block's feedback mode.
    pub hidden: bool,
    /// Dictation: the written answer and the melody, with a mark per note.
    pub dictation: Option<DictationView>,
    /// A wrong answer is not marked. The right answer is still shown.
    pub errorless: bool,
    /// Tint behind the result, the colour of the note that played.
    pub flash_color: Option<String>,
    /// The note's colour beside a named result; unlike `flash_color` the marks stay.
    pub answer_color: Option<String>,
}

/// The note to lock on to with the tuner after singing a single note.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct TunerTarget {
    /// Pitch class, 0 = C.
    pub pitch_class: u8,
    /// The exact note when the octave counts; otherwise the nearest note of the class is the target.
    pub midi: Option<i32>,
    /// E.g. `A♭`, or `A♭3` for an exact note.
    pub name: String,
    /// The note "Hear the note" plays, for a drone.
    pub reference_midi: Option<i32>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct DictationView {
    /// The melody that was played.
    pub target: Score,
    /// What was written.
    pub answer: Score,
    /// Per `target.notes` entry: `correct`, `wrong`, `missed`, or null for rests.
    pub target_marks: Vec<Option<String>>,
    /// Per `answer.notes` entry: `correct`, `wrong`, `extra`, or null for rests.
    pub answer_marks: Vec<Option<String>>,
}

#[derive(Clone, Debug, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct RunnerState {
    pub phase: Phase,
    pub session_id: Option<String>,
    pub title: String,
    pub notes: Option<String>,
    pub block_index: u32,
    pub block_count: u32,
    pub block_title: String,
    pub block_instructions: Option<String>,
    pub block_kind: Option<ExerciseKind>,
    pub trial_index: u32,
    pub trial_count: u32,
    pub feedback_mode: FeedbackMode,
    pub done_in_block: u32,
    pub correct_in_block: u32,
    pub trial: Option<TrialView>,
    pub feedback: Option<FeedbackView>,
    pub block_report: Option<BlockReport>,
    pub session_report: Option<SessionReport>,
    pub error: Option<String>,
    /// Master-clock time at which the clearing sound before a block ends.
    pub scrub_end_s: Option<f64>,
    /// Every block of the session, for the overview.
    pub outline: Vec<BlockOutline>,
    /// Increments on every change.
    #[ts(type = "number")]
    pub seq: u64,
}

/// An answer to light from `from_s` to `to_s` (master-clock seconds) while its note plays.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, TS)]
pub struct LitMark {
    pub id: String,
    pub from_s: f64,
    pub to_s: f64,
}

/// One block of the running session, as the overview lists it.
#[derive(Clone, Debug, Serialize, Deserialize, TS)]
pub struct BlockOutline {
    pub title: String,
    /// The exercise, e.g. "Name the note".
    pub kind: String,
    pub trials: u32,
    pub feedback: FeedbackMode,
    pub instructions: Option<String>,
    /// The course the block belongs to, its level, what the level is and what the course is for.
    pub course: Option<String>,
    pub level: Option<u32>,
    pub level_summary: Option<String>,
    pub course_about: Option<String>,
    /// A check or test whose results count towards a level or a test.
    pub check: bool,
    /// `upcoming`, `current` or `done`.
    pub status: String,
    pub done: u32,
    pub correct: u32,
}

impl BlockOutline {
    pub fn of(block: &trainer_core::schema::BlockSpec) -> Self {
        let course = block
            .course
            .as_ref()
            .and_then(|c| Some((c, trainer_core::courses::find_course(&c.course)?)));
        BlockOutline {
            title: block.display_title(),
            kind: block.exercise.kind().title().to_string(),
            trials: block.count,
            feedback: block.effective_feedback(),
            instructions: block.instructions.clone(),
            course: course.as_ref().map(|(_, c)| c.title.clone()),
            level: course
                .as_ref()
                .filter(|(_, c)| c.levels.len() > 1)
                .map(|(r, _)| r.level),
            level_summary: course.as_ref().and_then(|(r, c)| {
                c.levels
                    .get((r.level as usize).saturating_sub(1))
                    .map(|l| l.summary.clone())
            }),
            course_about: course.as_ref().map(|(_, c)| c.description.clone()),
            check: block.course.as_ref().map(|c| c.verify).unwrap_or(false) || block.assessment,
            status: "upcoming".into(),
            done: 0,
            correct: 0,
        }
    }
}

impl Default for RunnerState {
    fn default() -> Self {
        RunnerState {
            phase: Phase::Idle,
            session_id: None,
            title: String::new(),
            notes: None,
            block_index: 0,
            block_count: 0,
            block_title: String::new(),
            block_instructions: None,
            block_kind: None,
            trial_index: 0,
            trial_count: 0,
            feedback_mode: FeedbackMode::AfterTrial,
            done_in_block: 0,
            correct_in_block: 0,
            trial: None,
            feedback: None,
            block_report: None,
            session_report: None,
            error: None,
            scrub_end_s: None,
            outline: Vec::new(),
            seq: 0,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, TS)]
#[serde(tag = "kind", rename_all = "snake_case")]
#[ts(export)]
pub enum RunnerCommand {
    Begin,
    Answer {
        choice: String,
    },
    Next,
    Replay,
    Reference,
    ShowAnswer,
    Listen,
    /// Play back the recording of the attempt (or the written answer in dictation).
    PlayAnswer,
    /// Dictation: check the written melody.
    SubmitScore {
        notes: Vec<ScoreNote>,
    },
    /// Dictation: play the written notes.
    PlayScore {
        notes: Vec<ScoreNote>,
    },
    /// After answering: play what an answer sounds like.
    PlayChoice {
        choice: String,
    },
    /// Detection: the target just played.
    Tap,
    /// Note memory: move up (positive) or down this many steps.
    Step {
        delta: i32,
    },
    SingAlong,
    Retry,
    Skip,
    /// Leave the current part (block) and go on to the next.
    SkipPart,
    Abort,
    Close,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, TS)]
#[serde(tag = "source", rename_all = "snake_case")]
#[ts(export)]
pub enum StartRequest {
    #[ts(type = "{ source: \"preset\", id: string, params?: Record<string, unknown> }")]
    Preset {
        id: String,
        /// Merged into every block's params, e.g. the label and cadence options of a card.
        #[serde(default)]
        params: Option<serde_json::Value>,
    },
    Queued {
        session_id: String,
    },
    Repeat {
        session_id: String,
    },
    #[ts(type = "{ source: \"spec\", spec: Record<string, unknown> }")]
    Spec {
        spec: serde_json::Value,
    },
    /// Today's set for a category (absolute pitch by default), composed from its active courses.
    DailySet {
        #[serde(default)]
        #[ts(optional)]
        category: Option<String>,
    },
    /// One level of a course. `minutes` and `checks` default to the course's daily plan entry.
    CourseLevel {
        course: String,
        level: u32,
        #[serde(default)]
        #[ts(optional)]
        minutes: Option<f32>,
        #[serde(default)]
        #[ts(optional)]
        checks: Option<bool>,
    },
    /// A session that was ended early, from where it stopped.
    Resume {
        session_id: String,
    },
    /// A custom level of a workshop, saved (its id set) or a one-off try.
    Custom {
        level: trainer_core::workshops::CustomLevel,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct LivePoint {
    /// Master-clock time in seconds (latency compensated).
    pub t: f64,
    pub midi: Option<f32>,
    pub clarity: f32,
    pub rms_db: f32,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct LiveBatch {
    /// Master-clock time when the batch was sent.
    pub now_s: f64,
    pub points: Vec<LivePoint>,
    pub peak: f32,
}

#[derive(Clone, Debug, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct AppSettings {
    #[serde(default = "d_a4")]
    pub a4_hz: f64,
    #[serde(default)]
    pub audio: EngineSettings,
    #[serde(default = "d_true")]
    pub auto_advance_correct: bool,
    #[serde(default = "d_auto_ms")]
    pub auto_advance_ms: u32,
    #[serde(default = "d_true")]
    pub keep_recordings: bool,
    #[serde(default = "d_retention")]
    pub recording_retention_days: u32,
    #[serde(default)]
    pub onboarding_done: bool,
}

fn d_a4() -> f64 {
    440.0
}
fn d_true() -> bool {
    true
}
fn d_auto_ms() -> u32 {
    900
}
fn d_retention() -> u32 {
    60
}

impl Default for AppSettings {
    fn default() -> Self {
        serde_json::from_str("{}").unwrap()
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct AppInfo {
    pub version: String,
    pub data_dir: String,
    /// Command and arguments an agent uses to start the MCP server.
    pub mcp_command: String,
    pub mcp_args: Vec<String>,
    pub soundfont: Option<String>,
    pub banks: Vec<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct AudioView {
    pub settings: EngineSettings,
    pub info: Option<EngineInfo>,
    pub status: Option<EngineStatus>,
    pub error: Option<String>,
    pub roundtrip_ms: Option<f64>,
    pub calibration: Option<CalibrationRow>,
}

#[derive(Clone, Debug, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct PresetView {
    pub id: String,
    pub title: String,
    pub category: String,
    pub description: String,
    pub source: String,
    pub trial_count: u32,
    pub estimated_minutes: f64,
    pub block_titles: Vec<String>,
}

/// The melody library, for the Library page.
#[derive(Clone, Debug, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct LibraryView {
    pub folder: String,
    pub pieces: Vec<LibraryEntry>,
    /// Files that could not be read.
    pub errors: Vec<trainer_core::storage::LibraryFile>,
}

#[derive(Clone, Debug, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct LibraryEntry {
    pub id: String,
    pub title: String,
    pub collection: Option<String>,
    pub tags: Vec<String>,
    pub key: String,
    pub time: String,
    pub tempo_bpm: Option<f32>,
    pub bars: u32,
    /// Part names, top part first.
    pub parts: Vec<String>,
    /// Lowest and highest notes, e.g. `D4`.
    pub low: Option<String>,
    pub high: Option<String>,
    pub file: String,
}

/// A file chosen in the import dialog.
#[derive(Clone, Debug, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct ImportFile {
    pub name: String,
    pub data: Vec<u8>,
}

#[derive(Clone, Debug, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct ImportResult {
    pub name: String,
    /// Where it was written in the library folder.
    pub file: Option<String>,
    pub error: Option<String>,
}

#[cfg(test)]
mod export_more {
    use ts_rs::TS;

    /// Types used by commands but not reachable from the exported views.
    #[test]
    fn export_bindings_extra() {
        let cfg = ts_rs::Config::from_env();
        trainer_audio::device::HostInfo::export_all(&cfg).unwrap();
        trainer_audio::calibrate::CalibrationResult::export_all(&cfg).unwrap();
        trainer_core::storage::RangeProfile::export_all(&cfg).unwrap();
        trainer_core::stats::StatsQuery::export_all(&cfg).unwrap();
        trainer_core::stats::StatsRow::export_all(&cfg).unwrap();
        trainer_core::storage::TrialRow::export_all(&cfg).unwrap();
        trainer_core::schema::Anchor::export_all(&cfg).unwrap();
        trainer_core::courses::ApDashboard::export_all(&cfg).unwrap();
        trainer_core::aptitude::AptitudeProfile::export_all(&cfg).unwrap();
        trainer_core::courses::DailySet::export_all(&cfg).unwrap();
        trainer_core::courses::ApPrefs::export_all(&cfg).unwrap();
        trainer_core::labels::LabelSet::export_all(&cfg).unwrap();
        trainer_core::speech::SpeechReport::export_all(&cfg).unwrap();
        trainer_core::workshops::WorkshopView::export_all(&cfg).unwrap();
        trainer_core::courses::Resumable::export_all(&cfg).unwrap();
        trainer_core::workshops::CustomLevelView::export_all(&cfg).unwrap();
        crate::views::RunnerState::export_all(&cfg).unwrap();
    }
}
