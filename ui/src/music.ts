import type { PitchRefMode } from "./bindings/PitchRefMode";

const DISPLAY = ["C", "C♯", "D", "E♭", "E", "F", "F♯", "G", "A♭", "A", "B♭", "B"];
const SHARP_IDS = ["C", "C#", "D", "D#", "E", "F", "F#", "G", "G#", "A", "A#", "B"];

export function pcDisplay(midi: number): string {
  return DISPLAY[((Math.round(midi) % 12) + 12) % 12];
}

export function octave(midi: number): number {
  return Math.floor(Math.round(midi) / 12) - 1;
}

export function noteDisplay(midi: number): string {
  return `${pcDisplay(midi)}${octave(midi)}`;
}

/** Display name for a pitch-class id such as "G#". */
export function pcIdDisplay(id: string): string {
  const i = SHARP_IDS.indexOf(id);
  return i < 0 ? id : DISPLAY[i];
}

export function pcId(index: number): string {
  return SHARP_IDS[((index % 12) + 12) % 12];
}

/** Parses names like "F#4", "Eb3", "C4" into MIDI. */
export function parseNote(name: string): number | null {
  const m = /^([A-Ga-g])([#b♯♭]*)(-?\d+)$/.exec(name.trim());
  if (!m) return null;
  const base: Record<string, number> = { C: 0, D: 2, E: 4, F: 5, G: 7, A: 9, B: 11 };
  let semis = base[m[1].toUpperCase()];
  for (const c of m[2]) semis += c === "#" || c === "♯" ? 1 : -1;
  return (parseInt(m[3], 10) + 1) * 12 + semis;
}

export function noteId(midi: number): string {
  return `${SHARP_IDS[((midi % 12) + 12) % 12]}${octave(midi)}`;
}

export function hzToMidi(hz: number, a4 = 440): number {
  return 69 + 12 * Math.log2(hz / a4);
}

export function midiToHz(midi: number, a4 = 440): number {
  return a4 * Math.pow(2, (midi - 69) / 12);
}

export function describeCents(c: number): string {
  const r = Math.round(c);
  if (r === 0) return "in tune";
  if (Math.abs(r) === 1) return r > 0 ? "1 cent sharp" : "1 cent flat";
  return r > 0 ? `${r} cents sharp` : `${-r} cents flat`;
}

export function pct(v: number | null | undefined): string {
  return v == null ? "–" : `${Math.round(v * 100)}%`;
}

export function fmtMs(v: number | null | undefined): string {
  return v == null ? "–" : `${Math.round(v)} ms`;
}

export function fmtCents(v: number | null | undefined): string {
  return v == null ? "–" : `${Math.round(v)}¢`;
}

export function fmtDate(iso: string | null | undefined): string {
  if (!iso) return "–";
  const d = new Date(iso);
  return d.toLocaleString(undefined, { month: "short", day: "numeric", hour: "2-digit", minute: "2-digit" });
}

export function fmtDuration(s: number | null | undefined): string {
  if (s == null) return "–";
  const m = Math.floor(s / 60);
  const r = Math.round(s % 60);
  return m > 0 ? `${m} min ${r.toString().padStart(2, "0")} s` : `${r} s`;
}

/**
 * How a sung trace is drawn on a staff for the trial's grading mode: tinted by its distance from
 * the written notes unless graded relative to the singer's own start, and moved into the written
 * octave when the octave does not count.
 */
export function traceStyle(mode: PitchRefMode | null | undefined): { tint: boolean; foldOctaves: boolean } {
  return {
    tint: mode !== "relative_to_first" && mode !== "relative_drift_tolerant",
    foldOctaves: mode === "any_octave" || mode === "pitch_class",
  };
}

export const KIND_TITLES: Record<string, string> = {
  pitch_memory_match: "Note memory (find the tone)",
  span: "Memory span",
  pitch_id: "Name the note",
  sing_pitch: "Sing a named note",
  pitch_pairing: "Note pairing",
  tonal_center_id: "Find the key",
  interval_id: "Intervals",
  chord_id: "Chords",
  scale_id: "Scales",
  pitch_match: "Pitch matching",
  sustain: "Sustained note",
  sing_interval: "Interval singing",
  pattern: "Sung patterns",
  sight_sing: "Sight singing",
  melodic_dictation: "Melodic dictation",
  anchor_song: "Anchor songs",
  incidental_pairing: "Colour task",
  scale_degree_id: "Scale degrees",
  interval_comparison: "Interval comparison",
  harmony: "Harmony singing",
  pitch_detect: "Find the note",
  chord_color_id: "Chord colours",
};

/** Colours of the incidental-pairing task (same ids as the backend's PAIRING_COLORS). */
export const PAIRING_COLORS: Record<string, string> = {
  red: "#d1242f",
  orange: "#e16f24",
  yellow: "#d4a72c",
  olive: "#7d8b1d",
  green: "#1a7f37",
  teal: "#138a8a",
  sky: "#3a9ad9",
  blue: "#2d4fdf",
  indigo: "#5b3fc4",
  purple: "#9a46c9",
  pink: "#d6409f",
  brown: "#8a5a2b",
};
