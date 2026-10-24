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
        if self.is_active() {
            return Err(anyhow!("a session is already running"));
        }
        self.tx
            .send(RunnerMsg::Start { session_id, spec })
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
        let RunnerMsg::Start { session_id, spec } = msg else {
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
}

fn secs(engine: &AudioEngine, frame: u64) -> f64 {
    frame as f64 / engine.sample_rate() as f64
}

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
                ..RunnerState::default()
            };
        });
        for (bi, block) in blocks.iter().enumerate() {
            let trials = generate_block(block, block_seed(base_seed, bi, block), &ctx)
                .map_err(|e| anyhow!("block {}: {e}", bi + 1))?;
            let feedback = block.effective_feedback();
            self.set(|st| {
                st.phase = Phase::BlockIntro;
                st.block_index = bi as u32;
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
            let grading = |t: &TrialSpec| GradingConfig::resolve(&block.grading, default_mode(t));
            for (ti, trial) in trials.iter().enumerate() {
                let mut attempt = 0u32;
                let mut assisted = false;
                loop {
                    self.set(|st| {
                        st.trial_index = ti as u32;
                        st.phase = Phase::Preparing;
                        st.feedback = None;
                    });
                    let cfg = grading(trial);
                    let res = match trial {
                        TrialSpec::Identify(t) => self.run_identify(&engine, t, feedback, attempt),
                        TrialSpec::SingPitch(t) => {
                            self.run_sing_pitch(&engine, t, &cfg, feedback, attempt, &ctx)
                        }
                        TrialSpec::Pairing(t) => self.run_pairing(&engine, t),
                        TrialSpec::Sung(t) => {
                            self.run_sung(&engine, t, &cfg, feedback, attempt, assisted)
                        }
                        TrialSpec::Dictation(t) => self.run_dictation(&engine, t, attempt),
                    }?;
                    let Some((outcome, extras)) = res else {
                        break; // skipped
                    };
                    let row = self.store(
                        block, bi, ti, attempt, assisted, trial, &outcome, &extras, feedback,
                    )?;
                    if !assisted {
                        self.set(|st| {
                            st.done_in_block += 1;
                            if outcome.correct == Some(true) {
                                st.correct_in_block += 1;
                            }
                        });
                    }
                    match self.feedback(&engine, trial, &outcome, &extras, &row, feedback, &auto)? {
                        After::Next => break,
                        After::Retry { assisted: a } => {
                            attempt += 1;
                            assisted = a;
                        }
                    }
                }
            }
            let report = session_report(self.db, &self.session_id)?;
            let block_report = report.blocks.get(bi).cloned();
            let show = feedback != FeedbackMode::EndOfSession;
            self.set(|st| {
                st.phase = Phase::BlockSummary;
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
        }
    }

    // --- identification -------------------------------------------------------------------

    fn run_identify(
        &mut self,
        engine: &AudioEngine,
        t: &IdentifyTrial,
        feedback: FeedbackMode,
        attempt: u32,
    ) -> R<Option<(TrialOutcome, Extras)>> {
        let renderer = self.core.renderer();
        let stim = Arc::new(renderer.render_events(&t.events));
        let mask = renderer
            .render_mask(t.mask_before, now().timestamp_micros() as u64)
            .map(Arc::new);
        let sr = engine.sample_rate() as f64;
        let mut at = engine.frame_after_ms(80.0);
        if let Some(m) = &mask {
            engine.play(m.clone(), at, 1.0, Bus::Prompt)?;
            at += m.frames() as u64 + engine.frames_from_ms(400.0);
        }
        engine.play(stim.clone(), at, 1.0, Bus::Prompt)?;
        let onset_frame = at;
        let onset = Instant::now()
            + Duration::from_secs_f64(onset_frame.saturating_sub(engine.now()) as f64 / sr);
        let layout = match t.exercise {
            ExerciseKind::PitchId | ExerciseKind::TonalCenterId
                if t.choices.iter().all(|c| {
                    c.id.parse::<trainer_core::theory::PitchClass>().is_ok() || c.id == "other"
                }) =>
            {
                ChoiceLayout::PitchClasses
            }
            ExerciseKind::PitchId => ChoiceLayout::Notes,
            _ => ChoiceLayout::List,
        };
        let mut view = self.base_view(t.exercise, Interaction::Choice, &t.prompt, attempt);
        view.choices = t.choices.clone();
        view.layout = layout;
        view.deadline_ms = t.deadline_ms;
        view.can_replay = t.deadline_ms.is_none();
        view.can_reference = t.reference.is_some();
        view.response_start_s = Some(secs(engine, onset_frame));
        let _ = feedback;
        self.set(|st| {
            st.phase = Phase::Listening;
            st.trial = Some(view);
        });
        let deadline = t
            .deadline_ms
            .map(|d| onset + Duration::from_millis(d as u64));
        let mut answering = false;
        let mut replays = 0u32;
        let (answer, rt) = loop {
            let now_i = Instant::now();
            if !answering && now_i >= onset {
                answering = true;
                self.set(|st| st.phase = Phase::Answering);
            }
            let wake = if !answering {
                Some(onset.saturating_duration_since(now_i))
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
                    let rt = Instant::now()
                        .saturating_duration_since(onset)
                        .as_secs_f64()
                        * 1000.0;
                    break (Some(choice), Some(rt));
                }
                Some(RunnerCommand::Replay) if answering && deadline.is_none() => {
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
        let mut view = self.base_view(
            ExerciseKind::SingPitch,
            Interaction::SingPitch,
            &t.prompt,
            attempt,
        );
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

    fn lane_for(&self, t: &SungTrial) -> LaneSpec {
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
        let lo = notes
            .iter()
            .chain(&reference)
            .map(|n| n.midi)
            .min()
            .unwrap_or(60);
        let hi = notes
            .iter()
            .chain(&reference)
            .map(|n| n.midi)
            .max()
            .unwrap_or(60);
        let pad = ((12 - (hi - lo)).max(0) / 2).max(4);
        LaneSpec {
            notes,
            reference,
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
        }
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
        view.lane = Some(self.lane_for(t));
        view.score = t.score.clone();
        view.show_live_pitch = feedback == FeedbackMode::Continuous || assisted;
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
            k.total_beats = k.reference_beats.iter().map(|n| n.start_beat + n.beats).fold(0.0, f32::max) + 0.5;
            Arc::new(renderer.render_sung_trial(&k).backing)
        });
        let start = engine.frame_after_ms(150.0);
        engine.play(melody.clone(), start, 1.0, Bus::Prompt)?;
        let mut plays = 1u32;
        let mut view = self.base_view(ExerciseKind::MelodicDictation, Interaction::Dictation, &t.prompt, attempt);
        view.score = Some(t.given.clone());
        view.lane = Some(self.lane_for(&t.playback));
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
                    let ev = note_events(&notes, t.score.time, t.score.tempo_bpm, &t.playback.reference_timbre);
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
        let kind = block.exercise.kind();
        let mut tags: Vec<String> = kind.default_tags().iter().map(|s| s.to_string()).collect();
        for t in &block.tags {
            if !tags.contains(t) {
                tags.push(t.clone());
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
                && matches!(trial, TrialSpec::Identify(_) | TrialSpec::SingPitch(_) | TrialSpec::Dictation(_)))
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
        let trace: Vec<(f32, Option<f32>)> = if show {
            extras
                .track
                .as_ref()
                .map(|tr| {
                    tr.frames
                        .iter()
                        .map(|f| (f.t - capture_offset, f.midi))
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
        let auto_ms = if passive {
            Some(200)
        } else if !show {
            Some(700)
        } else if is_identify && outcome.correct == Some(true) && settings.auto_advance_correct {
            Some(settings.auto_advance_ms)
        } else {
            None
        };
        let fv = FeedbackView {
            correct: if show { outcome.correct } else { None },
            summary: if show {
                outcome.summary.clone()
            } else {
                String::new()
            },
            correct_choice: match trial {
                TrialSpec::Identify(t) if show => Some(t.correct.clone()),
                _ => None,
            },
            answered: outcome
                .response
                .get("answer")
                .and_then(|a| a.as_str())
                .map(String::from),
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
            can_listen: is_sung || is_dictation || (matches!(trial, TrialSpec::SingPitch(_)) && show),
            can_show_answer: is_identify && show,
            can_play_answer: extras.capture.is_some() || (is_dictation && !written.is_empty()),
            auto_advance_ms: auto_ms,
            hidden: !show,
        };
        self.set(|st| {
            st.phase = Phase::Feedback;
            st.feedback = Some(fv);
        });
        // Reveal the target of a no-reference singing trial right away when results are shown.
        if let TrialSpec::SingPitch(t) = trial {
            if show {
                if let Some(r) = &t.reveal {
                    engine.play_now(Arc::new(renderer.render_events(r)), 1.0, Bus::Prompt)?;
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
                Some(RunnerCommand::Next | RunnerCommand::Skip) => return Ok(After::Next),
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
                            let guide = trim_front(&audio.guide, beat_frames);
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
                            engine.play_now(Arc::new(dictation_audio(&renderer, &t.playback)), 1.0, Bus::Prompt)?;
                        }
                        _ => {}
                    }
                }
                Some(RunnerCommand::PlayAnswer) if is_dictation => {
                    interacted = true;
                    if let TrialSpec::Dictation(t) = trial {
                        let ev = note_events(&written, t.score.time, t.score.tempo_bpm, &t.playback.reference_timbre);
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
                            TrialSpec::Sung(t) => ((t.sing_start_ms() - 300.0).max(0.0) / 1000.0 * *sr as f64) as usize,
                            _ => 0,
                        };
                        let voice = &samples[skip.min(samples.len())..];
                        let peak = voice.iter().fold(0.0f32, |m, s| m.max(s.abs()));
                        let gain = if peak > 0.0 { (0.7 / peak).min(8.0) } else { 1.0 };
                        let mono: Vec<f32> = voice.iter().map(|s| s * gain).collect();
                        engine.stop_bus(Bus::Guide);
                        engine.play_now(Arc::new(SoundBuffer::from_mono(*sr, &mono)), 1.0, Bus::Guide)?;
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

/// A dictation's playback: cadence, count-in clicks and the melody in one buffer.
fn dictation_audio(renderer: &trainer_audio::render::Renderer, playback: &SungTrial) -> SoundBuffer {
    let audio = renderer.render_sung_trial(playback);
    let mut mix = audio.backing;
    mix.mix(&audio.guide, 0, 0.9);
    mix.limit(0.95);
    mix
}

/// The answer and the melody with a mark per note, from the stored grading metrics.
fn dictation_view(t: &DictationTrial, written: &[ScoreNote], metrics: &serde_json::Value) -> DictationView {
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
        match generate_block(&block, 5, &GenContext::default()).unwrap().remove(0) {
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
        let missed = v.target_marks.iter().flatten().filter(|m| *m == "missed").count();
        assert_eq!(missed, t.score.pitched().count() - 1);
        assert_eq!(v.answer_marks.iter().flatten().count(), 1);
    }
}
