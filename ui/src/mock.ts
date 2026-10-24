// A stand-in backend for running the UI in a plain browser (design work and screenshots).
// It imitates the desktop app closely enough to exercise every screen.

import type { Backend } from "./api";
import type { AudioView } from "./bindings/AudioView";
import type { FeedbackView } from "./bindings/FeedbackView";
import type { LaneSpec } from "./bindings/LaneSpec";
import type { LiveBatch } from "./bindings/LiveBatch";
import type { NoteResult } from "./bindings/NoteResult";
import type { PresetView } from "./bindings/PresetView";
import type { RunnerCommand } from "./bindings/RunnerCommand";
import type { RunnerState } from "./bindings/RunnerState";
import type { SessionReport } from "./bindings/SessionReport";
import type { SessionRow } from "./bindings/SessionRow";
import type { TrialView } from "./bindings/TrialView";
import type { AppSettings } from "./bindings/AppSettings";

const clock = () => performance.now() / 1000;

const presets: PresetView[] = [
  p("ap_progressive_01", "Absolute pitch: stage 1", "Absolute pitch", "Classes: C, E, G♯. Suggested criterion for the next stage: at least 90% over two sessions.", 72, 6.1, ["Name the note", "Sing the note"]),
  p("ap_daily", "Absolute pitch: daily", "Absolute pitch", "Name 36 notes and sing 12, all twelve pitch classes.", 48, 5.4, ["Name the note", "Sing the note"]),
  p("ap_assessment", "Absolute pitch assessment", "Absolute pitch", "Holdout tones, no feedback: identification across octaves and timbres, speed, singing and key. Run first in a session.", 156, 16.5, ["12-class identification", "Cross-octave identification"]),
  p("ear_intervals_melodic", "Intervals: melodic", "Ear training", "Melodic intervals up and down, minor 2nd to octave.", 30, 3.2, ["Interval identification"]),
  p("ear_chords", "Chords: triads and inversions", "Ear training", "Triads, inversions and seventh chords.", 60, 6.8, ["Triads", "Major and minor inversions", "Seventh chords"]),
  p("sing_pitch_matching", "Pitch matching and sustain", "Singing", "Hear a note and sing it back, then hold notes steadily.", 19, 3.1, ["Match the note", "Hold steadily"]),
  p("sing_scales", "Scales: listen and repeat", "Singing", "Listen to a scale, then sing it back in time.", 8, 4.9, ["Major scale", "Natural minor scale"]),
  p("sing_warmup", "Warm-up patterns", "Singing", "Five-note scales and arpeggios, moving up a semitone at a time.", 14, 5.6, ["Five-note scale, sing along", "Major arpeggio, echo"]),
];

function p(id: string, title: string, category: string, description: string, trial_count: number, estimated_minutes: number, block_titles: string[]): PresetView {
  return { id, title, category, description, source: "builtin", trial_count, estimated_minutes, block_titles };
}

const queued: SessionRow[] = [
  {
    id: "q1",
    title: "Descending sixths, then stage 2",
    source: "llm",
    protocol: "ap_progressive",
    spec: {},
    status: "queued",
    queued_at: new Date(Date.now() - 3600_000).toISOString(),
    started_at: null,
    completed_at: null,
    queue_note: "Your descending minor sixths are at 58% versus 91% ascending. Short interval drill first, then absolute pitch stage 2.",
    summary: null,
  },
];

const pastSessions: SessionRow[] = [
  session("s1", "Absolute pitch: stage 1", 0.86, 2),
  session("s2", "Scales: listen and repeat", 0.74, 26),
  session("s3", "Intervals: melodic", 0.81, 50),
];

function session(id: string, title: string, accuracy: number, hoursAgo: number): SessionRow {
  const start = new Date(Date.now() - hoursAgo * 3600_000);
  return {
    id,
    title,
    source: "preset",
    protocol: null,
    spec: {},
    status: "completed",
    queued_at: null,
    started_at: start.toISOString(),
    completed_at: new Date(start.getTime() + 9 * 60_000).toISOString(),
    queue_note: null,
    summary: { accuracy, trials: 48 },
  };
}

let state: RunnerState = idle();
let seq = 0;
let trialNo = 0;
const listeners: Record<string, ((p: unknown) => void)[]> = {};
let kind: "choice" | "sung" = "choice";
let laneStart = 0;
let lane: LaneSpec | null = null;

function idle(): RunnerState {
  return {
    phase: "idle",
    session_id: null,
    title: "",
    notes: null,
    block_index: 0,
    block_count: 0,
    block_title: "",
    block_instructions: null,
    block_kind: null,
    trial_index: 0,
    trial_count: 0,
    feedback_mode: "after_trial",
    done_in_block: 0,
    correct_in_block: 0,
    trial: null,
    feedback: null,
    block_report: null,
    session_report: null,
    error: null,
    seq: 0,
  };
}

function emit(next: Partial<RunnerState>) {
  state = { ...state, ...next, seq: ++seq };
  (listeners["runner-state"] ?? []).forEach((l) => l(state));
}

const PCS = ["C", "C#", "D", "D#", "E", "F", "F#", "G", "G#", "A", "A#", "B"];
const LABELS = ["C", "C♯", "D", "E♭", "E", "F", "F♯", "G", "A♭", "A", "B♭", "B"];

function choiceTrial(): TrialView {
  return {
    kind: "pitch_id",
    interaction: "choice",
    prompt: "Name the note",
    // Stage 1 of the progressive protocol: three classes.
    choices: ["C", "E", "G#"].map((id) => ({ id, label: LABELS[PCS.indexOf(id)] })),
    layout: "pitch_classes",
    deadline_ms: null,
    can_replay: true,
    can_reference: false,
    lane: null,
    show_live_pitch: false,
    label: null,
    label_at_s: null,
    timeline_start_s: null,
    response_start_s: clock(),
    response_end_s: null,
    attempt: 0,
    assisted: false,
  };
}

function scaleLane(): LaneSpec {
  const beat = 60 / 96;
  const degrees = [0, 2, 4, 5, 7, 9, 11, 12, 11, 9, 7, 5, 4, 2, 0];
  const tonic = 50;
  const sing = 20 * beat;
  return {
    notes: degrees.map((d, i) => ({ midi: tonic + d, start_s: sing + i * beat, dur_s: beat })),
    reference: degrees.map((d, i) => ({ midi: tonic + d, start_s: i * beat, dur_s: beat })),
    beat_s: beat,
    beats_per_bar: 4,
    sing_start_s: sing,
    total_s: sing + 16 * beat,
    midi_low: tonic - 4,
    midi_high: tonic + 16,
    targets_hidden: false,
    metronome: true,
  };
}

function sungTrial(): TrialView {
  lane = scaleLane();
  laneStart = clock() + 0.2 - (scenario === "sung" ? lane.sing_start_s - 1.0 : 0);
  return {
    kind: "pattern",
    interaction: "sung",
    prompt: "Major scale, up and down, from D3",
    choices: [],
    layout: "list",
    deadline_ms: null,
    can_replay: false,
    can_reference: false,
    lane,
    show_live_pitch: true,
    label: null,
    label_at_s: null,
    timeline_start_s: laneStart,
    response_start_s: laneStart + lane.sing_start_s,
    response_end_s: laneStart + lane.total_s,
    attempt: 0,
    assisted: false,
  };
}

/** The pitch the simulated singer produces at trial time t. */
function simulatedMidi(t: number): number | null {
  if (!lane) return null;
  for (const n of lane.notes) {
    if (t >= n.start_s + 0.03 && t < n.start_s + n.dur_s - 0.04) {
      const local = t - n.start_s;
      const scoop = local < 0.12 ? -0.5 * (1 - local / 0.12) : 0;
      const sharp = n.midi === lane.notes[6].midi ? 0.62 : 0.08;
      return n.midi + sharp + scoop + 0.25 * Math.sin(2 * Math.PI * 5.4 * t);
    }
  }
  return null;
}

function sungFeedback(): FeedbackView {
  const l = lane!;
  const notes: NoteResult[] = l.notes.map((n, i) => {
    const wrong = i === 6;
    const missed = i === 13;
    return {
      target_midi: n.midi,
      expected_onset_s: n.start_s,
      matched: !missed,
      sung_midi: missed ? null : n.midi + (wrong ? 0.62 : 0.08),
      cents_error: missed ? null : wrong ? 62 : 8 + (i % 3) * 4,
      onset_error_ms: missed ? null : 20 + (i % 4) * 10,
      correct: !wrong && !missed,
      in_tune: !wrong && !missed && i % 5 !== 2,
      on_time: !missed,
      onset_s: missed ? null : n.start_s + 0.03,
      offset_s: missed ? null : n.start_s + n.dur_s - 0.04,
    };
  });
  const trace: [number, number | null][] = [];
  for (let t = l.sing_start_s - 0.3; t < l.total_s; t += 0.01) {
    const i = l.notes.findIndex((n) => t >= n.start_s && t < n.start_s + n.dur_s);
    trace.push([t, i === 13 ? null : simulatedMidi(t)]);
  }
  return {
    correct: false,
    summary: "13 of 15 notes right, average error 14 cents, 1 missed",
    correct_choice: null,
    answered: null,
    cents: null,
    notes,
    trace,
    can_retry: true,
    can_sing_along: true,
    can_listen: true,
    can_show_answer: false,
    auto_advance_ms: null,
    hidden: false,
  };
}

const report: SessionReport = {
  session: pastSessions[0],
  accuracy: 0.86,
  duration_s: 541,
  blocks: [
    { index: 0, title: "Name the note", kind: "pitch_id", trials: 60, graded: 60, correct: 53, accuracy: 0.883, mean_score: 0.883, median_rt_ms: 1240, median_abs_cents: null, pass_mark: 0.9, passed: false, feedback: "after_trial", assessment: false },
    { index: 1, title: "Sing the note", kind: "sing_pitch", trials: 12, graded: 12, correct: 9, accuracy: 0.75, mean_score: 0.75, median_rt_ms: 820, median_abs_cents: 23, pass_mark: null, passed: null, feedback: "after_trial", assessment: false },
  ],
  trials: Array.from({ length: 72 }, (_, i) => {
    const sung = i >= 60;
    const j = sung ? i - 60 : i;
    const ok = j % 7 !== 3;
    const pc = ["C", "E", "G#"][j % 3];
    const name = LABELS[PCS.indexOf(pc)];
    return {
      id: `t${i}`,
      block_idx: sung ? 1 : 0,
      trial_idx: j,
      attempt: 0,
      type: sung ? "sing_pitch" : "pitch_id",
      correct: ok,
      score: ok ? 1 : 0,
      rt_ms: 900 + (j % 9) * 61,
      summary: sung ? (ok ? "12 cents sharp, settled in 180 ms" : "38 cents flat") : ok ? `Correct: ${name}.` : `You answered F. It was ${name}.`,
      stimulus: sung ? { pitch_class: pc } : { pitch_class: pc, octave: 3 + (j % 3), timbre: "gm:0" },
      assisted: false,
    };
  }),
};

let scenario = new URLSearchParams(typeof location !== "undefined" ? location.search : "").get("scenario");

function startScenario() {
  if (scenario === "pitch") {
    emit({ phase: "answering", session_id: "demo", title: "Absolute pitch: stage 1", block_count: 2, block_title: "Name the note", block_kind: "pitch_id", trial_index: 6, trial_count: 60, done_in_block: 6, correct_in_block: 5, trial: choiceTrial() });
  } else if (scenario === "sung" || scenario === "sung-feedback") {
    kind = "sung";
    const trial = sungTrial();
    emit({ phase: "recording", session_id: "demo", title: "Scales: listen and repeat", block_count: 2, block_title: "Major scale", block_kind: "pattern", trial_index: 1, trial_count: 4, done_in_block: 1, correct_in_block: 1, feedback_mode: "continuous", trial });
    if (scenario === "sung-feedback") emit({ phase: "feedback", feedback: sungFeedback() });
  } else if (scenario === "summary") {
    emit({ phase: "session_summary", session_id: "demo", title: "Absolute pitch: stage 1", block_count: 2, session_report: report });
  }
}

function runnerCommand(c: RunnerCommand) {
  switch (c.kind) {
    case "begin":
      if (state.phase === "block_intro") {
        trialNo = 0;
        nextTrial();
      }
      break;
    case "answer": {
      if (state.phase !== "answering") return;
      const correct = c.choice === "E";
      emit({
        phase: "feedback",
        done_in_block: state.done_in_block + 1,
        correct_in_block: state.correct_in_block + (correct ? 1 : 0),
        feedback: {
          correct,
          summary: correct ? "Correct: E." : `You answered ${LABELS[PCS.indexOf(c.choice)] ?? c.choice}. It was E.`,
          correct_choice: "E",
          answered: c.choice,
          cents: null,
          notes: [],
          trace: [],
          can_retry: false,
          can_sing_along: false,
          can_listen: false,
          can_show_answer: true,
          auto_advance_ms: correct ? 900 : null,
          hidden: false,
        },
      });
      if (correct) setTimeout(() => state.phase === "feedback" && nextTrial(), 900);
      break;
    }
    case "next":
      if (state.phase === "feedback") nextTrial();
      else if (state.phase === "block_summary") emit({ phase: "session_summary", session_report: report });
      else if (state.phase === "session_summary") emit(idle());
      else if (state.phase === "block_intro") nextTrial();
      break;
    case "retry":
    case "sing_along":
      if (kind === "sung") {
        emit({ phase: "recording", feedback: null, trial: { ...sungTrial(), assisted: c.kind === "sing_along" } });
        scheduleSungEnd();
      }
      break;
    case "close":
      emit(idle());
      break;
    case "abort":
      emit(idle());
      break;
    default:
      break;
  }
}

function scheduleSungEnd() {
  const ms = ((lane?.total_s ?? 0) - (clock() - laneStart)) * 1000 + 300;
  setTimeout(() => {
    if (state.phase === "recording") emit({ phase: "feedback", feedback: sungFeedback() });
  }, Math.max(ms, 500));
}

function nextTrial() {
  trialNo += 1;
  if (trialNo > 3) {
    emit({ phase: "block_summary", trial: null, feedback: null, block_report: report.blocks[0] });
    return;
  }
  if (kind === "sung") {
    emit({ phase: "recording", trial_index: trialNo - 1, feedback: null, trial: sungTrial() });
    scheduleSungEnd();
  } else {
    emit({ phase: "listening", trial_index: trialNo - 1, feedback: null, trial: choiceTrial() });
    setTimeout(() => emit({ phase: "answering" }), 700);
  }
}

const audioView: AudioView = {
  settings: { host: null, input_device: null, output_device: null, input_channel: 0, output_pair: 0, sample_rate: 48000, buffer_frames: null },
  info: {
    host: "ASIO",
    input_device: "Focusrite USB ASIO",
    input_device_id: "asio:Focusrite USB ASIO",
    output_device: "Focusrite USB ASIO",
    output_device_id: "asio:Focusrite USB ASIO",
    sample_rate: 48000,
    requested_buffer: null,
    input_channels: 2,
    output_channels: 2,
    input_channel: 0,
    output_pair: 0,
    input_format: "i32",
    output_format: "i32",
  },
  status: { running: true, out_frame: 0, in_frame: 0, buffer_frames: 128, load_percent: 6.2, xruns: 0, input_overflows: 0, voices: 0, roundtrip_estimate_frames: 412, input_peak: 0.2, errors: [] },
  error: null,
  roundtrip_ms: 9.1,
  calibration: null,
};

let settings: AppSettings = {
  a4_hz: 440,
  audio: audioView.settings,
  auto_advance_correct: true,
  auto_advance_ms: 900,
  keep_recordings: true,
  recording_retention_days: 60,
  onboarding_done: true,
};

export function mockBackend(): Backend {
  setTimeout(startScenario, 50);
  return {
    appInfo: async () => ({
      version: "0.1.0",
      data_dir: "C:\\Users\\you\\AppData\\Roaming\\SingingTrainer",
      mcp_command: "C:\\Users\\you\\AppData\\Local\\Singing Trainer\\singing-trainer.exe",
      mcp_args: ["--mcp"],
      soundfont: "GeneralUser-GS",
      banks: ["GeneralUser-GS"],
    }),
    getSettings: async () => settings,
    saveSettings: async (s) => {
      settings = s;
      return audioView;
    },
    listAudioHosts: async () => [
      { name: "ASIO", devices: [{ id: "asio:Focusrite USB ASIO", name: "Focusrite USB ASIO", input_channels: 2, output_channels: 2, default_sample_rate: 48000, buffer_min: 16, buffer_max: 1024, is_default_input: true, is_default_output: true }] },
      { name: "WASAPI", devices: [
        { id: "wasapi:in1", name: "Analogue 1 + 2 (Focusrite USB Audio)", input_channels: 2, output_channels: 0, default_sample_rate: 48000, buffer_min: null, buffer_max: null, is_default_input: true, is_default_output: false },
        { id: "wasapi:out1", name: "Speakers (Focusrite USB Audio)", input_channels: 0, output_channels: 2, default_sample_rate: 48000, buffer_min: null, buffer_max: null, is_default_input: false, is_default_output: true },
      ] },
    ],
    audioState: async () => audioView,
    startAudio: async () => audioView.info!,
    stopAudio: async () => {},
    playNote: async () => {},
    setDrone: async () => {},
    calibrate: async () => ({ residual_frames: 96, spread_ms: 0.2, matched: 8, expected: 8 }),
    listPresets: async () => presets,
    listQueue: async () => queued,
    removeQueued: async () => true,
    startSession: async (req) => {
      const preset = req.source === "preset" ? presets.find((x) => x.id === req.id) : null;
      kind = preset?.category === "Singing" ? "sung" : "choice";
      emit({ ...idle(), phase: "block_intro", session_id: "demo", title: preset?.title ?? "Session", block_count: 2, block_title: preset?.block_titles[0] ?? "Block", block_kind: kind === "sung" ? "pattern" : "pitch_id", trial_count: 3, block_instructions: null });
      return "demo";
    },
    runnerCommand: async (c) => runnerCommand(c),
    runnerState: async () => state,
    listSessions: async () => pastSessions,
    sessionReport: async () => report,
    trialDetail: async () => ({ summary: "12 cents sharp, settled in 180 ms", metrics: { cents_error: 12 }, trace_20hz: { rows: [] } }),
    playRecording: async () => {},
    getRangeProfile: async () => ({ comfortable_low: "A2", comfortable_high: "E4", lowest_reachable: "F2", highest_reachable: "A4", voice_type: "baritone", updated_at: null, source: "manual" }),
    setRangeProfile: async (p) => p,
    addComfortReport: async () => {},
    getStats: async () => [],
    subscribeLive: async (onBatch) => {
      let t0 = clock();
      setInterval(() => {
        const now = clock();
        const points = [];
        for (let t = t0; t < now; t += 0.01) {
          let midi: number | null;
          if (state.phase === "recording" && lane && state.trial?.interaction === "sung") {
            midi = simulatedMidi(t - laneStart);
          } else {
            midi = 57.12 + 0.18 * Math.sin(2 * Math.PI * 5.2 * t) + 0.05 * Math.sin(t * 0.7);
          }
          points.push({ t, midi, clarity: midi == null ? 0.2 : 0.97, rms_db: midi == null ? -60 : -18 });
        }
        t0 = now;
        const b: LiveBatch = { now_s: now, points, peak: 0.25 + 0.05 * Math.sin(now * 3) };
        onBatch(b);
      }, 16);
    },
    listen: async (event, cb) => {
      (listeners[event] ??= []).push(cb as (p: unknown) => void);
      return () => {
        listeners[event] = (listeners[event] ?? []).filter((l) => l !== cb);
      };
    },
  };
}
