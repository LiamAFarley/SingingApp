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
import type { Anchor } from "./bindings/Anchor";
import type { LibraryEntry } from "./bindings/LibraryEntry";
import type { Score } from "./bindings/Score";
import type { ScoreNote } from "./bindings/ScoreNote";

const clock = () => performance.now() / 1000;

const presets: PresetView[] = [
  p("ap_progressive_01", "Absolute pitch: stage 1", "Absolute pitch", "Classes: C, E, A♭. Suggested criterion for the next stage: at least 90% over two sessions.", 72, 7.6, ["Name the note", "Sing the note"]),
  p("ap_progressive_02", "Absolute pitch: stage 2", "Absolute pitch", "Classes: C, E, A♭, D. Suggested criterion for the next stage: at least 90% over two sessions.", 72, 7.6, ["Name the note", "Sing the note"]),
  p("ap_progressive_03", "Absolute pitch: stage 3", "Absolute pitch", "Classes: C, E, A♭, D, F♯. Suggested criterion for the next stage: at least 90% over two sessions.", 72, 7.6, ["Name the note", "Sing the note"]),
  p("ap_daily", "Absolute pitch: daily", "Absolute pitch", "Name 36 notes and sing 12, all twelve pitch classes.", 48, 5.4, ["Name the note", "Sing the note"]),
  p("ap_assessment", "Absolute pitch assessment", "Absolute pitch", "Holdout tones, no feedback: identification across octaves and timbres, speed, singing and key. Run first in a session.", 156, 16.5, ["12-class identification", "Cross-octave identification"]),
  p("ear_intervals_melodic", "Intervals: melodic", "Ear training", "Melodic intervals up and down, minor 2nd to octave.", 30, 3.2, ["Interval identification"]),
  p("degrees_01", "Scale degrees: level 1", "Ear training", "Degrees 1, 2 and 3 in C major. Suggested criterion for the next level: at least 90% over two sessions.", 30, 4.1, ["Scale degrees"]),
  p("degrees_02", "Scale degrees: level 2", "Ear training", "Degrees 1 to 5 in C major. Suggested criterion for the next level: at least 90% over two sessions.", 30, 4.1, ["Scale degrees"]),
  p("degrees_04", "Scale degrees: level 4", "Ear training", "All seven degrees; the major key changes every question. Suggested criterion for the next level: at least 90% over two sessions.", 30, 4.6, ["Scale degrees"]),
  p("ear_intervals_in_key", "Intervals in a key", "Ear training", "Diatonic intervals in C major after a cadence, both notes from the scale.", 30, 5.0, ["Melodic intervals in C major"]),
  p("ear_interval_comparison", "Interval comparison", "Ear training", "Two intervals in a row: say which is larger, or that they are the same.", 30, 3.5, ["Which interval is larger?"]),
  p("ear_chords", "Chords: triads and inversions", "Ear training", "Triads, inversions and seventh chords.", 60, 6.8, ["Triads", "Major and minor inversions", "Seventh chords"]),
  p("harmony_01", "Harmony: a third below", "Singing", "Sing a third below the melody: three times hearing your part, then six times by ear.", 9, 4.2, ["Practice: your part plays along", "By ear"]),
  p("harmony_05", "Harmony: a fifth below", "Singing", "Sing a fifth below the melody: three times hearing your part, then six times by ear.", 9, 4.2, ["Practice: your part plays along", "By ear"]),
  p("harmony_08", "Harmony: a fourth above", "Singing", "Sing a fourth above the melody: three times hearing your part, then six times by ear.", 9, 4.2, ["Practice: your part plays along", "By ear"]),
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

let anchors: Anchor[] = [
  { id: "a1", title: "Yesterday (The Beatles)", first_note: "G4", melody: null, created_at: "", source: "manual" },
  {
    id: "a2",
    title: "Happy Birthday (as the family sings it)",
    first_note: "G3",
    melody: { key: "C major", time: "3/4", clef: "auto", tempo_bpm: 100, notes: [{ note: "G3", beats: 0.75 }, { note: "G3", beats: 0.25 }, { note: "A3", beats: 1 }, { note: "G3", beats: 1 }, { note: "C4", beats: 1 }, { note: "B3", beats: 2 }], title: null },
    created_at: "",
    source: "llm",
  },
];

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
    grading_mode: null,
    label: null,
    label_at_s: null,
    timeline_start_s: null,
    response_start_s: clock(),
    response_end_s: null,
    attempt: 0,
    assisted: false,
    score: null,
    plays: 0,
    max_plays: 0,
  };
}

function singPitchTrial(): TrialView {
  const start = clock() + 0.1;
  return {
    kind: "sing_pitch",
    interaction: "sing_pitch",
    prompt: "A",
    choices: [],
    layout: "list",
    deadline_ms: null,
    can_replay: false,
    can_reference: false,
    lane: null,
    show_live_pitch: true,
    grading_mode: null,
    label: null,
    label_at_s: null,
    timeline_start_s: null,
    response_start_s: start,
    response_end_s: start + 4,
    attempt: 0,
    assisted: false,
    score: null,
    plays: 0,
    max_plays: 0,
  };
}

function pairingTrial(): TrialView {
  return {
    kind: "pitch_pairing",
    interaction: "pairing",
    prompt: "Listen",
    choices: [],
    layout: "list",
    deadline_ms: null,
    can_replay: false,
    can_reference: false,
    lane: null,
    show_live_pitch: false,
    grading_mode: null,
    label: "F♯",
    label_at_s: clock() + 0.3,
    timeline_start_s: null,
    response_start_s: null,
    response_end_s: null,
    attempt: 0,
    assisted: false,
    score: null,
    plays: 0,
    max_plays: 0,
  };
}

// --- sight singing ---------------------------------------------------------------------------

const SIGHT_NOTES: [string | null, number, number][] = [
  ["G4", 0, 4], ["A4", 4, 2], ["B4", 6, 2], ["C5", 8, 6], ["B4", 14, 2],
  ["A4", 16, 8], [null, 24, 4], ["D4", 28, 4],
  ["B4", 32, 4], ["C#5", 36, 2], ["D5", 38, 2], ["D5", 40, 12],
  ["C5", 52, 2], ["A4", 54, 2], ["G4", 56, 8],
];

function sightScore(): Score {
  return {
    key: { tonic: "G", mode: "major", fifths: 1 },
    time: "4/4",
    clef: "treble",
    tempo_bpm: 72,
    bars: 4,
    notes: SIGHT_NOTES.map(([pitch, start, dur]) => ({ pitch, start, dur })),
    title: null,
  };
}

const NOTE_MIDI: Record<string, number> = { G4: 67, A4: 69, B4: 71, C5: 72, "C#5": 73, D5: 74, D4: 62 };

function sightLane(): LaneSpec {
  const beat = 60 / 72;
  const sing = 13 * beat;
  const pitched = SIGHT_NOTES.filter((n) => n[0] != null);
  return {
    notes: pitched.map(([p, start, dur]) => ({ midi: NOTE_MIDI[p!], start_s: sing + (start / 4) * beat, dur_s: (dur / 4) * beat })),
    reference: [],
    accompaniment: [],
    beat_s: beat,
    beats_per_bar: 4,
    sing_start_s: sing,
    total_s: sing + 17 * beat,
    midi_low: 60,
    midi_high: 76,
    targets_hidden: true,
    metronome: true,
    sections: [
      { label: "Establish key", start_s: 0, end_s: 5 * beat },
      { label: "First note", start_s: 6 * beat, end_s: 8 * beat },
      { label: "Count-in", start_s: 9 * beat, end_s: sing },
      { label: "Sing", start_s: sing, end_s: sing + 16 * beat },
    ],
    cursor: false,
  };
}

function sightTrial(phaseOffset: number): TrialView {
  lane = sightLane();
  laneStart = clock() + 0.2 - phaseOffset;
  return {
    kind: "sight_sing",
    interaction: "sung",
    prompt: "Sing the melody",
    choices: [],
    layout: "list",
    deadline_ms: null,
    can_replay: false,
    can_reference: false,
    lane,
    show_live_pitch: true,
    grading_mode: "any_octave",
    label: null,
    label_at_s: null,
    timeline_start_s: laneStart,
    response_start_s: laneStart + lane.sing_start_s,
    response_end_s: laneStart + lane.total_s,
    attempt: 0,
    assisted: false,
    score: sightScore(),
    plays: 0,
    max_plays: 0,
  };
}

function sightFeedback(): FeedbackView {
  const l = sightLane();
  const pitched = l.notes;
  // Index 3 sung a semitone low, 5 a little sharp, 8 flat, 12 missed.
  const notes: NoteResult[] = pitched.map((n, i) => {
    const missed = i === 12;
    const wrong = i === 3;
    const cents = wrong ? -100 : i === 5 ? 31 : i === 8 ? -34 : [6, -9, 12, 4, -3][i % 5];
    return {
      target_midi: n.midi,
      expected_onset_s: n.start_s,
      matched: !missed,
      sung_midi: missed ? null : n.midi + cents / 100,
      cents_error: missed ? null : cents,
      onset_error_ms: missed ? null : 25 + (i % 3) * 20,
      correct: !missed && !wrong,
      in_tune: !missed && !wrong && Math.abs(cents) <= 25,
      on_time: !missed,
      onset_s: missed ? null : n.start_s + 0.04,
      offset_s: missed ? null : n.start_s + n.dur_s - 0.05,
    };
  });
  const trace: [number, number | null][] = [];
  for (let t = l.sing_start_s - 0.2; t < l.sing_start_s + 16 * l.beat_s; t += 0.01) {
    const i = pitched.findIndex((n) => t >= n.start_s + 0.03 && t < n.start_s + n.dur_s - 0.04);
    if (i < 0 || i === 12) {
      trace.push([t, null]);
      continue;
    }
    const n = pitched[i];
    const local = t - n.start_s;
    const scoop = local < 0.1 ? -0.4 * (1 - local / 0.1) : 0;
    trace.push([t, n.midi + (notes[i].cents_error ?? 0) / 100 + scoop + 0.12 * Math.sin(2 * Math.PI * 5.5 * t)]);
  }
  return {
    correct: false,
    summary: "11 of 14 notes right, 1 missed",
    correct_choice: null,
    answered: null,
    cents: null,
    notes,
    trace,
    can_retry: true,
    can_sing_along: true,
    can_listen: true,
    can_show_answer: false,
    can_play_answer: true,
    can_hear_choices: false,
    tuner: null,
    auto_advance_ms: null,
    hidden: false,
    dictation: null,
  };
}

// --- melodic dictation -------------------------------------------------------------------------

const DICTATION_TARGET: [string | null, number, number][] = [
  ["G4", 0, 4], ["A4", 4, 2], ["B4", 6, 2], ["C5", 8, 4], ["B4", 12, 4],
  ["A4", 16, 4], ["F#4", 20, 4], ["G4", 24, 8],
];

function dictationScore(notes: [string | null, number, number][]): Score {
  return {
    key: { tonic: "G", mode: "major", fifths: 1 },
    time: "4/4",
    clef: "treble",
    tempo_bpm: 80,
    bars: 2,
    notes: notes.map(([pitch, start, dur]) => ({ pitch, start, dur })),
    title: null,
  };
}

let dictationPlays = 1;

function dictationTrial(): TrialView {
  return {
    kind: "melodic_dictation",
    interaction: "dictation",
    prompt: "Write the melody",
    choices: [],
    layout: "list",
    deadline_ms: null,
    can_replay: true,
    can_reference: true,
    lane: null,
    show_live_pitch: false,
    grading_mode: null,
    label: null,
    label_at_s: null,
    timeline_start_s: clock(),
    response_start_s: null,
    response_end_s: null,
    attempt: 0,
    assisted: false,
    score: dictationScore([DICTATION_TARGET[0]]),
    plays: dictationPlays,
    max_plays: 4,
  };
}

/** A simple stand-in for the backend's grading: same pitch at the same position and length. */
function dictationFeedback(answer: ScoreNote[]): FeedbackView {
  const target = dictationScore(DICTATION_TARGET);
  const key = (n: ScoreNote) => `${n.pitch}@${n.start}/${n.dur}`;
  const written = new Set(answer.filter((n) => n.pitch).map(key));
  const wanted = new Set(target.notes.map(key));
  const target_marks = target.notes.map((n) => (written.has(key(n)) ? "correct" : answer.some((a) => a.start === n.start && a.pitch) ? "wrong" : "missed"));
  const answer_marks = answer.map((n) => (n.pitch ? (wanted.has(key(n)) ? "correct" : "wrong") : null));
  const right = target_marks.filter((m) => m === "correct").length;
  const all = right === target.notes.length && answer_marks.every((m) => m !== "wrong");
  return {
    correct: all,
    summary: all ? `All ${right} notes right.` : `${right} of ${target.notes.length} notes right.`,
    correct_choice: null,
    answered: null,
    cents: null,
    notes: [],
    trace: [],
    can_retry: true,
    can_sing_along: false,
    can_listen: true,
    can_show_answer: false,
    can_play_answer: answer.length > 0,
    can_hear_choices: false,
    tuner: null,
    auto_advance_ms: null,
    hidden: false,
    dictation: { target, answer: { ...target, notes: answer }, target_marks, answer_marks },
  };
}

function colorTrial(): TrialView {
  const now = clock();
  return {
    kind: "incidental_pairing",
    interaction: "choice",
    prompt: "Press the colour",
    choices: [
      { id: "red", label: "Red" },
      { id: "yellow", label: "Yellow" },
      { id: "green", label: "Green" },
      { id: "indigo", label: "Indigo" },
    ],
    layout: "colors",
    deadline_ms: 3000,
    can_replay: false,
    can_reference: false,
    lane: null,
    show_live_pitch: false,
    grading_mode: null,
    label: "green",
    label_at_s: now + 0.3,
    timeline_start_s: null,
    response_start_s: now,
    response_end_s: null,
    attempt: 0,
    assisted: false,
    score: null,
    plays: 0,
    max_plays: 0,
  };
}

function degreeTrial(): TrialView {
  const ids = ["1", "2", "3", "4", "5", "6", "7"];
  return {
    kind: "scale_degree_id",
    interaction: "choice",
    prompt: "Which degree? (major key)",
    choices: ids.map((id) => ({ id, label: id })),
    layout: "degrees",
    deadline_ms: null,
    can_replay: true,
    can_reference: false,
    lane: null,
    show_live_pitch: false,
    grading_mode: null,
    label: null,
    label_at_s: null,
    timeline_start_s: null,
    response_start_s: clock(),
    response_end_s: null,
    attempt: 0,
    assisted: false,
    score: null,
    plays: 0,
    max_plays: 0,
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
    accompaniment: [],
    beat_s: beat,
    beats_per_bar: 4,
    sing_start_s: sing,
    total_s: sing + 16 * beat,
    midi_low: tonic - 4,
    midi_high: tonic + 16,
    targets_hidden: false,
    metronome: true,
    sections: [],
    cursor: false,
  };
}

/** Harmony by ear: the melody plays (accompaniment) and the singer adds a third below. */
function harmonyLane(): LaneSpec {
  const beat = 60 / 80;
  const tonic = 50;
  const dia = (d: number) => 12 * Math.floor(d / 7) + [0, 2, 4, 5, 7, 9, 11][((d % 7) + 7) % 7];
  const melody: [number, number][] = [[9, 1], [8, 1], [7, 1], [8, 1], [9, 1], [9, 1], [9, 2], [8, 1], [8, 1], [8, 2], [9, 1], [11, 1], [11, 2], [9, 1], [8, 1], [7, 2]];
  const sing = 13 * beat;
  let t = 0;
  const voices = melody.map(([d, len]) => {
    const n = { d, start: t, len };
    t += len;
    return n;
  });
  const last = voices.length - 1;
  return {
    notes: voices.map((v, i) => ({ midi: tonic + dia(i === last ? v.d - 3 : v.d - 2), start_s: sing + v.start * beat, dur_s: v.len * beat })),
    reference: [{ midi: tonic + dia(7), start_s: 6 * beat, dur_s: 2 * beat }],
    accompaniment: voices.map((v) => ({ midi: tonic + dia(v.d), start_s: sing + v.start * beat, dur_s: v.len * beat })),
    beat_s: beat,
    beats_per_bar: 4,
    sing_start_s: sing,
    total_s: sing + 21 * beat,
    midi_low: tonic + 8,
    midi_high: tonic + 23,
    targets_hidden: true,
    metronome: true,
    sections: [
      { label: "Establish key", start_s: 0, end_s: 5 * beat },
      { label: "First note", start_s: 6 * beat, end_s: 8 * beat },
      { label: "Count-in", start_s: 9 * beat, end_s: sing },
      { label: "Sing", start_s: sing, end_s: sing + 20 * beat },
    ],
    cursor: false,
  };
}

function harmonyTrial(): TrialView {
  const trial = sungTrial();
  const practice = scenario === "harmony-practice";
  lane = { ...harmonyLane(), targets_hidden: !practice };
  laneStart = clock() + 0.2 - (scenario === "harmony" || practice ? lane.sing_start_s + 5 * lane.beat_s : 0);
  return {
    ...trial,
    kind: "harmony",
    prompt: "Sing a third below the melody",
    lane,
    assisted: practice,
    show_live_pitch: true,
    grading_mode: "absolute",
    timeline_start_s: laneStart,
    response_start_s: laneStart + lane.sing_start_s,
    response_end_s: laneStart + lane.total_s,
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
    grading_mode: "any_octave",
    label: null,
    label_at_s: null,
    timeline_start_s: laneStart,
    response_start_s: laneStart + lane.sing_start_s,
    response_end_s: laneStart + lane.total_s,
    attempt: 0,
    assisted: false,
    score: null,
    plays: 0,
    max_plays: 0,
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
    can_play_answer: true,
    can_hear_choices: false,
    tuner: null,
    auto_advance_ms: null,
    hidden: false,
    dictation: null,
  };
}

const report: SessionReport = {
  session: pastSessions[0],
  accuracy: 0.86,
  duration_s: 541,
  blocks: [
    { index: 0, title: "Name the note", kind: "pitch_id", trials: 60, assisted: 0, graded: 60, correct: 53, accuracy: 0.883, mean_score: 0.883, median_rt_ms: 1240, median_abs_cents: null, pass_mark: 0.9, passed: false, feedback: "after_trial", assessment: false },
    { index: 1, title: "Sing the note", kind: "sing_pitch", trials: 12, assisted: 2, graded: 12, correct: 9, accuracy: 0.75, mean_score: 0.75, median_rt_ms: 820, median_abs_cents: 23, pass_mark: null, passed: null, feedback: "after_trial", assessment: false },
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
const mockStart = clock();

function startScenario() {
  if (scenario === "pitch") {
    emit({ phase: "answering", session_id: "demo", title: "Absolute pitch: stage 1", block_count: 2, block_title: "Name the note", block_kind: "pitch_id", trial_index: 6, trial_count: 60, done_in_block: 6, correct_in_block: 5, trial: choiceTrial() });
  } else if (scenario === "sung" || scenario === "sung-feedback") {
    kind = "sung";
    const trial = sungTrial();
    emit({ phase: "recording", session_id: "demo", title: "Scales: listen and repeat", block_count: 2, block_title: "Major scale", block_kind: "pattern", trial_index: 1, trial_count: 4, done_in_block: 1, correct_in_block: 1, feedback_mode: "continuous", trial });
    if (scenario === "sung-feedback") emit({ phase: "feedback", feedback: sungFeedback() });
  } else if (scenario === "harmony" || scenario === "harmony-feedback" || scenario === "harmony-practice") {
    kind = "sung";
    const trial = harmonyTrial();
    const practice = scenario === "harmony-practice";
    emit({ phase: "recording", session_id: "demo", title: "Harmony: a third below", block_index: practice ? 0 : 1, block_count: 2, block_title: practice ? "Practice: your part plays along" : "By ear", block_kind: "harmony", trial_index: 1, trial_count: practice ? 3 : 6, done_in_block: practice ? 0 : 2, correct_in_block: practice ? 0 : 1, feedback_mode: "continuous", trial });
    if (scenario === "harmony-feedback") emit({ phase: "feedback", feedback: sungFeedback() });
  } else if (scenario === "sing-pitch-feedback") {
    emit({ phase: "recording", session_id: "demo", title: "Absolute pitch: stage 1", block_index: 1, block_count: 2, block_title: "Sing the note", block_kind: "sing_pitch", trial_index: 0, trial_count: 12, done_in_block: 1, correct_in_block: 0, feedback_mode: "after_trial", trial: { ...singPitchTrial(), prompt: "Sing A♭" } });
    emit({ phase: "feedback", feedback: {
      correct: false, summary: "Sang G3 (80 cents flat), did not settle", correct_choice: null, answered: null, cents: -80,
      notes: [], trace: [], can_retry: false, can_sing_along: false, can_listen: true, can_show_answer: false,
      can_play_answer: false, can_hear_choices: false, auto_advance_ms: null, hidden: false, dictation: null,
      tuner: { pitch_class: 8, midi: null, name: "A♭", reference_midi: 56 },
    } });
  } else if (scenario === "sing-pitch") {
    emit({ phase: "recording", session_id: "demo", title: "Absolute pitch: stage 1", block_index: 1, block_count: 2, block_title: "Sing the note", block_kind: "sing_pitch", trial_index: 3, trial_count: 12, done_in_block: 3, correct_in_block: 2, feedback_mode: "continuous", trial: singPitchTrial() });
  } else if (scenario === "pairing") {
    emit({ phase: "listening", session_id: "demo", title: "Absolute pitch: pairing", block_count: 1, block_title: "Hear and label", block_kind: "pitch_pairing", trial_index: 4, trial_count: 24, feedback_mode: "none", trial: pairingTrial() });
  } else if (scenario === "sight" || scenario === "sight-octave" || scenario === "sight-feedback" || scenario === "sight-intro") {
    kind = "sung";
    const offset = scenario === "sight-intro" ? 1.5 : 60 / 72 * 10.5;
    const trial = sightTrial(offset);
    emit({ phase: "recording", session_id: "demo", title: "Sight singing: stepwise", block_count: 1, block_title: "Four bars, major keys", block_kind: "sight_sing", trial_index: 2, trial_count: 6, done_in_block: 2, correct_in_block: 1, feedback_mode: "after_trial", trial });
    if (scenario === "sight-feedback") emit({ phase: "feedback", feedback: sightFeedback() });
  } else if (scenario === "dictation" || scenario === "dictation-feedback") {
    emit({ phase: "answering", session_id: "demo", title: "Melodic dictation", block_count: 1, block_title: "Two bars, stepwise", block_kind: "melodic_dictation", trial_index: 1, trial_count: 6, done_in_block: 1, correct_in_block: 1, feedback_mode: "after_trial", trial: dictationTrial() });
    if (scenario === "dictation-feedback") {
      const answer: ScoreNote[] = [
        { pitch: "G4", start: 0, dur: 4 }, { pitch: "A4", start: 4, dur: 2 }, { pitch: "B4", start: 6, dur: 2 },
        { pitch: "C5", start: 8, dur: 4 }, { pitch: "A4", start: 12, dur: 4 },
        { pitch: "A4", start: 16, dur: 4 }, { pitch: "F4", start: 20, dur: 4 }, { pitch: "G4", start: 24, dur: 8 },
      ];
      emit({ phase: "feedback", feedback: dictationFeedback(answer) });
    }
  } else if (scenario === "degrees" || scenario === "degrees-feedback") {
    emit({ phase: "answering", session_id: "demo", title: "Scale degrees: level 3", block_count: 1, block_title: "All seven degrees in C", block_kind: "scale_degree_id", trial_index: 8, trial_count: 30, done_in_block: 8, correct_in_block: 7, trial: degreeTrial() });
    if (scenario === "degrees-feedback") {
      emit({
        phase: "feedback",
        done_in_block: 9,
        feedback: {
          correct: false,
          summary: "You answered 5. It was 6.",
          correct_choice: "6",
          answered: "5",
          cents: null,
          notes: [],
          trace: [],
          can_retry: false,
          can_sing_along: false,
          can_listen: true,
          can_show_answer: true,
          can_play_answer: false,
          can_hear_choices: true,
          tuner: null,
          auto_advance_ms: null,
          hidden: false,
          dictation: null,
        },
      });
    }
  } else if (scenario === "colors") {
    emit({ phase: "answering", session_id: "demo", title: "Colour task (incidental pairing)", block_count: 1, block_title: "Press the colour", block_kind: "incidental_pairing", trial_index: 11, trial_count: 60, done_in_block: 11, correct_in_block: 11, trial: colorTrial() });
  } else if (scenario === "summary") {
    emit({ phase: "session_summary", session_id: "demo", title: "Absolute pitch: stage 1", block_count: 2, session_report: report });
  }
}

function runnerCommand(c: RunnerCommand) {
  switch (c.kind) {
    case "submit_score":
      if (state.phase === "answering" && state.trial?.interaction === "dictation") {
        emit({ phase: "feedback", done_in_block: state.done_in_block + 1, feedback: dictationFeedback(c.notes) });
      }
      break;
    case "replay":
      if (state.trial?.interaction === "dictation" && state.phase === "answering") {
        dictationPlays += 1;
        emit({ trial: { ...state.trial, plays: dictationPlays } });
      }
      break;
    case "play_score":
    case "play_choice":
      break;
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
          can_play_answer: false,
          can_hear_choices: true,
          tuner: null,
          dictation: null,
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

// --- melody library ----------------------------------------------------------------------------

function libraryEntry(id: string, title: string, collection: string | null, key: string, time: string, bars: number, low: string, high: string, parts: string[] = ["Part 1"], tags: string[] = []): LibraryEntry {
  const file = id.includes("#") ? `${id.split("#")[0]}.json` : `${id}.${collection ? "json" : "mid"}`;
  return { id, title, collection, tags, key, time, tempo_bpm: 72, bars, parts, low, high, file };
}

let libraryPieces: LibraryEntry[] = [
  libraryEntry("Ottman/Chapter 3/1", "No. 1", "Ottman/Chapter 3", "G major", "4/4", 4, "D4", "D5"),
  libraryEntry("Ottman/Chapter 3/2", "No. 2", "Ottman/Chapter 3", "F major", "3/4", 8, "C4", "C5"),
  libraryEntry("Ottman/Chapter 3/10", "No. 10", "Ottman/Chapter 3", "D minor", "6/8", 8, "A3", "D5", ["Part 1"], ["minor"]),
  libraryEntry("Duets/canon#1", "Canon in two parts", "Duets", "C major", "4/4", 8, "C4", "G5", ["Upper", "Lower"], ["two-part"]),
  libraryEntry("Duets/chorale", "Chorale phrase", "Duets", "D major", "4/4", 2, "D3", "F♯4", ["Soprano", "Alto", "Tenor", "Bass"], ["satb"]),
  libraryEntry("Folk tune", "Folk tune", null, "A minor", "3/4", 16, "E4", "E5"),
];

async function libraryScores(id: string): Promise<Score[]> {
  const piece = libraryPieces.find((p) => p.id === id);
  return piece && piece.parts.length > 1 ? [sightScore(), dictationScore(DICTATION_TARGET)] : [sightScore()];
}

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
    listAnchors: async () => anchors,
    saveAnchor: async (a) => {
      const saved = { ...a, id: a.id || `a${anchors.length + 1}`, created_at: new Date().toISOString(), source: "manual" };
      anchors = [...anchors.filter((x) => x.id !== saved.id), saved].sort((x, y) => x.title.localeCompare(y.title));
      return saved;
    },
    removeAnchor: async (id) => {
      const before = anchors.length;
      anchors = anchors.filter((x) => x.id !== id);
      return anchors.length < before;
    },
    libraryView: async () => ({
      folder: "C:\\Users\\you\\AppData\\Roaming\\SingingTrainer\\library",
      pieces: libraryPieces,
      errors: [{ file: "Ottman/Chapter 3/11.json", pieces: 0, error: "notes: bar 3 has 5 beats; a bar of 4/4 has 4", read_at: new Date().toISOString() }],
    }),
    libraryScores,
    libraryImport: async (files, collection) => {
      for (const f of files) {
        const id = `${collection ? `${collection}/` : ""}${f.name.replace(/\.[^.]+$/, "")}`;
        libraryPieces = [...libraryPieces, libraryEntry(id, f.name.replace(/\.[^.]+$/, ""), collection, "C major", "4/4", 4, "C4", "C5")];
      }
      return files.map((f) => ({ name: f.name, file: f.name, error: null }));
    },
    libraryRemoveFile: async (file) => {
      libraryPieces = libraryPieces.filter((p) => p.file !== file);
    },
    libraryOpenFolder: async () => {},
    libraryPlay: async () => 4000,
    stopPreview: async () => {},
    subscribeLive: async (onBatch) => {
      let t0 = clock();
      setInterval(() => {
        const now = clock();
        const points = [];
        for (let t = t0; t < now; t += 0.01) {
          let midi: number | null;
          if (state.phase === "recording" && lane && state.trial?.interaction === "sung") {
            midi = simulatedMidi(t - laneStart);
            // A man reading treble clef an octave down.
            if (midi != null && scenario === "sight-octave") midi -= 12;
          } else if (scenario === "sing-pitch-feedback") {
            // Gliding up from G onto A-flat.
            midi = 56 - Math.max(0, 0.8 - (t - mockStart) * 0.3) + 0.05 * Math.sin(2 * Math.PI * 5.2 * t);
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
