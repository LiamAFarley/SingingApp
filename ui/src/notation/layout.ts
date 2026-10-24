// Score → notation tokens (bars of notes and rests with durations, dots, ties and accidentals),
// plus the pitch ↔ staff-position mapping used by overlays and the editor.

import type { Clef } from "../bindings/Clef";
import type { Score } from "../bindings/Score";

export const TICKS_PER_QUARTER = 4;
const STEP_SEMITONES = [0, 2, 4, 5, 7, 9, 11];
const LETTERS = ["C", "D", "E", "F", "G", "A", "B"];
const SHARP_ORDER = [3, 0, 4, 1, 5, 2, 6];
const FLAT_ORDER = [6, 2, 5, 1, 4, 0, 3];

export interface SpelledPitch {
  /** 0 = C … 6 = B */
  step: number;
  alter: number;
  octave: number;
}

export function parsePitch(s: string): SpelledPitch | null {
  const m = /^([A-Ga-g])(#{1,2}|b{1,2}|x)?(-?\d)$/.exec(s.trim());
  if (!m) return null;
  const step = LETTERS.indexOf(m[1].toUpperCase());
  const acc = m[2] ?? "";
  const alter = acc === "x" ? 2 : acc.startsWith("#") ? acc.length : -acc.length;
  return { step, alter, octave: parseInt(m[3], 10) };
}

export function pitchMidi(p: SpelledPitch): number {
  return (p.octave + 1) * 12 + STEP_SEMITONES[p.step] + p.alter;
}

/** Diatonic staff position, seven per octave (C4 = 28). */
export function diatonic(p: SpelledPitch): number {
  return p.octave * 7 + p.step;
}

export function fromDiatonic(d: number, alter: number): SpelledPitch {
  return { step: ((d % 7) + 7) % 7, alter, octave: Math.floor(d / 7) };
}

export function accidentalText(alter: number): string {
  return alter === 2 ? "##" : alter === 1 ? "#" : alter === -1 ? "b" : alter === -2 ? "bb" : "";
}

export function pitchName(p: SpelledPitch): string {
  return `${LETTERS[p.step]}${accidentalText(p.alter)}${p.octave}`;
}

/** Display name with ♯/♭, e.g. `E♭4`. */
export function pitchDisplay(p: SpelledPitch): string {
  const acc = p.alter === 2 ? "𝄪" : p.alter === 1 ? "♯" : p.alter === -1 ? "♭" : p.alter === -2 ? "𝄫" : "";
  return `${LETTERS[p.step]}${acc}${p.octave}`;
}

export function parseTime(t: string): { beats: number; unit: number } {
  const [a, b] = t.split("/").map((x) => parseInt(x, 10));
  return { beats: a || 4, unit: b || 4 };
}

export function beatTicks(score: Score): number {
  return (TICKS_PER_QUARTER * 4) / parseTime(score.time).unit;
}

export function barTicks(score: Score): number {
  return beatTicks(score) * parseTime(score.time).beats;
}

export function signatureAlter(fifths: number, step: number): number {
  const n = Math.min(7, Math.abs(fifths));
  if (fifths > 0 && SHARP_ORDER.slice(0, n).includes(step)) return 1;
  if (fifths < 0 && FLAT_ORDER.slice(0, n).includes(step)) return -1;
  return 0;
}

/** The pitch a staff position has in this key (signature alteration applied). */
export function pitchAt(d: number, fifths: number): SpelledPitch {
  const p = fromDiatonic(d, 0);
  return { ...p, alter: signatureAlter(fifths, p.step) };
}

/** VexFlow staff line for a diatonic position: bottom line 1, top line 5. */
export function lineFor(d: number, clef: Clef): number {
  if (clef === "bass") return (d - 16) / 2;
  if (clef === "treble8vb") return (d - 21) / 2;
  return (d - 28) / 2;
}

export function diatonicForLine(line: number, clef: Clef): number {
  if (clef === "bass") return line * 2 + 16;
  if (clef === "treble8vb") return line * 2 + 21;
  return line * 2 + 28;
}

/** Written octave for VexFlow keys (treble 8vb is written an octave above how it sounds). */
export function writtenOctave(p: SpelledPitch, clef: Clef): number {
  return clef === "treble8vb" ? p.octave + 1 : p.octave;
}

export function vexKey(p: SpelledPitch, clef: Clef): string {
  return `${LETTERS[p.step].toLowerCase()}${accidentalText(p.alter)}/${writtenOctave(p, clef)}`;
}

export function vexClef(clef: Clef): string {
  return clef === "bass" ? "bass" : "treble";
}

export function restKey(clef: Clef): string {
  return clef === "bass" ? "d/3" : "b/4";
}

/** "G major", "E♭ minor". */
export function describeKey(score: Score): string {
  const t = score.key.tonic.replace("#", "♯").replace(/b$/, "♭");
  return `${t} ${score.key.mode}`;
}

export function keySpec(score: Score): string {
  return score.key.mode === "minor" ? `${score.key.tonic}m` : score.key.tonic;
}

// ---------------------------------------------------------------------------------------------

export interface Token {
  kind: "note" | "rest";
  bar: number;
  /** Absolute start tick. */
  tick: number;
  dur: number;
  /** VexFlow base duration: w, h, q, 8, 16. */
  code: string;
  dots: number;
  pitch?: SpelledPitch;
  /** Accidental to draw: #, b, n, ##, bb. */
  accidental?: string;
  /** Index among the score's sounding notes (for grading markers). */
  noteIndex?: number;
  /** Index into score.notes, or -1 for a rest that fills a gap. */
  scoreIndex: number;
  tieToNext: boolean;
  tiedFromPrev: boolean;
  fullBarRest: boolean;
}

const DURATIONS: [number, string, number][] = [
  [16, "w", 0],
  [12, "h", 1],
  [8, "h", 0],
  [6, "q", 1],
  [4, "q", 0],
  [3, "8", 1],
  [2, "8", 0],
  [1, "16", 0],
];

/** Splits a note's length into written values, largest first. */
function splitNote(len: number): [number, string, number][] {
  const out: [number, string, number][] = [];
  let left = len;
  while (left > 0) {
    const d = DURATIONS.find(([t]) => t <= left)!;
    out.push(d);
    left -= d[0];
  }
  return out;
}

/** Splits a rest so that each piece starts on a multiple of its own length (beat-aligned). */
function splitRest(start: number, len: number, beat: number): [number, string, number][] {
  const out: [number, string, number][] = [];
  let t = start;
  const end = start + len;
  while (t < end) {
    const left = end - t;
    let piece = DURATIONS.find(([d, , dots]) => d <= left && dots === 0 && t % d === 0 && (d <= beat || t % beat === 0));
    if (!piece) piece = DURATIONS.find(([d]) => d <= left)!;
    out.push(piece);
    t += piece[0];
  }
  return out;
}

/** Bars of tokens for a score. Notes crossing barlines (or with unwritable lengths) are tied. */
export function layoutTokens(score: Score): Token[][] {
  const bar = barTicks(score);
  const beat = beatTicks(score);
  const total = score.bars * bar;
  const bars: Token[][] = Array.from({ length: score.bars }, () => []);
  let noteIndex = 0;

  const addRest = (start: number, len: number, scoreIndex: number) => {
    let t = start;
    const end = start + len;
    while (t < end) {
      const b = Math.floor(t / bar);
      const barEnd = (b + 1) * bar;
      const segEnd = Math.min(end, barEnd);
      if (t === b * bar && segEnd === barEnd) {
        bars[b].push({ kind: "rest", bar: b, tick: t, dur: bar, code: "w", dots: 0, scoreIndex, tieToNext: false, tiedFromPrev: false, fullBarRest: true });
      } else {
        let at = t;
        for (const [d, code, dots] of splitRest(t - b * bar, segEnd - t, beat)) {
          bars[b].push({ kind: "rest", bar: b, tick: at, dur: d, code, dots, scoreIndex, tieToNext: false, tiedFromPrev: false, fullBarRest: false });
          at += d;
        }
      }
      t = segEnd;
    }
  };

  const addNote = (start: number, len: number, pitch: SpelledPitch, scoreIndex: number) => {
    const pieces: Token[] = [];
    let t = start;
    const end = start + len;
    while (t < end) {
      const b = Math.floor(t / bar);
      const segEnd = Math.min(end, (b + 1) * bar);
      for (const [d, code, dots] of splitNote(segEnd - t)) {
        const tok: Token = { kind: "note", bar: b, tick: t, dur: d, code, dots, pitch, noteIndex, scoreIndex, tieToNext: false, tiedFromPrev: pieces.length > 0, fullBarRest: false };
        pieces.push(tok);
        bars[b].push(tok);
        t += d;
      }
    }
    pieces.forEach((p, i) => (p.tieToNext = i < pieces.length - 1));
    noteIndex += 1;
  };

  let t = 0;
  score.notes.forEach((n, i) => {
    if (n.start > t) addRest(t, n.start - t, -1);
    const pitch = n.pitch ? parsePitch(n.pitch) : null;
    if (pitch) addNote(n.start, n.dur, pitch, i);
    else addRest(n.start, n.dur, i);
    t = n.start + n.dur;
  });
  if (t < total) addRest(t, total - t, -1);

  // Accidentals: shown when a note differs from the key signature or from an earlier
  // alteration of the same staff position in the bar. Tied continuations show none.
  for (const tokens of bars) {
    const altered = new Map<number, number>();
    for (const tok of tokens) {
      // Tied continuations carry the first piece's accidental; one tied in across the barline
      // shows none, and later notes of the bar still need theirs.
      if (tok.kind !== "note" || !tok.pitch || tok.tiedFromPrev) continue;
      const d = diatonic(tok.pitch);
      const current = altered.get(d) ?? signatureAlter(score.key.fifths, tok.pitch.step);
      if (tok.pitch.alter !== current) {
        tok.accidental = tok.pitch.alter === 0 ? "n" : accidentalText(tok.pitch.alter);
        altered.set(d, tok.pitch.alter);
      }
    }
  }
  return bars;
}

/**
 * Continuous staff position for a (fractional) MIDI pitch: linear between the key's scale notes,
 * with the score's own spelled pitches as anchors so a note sung exactly in tune sits on its
 * notehead.
 */
export function makePitchToDiatonic(score: Score): (midi: number) => number {
  const anchors = new Map<number, number>();
  for (let octave = -1; octave <= 9; octave++) {
    for (let step = 0; step < 7; step++) {
      const p = { step, alter: signatureAlter(score.key.fifths, step), octave };
      anchors.set(pitchMidi(p), diatonic(p));
    }
  }
  for (const n of score.notes) {
    const p = n.pitch ? parsePitch(n.pitch) : null;
    if (p) anchors.set(pitchMidi(p), diatonic(p));
  }
  const pts = [...anchors.entries()].sort((a, b) => a[0] - b[0]);
  return (midi: number) => {
    let lo = 0;
    let hi = pts.length - 1;
    if (midi <= pts[0][0]) return pts[0][1];
    if (midi >= pts[hi][0]) return pts[hi][1];
    while (hi - lo > 1) {
      const mid = (lo + hi) >> 1;
      if (pts[mid][0] <= midi) lo = mid;
      else hi = mid;
    }
    const [m0, d0] = pts[lo];
    const [m1, d1] = pts[hi];
    return d0 + ((midi - m0) / (m1 - m0)) * (d1 - d0);
  };
}
