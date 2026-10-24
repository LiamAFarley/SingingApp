import { useEffect, useRef, useState } from "react";
import type { LaneSpec } from "../bindings/LaneSpec";
import type { NoteResult } from "../bindings/NoteResult";
import { live, useAnimationFrame } from "../live";
import { noteDisplay } from "../music";

interface Props {
  lane: LaneSpec;
  /** Master-clock time (s) of trial time 0. */
  timelineStart: number | null;
  showLive: boolean;
  /** After grading: the analysed trace (trial time, MIDI) and per-note results. */
  graded?: { trace: [number, number | null][]; notes: NoteResult[] } | null;
  height?: number;
}

const COLORS = {
  grid: "#eef0f3",
  gridC: "#dde1e6",
  beat: "#eef0f3",
  bar: "#d5dae0",
  label: "#8c959f",
  labelC: "#59636e",
  labelTarget: "#1f2328",
  currentRow: "#f4f7fd",
  refFill: "#f1f3f5",
  refStroke: "#c7cdd4",
  accFill: "#f3f1f8",
  accStroke: "#bcb3d4",
  target: "#e8f0fd",
  targetStroke: "#b8cdf5",
  current: "#d3e2fb",
  currentStroke: "#2d6cdf",
  trace: "#1f2328",
  now: "#2d6cdf",
  correct: ["#e6f4ea", "#1a7f37"],
  marginal: ["#fff4d6", "#9a6700"],
  wrong: ["#fde8e9", "#cf222e"],
  missed: ["#f3f4f6", "#8c959f"],
};

const VIEW_SPAN_S = 6;
const NOW_AT = 0.3;
const GUTTER = 46;

export function PitchLane({ lane, timelineStart, showLive, graded, height = 300 }: Props) {
  const wrap = useRef<HTMLDivElement>(null);
  const canvas = useRef<HTMLCanvasElement>(null);
  const [width, setWidth] = useState(800);
  const animating = !graded && timelineStart != null;
  useAnimationFrame(animating);

  useEffect(() => {
    const el = wrap.current;
    if (!el) return;
    const ro = new ResizeObserver(() => setWidth(el.clientWidth));
    ro.observe(el);
    setWidth(el.clientWidth);
    return () => ro.disconnect();
  }, []);

  // Draw on every render (animation frames re-render while recording).
  const c = canvas.current;
  if (c) {
    const dpr = window.devicePixelRatio || 1;
    if (c.width !== Math.round(width * dpr) || c.height !== Math.round(height * dpr)) {
      c.width = Math.round(width * dpr);
      c.height = Math.round(height * dpr);
    }
    const ctx = c.getContext("2d");
    if (ctx) {
      ctx.setTransform(dpr, 0, 0, dpr, 0, 0);
      draw(ctx, width, height, lane, timelineStart, showLive, graded ?? null);
    }
  }

  return (
    <div className="lane-wrap" ref={wrap}>
      <canvas ref={canvas} style={{ width: "100%", height }} />
    </div>
  );
}

function draw(
  ctx: CanvasRenderingContext2D,
  w: number,
  h: number,
  lane: LaneSpec,
  timelineStart: number | null,
  showLive: boolean,
  graded: { trace: [number, number | null][]; notes: NoteResult[] } | null,
) {
  ctx.clearRect(0, 0, w, h);
  ctx.fillStyle = "#ffffff";
  ctx.fillRect(0, 0, w, h);
  const plotW = w - GUTTER;
  const top = 18;
  const bottom = h - 8;
  const lo = lane.midi_low;
  const hi = lane.midi_high;
  const rowH = (bottom - top) / (hi - lo + 1);
  // Centre of the row for MIDI note m (fractional m for pitch traces).
  const y = (m: number) => top + (hi + 0.5 - m) * rowH;

  const now = timelineStart != null ? live.now() - timelineStart : 0;
  let t0: number;
  let t1: number;
  if (graded) {
    t0 = Math.max(0, lane.sing_start_s - 0.6);
    t1 = lane.total_s;
  } else {
    t0 = now - NOW_AT * VIEW_SPAN_S;
    t1 = t0 + VIEW_SPAN_S;
  }
  const x = (t: number) => GUTTER + ((t - t0) / (t1 - t0)) * plotW;

  const current = !graded && !lane.targets_hidden ? lane.notes.find((n) => now >= n.start_s && now < n.start_s + n.dur_s) : undefined;

  // Row band of the note being sung.
  if (current) {
    ctx.fillStyle = COLORS.currentRow;
    ctx.fillRect(GUTTER, y(current.midi) - rowH / 2, plotW, rowH);
  }

  // Semitone lines through the middle of each row.
  for (let m = lo; m <= hi; m++) {
    const isC = ((m % 12) + 12) % 12 === 0;
    ctx.strokeStyle = isC ? COLORS.gridC : COLORS.grid;
    ctx.lineWidth = 1;
    ctx.beginPath();
    ctx.moveTo(GUTTER, Math.round(y(m)) + 0.5);
    ctx.lineTo(w, Math.round(y(m)) + 0.5);
    ctx.stroke();
  }

  // Note labels: target notes first, then each C, then the other naturals, without overlapping.
  const targetSet = new Set<number>(lane.targets_hidden && !graded ? [] : lane.notes.map((n) => n.midi));
  for (const n of lane.reference) targetSet.add(n.midi);
  const pcOf = (m: number) => ((m % 12) + 12) % 12;
  const rank = (m: number) => (targetSet.has(m) ? 0 : pcOf(m) === 0 ? 1 : [2, 4, 5, 7, 9, 11].includes(pcOf(m)) ? 2 : 3);
  // With visible targets only they and the Cs are named; otherwise every natural note.
  const maxRank = targetSet.size > 0 ? 1 : 2;
  const order = Array.from({ length: hi - lo + 1 }, (_, i) => lo + i)
    .filter((m) => rank(m) <= maxRank)
    .sort((a, b) => rank(a) - rank(b) || b - a);
  const placed: number[] = [];
  ctx.font = "11px 'Segoe UI Variable Text', 'Segoe UI', system-ui, sans-serif";
  ctx.textBaseline = "middle";
  for (const m of order) {
    const yy = y(m);
    if (placed.some((p) => Math.abs(p - yy) < 12)) continue;
    placed.push(yy);
    const r = rank(m);
    ctx.fillStyle = current && current.midi === m ? COLORS.currentStroke : r === 0 ? COLORS.labelTarget : r === 1 ? COLORS.labelC : COLORS.label;
    ctx.fillText(noteDisplay(m), 8, yy);
  }

  // Beat grid.
  if (lane.beat_s > 0) {
    const first = Math.max(0, Math.floor(t0 / lane.beat_s));
    const last = Math.ceil(t1 / lane.beat_s);
    for (let b = first; b <= last; b++) {
      const t = b * lane.beat_s;
      if (t > lane.total_s + 0.001) break;
      const isBar = b % lane.beats_per_bar === 0;
      ctx.strokeStyle = isBar ? COLORS.bar : COLORS.beat;
      ctx.beginPath();
      ctx.moveTo(Math.round(x(t)) + 0.5, top);
      ctx.lineTo(Math.round(x(t)) + 0.5, bottom);
      ctx.stroke();
    }
    // Count-in numbers in the bar before singing.
    if (lane.metronome && !graded) {
      ctx.fillStyle = COLORS.label;
      ctx.textBaseline = "alphabetic";
      const startBeat = Math.round(lane.sing_start_s / lane.beat_s);
      for (let k = lane.beats_per_bar; k >= 1; k--) {
        const t = (startBeat - k) * lane.beat_s;
        if (t < 0) continue;
        const xx = x(t);
        if (xx > GUTTER - 4 && xx < w) ctx.fillText(String(lane.beats_per_bar - k + 1), xx + 3, top - 5);
      }
      ctx.textBaseline = "middle";
    }
  }

  // "Sing" marker.
  const xs = x(lane.sing_start_s);
  if (xs > GUTTER && xs < w) {
    ctx.strokeStyle = COLORS.currentStroke;
    ctx.globalAlpha = 0.35;
    ctx.beginPath();
    ctx.moveTo(Math.round(xs) + 0.5, top);
    ctx.lineTo(Math.round(xs) + 0.5, bottom);
    ctx.stroke();
    ctx.globalAlpha = 1;
    ctx.fillStyle = COLORS.currentStroke;
    ctx.textBaseline = "alphabetic";
    ctx.fillText("Sing", xs + 4, top - 5);
    ctx.textBaseline = "middle";
  }

  const bar = (t: number, d: number, m: number, fill: string, stroke: string, label?: string, dashed = false) => {
    const xa = Math.max(GUTTER, x(t));
    const xb = Math.min(w, x(t + d) - 2);
    if (xb <= xa) return;
    const hh = Math.max(Math.min(rowH - 4, 16), 4);
    const yy = y(m) - hh / 2;
    ctx.fillStyle = fill;
    ctx.strokeStyle = stroke;
    ctx.lineWidth = 1;
    ctx.setLineDash(dashed ? [4, 3] : []);
    roundRect(ctx, xa, yy, xb - xa, hh, 3);
    ctx.fill();
    ctx.stroke();
    ctx.setLineDash([]);
    if (label && xb - xa > 28 && hh >= 11) {
      ctx.fillStyle = stroke;
      ctx.fillText(label, xa + 5, yy + hh / 2);
    }
  };

  for (const n of lane.reference) bar(n.start_s, n.dur_s, n.midi, COLORS.refFill, COLORS.refStroke);
  // The other voice of a harmony exercise, named so it reads as the part you hear.
  for (const n of lane.accompaniment) bar(n.start_s, n.dur_s, n.midi, COLORS.accFill, COLORS.accStroke, noteDisplay(n.midi));

  const showTargets = !lane.targets_hidden || !!graded;
  if (showTargets) {
    lane.notes.forEach((n, i) => {
      if (graded) {
        const r = graded.notes[i];
        const kind = !r || !r.matched ? "missed" : !r.correct ? "wrong" : r.in_tune ? "correct" : "marginal";
        const [fill, stroke] = COLORS[kind];
        bar(n.start_s, n.dur_s, n.midi, fill, stroke, noteDisplay(n.midi), kind === "missed");
      } else {
        const isCurrent = n === current;
        bar(
          n.start_s,
          n.dur_s,
          n.midi,
          isCurrent ? COLORS.current : COLORS.target,
          isCurrent ? COLORS.currentStroke : COLORS.targetStroke,
          noteDisplay(n.midi),
        );
      }
    });
  }

  // Pitch trace.
  ctx.strokeStyle = COLORS.trace;
  ctx.lineWidth = 2;
  ctx.lineJoin = "round";
  const plot = (pts: [number, number | null][]) => {
    let drawing = false;
    let lastT = -Infinity;
    ctx.beginPath();
    for (const [t, m] of pts) {
      if (m == null || t < t0 || t > t1 || m < lo - 1.5 || m > hi + 1.5 || t - lastT > 0.06) {
        if (drawing) ctx.stroke();
        drawing = false;
        ctx.beginPath();
        if (m == null || t < t0 || t > t1) {
          lastT = -Infinity;
          continue;
        }
      }
      const xx = x(t);
      const yy = y(m);
      if (!drawing) {
        ctx.moveTo(xx, yy);
        drawing = true;
      } else {
        ctx.lineTo(xx, yy);
      }
      lastT = t;
    }
    if (drawing) ctx.stroke();
  };
  if (graded) {
    plot(graded.trace);
  } else if (showLive && timelineStart != null) {
    const pts = live.since(timelineStart + t0).map((p) => [p.t - timelineStart, p.midi] as [number, number | null]);
    plot(pts);
  }

  // Now line.
  if (!graded && timelineStart != null) {
    const xn = x(now);
    ctx.strokeStyle = COLORS.now;
    ctx.lineWidth = 1.5;
    ctx.beginPath();
    ctx.moveTo(xn, top - 2);
    ctx.lineTo(xn, bottom);
    ctx.stroke();
  }

  // Gutter separator.
  ctx.strokeStyle = COLORS.gridC;
  ctx.lineWidth = 1;
  ctx.beginPath();
  ctx.moveTo(GUTTER - 0.5, top);
  ctx.lineTo(GUTTER - 0.5, bottom);
  ctx.stroke();
}

function roundRect(ctx: CanvasRenderingContext2D, x: number, y: number, w: number, h: number, r: number) {
  const rr = Math.min(r, w / 2, h / 2);
  ctx.beginPath();
  ctx.moveTo(x + rr, y);
  ctx.arcTo(x + w, y, x + w, y + h, rr);
  ctx.arcTo(x + w, y + h, x, y + h, rr);
  ctx.arcTo(x, y + h, x, y, rr);
  ctx.arcTo(x, y, x + w, y, rr);
  ctx.closePath();
}
