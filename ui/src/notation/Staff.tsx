// Staff notation for a monophonic score (VexFlow), with an overlay for the sung pitch trace,
// per-note result markers, a cursor and editor selection.

import { useEffect, useLayoutEffect, useMemo, useRef, useState, type PointerEvent as ReactPointerEvent } from "react";
import { Accidental, BarlineType, Beam, Dot, Formatter, Fraction, Renderer, Stave, StaveNote, StaveTie, Voice } from "vexflow/bravura";
import type { NoteResult } from "../bindings/NoteResult";
import type { Score } from "../bindings/Score";
import {
  barTicks,
  beatTicks,
  diatonicForLine,
  keySpec,
  layoutTokens,
  lineFor,
  makePitchToDiatonic,
  parsePitch,
  parseTime,
  pitchDisplay,
  pitchMidi,
  restKey,
  vexClef,
  vexKey,
  type Token,
} from "./layout";

const fontsReady: Promise<void> =
  typeof document !== "undefined" && document.fonts
    ? Promise.all([document.fonts.load("30px Bravura"), document.fonts.load("12px Academico")])
        .then(() => undefined)
        .catch(() => undefined)
    : Promise.resolve();

const SPACING = 10;
const SYSTEM_H = 132;
const TOP = 6;
const MARGIN = 10;
/** Scales tried from large to small; the first whose height fits `maxHeight` is used. */
const SCALES = [1.75, 1.55, 1.35, 1.2];

export interface SystemGeom {
  startTick: number;
  endTick: number;
  /** y of the top staff line (unscaled). */
  topLineY: number;
  x0: number;
  x1: number;
  /** (tick, x) anchors, ascending. */
  anchors: [number, number][];
  /** Bar start x positions inside the stave (for insertion carets). */
  barStarts: { tick: number; x: number }[];
}

export interface TokenGeom {
  token: Token;
  system: number;
  x: number;
  y: number;
}

export interface StaffGeometry {
  scale: number;
  width: number;
  height: number;
  systems: SystemGeom[];
  tokens: TokenGeom[];
}

export interface StaffPointer {
  system: number;
  tick: number;
  /** Staff position under the pointer (diatonic, C4 = 28). */
  diatonic: number;
  /** Index into the flat token list of the token under the pointer, if any. */
  tokenIndex: number | null;
}

interface Props {
  score: Score;
  /** Fixed scale; by default the largest that fits `maxHeight`. */
  scale?: number;
  /** CSS pixels the staff may take vertically when choosing a scale. */
  maxHeight?: number;
  /** Graded or live pitch trace: (seconds on the trial timeline, MIDI or null). */
  trace?: [number, number | null][];
  /** Tint the trace by how close it is to the written note it is sung against. */
  tint?: boolean;
  /** Draw the trace in the octave of the written note (for exercises where the octave does not count). */
  foldOctaves?: boolean;
  /** Maps trial-timeline seconds to score ticks. */
  timing?: { singStartS: number; beatS: number };
  /** One result per sounding note, in order. */
  results?: NoteResult[] | null;
  /** Cursor position in ticks. */
  cursorTick?: number | null;
  /** Colour per score.notes index (noteheads). */
  noteColors?: (string | null | undefined)[];
  /** Colour of rests that only fill unwritten space (the editor's placeholders). */
  fillerColor?: string;
  /** Highlight for the editor: a score.notes index. */
  selected?: number | null;
  /** Editor caret in ticks (drawn as a thin line). */
  caretTick?: number | null;
  /** Ghost note at a staff position (editor hover preview). */
  ghost?: { tick: number; diatonic: number } | null;
  onPointerMove?: (p: StaffPointer | null) => void;
  onPointerDown?: (p: StaffPointer) => void;
}

export function Staff({ score, scale, maxHeight = 460, fillerColor, trace, tint = false, foldOctaves = false, timing, results, cursorTick, noteColors, selected, caretTick, ghost, onPointerMove, onPointerDown }: Props) {
  const wrap = useRef<HTMLDivElement>(null);
  const host = useRef<HTMLDivElement>(null);
  const [width, setWidth] = useState(0);
  const [fontOk, setFontOk] = useState(false);
  const [geom, setGeom] = useState<StaffGeometry | null>(null);

  useEffect(() => {
    let alive = true;
    fontsReady.then(() => alive && setFontOk(true));
    return () => {
      alive = false;
    };
  }, []);

  useEffect(() => {
    const el = wrap.current;
    if (!el) return;
    const ro = new ResizeObserver(() => setWidth(el.clientWidth));
    ro.observe(el);
    setWidth(el.clientWidth);
    return () => ro.disconnect();
  }, []);

  const colorsKey = (noteColors ?? []).join("|");
  useLayoutEffect(() => {
    const el = host.current;
    if (!el || !fontOk || width < 100) return;
    el.innerHTML = "";
    try {
      setGeom(render(el, score, width, scale ? [scale] : SCALES, maxHeight, noteColors ?? [], selected ?? null, fillerColor ?? null));
    } catch (e) {
      el.textContent = `Could not draw the score: ${(e as Error).message}`;
      setGeom(null);
    }
    // noteColors is compared through colorsKey.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [score, width, scale, maxHeight, fontOk, colorsKey, selected, fillerColor]);

  const toDiatonic = useMemo(() => makePitchToDiatonic(score), [score]);
  const bt = beatTicks(score);

  const pointer = (e: ReactPointerEvent<SVGSVGElement>): StaffPointer | null => {
    if (!geom) return null;
    const rect = (e.currentTarget as SVGSVGElement).getBoundingClientRect();
    const x = (e.clientX - rect.left) / geom.scale;
    const y = (e.clientY - rect.top) / geom.scale;
    const system = Math.max(0, Math.min(geom.systems.length - 1, Math.floor((y - TOP) / SYSTEM_H)));
    const s = geom.systems[system];
    const line = 5 - (y - s.topLineY) / SPACING;
    const d = Math.round(diatonicForLine(Math.round(line * 2) / 2, score.clef));
    const tick = xToTick(s, x);
    let tokenIndex: number | null = null;
    let best = 14;
    geom.tokens.forEach((t, i) => {
      if (t.system !== system) return;
      const dist = Math.abs(t.x - x);
      if (dist < best) {
        best = dist;
        tokenIndex = i;
      }
    });
    return { system, tick, diatonic: d, tokenIndex };
  };

  return (
    <div className="staff-wrap" ref={wrap}>
      <div ref={host} className="staff-host" />
      {geom && (
        <svg
          className="staff-overlay"
          viewBox={`0 0 ${geom.width} ${geom.height}`}
          style={{ width: geom.width * geom.scale, height: geom.height * geom.scale }}
          onPointerMove={onPointerMove ? (e) => onPointerMove(pointer(e)) : undefined}
          onPointerLeave={onPointerMove ? () => onPointerMove(null) : undefined}
          onPointerDown={
            onPointerDown
              ? (e) => {
                  const p = pointer(e);
                  if (p) onPointerDown(p);
                }
              : undefined
          }
        >
          {selected != null && <Selection geom={geom} index={selected} />}
          {caretTick != null && <Caret geom={geom} tick={caretTick} />}
          {ghost && <Ghost geom={geom} ghost={ghost} clef={score.clef} />}
          {trace && timing && (
            <Trace geom={geom} trace={trace} timing={timing} beatTicks={bt} toDiatonic={toDiatonic} score={score} tint={tint} fold={foldOctaves} />
          )}
          {results && <Markers geom={geom} results={results} />}
          {cursorTick != null && <Cursor geom={geom} tick={cursorTick} />}
        </svg>
      )}
    </div>
  );
}

// ---------------------------------------------------------------------------------------------
// VexFlow rendering
// ---------------------------------------------------------------------------------------------

function tokenWidth(t: Token): number {
  let w = t.kind === "rest" ? 22 : 26;
  if (t.accidental) w += 12;
  if (t.dots) w += 6;
  if (t.code === "16") w += 2;
  if (t.code === "w" || t.code === "h") w += 8;
  return w;
}

/** Splits bars into the fewest lines that fit, balancing line widths (linear partition). */
function breakLines(minBar: number[], usable: number, lineExtra: number, timeWidth: number): number[][] {
  const n = minBar.length;
  const prefix = [0];
  minBar.forEach((w) => prefix.push(prefix[prefix.length - 1] + w));
  const lineWidth = (j: number, i: number) => prefix[i] - prefix[j] + lineExtra + (j === 0 ? timeWidth : 0);
  for (let lines = 1; lines <= n; lines++) {
    const best: number[][] = Array.from({ length: lines + 1 }, () => new Array(n + 1).fill(Infinity));
    const cut: number[][] = Array.from({ length: lines + 1 }, () => new Array(n + 1).fill(0));
    best[0][0] = 0;
    for (let k = 1; k <= lines; k++) {
      for (let i = 1; i <= n; i++) {
        for (let j = k - 1; j < i; j++) {
          const v = Math.max(best[k - 1][j], lineWidth(j, i));
          if (v < best[k][i]) {
            best[k][i] = v;
            cut[k][i] = j;
          }
        }
      }
    }
    if (best[lines][n] <= usable || lines === n) {
      const out: number[][] = [];
      let i = n;
      for (let k = lines; k >= 1; k--) {
        const j = cut[k][i];
        out.unshift(Array.from({ length: i - j }, (_, x) => j + x));
        i = j;
      }
      return out;
    }
  }
  return [minBar.map((_, i) => i)];
}

function render(
  el: HTMLDivElement,
  score: Score,
  cssWidth: number,
  scales: number[],
  maxHeight: number,
  colors: (string | null | undefined)[],
  selected: number | null,
  fillerColor: string | null,
): StaffGeometry {
  const bars = layoutTokens(score);
  const time = parseTime(score.time);
  const barLen = barTicks(score);
  const keyWidth = 12 + Math.abs(score.key.fifths) * 10;
  const clefWidth = 34;
  const timeWidth = 26;
  const minBar = bars.map((tokens) => Math.max(110, 24 + tokens.reduce((s, t) => s + tokenWidth(t), 0)));

  // The largest scale whose layout fits the height budget.
  let scale = scales[scales.length - 1];
  let systems: number[][] = [];
  for (const sc of scales) {
    const usableAt = cssWidth / sc - 2 * MARGIN;
    const lines = breakLines(minBar, usableAt, clefWidth + keyWidth, timeWidth);
    scale = sc;
    systems = lines;
    if ((TOP + lines.length * SYSTEM_H) * sc <= maxHeight) break;
  }
  const width = cssWidth / scale;
  const usable = width - 2 * MARGIN;
  const height = TOP + systems.length * SYSTEM_H;
  const renderer = new Renderer(el, Renderer.Backends.SVG);
  renderer.resize(cssWidth, height * scale);
  const ctx = renderer.getContext();
  ctx.scale(scale, scale);

  const beamGroup = time.unit === 8 && time.beats % 3 === 0 ? new Fraction(3, 8) : new Fraction(1, 4);
  const geomSystems: SystemGeom[] = [];
  const geomTokens: TokenGeom[] = [];
  const staveNotes: StaveNote[] = [];
  const flatTokens: Token[] = [];
  let stretchCap = Infinity;

  systems.forEach((barsInLine, si) => {
    const y = TOP + si * SYSTEM_H;
    const extras = barsInLine.map((b, k) => (k === 0 ? clefWidth + keyWidth : 0) + (b === 0 ? timeWidth : 0));
    const mins = barsInLine.map((b) => minBar[b]);
    // Justify; the last line is not stretched more than the one before it.
    let stretch = (usable - extras.reduce((a, b) => a + b, 0)) / mins.reduce((a, b) => a + b, 0);
    if (si === systems.length - 1) stretch = Math.min(stretch, stretchCap, 1.8);
    stretchCap = stretch;
    let x = MARGIN;
    const sys: SystemGeom = {
      startTick: barsInLine[0] * barLen,
      endTick: (barsInLine[barsInLine.length - 1] + 1) * barLen,
      topLineY: 0,
      x0: MARGIN,
      x1: MARGIN,
      anchors: [],
      barStarts: [],
    };
    barsInLine.forEach((b, k) => {
      const w = mins[k] * stretch + extras[k];
      const stave = new Stave(x, y, w);
      if (k === 0) {
        if (score.clef === "treble8vb") stave.addClef("treble", "default", "8vb");
        else stave.addClef(vexClef(score.clef));
        stave.addKeySignature(keySpec(score));
      }
      if (b === 0) stave.addTimeSignature(score.time);
      if (b === bars.length - 1) stave.setEndBarType(BarlineType.END);
      stave.setContext(ctx).draw();
      sys.topLineY = stave.getYForLine(0);
      sys.barStarts.push({ tick: b * barLen, x: stave.getNoteStartX() });

      const tokens = bars[b];
      const notes = tokens.map((t) => {
        const color = t.scoreIndex >= 0 ? colors[t.scoreIndex] : t.kind === "rest" ? fillerColor : null;
        const sn =
          t.kind === "note" && t.pitch
            ? new StaveNote({ keys: [vexKey(t.pitch, score.clef)], duration: t.code, dots: t.dots, clef: vexClef(score.clef), autoStem: true })
            : new StaveNote({ keys: [restKey(score.clef)], duration: `${t.code}r`, dots: t.dots, clef: vexClef(score.clef), alignCenter: t.fullBarRest });
        if (t.accidental) sn.addModifier(new Accidental(t.accidental), 0);
        if (t.dots) Dot.buildAndAttach([sn], { all: true });
        if (color) sn.setStyle({ fillStyle: color, strokeStyle: color });
        else if (selected != null && t.scoreIndex === selected) sn.setStyle({ fillStyle: "#2d6cdf", strokeStyle: "#2d6cdf" });
        return sn;
      });
      const voice = new Voice({ numBeats: time.beats, beatValue: time.unit }).setMode(Voice.Mode.SOFT);
      voice.addTickables(notes);
      // Rests are passed too (they break beams) so beat groups line up with the bar.
      const beams = Beam.generateBeams(notes, { groups: [beamGroup], beamRests: false });
      new Formatter().joinVoices([voice]).formatToStave([voice], stave);
      voice.draw(ctx, stave);
      beams.forEach((bm) => bm.setContext(ctx).draw());
      notes.forEach((sn, i) => {
        const t = tokens[i];
        const xx = (sn.getNoteHeadBeginX() + sn.getNoteHeadEndX()) / 2;
        const yy = sn.getYs()[0];
        geomTokens.push({ token: t, system: si, x: xx, y: yy });
        sys.anchors.push([t.tick, xx]);
        staveNotes.push(sn);
        flatTokens.push(t);
      });
      x += w;
      sys.x1 = x;
    });
    sys.anchors.push([sys.endTick, sys.x1 - 8]);
    geomSystems.push(sys);
  });

  // Ties, split into two halves where a line breaks.
  flatTokens.forEach((t, i) => {
    if (!t.tieToNext || i + 1 >= flatTokens.length) return;
    const a = staveNotes[i];
    const b = staveNotes[i + 1];
    const sameLine = geomTokens[i].system === geomTokens[i + 1].system;
    if (sameLine) {
      new StaveTie({ firstNote: a, lastNote: b, firstIndexes: [0], lastIndexes: [0] }).setContext(ctx).draw();
    } else {
      new StaveTie({ firstNote: a, lastNote: null, firstIndexes: [0], lastIndexes: [0] }).setContext(ctx).draw();
      new StaveTie({ firstNote: null, lastNote: b, firstIndexes: [0], lastIndexes: [0] }).setContext(ctx).draw();
    }
  });

  return { scale, width, height, systems: geomSystems, tokens: geomTokens };
}

// ---------------------------------------------------------------------------------------------
// Overlay
// ---------------------------------------------------------------------------------------------

export function tickToPoint(geom: StaffGeometry, tick: number): { system: number; x: number } | null {
  const si = geom.systems.findIndex((s) => tick >= s.startTick && tick < s.endTick);
  const idx = si >= 0 ? si : tick >= (geom.systems.at(-1)?.endTick ?? 0) ? geom.systems.length - 1 : 0;
  const s = geom.systems[idx];
  if (!s) return null;
  return { system: idx, x: tickToX(s, tick) };
}

function tickToX(s: SystemGeom, tick: number): number {
  const a = s.anchors;
  if (a.length === 0) return s.x0;
  if (tick <= a[0][0]) return a[0][1] - Math.min(12, (a[0][0] - tick) * 3);
  for (let i = 1; i < a.length; i++) {
    if (tick <= a[i][0]) {
      const [t0, x0] = a[i - 1];
      const [t1, x1] = a[i];
      return t1 === t0 ? x1 : x0 + ((tick - t0) / (t1 - t0)) * (x1 - x0);
    }
  }
  return a[a.length - 1][1];
}

function xToTick(s: SystemGeom, x: number): number {
  const a = s.anchors;
  if (a.length === 0) return s.startTick;
  if (x <= a[0][1]) return a[0][0];
  for (let i = 1; i < a.length; i++) {
    if (x <= a[i][1]) {
      const [t0, x0] = a[i - 1];
      const [t1, x1] = a[i];
      return x1 === x0 ? t1 : t0 + ((x - x0) / (x1 - x0)) * (t1 - t0);
    }
  }
  return a[a.length - 1][0];
}

const lineY = (s: SystemGeom, line: number) => s.topLineY + (5 - line) * SPACING;

/** How close a sung pitch is to the written note, in cents: in tune, near, or off. */
function tint(cents: number): "good" | "near" | "off" {
  const c = Math.abs(cents);
  return c <= 25 ? "good" : c <= 50 ? "near" : "off";
}

function Trace({
  geom,
  trace,
  timing,
  beatTicks: bt,
  toDiatonic,
  score,
  tint: tinted,
  fold,
}: {
  geom: StaffGeometry;
  trace: [number, number | null][];
  timing: { singStartS: number; beatS: number };
  beatTicks: number;
  toDiatonic: (m: number) => number;
  score: Score;
  tint: boolean;
  fold: boolean;
}) {
  const total = geom.systems.at(-1)?.endTick ?? 0;
  // The written notes as (start tick, end tick, MIDI), to compare the trace with.
  const written = useMemo(
    () =>
      score.notes.flatMap((n) => {
        const p = n.pitch ? parsePitch(n.pitch) : null;
        return p ? [{ start: n.start, end: n.start + n.dur, midi: pitchMidi(p) }] : [];
      }),
    [score],
  );
  // Placed points; a gap in time or a new system starts a new segment.
  const pts: { x: number; y: number; system: number; seg: number; t: number; cents: number | null; note: number }[] = [];
  let seg = 0;
  let lastT = -Infinity;
  for (const [t, m] of trace) {
    const tick = ((t - timing.singStartS) / timing.beatS) * bt;
    if (m == null || tick < -bt * 0.5 || tick > total + bt * 0.25 || t - lastT > 0.08) {
      seg++;
      if (m == null) {
        lastT = -Infinity;
        continue;
      }
    }
    const p = tickToPoint(geom, Math.max(0, Math.min(total - 0.01, tick)));
    if (!p) continue;
    const ti = written.findIndex((n) => tick >= n.start && tick < n.end);
    const target = ti >= 0 ? written[ti] : null;
    // Sung against a written note: in its octave when the octave does not count.
    const shown = target && fold ? m + 12 * Math.round((target.midi - m) / 12) : m;
    if (pts.length && pts[pts.length - 1].system !== p.system) seg++;
    const y = lineY(geom.systems[p.system], lineFor(toDiatonic(shown), score.clef));
    pts.push({ x: p.x, y, system: p.system, seg, t, cents: target ? (shown - target.midi) * 100 : null, note: ti });
    lastT = t;
  }
  // Tint by the note's centre rather than each vibrato swing: the mean over about 0.2 s of the
  // same note and segment.
  const HALF = 0.1;
  const tintOf = pts.map((p, i) => {
    if (!tinted) return "";
    if (p.cents == null) return "rest";
    let sum = 0;
    let n = 0;
    const add = (j: number) => {
      if (pts[j].seg === p.seg && pts[j].note === p.note) {
        sum += pts[j].cents!;
        n++;
      }
    };
    for (let j = i; j >= 0 && p.t - pts[j].t <= HALF; j--) add(j);
    for (let j = i + 1; j < pts.length && pts[j].t - p.t <= HALF; j++) add(j);
    return tint(sum / n);
  });
  // Runs of one tint; each starts where the previous ended so the line stays continuous.
  const runs: { cls: string; d: string }[] = [];
  let run: { cls: string; d: string } | null = null;
  for (let i = 0; i < pts.length; i++) {
    const p = pts[i];
    const prev = i > 0 && pts[i - 1].seg === p.seg ? pts[i - 1] : null;
    if (!prev) run = null;
    if (!run || run.cls !== tintOf[i]) {
      run = { cls: tintOf[i], d: prev ? `M${prev.x.toFixed(1)} ${prev.y.toFixed(1)} ` : "" };
      runs.push(run);
    }
    run.d += `${run.d ? "L" : "M"}${p.x.toFixed(1)} ${p.y.toFixed(1)} `;
  }
  return (
    <g className="staff-trace">
      {runs.map((r, i) => (
        <path key={i} d={r.d} className={r.cls || undefined} />
      ))}
    </g>
  );
}

function Markers({ geom, results }: { geom: StaffGeometry; results: NoteResult[] }) {
  const firsts = geom.tokens.filter((t) => t.token.kind === "note" && !t.token.tiedFromPrev);
  return (
    <g className="staff-markers">
      {firsts.map((t, i) => {
        const r = results[i];
        if (!r) return null;
        const s = geom.systems[t.system];
        const y = Math.max(lineY(s, 1) + 24, t.y + 20);
        const kind = !r.matched ? "missed" : !r.correct ? "wrong" : r.in_tune ? "correct" : "marginal";
        const cents = r.cents_error != null ? Math.round(r.cents_error) : null;
        const label = `${t.token.pitch ? pitchDisplay(t.token.pitch) : ""}: ${
          kind === "missed"
            ? "missed"
            : kind === "wrong"
              ? "wrong note"
              : cents == null
                ? "ok"
                : cents === 0
                  ? "in tune"
                  : `${Math.abs(cents)} cents ${cents > 0 ? "sharp" : "flat"}`
        }${r.onset_error_ms != null && Math.abs(r.onset_error_ms) > 80 ? `, ${Math.round(Math.abs(r.onset_error_ms))} ms ${r.onset_error_ms > 0 ? "late" : "early"}` : ""}`;
        return (
          <g key={i} transform={`translate(${t.x.toFixed(1)} ${y.toFixed(1)})`} className={`marker ${kind}`}>
            <title>{label}</title>
            {kind === "correct" && <path d="M-5 0 L-1.5 3.8 L5.5 -4.5" />}
            {kind === "wrong" && <path d="M-4.5 -4.5 L4.5 4.5 M4.5 -4.5 L-4.5 4.5" />}
            {kind === "marginal" && (cents ?? 0) > 0 && <path d="M0 -5 L0 5 M-4 -1 L0 -5 L4 -1" />}
            {kind === "marginal" && (cents ?? 0) <= 0 && <path d="M0 5 L0 -5 M-4 1 L0 5 L4 1" />}
            {kind === "missed" && <circle r={4} />}
          </g>
        );
      })}
    </g>
  );
}

function Cursor({ geom, tick }: { geom: StaffGeometry; tick: number }) {
  const p = tickToPoint(geom, tick);
  if (!p) return null;
  const s = geom.systems[p.system];
  return <line className="staff-cursor" x1={p.x} x2={p.x} y1={lineY(s, 5) - 14} y2={lineY(s, 1) + 14} />;
}

function Caret({ geom, tick }: { geom: StaffGeometry; tick: number }) {
  const total = geom.systems.at(-1)?.endTick ?? 0;
  let p = tickToPoint(geom, Math.min(tick, total - 0.001));
  if (!p) return null;
  // At the very end, sit just after the last note.
  if (tick >= total) p = { system: geom.systems.length - 1, x: (geom.systems.at(-1)?.x1 ?? 0) - 16 };
  else {
    const tok = geom.tokens.find((t) => t.token.tick === tick);
    if (tok) p = { system: tok.system, x: tok.x - 12 };
  }
  const s = geom.systems[p.system];
  return <line className="staff-caret" x1={p.x} x2={p.x} y1={lineY(s, 5) - 10} y2={lineY(s, 1) + 10} />;
}

function Selection({ geom, index }: { geom: StaffGeometry; index: number }) {
  const toks = geom.tokens.filter((t) => t.token.scoreIndex === index);
  return (
    <g className="staff-selection">
      {toks.map((t, i) => {
        const s = geom.systems[t.system];
        return <rect key={i} x={t.x - 11} y={lineY(s, 5) - 16} width={22} height={SPACING * 4 + 32} rx={4} />;
      })}
    </g>
  );
}

function Ghost({ geom, ghost, clef }: { geom: StaffGeometry; ghost: { tick: number; diatonic: number }; clef: Score["clef"] }) {
  const p = tickToPoint(geom, ghost.tick);
  if (!p) return null;
  const s = geom.systems[p.system];
  const line = lineFor(ghost.diatonic, clef);
  const y = lineY(s, line);
  const ledgers: number[] = [];
  for (let l = 0; l >= line; l -= 1) ledgers.push(l);
  for (let l = 6; l <= line; l += 1) ledgers.push(l);
  return (
    <g className="staff-ghost">
      {ledgers.map((l) => (
        <line key={l} x1={p.x - 9} x2={p.x + 9} y1={lineY(s, l)} y2={lineY(s, l)} />
      ))}
      <ellipse cx={p.x} cy={y} rx={6} ry={4.6} transform={`rotate(-20 ${p.x} ${y})`} />
    </g>
  );
}
