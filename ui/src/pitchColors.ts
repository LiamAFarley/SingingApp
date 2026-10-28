/** Pitch-class colours for the sight-singing note labels (Taneda, or the user's colours). */

import { useEffect, useState } from "react";
import { api } from "./api";
import { parseNote, pcId } from "./music";

/**
 * Taneda's palette, used when My colours has no entry for a pitch class.
 * Keep in step with `trainer-core/src/labels.rs`.
 */
const DEFAULT_PITCH_COLORS: Record<string, string> = {
  C: "#d64545",
  "C#": "#c43d8a",
  D: "#3a6fd8",
  "D#": "#2a9d8f",
  E: "#e6c229",
  F: "#e07a2f",
  "F#": "#c9a227",
  G: "#8a5a32",
  "G#": "#3d3d8f",
  A: "#8d8d8d",
  "A#": "#e06aa0",
  B: "#3d9a4a",
};

/** Canonical pitch-class id (`C#`, enharmonic spellings shared) for a written pitch. */
export function pitchClassId(pitch: string): string | null {
  const midi = parseNote(pitch);
  return midi == null ? null : pcId(midi);
}

let cached: Record<string, string> | null = null;
let loading: Promise<Record<string, string>> | null = null;

/**
 * My colours when it has entries, otherwise the Taneda palette. Loaded once for the session.
 * A custom entry only replaces that pitch class, so the aid still has a colour while it is
 * being filled in.
 */
export function usePitchColors(): Record<string, string> {
  const [colors, setColors] = useState(cached ?? DEFAULT_PITCH_COLORS);

  useEffect(() => {
    if (cached) return;
    loading ??= api("listLabelSets")
      .then((sets) => {
        const mine = sets.find((s) => s.id === "my_colors");
        const out = { ...DEFAULT_PITCH_COLORS };
        for (const [pc, entry] of Object.entries(mine?.entries ?? {})) {
          if (entry.color) out[pc] = entry.color;
        }
        cached = out;
        return out;
      })
      .catch(() => DEFAULT_PITCH_COLORS);
    let alive = true;
    loading.then((next) => {
      if (alive) setColors(next);
    });
    return () => {
      alive = false;
    };
  }, []);

  return colors;
}
