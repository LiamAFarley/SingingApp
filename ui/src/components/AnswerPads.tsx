import { useEffect } from "react";
import type { Choice } from "../bindings/Choice";
import { pcIdDisplay } from "../music";

/** Computer-keyboard keys for a piano octave: white keys on the home row, black keys above. */
const KEY_FOR_PC: Record<string, string> = {
  C: "a",
  "C#": "w",
  D: "s",
  "D#": "e",
  E: "d",
  F: "f",
  "F#": "t",
  G: "g",
  "G#": "y",
  A: "h",
  "A#": "u",
  B: "j",
  other: "o",
};

const WHITE = ["C", "D", "E", "F", "G", "A", "B"];
const BLACK: [string, number][] = [
  ["C#", 2],
  ["D#", 4],
  ["F#", 8],
  ["G#", 10],
  ["A#", 12],
];

export type KeyState = "idle" | "picked" | "correct" | "wrong";

interface PadProps {
  choices: Choice[];
  enabled: boolean;
  states: Record<string, KeyState>;
  onPick: (id: string) => void;
  /** Suffix for note-with-octave pads, e.g. "4" makes ids like "C#4". */
  octave?: number;
  compact?: boolean;
  keyboard?: boolean;
}

export function PianoPad({ choices, enabled, states, onPick, octave, compact, keyboard = true }: PadProps) {
  const idFor = (pc: string) => (octave == null ? pc : `${pc}${octave}`);
  const byId = new Map(choices.map((c) => [c.id, c]));
  const other = byId.get("other");

  useEffect(() => {
    if (!keyboard || !enabled) return;
    const onKey = (e: KeyboardEvent) => {
      if (e.repeat || e.ctrlKey || e.metaKey || e.altKey) return;
      const k = e.key.toLowerCase();
      for (const [pc, key] of Object.entries(KEY_FOR_PC)) {
        if (key === k) {
          const id = pc === "other" ? "other" : idFor(pc);
          if (byId.has(id)) {
            e.preventDefault();
            onPick(id);
          }
          return;
        }
      }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  });

  const size = compact ? 30 : 37;
  const key = (pc: string, row: number, col: number, sharp: boolean) => {
    const id = idFor(pc);
    const c = byId.get(id);
    const st = states[id] ?? "idle";
    return (
      <button
        key={pc}
        className={`key ${sharp ? "sharp" : ""} ${c ? "" : "inactive"} ${st !== "idle" ? st : ""}`}
        style={{ gridRow: row, gridColumn: `${col} / span 2`, width: "auto", height: compact ? 44 : 64, fontSize: compact ? 14 : 17 }}
        disabled={!enabled || !c}
        onClick={() => c && onPick(id)}
        tabIndex={c ? 0 : -1}
      >
        <span>{c?.label ?? pcIdDisplay(pc)}</span>
        {keyboard && !compact && c && <span className="hint">{KEY_FOR_PC[pc].toUpperCase()}</span>}
      </button>
    );
  };

  return (
    <div className="pad">
      <div style={{ display: "grid", gridTemplateColumns: `repeat(14, ${size}px)`, gap: 6, rowGap: 6 }}>
        {BLACK.map(([pc, col]) => key(pc, 1, col, true))}
        {WHITE.map((pc, i) => key(pc, 2, i * 2 + 1, false))}
      </div>
      {other && (
        <button
          className={`key wide ${states.other && states.other !== "idle" ? states.other : ""}`}
          disabled={!enabled}
          onClick={() => onPick("other")}
        >
          <span>Other</span>
          {keyboard && <span className="hint">O</span>}
        </button>
      )}
    </div>
  );
}

/** Note-with-octave answers: one compact piano octave per row. */
export function NotesPad({ choices, enabled, states, onPick }: Omit<PadProps, "octave" | "compact">) {
  const octaves = Array.from(new Set(choices.map((c) => parseInt(c.id.replace(/^[A-G]#?/, ""), 10)))).sort((a, b) => b - a);
  return (
    <div className="pad" style={{ gap: 14 }}>
      {octaves.map((o) => (
        <div key={o} className="row" style={{ gap: 12 }}>
          <span className="faint num" style={{ width: 52, textAlign: "right" }}>
            Octave {o}
          </span>
          <PianoPad choices={choices} enabled={enabled} states={states} onPick={onPick} octave={o} compact keyboard={false} />
        </div>
      ))}
    </div>
  );
}

export function ChoiceGrid({ choices, enabled, states, onPick }: Omit<PadProps, "octave" | "compact">) {
  useEffect(() => {
    if (!enabled) return;
    const onKey = (e: KeyboardEvent) => {
      if (e.repeat || e.ctrlKey || e.metaKey || e.altKey) return;
      const n = e.key === "0" ? 10 : parseInt(e.key, 10);
      if (!Number.isNaN(n) && n >= 1 && n <= choices.length && n <= 10) {
        e.preventDefault();
        onPick(choices[n - 1].id);
      }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  });
  return (
    <div className="choices">
      {choices.map((c, i) => {
        const st = states[c.id] ?? "idle";
        return (
          <button key={c.id} className={`choice ${st === "correct" || st === "wrong" ? st : ""}`} disabled={!enabled} onClick={() => onPick(c.id)}>
            <span>{c.label}</span>
            {i < 10 && <span className="kbd">{i === 9 ? 0 : i + 1}</span>}
          </button>
        );
      })}
    </div>
  );
}

/** The number a degree id carries ("b3" → "3"). */
const degreeDigit = (id: string) => id.replace(/[^0-9]/g, "");

/** Scale degrees in a row, in pitch order. Number keys pick the degree with that number (the plain one first). */
export function DegreePad({ choices, enabled, states, onPick }: Omit<PadProps, "octave" | "compact">) {
  const forDigit = (d: string) => choices.find((c) => c.id === d) ?? choices.find((c) => degreeDigit(c.id) === d);
  useEffect(() => {
    if (!enabled) return;
    const onKey = (e: KeyboardEvent) => {
      if (e.repeat || e.ctrlKey || e.metaKey || e.altKey || !/^[1-7]$/.test(e.key)) return;
      const c = forDigit(e.key);
      if (c) {
        e.preventDefault();
        onPick(c.id);
      }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  });
  return (
    <div className="degrees">
      {choices.map((c) => {
        const st = states[c.id] ?? "idle";
        const digit = forDigit(degreeDigit(c.id))?.id === c.id ? degreeDigit(c.id) : "";
        // Number labels already show the key.
        const hint = digit && digit !== c.label ? digit : "";
        return (
          <button key={c.id} className={`key degree ${st !== "idle" ? st : ""}`} disabled={!enabled} onClick={() => onPick(c.id)}>
            <span>{c.label}</span>
            {hint && <span className="hint">{hint}</span>}
          </button>
        );
      })}
    </div>
  );
}
