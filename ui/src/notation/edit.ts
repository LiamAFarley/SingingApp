// Pitch and duration helpers for the score editor.

import type { Clef } from "../bindings/Clef";
import type { Score } from "../bindings/Score";
import type { ScoreNote } from "../bindings/ScoreNote";
import { barTicks, diatonic, fromDiatonic, parsePitch, pitchMidi, pitchName, signatureAlter, type SpelledPitch } from "./layout";

export interface Entry {
  pitch: SpelledPitch | null;
  dur: number;
  /** Given by the exercise; cannot be changed. */
  locked: boolean;
}

/** Durations by MuseScore's number keys: 2 sixteenth … 6 whole (ticks, a quarter is 4). */
export const DURATION_KEYS: Record<string, number> = { "2": 1, "3": 2, "4": 4, "5": 8, "6": 16 };

export const DURATION_NAMES: Record<number, string> = {
  1: "Sixteenth",
  2: "Eighth",
  3: "Dotted eighth",
  4: "Quarter",
  6: "Dotted quarter",
  8: "Half",
  12: "Dotted half",
  16: "Whole",
};

/** The middle of the staff, where the first typed note goes when there is nothing to follow. */
function clefCentre(clef: Clef): number {
  return clef === "bass" ? 22 : clef === "treble8vb" ? 27 : 34;
}

/** A letter typed in the key, in the octave nearest `near` (or the middle of the staff). */
export function letterPitch(letter: string, near: SpelledPitch | null, fifths: number, clef: Clef): SpelledPitch {
  const step = "CDEFGAB".indexOf(letter.toUpperCase());
  const target = near ? diatonic(near) : clefCentre(clef);
  let best = 0;
  let bestDist = Infinity;
  for (let octave = 0; octave <= 8; octave++) {
    const d = octave * 7 + step;
    const dist = Math.abs(d - target);
    if (dist < bestDist) {
      bestDist = dist;
      best = d;
    }
  }
  const p = fromDiatonic(best, 0);
  return { ...p, alter: signatureAlter(fifths, p.step) };
}

/**
 * A semitone up or down: the key's own note when there is one, otherwise the same letter with an
 * accidental (F → F♯, B → B♭), otherwise the neighbouring letter.
 */
export function semitone(p: SpelledPitch, dir: 1 | -1, fifths: number): SpelledPitch {
  const midi = pitchMidi(p) + dir;
  for (const d of [diatonic(p) + dir, diatonic(p)]) {
    const q = fromDiatonic(d, 0);
    const keyed = { ...q, alter: signatureAlter(fifths, q.step) };
    if (pitchMidi(keyed) === midi) return keyed;
  }
  const same = { ...p, alter: p.alter + dir };
  if (Math.abs(same.alter) <= 1) return same;
  const next = fromDiatonic(diatonic(p) + dir, 0);
  return { ...next, alter: midi - pitchMidi(next) };
}

export function octaveShift(p: SpelledPitch, dir: 1 | -1): SpelledPitch {
  return { ...p, octave: p.octave + dir };
}

/** Entries with their start ticks, as score notes. */
export function toScoreNotes(entries: Entry[]): ScoreNote[] {
  let t = 0;
  return entries.map((e) => {
    const n: ScoreNote = { pitch: e.pitch ? pitchName(e.pitch) : null, start: t, dur: e.dur };
    t += e.dur;
    return n;
  });
}

export function fromScore(score: Score): Entry[] {
  const out: Entry[] = [];
  let t = 0;
  for (const n of score.notes) {
    if (n.start > t) out.push({ pitch: null, dur: n.start - t, locked: true });
    out.push({ pitch: n.pitch ? parsePitch(n.pitch) : null, dur: n.dur, locked: true });
    t = n.start + n.dur;
  }
  return out;
}

export function totalTicks(score: Score): number {
  return score.bars * barTicks(score);
}

export function used(entries: Entry[]): number {
  return entries.reduce((s, e) => s + e.dur, 0);
}

/** "Bar 2, beat 3" for a tick position. */
export function describePosition(score: Score, tick: number): string {
  const bar = barTicks(score);
  const beat = bar / parseInt(score.time.split("/")[0], 10);
  const b = Math.floor(tick / bar);
  const within = tick - b * bar;
  const beatNo = within / beat + 1;
  return `Bar ${b + 1}, beat ${Number.isInteger(beatNo) ? beatNo : beatNo.toFixed(2).replace(/0+$/, "")}`;
}
