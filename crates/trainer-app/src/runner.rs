//! The session runner: a thread that walks a session block by block and trial by trial, plays
//! and records through the engine, grades with `trainer-core`, stores everything and publishes
//! a `RunnerState` for the UI after every change.

use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};

use anyhow::{Result, anyhow};
use crossbeam_channel::{Receiver, RecvTimeoutError, Sender};
use parking_lot::RwLock;
use serde_json::json;
use trainer_audio::{AudioEngine, Bus, SoundBuffer};
use trainer_core::generate::{block_seed, default_mode, generate_block};
use trainer_core::grade::{
    GradingConfig, TrialOutcome, grade_choice, grade_dictation, grade_sing_pitch, grade_sung,
};
use trainer_core::pitch::{PitchTrack, TrackOptions, track};
use trainer_core::reports::{session_report, session_summary};
use trainer_core::schema::*;
use trainer_core::score::{ScoreNote, note_events};
use trainer_core::storage::{Db, SessionStatus, TrialRow, db_path, new_id, now};
use trainer_core::theory::midi_to_hz;

use crate::state::Core;
use crate::views::*;

pub enum RunnerMsg {
    Start {
        session_id: String,
        spec: SessionSpec,
        /// Resume at this block, with this many of its questions already answered.
        from: Option<(usize, u32)>,
    },
    Cmd(RunnerCommand),
}

pub struct Runner {
    tx: Sender<RunnerMsg>,
    state: Arc<RwLock<RunnerState>>,
}

impl Runner {
    pub fn spawn(core: Arc<Core>) -> Runner {
        let (tx, rx) = crossbeam_channel::unbounded();
        let state = Arc::new(RwLock::new(RunnerState::default()));
        let st = state.clone();
        std::thread::Builder::new()
            .name("session-runner".into())
            .spawn(move || runner_main(core, rx, st))
            .expect("spawn runner");
        Runner { tx, state }
    }

    pub fn state(&self) -> RunnerState {
        self.state.read().clone()
    }

    pub fn is_active(&self) -> bool {
        self.state.read().phase != Phase::Idle
    }

    pub fn start(&self, session_id: String, spec: SessionSpec) -> Result<()> {
        self.start_from(session_id, spec, None)
    }

    /// Starts a session, or resumes one at a block with some of its questions done.
    pub fn start_from(
        &self,
        session_id: String,
        spec: SessionSpec,
        from: Option<(usize, u32)>,
    ) -> Result<()> {
        if self.is_active() {
            return Err(anyhow!("a session is already running"));
        }
        self.tx
            .send(RunnerMsg::Start {
                session_id,
                spec,
                from,
            })
            .map_err(|_| anyhow!("runner stopped"))
    }

    pub fn command(&self, cmd: RunnerCommand) {
        let _ = self.tx.send(RunnerMsg::Cmd(cmd));
    }
}

fn runner_main(core: Arc<Core>, rx: Receiver<RunnerMsg>, state: Arc<RwLock<RunnerState>>) {
    let db = match Db::open(&db_path(&core.data_dir)) {
        Ok(db) => db,
        Err(e) => {
            tracing::error!("runner cannot open the database: {e:#}");
            return;
        }
    };
    let mut last_labeled = db.last_labeled_reference_at().ok().flatten();
    loop {
        let Ok(msg) = rx.recv() else { return };
        let RunnerMsg::Start {
            session_id,
            spec,
            from,
        } = msg
        else {
            continue;
        };
        let mut s = SessionRun {
            core: core.clone(),
            db: &db,
            rx: &rx,
            state: state.clone(),
            session_id,
            spec,
            last_labeled,
            stream_next: None,
            hold_until: None,
            from,
            skip_part: std::cell::Cell::new(false),
        };
        let result = s.run();
        last_labeled = s.last_labeled;
        match result {
            Ok(()) => {}
            Err(Stop::Aborted) => {
                if let Some(e) = core.engine() {
                    e.stop_bus(Bus::Prompt);
                    e.stop_bus(Bus::Backing);
                    e.stop_bus(Bus::Guide);
                }
                let summary = session_report(&db, &s.session_id)
                    .map(|r| session_summary(&r))
                    .unwrap_or_default();
                let _ = db.finish_session(&s.session_id, SessionStatus::Abandoned, &summary);
                // Progress may have moved, and today's set can now be resumed.
                if let Err(e) = trainer_core::courses::refresh_progress(&db) {
                    tracing::warn!("course progress: {e:#}");
                }
                core.emit("courses-changed", ());
                s.set(|st| {
                    *st = RunnerState {
                        seq: st.seq,
                        ..RunnerState::default()
                    }
                });
            }
            Err(Stop::Failed(e)) => {
                let summary = session_report(&db, &s.session_id)
                    .map(|r| session_summary(&r))
                    .unwrap_or_default();
                let _ = db.finish_session(&s.session_id, SessionStatus::Abandoned, &summary);
                s.set(|st| {
                    st.phase = Phase::Error;
                    st.error = Some(format!("{e:#}"));
                });
                // Wait for the user to close the error.
                loop {
                    match rx.recv() {
                        Ok(RunnerMsg::Cmd(
                            RunnerCommand::Close | RunnerCommand::Abort | RunnerCommand::Next,
                        ))
                        | Err(_) => break,
                        _ => {}
                    }
                }
                s.set(|st| {
                    *st = RunnerState {
                        seq: st.seq,
                        ..RunnerState::default()
                    }
                });
            }
        }
    }
}

enum Stop {
    Aborted,
    Failed(anyhow::Error),
}

impl From<anyhow::Error> for Stop {
    fn from(e: anyhow::Error) -> Self {
        Stop::Failed(e)
    }
}

type R<T> = std::result::Result<T, Stop>;

/// What the user chose after a trial's feedback.
enum After {
    Next,
    Retry { assisted: bool },
}

/// Per-trial data that goes into the stored row besides the outcome.
#[derive(Default)]
struct Extras {
    start_frame: Option<u64>,
    labeled_reference: bool,
    capture: Option<(Vec<f32>, u32)>,
    track: Option<PitchTrack>,
    calibrated: bool,
}

struct SessionRun<'a> {
    core: Arc<Core>,
    db: &'a Db,
    rx: &'a Receiver<RunnerMsg>,
    state: Arc<RwLock<RunnerState>>,
    session_id: String,
    spec: SessionSpec,
    last_labeled: Option<chrono::DateTime<chrono::Utc>>,
    /// The next question waits for this frame: a resolution still playing, and a pause after it.
    hold_until: Option<u64>,
    /// The output frame of the next tone in a paced stream.
    stream_next: Option<u64>,
    /// Resuming: the block to start at and how many of its questions were answered.
    from: Option<(usize, u32)>,
    /// Skip part was pressed: the rest of the current block is left out.
    skip_part: std::cell::Cell<bool>,
}

/// A block answered without help starts with the clearing sound if a named tone was heard this
/// recently.
/// How fast the last trial's sound fades when the next one starts.
const TRIAL_FADE_MS: f64 = 120.0;
const SCRUB_WITHIN: Duration = Duration::from_secs(5 * 60);

fn secs(engine: &AudioEngine, frame: u64) -> f64 {
    frame as f64 / engine.sample_rate() as f64
}

/// The answers to light while `events` play from master-clock time `start_s`.
fn lit_marks(t: &IdentifyTrial, events: &[SoundEvent], start_s: f64) -> Vec<LitMark> {
    events
        .iter()
        .filter_map(|e| {
            let (_, id) = t.note_choices.iter().find(|(m, _)| *m == e.midi)?;
            Some(LitMark {
                id: id.clone(),
                from_s: start_s + e.start_ms / 1000.0,
                to_s: start_s + (e.start_ms + e.dur_ms) / 1000.0,
            })
        })
        .collect()
}

/// When `events` played from `start` have finished, plus the pause before anything else.
fn end_frame(engine: &AudioEngine, events: &[SoundEvent], start: u64) -> u64 {
    let end_ms = events
        .iter()
        .map(|e| e.start_ms + e.dur_ms)
        .fold(0.0, f64::max);
    start + engine.frames_from_ms(end_ms + AFTER_SOUND_PAUSE_MS)
}

/// Silence after a resolution or other answer sound before the next question starts.
const AFTER_SOUND_PAUSE_MS: f64 = 500.0;

impl<'a> SessionRun<'a> {
    fn set(&self, f: impl FnOnce(&mut RunnerState)) {
        let snapshot = {
            let mut st = self.state.write();
            f(&mut st);
            st.seq += 1;
            st.clone()
        };
        self.core.emit("runner-state", snapshot);
    }

    /// Waits for a command. `Abort` unwinds the session; `None` means the timeout elapsed.
    fn wait(&self, timeout: Option<Duration>) -> R<Option<RunnerCommand>> {
        let deadline = timeout.map(|t| Instant::now() + t);
        loop {
            let msg = match deadline {
                Some(d) => {
                    let left = d.saturating_duration_since(Instant::now());
                    match self.rx.recv_timeout(left) {
                        Ok(m) => m,
                        Err(RecvTimeoutError::Timeout) => return Ok(None),
                        Err(RecvTimeoutError::Disconnected) => return Err(Stop::Aborted),
                    }
                }
                None => self.rx.recv().map_err(|_| Stop::Aborted)?,
            };
            match msg {
                RunnerMsg::Cmd(RunnerCommand::Abort) => return Err(Stop::Aborted),
                // Leaving a part: whatever is waiting sees a skip, and the block ends.
                RunnerMsg::Cmd(RunnerCommand::SkipPart) => {
                    self.skip_part.set(true);
                    return Ok(Some(RunnerCommand::Skip));
                }
                RunnerMsg::Cmd(c) => return Ok(Some(c)),
                RunnerMsg::Start { .. } => {}
            }
        }
    }

    /// Waits until the master clock passes `frame`, handling Skip (returns true) and Abort.
    fn wait_until_frame(&self, engine: &AudioEngine, frame: u64) -> R<bool> {
        loop {
            let now = engine.now();
            if now >= frame {
                return Ok(false);
            }
            let left = (frame - now) as f64 / engine.sample_rate() as f64;
            if let Some(RunnerCommand::Skip) =
                self.wait(Some(Duration::from_secs_f64(left.min(0.05))))?
            {
                return Ok(true);
            }
            if !engine.is_running() {
                return Err(Stop::Failed(anyhow!("the audio device stopped")));
            }
        }
    }

    fn run(&mut self) -> R<()> {
        let engine = self.core.ensure_engine()?;
        self.db.mark_session_running(&self.session_id)?;
        let ctx = self.core.gen_context();
        let base_seed = self
            .spec
            .seed
            .unwrap_or_else(|| now().timestamp_micros() as u64);
        let blocks = self.spec.blocks.clone();
        let auto = self.core.settings.read().clone();
        self.set(|st| {
            *st = RunnerState {
                seq: st.seq,
                session_id: Some(self.session_id.clone()),
                title: self.spec.title.clone(),
                notes: self.spec.notes.clone(),
                block_count: blocks.len() as u32,
                outline: blocks.iter().map(BlockOutline::of).collect(),
                ..RunnerState::default()
            };
        });
        let (from_block, done_before) = self.from.unwrap_or((0, 0));
        for (bi, block) in blocks.iter().enumerate() {
            // Resuming: the blocks done before are passed over.
            if bi < from_block {
                self.set(|st| {
                    if let Some(o) = st.outline.get_mut(bi) {
                        o.status = "done".into();
                    }
                });
                continue;
            }
            self.skip_part.set(false);
            let mut trials = generate_block(block, block_seed(base_seed, bi, block), &ctx)
                .map_err(|e| anyhow!("block {}: {e}", bi + 1))?;
            // The questions answered before it stopped are not asked again.
            let offset = if bi == from_block {
                (done_before as usize).min(trials.len().saturating_sub(1))
            } else {
                0
            };
            trials.drain(..offset);
            let feedback = block.effective_feedback();
            self.set(|st| {
                st.phase = Phase::BlockIntro;
                st.block_index = bi as u32;
                if let Some(o) = st.outline.get_mut(bi) {
                    o.status = "current".into();
                    o.trials = trials.len() as u32;
                }
                st.block_title = block.display_title();
                st.block_instructions = block.instructions.clone();
                st.block_kind = Some(block.exercise.kind());
                st.trial_index = 0;
                st.trial_count = trials.len() as u32;
                st.feedback_mode = feedback;
                st.done_in_block = 0;
                st.correct_in_block = 0;
                st.trial = None;
                st.feedback = None;
                st.block_report = None;
            });
            loop {
                match self.wait(None)? {
                    Some(RunnerCommand::Begin | RunnerCommand::Next) => break,
                    Some(RunnerCommand::Skip) => break,
                    _ => {}
                }
            }
            self.stream_next = None;
            let mut requeues: std::collections::BTreeMap<String, u32> =
                std::collections::BTreeMap::new();
            let recent_label = self
                .last_labeled
                .map(|l| {
                    (now() - l)
                        .to_std()
                        .map(|d| d < SCRUB_WITHIN)
                        .unwrap_or(true)
                })
                .unwrap_or(false);
            // Only absolute-pitch blocks need pitch memory cleared.
            let ap_block = matches!(
                block.exercise.kind(),
                ExerciseKind::PitchId
                    | ExerciseKind::SingPitch
                    | ExerciseKind::TonalCenterId
                    | ExerciseKind::AnchorSong
                    | ExerciseKind::PitchDetect
                    | ExerciseKind::ChordColorId
            );
            let scrub = match block.pre_mask {
                Some(m) => m,
                None if ap_block && !feedback.shows_trial_result() && recent_label => {
                    Mask::ShepardGlissando
                }
                None => Mask::None,
            };
            self.pre_mask(&engine, scrub)?;
            let grading = |t: &TrialSpec| GradingConfig::resolve(&block.grading, default_mode(t));
            let mut ti = 0usize;
            while ti < trials.len() && !self.skip_part.get() {
                let trial = trials[ti].clone();
                let mut attempt = 0u32;
                // Practice plays the answer along, like a sing-along retry.
                let mut assisted = trial.is_practice();
                let mut last_correct: Option<bool> = None;
                loop {
                    // Whatever the last trial left ringing fades out quickly instead of
                    // sounding under this one, and nothing waits for it to end.
                    engine.fade_bus(Bus::Prompt, TRIAL_FADE_MS);
                    engine.fade_bus(Bus::Guide, TRIAL_FADE_MS);
                    self.hold_until = None;
                    self.set(|st| {
                        st.trial_index = ti as u32;
                        st.phase = Phase::Preparing;
                        st.feedback = None;
                    });
                    let cfg = grading(&trial);
                    let res = match &trial {
                        TrialSpec::Identify(t) => {
                            self.run_identify(&engine, t, feedback, attempt, block.assessment)
                        }
                        TrialSpec::SingPitch(t) => {
                            self.run_sing_pitch(&engine, t, &cfg, feedback, attempt, &ctx)
                        }
                        TrialSpec::Pairing(t) => self.run_pairing(&engine, t),
                        TrialSpec::Sung(t) => {
                            self.run_sung(&engine, t, &cfg, feedback, attempt, assisted)
                        }
                        TrialSpec::Dictation(t) => self.run_dictation(&engine, t, attempt),
                        TrialSpec::Detect(t) => self.run_detect(&engine, t, feedback, attempt),
                    }?;
                    let Some((outcome, extras)) = res else {
                        break; // skipped
                    };
                    last_correct = outcome.correct;
                    let row = self.store(
                        block,
                        bi,
                        ti + offset,
                        attempt,
                        assisted,
                        &trial,
                        &outcome,
                        &extras,
                        feedback,
                    )?;
                    if !assisted {
                        self.set(|st| {
                            st.done_in_block += 1;
                            if outcome.correct == Some(true) {
                                st.correct_in_block += 1;
                            }
                            if let Some(o) = st.outline.get_mut(bi) {
                                o.done += 1;
                                o.correct += (outcome.correct == Some(true)) as u32;
                            }
                        });
                    }
                    match self
                        .feedback(&engine, &trial, &outcome, &extras, &row, feedback, &auto)?
                    {
                        After::Next => break,
                        After::Retry { assisted: a } => {
                            attempt += 1;
                            assisted = a || trial.is_practice();
                        }
                    }
                }
                if last_correct == Some(false) {
                    let key = requeue_key(&trial);
                    let n = requeues.entry(key).or_insert(0);
                    if *n < 2 {
                        if let Some(extra) = trainer_core::generate::requeue_trial(&trial) {
                            *n += 1;
                            let at = (ti + 4).min(trials.len());
                            trials.insert(at, extra);
                            let count = trials.len() as u32;
                            self.set(|st| st.trial_count = count);
                        }
                    }
                }
                ti += 1;
            }
            // A skipped part goes straight on to the next.
            if self.skip_part.get() {
                self.set(|st| {
                    if let Some(o) = st.outline.get_mut(bi) {
                        o.status = "skipped".into();
                    }
                });
                continue;
            }
            let report = session_report(self.db, &self.session_id)?;
            let block_report = report.blocks.get(bi).cloned();
            let show = feedback != FeedbackMode::EndOfSession;
            self.set(|st| {
                st.phase = Phase::BlockSummary;
                if let Some(o) = st.outline.get_mut(bi) {
                    o.status = "done".into();
                }
                st.trial = None;
                st.feedback = None;
                st.block_report = if show { block_report } else { None };
            });
            loop {
                match self.wait(None)? {
                    Some(RunnerCommand::Next | RunnerCommand::Begin | RunnerCommand::Skip) => break,
                    _ => {}
                }
            }
        }
        let report = session_report(self.db, &self.session_id)?;
        self.db.finish_session(
            &self.session_id,
            SessionStatus::Completed,
            &session_summary(&report),
        )?;
        // Courses move on as soon as a level's checks are passed.
        if let Err(e) = trainer_core::courses::refresh_progress(self.db) {
            tracing::warn!("course progress: {e:#}");
        }
        self.core.emit("courses-changed", ());
        let report = session_report(self.db, &self.session_id)?;
        self.set(|st| {
            st.phase = Phase::SessionSummary;
            st.trial = None;
            st.feedback = None;
            st.session_report = Some(report);
        });
        loop {
            match self.wait(None)? {
                Some(RunnerCommand::Close | RunnerCommand::Next) => break,
                _ => {}
            }
        }
        self.set(|st| {
            *st = RunnerState {
                seq: st.seq,
                ..RunnerState::default()
            }
        });
        let _ = engine;
        Ok(())
    }

    /// Plays a mask before a block (the clearing glissando), showing how long is left. Skip cuts it
    /// short.
    fn pre_mask(&mut self, engine: &AudioEngine, mask: Mask) -> R<()> {
        let Some(buf) = self
            .core
            .renderer()
            .render_mask(mask, now().timestamp_micros() as u64)
        else {
            return Ok(());
        };
        let at = engine.frame_after_ms(150.0);
        let end = at + buf.frames() as u64;
        engine.play(Arc::new(buf), at, 1.0, Bus::Prompt)?;
        self.set(|st| {
            st.phase = Phase::Scrubbing;
            st.scrub_end_s = Some(secs(engine, end));
        });
        if self.wait_until_frame(engine, end)? {
            engine.stop_bus(Bus::Prompt);
        }
        self.set(|st| st.scrub_end_s = None);
        Ok(())
    }

    fn base_view(
        &self,
        kind: ExerciseKind,
        interaction: Interaction,
        prompt: &str,
        attempt: u32,
    ) -> TrialView {
        TrialView {
            kind,
            interaction,
            prompt: prompt.to_string(),
            choices: Vec::new(),
            layout: ChoiceLayout::List,
            deadline_ms: None,
            can_replay: false,
            can_reference: false,
            lane: None,
            show_live_pitch: false,
            grading_mode: None,
            label: None,
            label_at_s: None,
            timeline_start_s: None,
            response_start_s: None,
            response_end_s: None,
            attempt,
            assisted: false,
            score: None,
            plays: 0,
            max_plays: 0,
            flash_color: None,
            detect_total: None,
            detect_index: None,
            detect_mark: None,
        }
    }

    // --- identification -------------------------------------------------------------------

    fn run_identify(
        &mut self,
        engine: &AudioEngine,
        t: &IdentifyTrial,
        feedback: FeedbackMode,
        attempt: u32,
        assessment: bool,
    ) -> R<Option<(TrialOutcome, Extras)>> {
        let renderer = self.core.renderer();
        let stim = Arc::new(renderer.render_events(&t.events));
        let mask = renderer
            .render_mask(t.mask_before, now().timestamp_micros() as u64)
            .map(Arc::new);
        let sr = engine.sample_rate() as f64;
        let mut at = engine.frame_after_ms(80.0);
        // Not over the end of the last answer's resolution.
        if let Some(h) = self.hold_until.take() {
            at = at.max(h);
        }
        // A paced stream keeps its schedule: the next tone waits for its slot.
        if let (Some(_), Some(next)) = (t.pace_ms, self.stream_next) {
            at = at.max(next);
        }
        if let Some(m) = &mask {
            engine.play(m.clone(), at, 1.0, Bus::Prompt)?;
            at += m.frames() as u64 + engine.frames_from_ms(150.0);
        }
        engine.play(stim.clone(), at, 1.0, Bus::Prompt)?;
        let onset_frame = at;
        if let Some(ioi) = t.pace_ms {
            self.stream_next = Some(onset_frame + engine.frames_from_ms(ioi as f64));
        }
        // In a stream, answers close shortly before the next tone.
        let deadline_ms = t.deadline_ms.or(t.pace_ms.map(|p| p.saturating_sub(250)));
        let onset = Instant::now()
            + Duration::from_secs_f64(onset_frame.saturating_sub(engine.now()) as f64 / sr);
        // Answers open at `answer_after_ms` (after a key-setting cadence). Response times and the
        // deadline count from there, or from a visual cue (incidental pairing) when later.
        let answer_from =
            onset + Duration::from_secs_f64(t.answer_after_ms.unwrap_or(0.0).max(0.0) / 1000.0);
        let cue_delay =
            Duration::from_millis(t.cue.as_ref().map(|c| c.delay_ms as u64).unwrap_or(0));
        let rt_from = answer_from.max(onset + cue_delay);
        let pitch_ids = t
            .choices
            .iter()
            .all(|c| c.id == "other" || c.id.parse::<trainer_core::theory::PitchClass>().is_ok());
        let long_label = t.choices.iter().any(|c| c.label.chars().count() > 5);
        // Names shown instead of note names: a piano layout or piano keys would give the note away.
        let names_hidden = t
            .choices
            .iter()
            .filter(|c| c.id != "other")
            .all(|c| c.word.is_some() && c.word.as_deref() == Some(c.label.as_str()));
        let layout = match t.exercise {
            ExerciseKind::IncidentalPairing | ExerciseKind::ChordColorId => ChoiceLayout::Colors,
            ExerciseKind::ScaleDegreeId => ChoiceLayout::Degrees,
            ExerciseKind::PitchId | ExerciseKind::TonalCenterId
                if pitch_ids && !long_label && !names_hidden =>
            {
                ChoiceLayout::PitchClasses
            }
            ExerciseKind::PitchId if !pitch_ids => ChoiceLayout::Notes,
            _ => ChoiceLayout::List,
        };
        let mut view = self.base_view(t.exercise, Interaction::Choice, &t.prompt, attempt);
        view.choices = t.choices.clone();
        view.layout = layout;
        view.deadline_ms = deadline_ms;
        // A test plays each tone once, whatever its answer limit; so does a single-pass trial.
        let can_replay = deadline_ms.is_none() && !assessment && !t.no_replay;
        view.score = t.score.clone();
        view.can_replay = can_replay;
        view.can_reference = t.reference.is_some();
        // The countdown starts where response times do.
        view.response_start_s = Some(
            secs(engine, onset_frame) + rt_from.saturating_duration_since(onset).as_secs_f64(),
        );
        if let Some(c) = &t.cue {
            view.label = Some(c.color.clone());
            view.label_at_s = Some(secs(engine, onset_frame) + c.delay_ms as f64 / 1000.0);
            view.can_replay = false;
        }
        let _ = feedback;
        self.set(|st| {
            st.phase = Phase::Listening;
            st.trial = Some(view);
        });
        let deadline = deadline_ms.map(|d| rt_from + Duration::from_millis(d as u64));
        let mut answering = false;
        let mut replays = 0u32;
        let (answer, rt) = loop {
            let now_i = Instant::now();
            if !answering && now_i >= answer_from {
                answering = true;
                self.set(|st| st.phase = Phase::Answering);
            }
            let wake = if !answering {
                Some(answer_from.saturating_duration_since(now_i))
            } else {
                deadline.map(|d| d.saturating_duration_since(now_i))
            };
            match self.wait(wake.map(|w| w.max(Duration::from_millis(1))))? {
                None => {
                    if answering && deadline.map(|d| Instant::now() >= d).unwrap_or(false) {
                        break (None, None);
                    }
                }
                Some(RunnerCommand::Answer { choice }) if answering => {
                    // Negative when a cued answer comes before its cue (anticipation).
                    let now_i = Instant::now();
                    let rt = if now_i >= rt_from {
                        now_i.duration_since(rt_from).as_secs_f64() * 1000.0
                    } else {
                        -(rt_from.duration_since(now_i).as_secs_f64() * 1000.0)
                    };
                    break (Some(choice), Some(rt));
                }
                Some(RunnerCommand::Replay) if answering && can_replay => {
                    replays += 1;
                    engine.play_now(stim.clone(), 1.0, Bus::Prompt)?;
                }
                Some(RunnerCommand::Reference) => {
                    if let Some(r) = &t.reference {
                        engine.play_now(Arc::new(renderer.render_events(r)), 1.0, Bus::Prompt)?;
                    }
                }
                Some(RunnerCommand::Skip) => {
                    engine.stop_bus(Bus::Prompt);
                    return Ok(None);
                }
                _ => {}
            }
        };
        let mut outcome = grade_choice(t, answer.as_deref(), rt);
        outcome.response["replays"] = json!(replays);
        let extras = Extras {
            start_frame: Some(onset_frame),
            ..Default::default()
        };
        Ok(Some((outcome, extras)))
    }

    // --- detection ---------------------------------------------------------------------------

    fn run_detect(
        &mut self,
        engine: &AudioEngine,
        t: &trainer_core::schema::DetectTrial,
        feedback: FeedbackMode,
        attempt: u32,
    ) -> R<Option<(TrialOutcome, Extras)>> {
        let renderer = self.core.renderer();
        let mut view = self.base_view(t.exercise, Interaction::Detect, &t.prompt, attempt);
        view.label = Some(t.target_label.clone());
        view.flash_color = t.target_color.clone();
        view.detect_total = Some(t.tones.len() as u32);
        view.detect_index = Some(0);
        self.set(|st| {
            st.phase = Phase::Listening;
            st.trial = Some(view);
        });
        let at = engine.frame_after_ms(80.0);
        engine.play(
            Arc::new(renderer.render_events(&t.prompt_events)),
            at,
            1.0,
            Bus::Prompt,
        )?;
        if let Some(rel) = &t.speech {
            self.play_speech(engine, rel, at);
        }
        let prompt_end = at + engine.frames_from_ms(t.prompt_ms as f64);
        if self.wait_until_frame(engine, prompt_end)? {
            engine.stop_bus(Bus::Prompt);
            return Ok(None);
        }
        engine.stop_bus(Bus::Prompt);
        let mut presses = Vec::with_capacity(t.tones.len());
        for (i, tone) in t.tones.iter().enumerate() {
            let mark_each =
                t.marks == trainer_core::schema::DetectMarks::Each && feedback.shows_trial_result();
            self.set(|st| {
                if let Some(view) = st.trial.as_mut() {
                    view.detect_index = Some(i as u32 + 1);
                    view.detect_mark = None;
                }
            });
            let start = engine.frame_after_ms(30.0);
            engine.play(
                Arc::new(renderer.render_events(&tone.events)),
                start,
                1.0,
                Bus::Prompt,
            )?;
            let onset = Instant::now()
                + Duration::from_secs_f64(
                    start.saturating_sub(engine.now()) as f64 / engine.sample_rate() as f64,
                );
            let window = Duration::from_millis(t.window_ms as u64);
            let mut press: Option<f64> = None;
            loop {
                let elapsed = Instant::now().saturating_duration_since(onset);
                if elapsed >= window {
                    break;
                }
                let wake = window.saturating_sub(elapsed);
                match self.wait(Some(wake.max(Duration::from_millis(1))))? {
                    Some(RunnerCommand::Tap) | Some(RunnerCommand::Answer { .. })
                        if press.is_none() =>
                    {
                        press = Some(
                            Instant::now()
                                .saturating_duration_since(onset)
                                .as_secs_f64()
                                * 1000.0,
                        );
                        if mark_each {
                            let mark = if tone.target { "hit" } else { "false" };
                            self.set(|st| {
                                if let Some(view) = st.trial.as_mut() {
                                    view.detect_mark = Some(mark.into());
                                }
                            });
                        }
                    }
                    Some(RunnerCommand::Skip) => {
                        engine.stop_bus(Bus::Prompt);
                        return Ok(None);
                    }
                    None => break,
                    _ => {}
                }
            }
            if mark_each && press.is_none() && tone.target {
                self.set(|st| {
                    if let Some(view) = st.trial.as_mut() {
                        view.detect_mark = Some("miss".into());
                    }
                });
            }
            presses.push(press);
        }
        engine.stop_bus(Bus::Prompt);
        let outcome = trainer_core::grade::grade_detect(t, &presses);
        Ok(Some((
            outcome,
            Extras {
                labeled_reference: true,
                ..Default::default()
            },
        )))
    }

    /// Plays a speech file if it is in the folder. A missing or unreadable file is skipped.
    fn play_speech(&self, engine: &AudioEngine, rel: &str, at: u64) {
        let dir = trainer_core::speech::speech_dir(&self.core.data_dir);
        let Some(path) = trainer_core::speech::resolve(&dir, rel) else {
            return;
        };
        match trainer_audio::speech::load(&path, engine.sample_rate()) {
            Ok(buf) => {
                let _ = engine.play(Arc::new(buf), at, 1.0, Bus::Prompt);
            }
            Err(e) => tracing::warn!("speech {}: {e:#}", path.display()),
        }
    }

    // --- passive pairing ---------------------------------------------------------------------

    fn run_pairing(
        &mut self,
        engine: &AudioEngine,
        t: &PairingTrial,
    ) -> R<Option<(TrialOutcome, Extras)>> {
        let renderer = self.core.renderer();
        let tone = Arc::new(renderer.render_events(&t.events));
        let at = engine.frame_after_ms(80.0);
        engine.play(tone.clone(), at, 1.0, Bus::Prompt)?;
        let mut view = self.base_view(
            ExerciseKind::PitchPairing,
            Interaction::Pairing,
            "Listen",
            0,
        );
        view.label = Some(t.label.clone());
        view.label_at_s = Some(secs(engine, at) + 0.35);
        view.flash_color = t.flash_color.clone();
        if let Some(rel) = &t.speech {
            self.play_speech(&engine, rel, at);
        }
        self.set(|st| {
            st.phase = Phase::Listening;
            st.trial = Some(view);
        });
        let end = at + tone.frames() as u64 + engine.frames_from_ms(t.gap_ms as f64);
        if self.wait_until_frame(engine, end)? {
            engine.stop_bus(Bus::Prompt);
            return Ok(None);
        }
        let mut outcome = TrialOutcome::passive();
        outcome.summary = format!("Heard {}", t.label);
        Ok(Some((
            outcome,
            Extras {
                start_frame: Some(at),
                labeled_reference: true,
                ..Default::default()
            },
        )))
    }

    // --- sung trials ------------------------------------------------------------------------

    fn capture_and_track(
        &self,
        engine: &AudioEngine,
        out_start: u64,
        out_end: u64,
        fmin: f32,
        fmax: f32,
    ) -> R<(Vec<f32>, PitchTrack, bool)> {
        let (roundtrip, calibrated) = self.core.roundtrip(engine);
        let a = (out_start as i64 + roundtrip).max(0) as u64;
        let b = (out_end as i64 + roundtrip).max(0) as u64;
        let samples = engine.capture(a, b, Duration::from_secs(3))?;
        let mut opts = TrackOptions::new(engine.sample_rate() as f32);
        opts.fmin = fmin;
        opts.fmax = fmax;
        opts.a4_hz = self.core.settings.read().a4_hz;
        let tr = track(&samples, &opts);
        Ok((samples, tr, calibrated))
    }

    fn run_sing_pitch(
        &mut self,
        engine: &AudioEngine,
        t: &SingPitchTrial,
        cfg: &GradingConfig,
        feedback: FeedbackMode,
        attempt: u32,
        ctx: &trainer_core::generate::GenContext,
    ) -> R<Option<(TrialOutcome, Extras)>> {
        let renderer = self.core.renderer();
        let mut at = engine.frame_after_ms(80.0);
        if let Some(m) = renderer.render_mask(t.pre_mask, now().timestamp_micros() as u64) {
            let frames = m.frames() as u64;
            engine.play(Arc::new(m), at, 1.0, Bus::Prompt)?;
            at += frames + engine.frames_from_ms(500.0);
        }
        let rec_start = at + engine.frames_from_ms(t.delay_ms as f64);
        let rec_end = rec_start + engine.frames_from_ms(t.response_ms as f64);
        let mut view = self.base_view(t.exercise, Interaction::SingPitch, &t.prompt, attempt);
        view.show_live_pitch = feedback == FeedbackMode::Continuous;
        view.response_start_s = Some(secs(engine, rec_start));
        view.response_end_s = Some(secs(engine, rec_end));
        view.timeline_start_s = Some(secs(engine, rec_start));
        self.set(|st| {
            st.phase = Phase::Recording;
            st.trial = Some(view);
        });
        if self.wait_until_frame(engine, rec_end + engine.frames_from_ms(150.0))? {
            return Ok(None);
        }
        self.set(|st| st.phase = Phase::Grading);
        let a4 = self.core.settings.read().a4_hz;
        let fmin = (midi_to_hz(ctx.comfortable.low.0 as f64 - 14.0, a4) as f32).max(55.0);
        let fmax = (midi_to_hz(ctx.comfortable.high.0 as f64 + 14.0, a4) as f32).min(1600.0);
        let lead = engine.frames_from_ms(150.0);
        let (samples, tr, calibrated) = self.capture_and_track(
            engine,
            rec_start.saturating_sub(lead),
            rec_end + lead,
            fmin,
            fmax,
        )?;
        let mut outcome = grade_sing_pitch(t, &tr, cfg, lead as f32 / engine.sample_rate() as f32);
        outcome.response["capture_offset_s"] = json!(lead as f64 / engine.sample_rate() as f64);
        let sr = engine.sample_rate();
        Ok(Some((
            outcome,
            Extras {
                start_frame: Some(rec_start),
                capture: Some((samples, sr)),
                track: Some(tr),
                calibrated,
                ..Default::default()
            },
        )))
    }

    fn run_sung(
        &mut self,
        engine: &AudioEngine,
        t: &SungTrial,
        cfg: &GradingConfig,
        feedback: FeedbackMode,
        attempt: u32,
        assisted: bool,
    ) -> R<Option<(TrialOutcome, Extras)>> {
        let renderer = self.core.renderer();
        let audio = renderer.render_sung_trial(t);
        let start = engine.frame_after_ms(200.0);
        let total = engine.frames_from_ms(t.total_ms());
        engine.play(Arc::new(audio.backing), start, 1.0, Bus::Backing)?;
        if audio.guide_audible || assisted {
            engine.play(Arc::new(audio.guide), start, 0.9, Bus::Guide)?;
        }
        let mut view = self.base_view(t.exercise, Interaction::Sung, &t.prompt, attempt);
        view.lane = Some(lane_for(t));
        view.score = t.score.clone();
        // On a staff the live line never gives the notes away, so it shows on every attempt
        // whose result the block shows.
        view.show_live_pitch = feedback == FeedbackMode::Continuous
            || assisted
            || (t.score.is_some() && feedback.shows_trial_result());
        view.grading_mode = Some(cfg.mode);
        view.timeline_start_s = Some(secs(engine, start));
        view.response_start_s = Some(secs(engine, start) + t.sing_start_ms() / 1000.0);
        view.response_end_s = Some(secs(engine, start + total));
        view.assisted = assisted;
        self.set(|st| {
            st.phase = Phase::Recording;
            st.trial = Some(view);
        });
        if self.wait_until_frame(engine, start + total + engine.frames_from_ms(120.0))? {
            engine.stop_bus(Bus::Backing);
            engine.stop_bus(Bus::Guide);
            return Ok(None);
        }
        self.set(|st| st.phase = Phase::Grading);
        let a4 = self.core.settings.read().a4_hz;
        let lo = t.notes.iter().map(|n| n.midi).min().unwrap_or(48);
        let hi = t.notes.iter().map(|n| n.midi).max().unwrap_or(72);
        // When the octave does not count, the voice may be an octave or two from the target:
        // track the whole singing range (E2–C6), not just around the written notes.
        let any_octave =
            !matches!(cfg.mode, trainer_core::schema::PitchRefMode::Absolute) || t.notes.is_empty();
        let (lo, hi) = if any_octave {
            (lo.min(40 + 9), hi.max(84 - 9))
        } else {
            (lo, hi)
        };
        let fmin = (midi_to_hz(lo as f64 - 9.0, a4) as f32).max(55.0);
        let fmax = (midi_to_hz(hi as f64 + 9.0, a4) as f32).min(1600.0);
        let (samples, tr, calibrated) =
            self.capture_and_track(engine, start, start + total, fmin, fmax)?;
        let outcome = grade_sung(t, &tr, cfg, (t.sing_start_ms() / 1000.0) as f32);
        let sr = engine.sample_rate();
        Ok(Some((
            outcome,
            Extras {
                start_frame: Some(start),
                capture: Some((samples, sr)),
                track: Some(tr),
                calibrated,
                ..Default::default()
            },
        )))
    }

    // --- melodic dictation -------------------------------------------------------------------

    fn run_dictation(
        &mut self,
        engine: &AudioEngine,
        t: &DictationTrial,
        attempt: u32,
    ) -> R<Option<(TrialOutcome, Extras)>> {
        let renderer = self.core.renderer();
        let melody = Arc::new(dictation_audio(&renderer, &t.playback));
        let key = (!t.playback.reference_beats.is_empty()).then(|| {
            // Just the cadence: the same timeline without melody, click or count-in.
            let mut k = t.playback.clone();
            k.notes.clear();
            k.guide = false;
            k.backing = Backing::None;
            k.count_in_beat = None;
            k.total_beats = k
                .reference_beats
                .iter()
                .map(|n| n.start_beat + n.beats)
                .fold(0.0, f32::max)
                + 0.5;
            Arc::new(renderer.render_sung_trial(&k).backing)
        });
        let start = engine.frame_after_ms(150.0);
        engine.play(melody.clone(), start, 1.0, Bus::Prompt)?;
        let mut plays = 1u32;
        let mut view = self.base_view(
            ExerciseKind::MelodicDictation,
            Interaction::Dictation,
            &t.prompt,
            attempt,
        );
        view.score = Some(t.given.clone());
        view.lane = Some(lane_for(&t.playback));
        view.can_replay = true;
        view.can_reference = key.is_some();
        view.plays = plays;
        view.max_plays = t.max_plays;
        view.timeline_start_s = Some(secs(engine, start));
        self.set(|st| {
            st.phase = Phase::Answering;
            st.trial = Some(view);
        });
        let began = Instant::now();
        loop {
            match self.wait(None)? {
                Some(RunnerCommand::Replay) if t.max_plays == 0 || plays < t.max_plays => {
                    engine.stop_bus(Bus::Prompt);
                    engine.stop_bus(Bus::Guide);
                    let at = engine.frame_after_ms(60.0);
                    engine.play(melody.clone(), at, 1.0, Bus::Prompt)?;
                    plays += 1;
                    let s = secs(engine, at);
                    self.set(|st| {
                        if let Some(v) = st.trial.as_mut() {
                            v.plays = plays;
                            v.timeline_start_s = Some(s);
                        }
                    });
                }
                Some(RunnerCommand::Reference) => {
                    if let Some(k) = &key {
                        engine.stop_bus(Bus::Prompt);
                        engine.play_now(k.clone(), 1.0, Bus::Prompt)?;
                    }
                }
                Some(RunnerCommand::PlayScore { notes }) => {
                    let ev = note_events(
                        &notes,
                        t.score.time,
                        t.score.tempo_bpm,
                        &t.playback.reference_timbre,
                    );
                    engine.stop_bus(Bus::Guide);
                    engine.play_now(Arc::new(renderer.render_events(&ev)), 1.0, Bus::Guide)?;
                }
                Some(RunnerCommand::SubmitScore { notes }) => {
                    engine.stop_bus(Bus::Prompt);
                    engine.stop_bus(Bus::Guide);
                    let mut outcome = grade_dictation(t, &notes, plays);
                    outcome.rt_ms = Some(began.elapsed().as_secs_f64() * 1000.0);
                    return Ok(Some((outcome, Extras::default())));
                }
                Some(RunnerCommand::Skip) => {
                    engine.stop_bus(Bus::Prompt);
                    engine.stop_bus(Bus::Guide);
                    return Ok(None);
                }
                _ => {}
            }
        }
    }

    // --- storing and feedback ----------------------------------------------------------------

    #[allow(clippy::too_many_arguments)]
    fn store(
        &mut self,
        block: &BlockSpec,
        bi: usize,
        ti: usize,
        attempt: u32,
        assisted: bool,
        trial: &TrialSpec,
        outcome: &TrialOutcome,
        extras: &Extras,
        feedback: FeedbackMode,
    ) -> R<TrialRow> {
        let id = new_id();
        let kind = stored_kind(block, trial);
        let mut tags: Vec<String> = kind.default_tags().iter().map(|s| s.to_string()).collect();
        for t in &block.tags {
            if !tags.contains(t) {
                tags.push(t.clone());
            }
        }
        if let Some(c) = &block.course {
            tags.push(c.tag());
            if c.verify {
                tags.push(trainer_core::courses::CHECK_TAG.into());
            }
        }
        let started = now();
        let secs_since = self
            .last_labeled
            .map(|l| (started - l).num_milliseconds() as f64 / 1000.0);
        // A trial reveals a labelled tone when its result (with the right answer) is shown.
        // Notation with a played reference (sight singing's first note, a dictation's answer)
        // also pairs sound with note names.
        let labeled = extras.labeled_reference
            || (feedback.shows_trial_result()
                && matches!(
                    trial,
                    TrialSpec::Identify(_) | TrialSpec::SingPitch(_) | TrialSpec::Dictation(_)
                ))
            || matches!(trial, TrialSpec::Sung(t) if t.score.is_some() && !t.reference_beats.is_empty());
        let row = TrialRow {
            id: id.clone(),
            session_id: self.session_id.clone(),
            block_idx: bi as u32,
            trial_idx: ti as u32,
            attempt,
            kind: kind.as_str().to_string(),
            protocol: self.spec.protocol.clone(),
            tags,
            stimulus: trial.stimulus().clone(),
            spec: serde_json::to_value(trial).unwrap_or_default(),
            pool: block.exercise.pool().map(|p| p.as_str().to_string()),
            feedback_mode: feedback.as_str().to_string(),
            response: outcome.response.clone(),
            metrics: outcome.metrics.clone(),
            summary: outcome.summary.clone(),
            correct: outcome.correct,
            score: outcome.score,
            rt_ms: outcome.rt_ms,
            assisted,
            assessment: block.assessment,
            labeled_reference: labeled,
            started_at: started.to_rfc3339(),
            start_frame: extras.start_frame.map(|f| f as i64),
            device_profile: self.core.engine().map(|e| self.core.device_key(&e)),
            calibrated: extras.calibrated,
            secs_since_labeled_reference: secs_since,
            interrupted: false,
        };
        self.db.insert_trial(&row)?;
        if let Some(tr) = &extras.track {
            self.db.insert_trace(&id, tr)?;
        }
        if let Some((samples, sr)) = &extras.capture {
            if self.core.settings.read().keep_recordings {
                if let Ok(path) = write_recording(&self.core.recordings_dir(), &id, samples, *sr) {
                    let _ = self
                        .db
                        .insert_recording(&id, &path, *sr, samples.len() as u64, false);
                }
            }
        }
        if labeled {
            self.last_labeled = Some(started);
        }
        Ok(row)
    }

    #[allow(clippy::too_many_arguments)]
    fn feedback(
        &mut self,
        engine: &AudioEngine,
        trial: &TrialSpec,
        outcome: &TrialOutcome,
        extras: &Extras,
        row: &TrialRow,
        feedback: FeedbackMode,
        settings: &AppSettings,
    ) -> R<After> {
        let show = feedback.shows_trial_result();
        let renderer = self.core.renderer();
        let notes: Vec<trainer_core::grade::NoteResult> = outcome
            .metrics
            .get("notes")
            .and_then(|v| serde_json::from_value(v.clone()).ok())
            .unwrap_or_default();
        let capture_offset = outcome
            .response
            .get("capture_offset_s")
            .and_then(|v| v.as_f64())
            .unwrap_or(0.0) as f32;
        // A line sung in another octave is drawn in the octave of the targets it was graded against.
        let octave_shift = 12.0
            * outcome
                .metrics
                .get("octave_offset")
                .and_then(|v| v.as_f64())
                .unwrap_or(0.0) as f32;
        let trace: Vec<(f32, Option<f32>)> = if show {
            extras
                .track
                .as_ref()
                .map(|tr| {
                    tr.frames
                        .iter()
                        .map(|f| (f.t - capture_offset, f.midi.map(|m| m - octave_shift)))
                        .collect()
                })
                .unwrap_or_default()
        } else {
            Vec::new()
        };
        let (is_sung, is_identify, is_dictation) = (
            matches!(trial, TrialSpec::Sung(_)),
            matches!(trial, TrialSpec::Identify(_)),
            matches!(trial, TrialSpec::Dictation(_)),
        );
        let written: Vec<ScoreNote> = outcome
            .response
            .get("answer")
            .and_then(|v| serde_json::from_value(v.clone()).ok())
            .unwrap_or_default();
        let dictation = match trial {
            TrialSpec::Dictation(t) if show => Some(dictation_view(t, &written, &outcome.metrics)),
            _ => None,
        };
        let passive = matches!(trial, TrialSpec::Pairing(_));
        let correct_label = match trial {
            TrialSpec::Identify(t) => t
                .choices
                .iter()
                .find(|c| c.id == t.correct)
                .map(|c| c.label.clone())
                .unwrap_or_else(|| t.correct.clone()),
            _ => String::new(),
        };
        let answered_id = outcome.response.get("answer").and_then(|a| a.as_str());
        let shown = trainer_core::grade::present_feedback(
            feedback,
            outcome.correct,
            &outcome.summary,
            answered_id,
            &correct_label,
        );
        let flash = match trial {
            TrialSpec::Identify(t) if show => t.flash_color.clone(),
            _ => None,
        };
        // A stream moves on in time for the next tone's slot.
        let stream_ms = match (trial, self.stream_next) {
            (TrialSpec::Identify(t), Some(next)) if t.pace_ms.is_some() => {
                let left =
                    next.saturating_sub(engine.now()) as f64 * 1000.0 / engine.sample_rate() as f64;
                Some((left - 120.0).max(100.0) as u32)
            }
            _ => None,
        };
        let auto_ms = if let Some(ms) = stream_ms {
            Some(ms)
        } else if passive {
            Some(200)
        } else if !show {
            Some(700)
        } else if shown.errorless {
            Some(settings.auto_advance_ms.max(900))
        } else if is_identify && outcome.correct == Some(true) && settings.auto_advance_correct {
            Some(settings.auto_advance_ms)
        } else {
            None
        };
        // A resolution plays after the answer: moving on waits for it and a short pause.
        let after_ms = match trial {
            TrialSpec::Identify(t) if show => t.after_answer.as_ref().map(|a| {
                a.iter().map(|e| e.start_ms + e.dur_ms).fold(0.0, f64::max)
                    + 250.0
                    + AFTER_SOUND_PAUSE_MS
            }),
            _ => None,
        };
        let auto_ms = match (auto_ms, after_ms) {
            (Some(a), Some(w)) => Some(a.max(w as u32)),
            (a, _) => a,
        };
        let fv = FeedbackView {
            lit: Vec::new(),
            correct: shown.correct,
            summary: shown.summary.clone(),
            correct_choice: match trial {
                TrialSpec::Identify(t) if show => Some(t.correct.clone()),
                _ => None,
            },
            answered: shown.answered.clone(),
            dictation,
            cents: outcome
                .metrics
                .get("cents_error")
                .and_then(|v| v.as_f64())
                .map(|v| v as f32),
            notes: if show { notes } else { Vec::new() },
            trace,
            can_retry: is_sung || is_dictation,
            can_sing_along: is_sung,
            can_listen: is_sung
                || is_dictation
                || (matches!(trial, TrialSpec::SingPitch(_)) && show)
                || matches!(trial, TrialSpec::Identify(t) if show && t.after_answer.is_some()),
            can_show_answer: is_identify && show,
            can_play_answer: extras.capture.is_some() || (is_dictation && !written.is_empty()),
            can_hear_choices: matches!(trial, TrialSpec::Identify(t) if show && !t.choice_sounds.is_empty()),
            tuner: match trial {
                TrialSpec::SingPitch(t) if show => Some(TunerTarget {
                    pitch_class: t.target_class.index(),
                    midi: t.target_midi,
                    name: match t.target_midi {
                        Some(m) => trainer_core::theory::Note(m).display_name(),
                        None => t.target_class.display_name().to_string(),
                    },
                    reference_midi: t.reveal.as_ref().and_then(|r| r.first()).map(|e| e.midi),
                }),
                _ => None,
            },
            auto_advance_ms: auto_ms,
            hidden: !show,
            errorless: shown.errorless,
            flash_color: flash,
            answer_color: match trial {
                TrialSpec::Identify(t) if show => t.answer_color.clone(),
                TrialSpec::SingPitch(t) if show => t.answer_color.clone(),
                _ => None,
            },
        };
        self.set(|st| {
            st.phase = Phase::Feedback;
            st.feedback = Some(fv);
        });
        // Reveal the target of a no-reference singing trial right away when results are shown,
        // and play what follows an identification answer (a degree resolving to the tonic).
        if let TrialSpec::SingPitch(t) = trial {
            if show {
                if let Some(r) = &t.reveal {
                    engine.play_now(Arc::new(renderer.render_events(r)), 1.0, Bus::Prompt)?;
                }
            }
        }
        if let TrialSpec::Identify(t) = trial {
            if let (true, Some(after)) = (show, &t.after_answer) {
                engine.stop_bus(Bus::Prompt);
                let at = engine.frame_after_ms(250.0);
                engine.play(
                    Arc::new(renderer.render_events(after)),
                    at,
                    1.0,
                    Bus::Prompt,
                )?;
                self.hold_until = Some(end_frame(engine, after, at));
                let lit = lit_marks(t, after, secs(engine, at));
                self.set(|st| {
                    if let Some(f) = st.feedback.as_mut() {
                        f.lit = lit;
                    }
                });
            } else if shown.errorless {
                if let Some(events) = t.choice_sound(&t.correct) {
                    engine.stop_bus(Bus::Prompt);
                    engine.play_now(Arc::new(renderer.render_events(events)), 1.0, Bus::Prompt)?;
                }
            }
        }
        let mut interacted = false;
        loop {
            let timeout = if interacted {
                None
            } else {
                auto_ms.map(|ms| Duration::from_millis(ms as u64))
            };
            match self.wait(timeout)? {
                None => return Ok(After::Next),
                Some(RunnerCommand::Next | RunnerCommand::Skip) => {
                    // Moving on by hand in a paced stream plays the next tone now, not at its slot.
                    if stream_ms.is_some() {
                        self.stream_next = None;
                    }
                    return Ok(After::Next);
                }
                Some(RunnerCommand::Retry) if is_sung || is_dictation => {
                    return Ok(After::Retry { assisted: false });
                }
                Some(RunnerCommand::SingAlong) if is_sung => {
                    return Ok(After::Retry { assisted: true });
                }
                Some(RunnerCommand::Listen) => {
                    interacted = true;
                    match trial {
                        TrialSpec::Sung(t) => {
                            let audio = renderer.render_sung_trial(t);
                            let beat_frames = engine
                                .frames_from_ms(t.beat_ms() * t.sing_start_beat as f64)
                                as usize;
                            // Your part, with the other voice of a harmony exercise.
                            let mut guide = audio.guide;
                            guide.mix(&audio.accompaniment, 0, 1.0);
                            guide.limit(0.95);
                            let guide = trim_front(&guide, beat_frames);
                            engine.play_now(Arc::new(guide), 1.0, Bus::Guide)?;
                        }
                        TrialSpec::SingPitch(t) => {
                            if let Some(r) = &t.reveal {
                                engine.play_now(
                                    Arc::new(renderer.render_events(r)),
                                    1.0,
                                    Bus::Prompt,
                                )?;
                            }
                        }
                        TrialSpec::Dictation(t) => {
                            engine.stop_bus(Bus::Prompt);
                            engine.stop_bus(Bus::Guide);
                            engine.play_now(
                                Arc::new(dictation_audio(&renderer, &t.playback)),
                                1.0,
                                Bus::Prompt,
                            )?;
                        }
                        TrialSpec::Identify(t) => {
                            if let Some(after) = &t.after_answer {
                                engine.stop_bus(Bus::Prompt);
                                let (_, at) = engine.play_now(
                                    Arc::new(renderer.render_events(after)),
                                    1.0,
                                    Bus::Prompt,
                                )?;
                                self.hold_until = Some(end_frame(engine, after, at));
                                let lit = lit_marks(t, after, secs(engine, at));
                                self.set(|st| {
                                    if let Some(f) = st.feedback.as_mut() {
                                        f.lit = lit;
                                    }
                                });
                            }
                        }
                        _ => {}
                    }
                }
                Some(RunnerCommand::PlayAnswer) if is_dictation => {
                    interacted = true;
                    if let TrialSpec::Dictation(t) = trial {
                        let ev = note_events(
                            &written,
                            t.score.time,
                            t.score.tempo_bpm,
                            &t.playback.reference_timbre,
                        );
                        engine.stop_bus(Bus::Prompt);
                        engine.stop_bus(Bus::Guide);
                        engine.play_now(Arc::new(renderer.render_events(&ev)), 1.0, Bus::Guide)?;
                    }
                }
                Some(RunnerCommand::PlayAnswer) => {
                    interacted = true;
                    if let Some((samples, sr)) = &extras.capture {
                        // From just before singing starts (sung trials), so the lead-in is skipped.
                        let skip = match trial {
                            TrialSpec::Sung(t) => {
                                ((t.sing_start_ms() - 300.0).max(0.0) / 1000.0 * *sr as f64)
                                    as usize
                            }
                            _ => 0,
                        };
                        let voice = &samples[skip.min(samples.len())..];
                        let peak = voice.iter().fold(0.0f32, |m, s| m.max(s.abs()));
                        let gain = if peak > 0.0 {
                            (0.7 / peak).min(8.0)
                        } else {
                            1.0
                        };
                        let mono: Vec<f32> = voice.iter().map(|s| s * gain).collect();
                        engine.stop_bus(Bus::Guide);
                        engine.play_now(
                            Arc::new(SoundBuffer::from_mono(*sr, &mono)),
                            1.0,
                            Bus::Guide,
                        )?;
                    }
                }
                Some(RunnerCommand::PlayChoice { choice }) if show => {
                    interacted = true;
                    if let Some(events) = trial_choice_sound(trial, &choice) {
                        engine.stop_bus(Bus::Prompt);
                        let (_, at) = engine.play_now(
                            Arc::new(renderer.render_events(events)),
                            1.0,
                            Bus::Prompt,
                        )?;
                        self.hold_until = Some(end_frame(engine, events, at));
                        if let TrialSpec::Identify(t) = trial {
                            let lit = lit_marks(t, events, secs(engine, at));
                            self.set(|st| {
                                if let Some(f) = st.feedback.as_mut() {
                                    f.lit = lit;
                                }
                            });
                        }
                        // A named sound: it counts as exposure to a labelled reference.
                        self.last_labeled = Some(now());
                    }
                }
                Some(RunnerCommand::ShowAnswer | RunnerCommand::Replay) if is_identify => {
                    interacted = true;
                    if let TrialSpec::Identify(t) = trial {
                        engine.play_now(
                            Arc::new(renderer.render_events(&t.events)),
                            1.0,
                            Bus::Prompt,
                        )?;
                        if !row.labeled_reference {
                            self.last_labeled = Some(now());
                        }
                    }
                }
                Some(_) => interacted = true,
            }
        }
    }
}

fn trial_choice_sound<'t>(trial: &'t TrialSpec, choice: &str) -> Option<&'t [SoundEvent]> {
    match trial {
        TrialSpec::Identify(t) => t.choice_sound(choice),
        _ => None,
    }
}

/// The exercise stored for a trial. A note pulled out of a chord is a `pitch_id` trial.
fn stored_kind(block: &BlockSpec, trial: &TrialSpec) -> ExerciseKind {
    match trial {
        TrialSpec::Identify(t) => t.exercise,
        TrialSpec::SingPitch(t) => t.exercise,
        TrialSpec::Sung(t) => t.exercise,
        TrialSpec::Detect(t) => t.exercise,
        TrialSpec::Pairing(_) | TrialSpec::Dictation(_) => block.exercise.kind(),
    }
}

fn requeue_key(trial: &TrialSpec) -> String {
    match trial {
        TrialSpec::Identify(t) => t
            .stimulus
            .get("pitch_class")
            .or_else(|| t.stimulus.get("chord"))
            .and_then(|v| v.as_str())
            .unwrap_or(t.correct.as_str())
            .to_string(),
        _ => String::new(),
    }
}

/// What the follow-along lane draws for a sung trial: targets, the reference, the other voice of
/// a harmony exercise, and the timeline.
fn lane_for(t: &SungTrial) -> LaneSpec {
    let beat = (60.0 / t.tempo_bpm) as f32;
    let notes: Vec<LaneNote> = t
        .notes
        .iter()
        .map(|n| LaneNote {
            midi: n.midi,
            start_s: (t.sing_start_beat + n.start_beat) * beat,
            dur_s: n.beats * beat,
        })
        .collect();
    let reference: Vec<LaneNote> = t
        .reference_beats
        .iter()
        .map(|n| LaneNote {
            midi: n.midi,
            start_s: n.start_beat * beat,
            dur_s: n.beats * beat,
        })
        .collect();
    let accompaniment: Vec<LaneNote> = t
        .accompaniment
        .iter()
        .map(|n| LaneNote {
            midi: n.midi,
            start_s: (t.sing_start_beat + n.start_beat) * beat,
            dur_s: n.beats * beat,
        })
        .collect();
    let lo = notes
        .iter()
        .chain(&reference)
        .chain(&accompaniment)
        .map(|n| n.midi)
        .min()
        .unwrap_or(60);
    let hi = notes
        .iter()
        .chain(&reference)
        .chain(&accompaniment)
        .map(|n| n.midi)
        .max()
        .unwrap_or(60);
    let pad = ((12 - (hi - lo)).max(0) / 2).max(4);
    LaneSpec {
        notes,
        reference,
        accompaniment,
        beat_s: beat,
        beats_per_bar: t.beats_per_bar,
        sing_start_s: t.sing_start_beat * beat,
        total_s: t.total_beats * beat,
        midi_low: lo - pad,
        midi_high: hi + pad,
        targets_hidden: t.lane_targets == LaneTargets::HiddenUntilDone,
        metronome: t.backing.has_click() || t.count_in_beat.is_some(),
        sections: t
            .sections
            .iter()
            .map(|s| LaneSection {
                label: s.label.clone(),
                start_s: s.start_beat * beat,
                end_s: s.end_beat * beat,
            })
            .collect(),
        cursor: t.follow_cursor,
        score_until_s: t.score_until_beat.map(|b| b * beat),
        silent: t.silent.iter().map(|i| *i as u32).collect(),
    }
}

/// A dictation's playback: cadence, count-in clicks and the melody in one buffer.
fn dictation_audio(
    renderer: &trainer_audio::render::Renderer,
    playback: &SungTrial,
) -> SoundBuffer {
    let audio = renderer.render_sung_trial(playback);
    let mut mix = audio.backing;
    mix.mix(&audio.guide, 0, 0.9);
    mix.limit(0.95);
    mix
}

/// The answer and the melody with a mark per note, from the stored grading metrics.
fn dictation_view(
    t: &DictationTrial,
    written: &[ScoreNote],
    metrics: &serde_json::Value,
) -> DictationView {
    let notes: Vec<trainer_core::grade::DictationNote> = metrics
        .get("notes")
        .and_then(|v| serde_json::from_value(v.clone()).ok())
        .unwrap_or_default();
    let marks: Vec<String> = metrics
        .get("answer_marks")
        .and_then(|v| serde_json::from_value(v.clone()).ok())
        .unwrap_or_default();
    let mut k = 0usize;
    let target_marks = t
        .score
        .notes
        .iter()
        .map(|n| {
            n.pitch.map(|_| {
                let r = notes.get(k);
                k += 1;
                match r {
                    Some(r) if r.correct => "correct".to_string(),
                    Some(r) if r.answer_index.is_some() => "wrong".to_string(),
                    _ => "missed".to_string(),
                }
            })
        })
        .collect();
    let mut j = 0usize;
    let answer_marks = written
        .iter()
        .map(|n| {
            n.pitch.map(|_| {
                let m = marks.get(j).cloned().unwrap_or_else(|| "extra".into());
                j += 1;
                m
            })
        })
        .collect();
    let mut answer = t.given.clone();
    answer.notes = written.to_vec();
    DictationView {
        target: t.score.clone(),
        answer,
        target_marks,
        answer_marks,
    }
}

/// The guide melody without the lead-in before singing starts.
fn trim_front(b: &SoundBuffer, frames: usize) -> SoundBuffer {
    let start = (frames * 2).min(b.data.len());
    SoundBuffer {
        sample_rate: b.sample_rate,
        data: b.data[start..].to_vec(),
    }
}

fn write_recording(dir: &std::path::Path, id: &str, samples: &[f32], sr: u32) -> Result<PathBuf> {
    let now = chrono::Local::now();
    let folder = dir
        .join(now.format("%Y").to_string())
        .join(now.format("%m").to_string());
    std::fs::create_dir_all(&folder)?;
    let path = folder.join(format!("{id}.wav"));
    let spec = hound::WavSpec {
        channels: 1,
        sample_rate: sr,
        bits_per_sample: 16,
        sample_format: hound::SampleFormat::Int,
    };
    let mut w = hound::WavWriter::create(&path, spec)?;
    for &s in samples {
        w.write_sample((s.clamp(-1.0, 1.0) * i16::MAX as f32) as i16)?;
    }
    w.finalize()?;
    Ok(path)
}

#[cfg(test)]
mod tests {
    use super::*;
    use trainer_core::generate::GenContext;

    fn dictation_trial() -> DictationTrial {
        let block: BlockSpec = serde_json::from_value(serde_json::json!({
            "type": "melodic_dictation", "count": 1, "params": {}
        }))
        .unwrap();
        match generate_block(&block, 5, &GenContext::default())
            .unwrap()
            .remove(0)
        {
            TrialSpec::Dictation(t) => t,
            _ => panic!("not dictation"),
        }
    }

    #[test]
    fn dictation_view_marks_every_note() {
        let t = dictation_trial();
        // A perfect answer: every note right.
        let perfect = t.score.notes.clone();
        let o = grade_dictation(&t, &perfect, 1);
        let v = dictation_view(&t, &perfect, &o.metrics);
        assert_eq!(v.target_marks.len(), t.score.notes.len());
        assert!(v.target_marks.iter().flatten().all(|m| m == "correct"));
        assert!(v.answer_marks.iter().flatten().all(|m| m == "correct"));
        assert_eq!(v.answer.key, t.score.key);

        // Only the given first note: the rest are missed.
        let first: Vec<ScoreNote> = t.given.notes.clone();
        let o = grade_dictation(&t, &first, 2);
        let v = dictation_view(&t, &first, &o.metrics);
        let missed = v
            .target_marks
            .iter()
            .flatten()
            .filter(|m| *m == "missed")
            .count();
        assert_eq!(missed, t.score.pitched().count() - 1);
        assert_eq!(v.answer_marks.iter().flatten().count(), 1);
    }

    #[test]
    fn harmony_lane_shows_the_other_voice_while_singing() {
        let block: trainer_core::schema::BlockSpec = serde_json::from_value(serde_json::json!({
            "type": "harmony", "count": 1, "params": {"source": {"parallel": {"voice": "sixth_above"}}}
        }))
        .unwrap();
        let ctx = trainer_core::generate::GenContext::default();
        let TrialSpec::Sung(t) = trainer_core::generate::generate_block(&block, 5, &ctx)
            .unwrap()
            .remove(0)
        else {
            panic!("not sung")
        };
        let lane = lane_for(&t);
        assert_eq!(lane.accompaniment.len(), lane.notes.len());
        assert!(lane.targets_hidden);
        for (sung, other) in lane.notes.iter().zip(&lane.accompaniment) {
            assert_eq!(sung.start_s, other.start_s);
            assert!(other.start_s >= lane.sing_start_s);
            assert!(other.midi >= lane.midi_low && other.midi <= lane.midi_high);
        }
    }
}
