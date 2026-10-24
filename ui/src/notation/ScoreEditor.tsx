// A small monophonic score editor for dictation: durations, dots, rests, accidentals, keyboard
// and mouse entry, undo. Keys follow MuseScore: 2–6 durations, "." dot, 0 rest, A–G notes,
// arrows for pitch (Shift for octaves), +/−/= sharp, flat, natural.

import { useEffect, useMemo, useState } from "react";
import type { Score } from "../bindings/Score";
import type { ScoreNote } from "../bindings/ScoreNote";
import { useKeyOwner } from "../keyboard";
import { DURATION_KEYS, DURATION_NAMES, type Entry, describePosition, fromScore, letterPitch, octaveShift, semitone, toScoreNotes, totalTicks, used } from "./edit";
import { pitchAt, pitchDisplay, type SpelledPitch } from "./layout";
import { Staff, type StaffPointer } from "./Staff";

interface Props {
  given: Score;
  disabled?: boolean;
  /** Called with the written notes whenever they change. */
  onChange?: (notes: ScoreNote[]) => void;
}

export function ScoreEditor({ given, disabled, onChange }: Props) {
  const [entries, setEntries] = useState<Entry[]>(() => fromScore(given));
  const [history, setHistory] = useState<Entry[][]>([]);
  const [selected, setSelected] = useState<number | null>(null);
  const [dur, setDur] = useState(4);
  const [dotted, setDotted] = useState(false);
  const [hover, setHover] = useState<StaffPointer | null>(null);
  const total = totalTicks(given);
  const filled = used(entries);
  const fifths = given.key.fifths;

  useEffect(() => {
    onChange?.(toScoreNotes(entries));
    // onChange is a callback prop; only entries matter.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [entries]);

  const commit = (next: Entry[]) => {
    setHistory((h) => [...h.slice(-99), entries]);
    setEntries(next);
  };
  const undo = () => {
    setHistory((h) => {
      if (h.length === 0) return h;
      setEntries(h[h.length - 1]);
      setSelected(null);
      return h.slice(0, -1);
    });
  };

  const current = dotted ? Math.round(dur * 1.5) : dur;
  const editable = (i: number | null) => i != null && i >= 0 && i < entries.length && !entries[i].locked;
  const lastPitch = (before = entries.length): SpelledPitch | null => {
    for (let i = before - 1; i >= 0; i--) if (entries[i].pitch) return entries[i].pitch;
    return null;
  };

  const append = (pitch: SpelledPitch | null) => {
    const left = total - filled;
    if (left <= 0) return;
    commit([...entries, { pitch, dur: Math.min(current, left), locked: false }]);
    setSelected(null);
  };

  const setPitch = (i: number, pitch: SpelledPitch | null) => {
    commit(entries.map((e, k) => (k === i ? { ...e, pitch } : e)));
  };

  const setDuration = (i: number, d: number) => {
    const others = filled - entries[i].dur;
    const fit = Math.min(d, total - others);
    if (fit <= 0) return;
    commit(entries.map((e, k) => (k === i ? { ...e, dur: fit } : e)));
  };

  const remove = (i: number) => {
    commit(entries.filter((_, k) => k !== i));
    setSelected(null);
  };

  /** The note an edit applies to: the selection, or the last written note. */
  const target = (): number | null => {
    if (editable(selected)) return selected;
    for (let i = entries.length - 1; i >= 0; i--) if (!entries[i].locked) return entries[i].pitch ? i : null;
    return null;
  };

  const adjust = (f: (p: SpelledPitch) => SpelledPitch) => {
    const i = target();
    if (i == null || !entries[i].pitch) return;
    setPitch(i, f(entries[i].pitch!));
  };

  const chooseDuration = (d: number) => {
    setDur(d);
    setDotted(false);
    if (editable(selected)) setDuration(selected!, d);
  };

  const toggleDot = () => {
    if (editable(selected)) {
      const e = entries[selected!];
      // A dotted sixteenth is finer than the grid.
      if (e.dur === 1) return;
      const base = [1, 2, 4, 8, 16].includes(e.dur) ? e.dur : e.dur % 3 === 0 ? (e.dur / 3) * 2 : e.dur;
      setDuration(selected!, e.dur === base ? Math.round(base * 1.5) : base);
    } else {
      setDotted((d) => !d && dur > 1);
    }
  };

  useKeyOwner((e) => {
    if (disabled) return false;
    const target = e.target as HTMLElement;
    if (target && ["INPUT", "TEXTAREA", "SELECT"].includes(target.tagName)) return false;
    const k = e.key;
    const mod = e.ctrlKey || e.metaKey;
    if (mod && k.toLowerCase() === "z") {
      undo();
      return true;
    }
    if (mod || e.altKey) return false;
    if (k in DURATION_KEYS) {
      chooseDuration(DURATION_KEYS[k]);
      return true;
    }
    if (k === ".") {
      toggleDot();
      return true;
    }
    if (k === "0") {
      if (editable(selected)) setPitch(selected!, null);
      else append(null);
      return true;
    }
    if (/^[a-gA-G]$/.test(k)) {
      if (editable(selected)) setPitch(selected!, letterPitch(k, entries[selected!].pitch ?? lastPitch(selected!), fifths, given.clef));
      else append(letterPitch(k, lastPitch(), fifths, given.clef));
      return true;
    }
    if (k === "ArrowUp" || k === "ArrowDown") {
      const dir = k === "ArrowUp" ? 1 : -1;
      adjust((p) => (e.shiftKey ? octaveShift(p, dir) : semitone(p, dir, fifths)));
      return true;
    }
    if (k === "+" || k === "-" || k === "=") {
      adjust((p) => ({ ...p, alter: k === "=" ? 0 : Math.max(-2, Math.min(2, p.alter + (k === "+" ? 1 : -1))) }));
      return true;
    }
    if (k === "ArrowLeft") {
      const from = selected ?? entries.length;
      for (let i = from - 1; i >= 0; i--) {
        if (!entries[i].locked) {
          setSelected(i);
          break;
        }
      }
      return true;
    }
    if (k === "ArrowRight") {
      if (selected == null) return true;
      const next = entries.findIndex((en, i) => i > selected && !en.locked);
      setSelected(next >= 0 ? next : null);
      return true;
    }
    if (k === "Backspace" || k === "Delete") {
      if (editable(selected)) remove(selected!);
      else if (k === "Backspace" && entries.length > 0 && !entries[entries.length - 1].locked) remove(entries.length - 1);
      return true;
    }
    if (k === "Escape" && selected != null) {
      setSelected(null);
      return true;
    }
    return false;
  }, !disabled);

  const display: Score = useMemo(() => ({ ...given, notes: toScoreNotes(entries) }), [given, entries]);
  const colors = useMemo(() => entries.map((e) => (e.locked ? "#8c959f" : null)), [entries]);

  // Mouse: click on a written note to select it (or move it to the clicked line); click after the
  // last note to add one there.
  const notes = display.notes;
  const indexAt = (p: StaffPointer): number | null => {
    for (let i = 0; i < notes.length; i++) {
      const n = notes[i];
      if (p.tick >= n.start - 0.6 && p.tick < n.start + Math.max(0.9, Math.min(n.dur, 2))) return i;
    }
    return null;
  };
  const atEnd = (p: StaffPointer) => p.tick >= filled - 0.4;
  const onDown = (p: StaffPointer) => {
    if (disabled) return;
    const i = indexAt(p);
    const pitch = pitchAt(p.diatonic, fifths);
    if (i != null && !atEnd(p)) {
      if (!editable(i)) return;
      const e = entries[i];
      if (selected === i && e.pitch && p.diatonic !== e.pitch.octave * 7 + e.pitch.step) setPitch(i, pitch);
      else setSelected(i);
      return;
    }
    if (atEnd(p)) append(pitch);
    else setSelected(null);
  };
  const ghost =
    hover && !disabled && filled < total && atEnd(hover)
      ? { tick: filled, diatonic: hover.diatonic }
      : hover && selected != null && indexAt(hover) === selected
        ? { tick: notes[selected].start, diatonic: hover.diatonic }
        : null;

  const sel = editable(selected) ? entries[selected!] : null;
  const status =
    filled >= total
      ? "All bars are full."
      : `${describePosition(given, filled)} · ${DURATION_NAMES[current] ?? `${current} ticks`}${sel ? "" : " next"}`;

  return (
    <div className="editor">
      <div className="editor-tools" role="toolbar" aria-label="Note input">
        {[16, 8, 4, 2, 1].map((d) => (
          <button
            key={d}
            className={`tool ${(sel ? sel.dur === d || sel.dur === d * 1.5 : dur === d) ? "on" : ""}`}
            title={`${DURATION_NAMES[d]} (${Object.entries(DURATION_KEYS).find(([, v]) => v === d)?.[0]})`}
            onClick={() => chooseDuration(d)}
            disabled={disabled}
          >
            <DurIcon ticks={d} />
            <span className="hint">{Object.entries(DURATION_KEYS).find(([, v]) => v === d)?.[0]}</span>
          </button>
        ))}
        <button className={`tool ${(sel ? sel.dur % 3 === 0 : dotted) ? "on" : ""}`} title="Dot (.)" onClick={toggleDot} disabled={disabled}>
          <span className="glyph">•</span>
          <span className="hint">.</span>
        </button>
        <button className={`tool ${sel && !sel.pitch ? "on" : ""}`} title="Rest (0)" onClick={() => (editable(selected) ? setPitch(selected!, null) : append(null))} disabled={disabled}>
          <span className="glyph small">Rest</span>
          <span className="hint">0</span>
        </button>
        <span className="tool-sep" />
        <button className="tool" title="Sharp (+)" onClick={() => adjust((p) => ({ ...p, alter: Math.min(2, p.alter + 1) }))} disabled={disabled}>
          <span className="glyph">♯</span>
          <span className="hint">+</span>
        </button>
        <button className="tool" title="Flat (−)" onClick={() => adjust((p) => ({ ...p, alter: Math.max(-2, p.alter - 1) }))} disabled={disabled}>
          <span className="glyph">♭</span>
          <span className="hint">−</span>
        </button>
        <button className="tool" title="Natural (=)" onClick={() => adjust((p) => ({ ...p, alter: 0 }))} disabled={disabled}>
          <span className="glyph">♮</span>
          <span className="hint">=</span>
        </button>
        <span className="tool-sep" />
        <button className="tool wide" title="Undo (Ctrl+Z)" onClick={undo} disabled={disabled || history.length === 0}>
          Undo
        </button>
        <button
          className="tool wide"
          title="Delete (Backspace)"
          onClick={() => {
            if (editable(selected)) remove(selected!);
            else if (entries.length > 0 && !entries[entries.length - 1].locked) remove(entries.length - 1);
          }}
          disabled={disabled || entries.every((e) => e.locked)}
        >
          Delete
        </button>
        <span className="grow" />
        <span className="small muted num">{sel?.pitch ? `${pitchDisplay(sel.pitch)} selected` : status}</span>
      </div>
      <Staff score={display} noteColors={colors} fillerColor="#c4c9cf" selected={editable(selected) ? selected : null} caretTick={selected == null && filled < total ? filled : null} ghost={ghost} onPointerMove={setHover} onPointerDown={onDown} maxHeight={360} />
      <div className="small faint">
        Type A–G for notes in the key, 2–6 for lengths (4 is a quarter), “.” to dot, 0 for a rest, arrows to move a note (Shift for an octave), +/−/= for accidentals.
        Click after the last note to add one, or on a note to select it.
      </div>
    </div>
  );
}

/** Small note-value icons for the toolbar. */
function DurIcon({ ticks }: { ticks: number }) {
  const hollow = ticks >= 8;
  const stem = ticks < 16;
  const flags = ticks === 2 ? 1 : ticks === 1 ? 2 : 0;
  return (
    <svg width="18" height="24" viewBox="0 0 18 24" aria-hidden>
      <ellipse cx="7" cy="18" rx="5" ry="3.6" transform="rotate(-20 7 18)" fill={hollow ? "none" : "currentColor"} stroke="currentColor" strokeWidth="1.5" />
      {stem && <line x1="11.4" y1="17" x2="11.4" y2="3" stroke="currentColor" strokeWidth="1.4" />}
      {Array.from({ length: flags }, (_, i) => (
        <path key={i} d={`M11.4 ${3 + i * 4.5} q4 3 3 8`} fill="none" stroke="currentColor" strokeWidth="1.4" />
      ))}
    </svg>
  );
}
