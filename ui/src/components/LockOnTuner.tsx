import { useEffect, useRef, useState } from "react";
import { api } from "../api";
import type { TunerTarget } from "../bindings/TunerTarget";
import { live, useAnimationFrame } from "../live";
import { describeCents, noteDisplay } from "../music";

/** Within this many cents of the target counts as on it. */
const LOCK_CENTS = 15;
/** Held on the target this long counts as locked on. */
const LOCK_S = 1;
const HISTORY_S = 6;
const METER_CENTS = 100;

/** The target note nearest to `midi`: the exact note, or the nearest note of the pitch class. */
function goalFor(target: TunerTarget, midi: number): number {
  if (target.midi != null) return target.midi;
  const below = Math.floor(midi) - ((((Math.floor(midi) - target.pitch_class) % 12) + 12) % 12);
  return midi - below > 6 ? below + 12 : below;
}

function describeOffset(cents: number): string {
  if (Math.abs(cents) < 100) return describeCents(cents);
  const semis = Math.abs(cents) / 100;
  return `${semis.toFixed(1)} semitones ${cents < 0 ? "low" : "high"}`;
}

/**
 * A live tuner aimed at one note, shown after a sung note: sing, watch how far off you are, and
 * hold it within 15 cents for a second to lock on.
 */
export function LockOnTuner({ target }: { target: TunerTarget }) {
  useAnimationFrame(true);
  const [drone, setDrone] = useState(false);
  const runStart = useRef<number | null>(null);
  const locks = useRef(0);
  const lockedThisRun = useRef(false);

  useEffect(() => () => void api("setDrone", null).catch(() => undefined), []);

  const toggleDrone = () => {
    if (target.reference_midi == null) return;
    const next = !drone;
    setDrone(next);
    api("setDrone", next ? target.reference_midi : null, "gm:52").catch(() => setDrone(false));
  };

  const p = live.latest(0.25);
  const midi = p?.midi ?? null;
  const goal = midi != null ? goalFor(target, midi) : null;
  const cents = midi != null && goal != null ? (midi - goal) * 100 : null;
  const now = live.now();
  const on = cents != null && Math.abs(cents) <= LOCK_CENTS;
  if (on) {
    runStart.current ??= now;
  } else {
    runStart.current = null;
    lockedThisRun.current = false;
  }
  const held = runStart.current != null ? now - runStart.current : 0;
  if (held >= LOCK_S && !lockedThisRun.current) {
    lockedThisRun.current = true;
    locks.current += 1;
  }
  const locked = held >= LOCK_S;
  const cls = cents == null ? "" : Math.abs(cents) <= LOCK_CENTS ? "correct" : Math.abs(cents) <= 35 ? "marginal" : "wrong";
  const color = cls === "correct" ? "var(--correct)" : cls === "marginal" ? "var(--marginal)" : cls === "wrong" ? "var(--wrong)" : "var(--text-faint)";

  return (
    <div className="lock-tuner">
      <div className="row" style={{ justifyContent: "space-between", alignItems: "baseline" }}>
        <div className="title">Lock on to {target.name}</div>
        <div className="small muted num">{locks.current > 0 ? `Locked on ${locks.current}×` : `Hold within ${LOCK_CENTS} cents for ${LOCK_S} s`}</div>
      </div>
      <div className="lock-readout">
        <div className="lock-note" style={{ color: midi == null ? "var(--text-faint)" : "var(--text)" }}>
          {midi == null ? "–" : noteDisplay(Math.round(midi))}
        </div>
        <div className="grow">
          <div className="num" style={{ color, fontWeight: 600 }}>
            {cents == null ? "Sing to start" : locked ? "Locked on" : describeOffset(cents)}
          </div>
          <div className="lock-hold">
            <div className="lock-hold-fill" style={{ width: `${Math.min(1, held / LOCK_S) * 100}%` }} />
          </div>
        </div>
      </div>
      <OffsetMeter cents={cents} color={color} />
      <TargetHistory target={target} />
      <div className="row" style={{ justifyContent: "space-between" }}>
        <span className="small faint">{target.midi != null ? "Aim for this exact note." : "Any octave counts."}</span>
        {target.reference_midi != null && (
          <button className={`btn small ${drone ? "primary" : ""}`} onClick={toggleDrone}>
            {drone ? "Stop drone" : `Drone ${target.name}`}
          </button>
        )}
      </div>
    </div>
  );
}

/** Offset from the target, ±100 cents, with the lock zone marked. */
function OffsetMeter({ cents, color }: { cents: number | null; color: string }) {
  const clamp = (c: number) => Math.max(-METER_CENTS, Math.min(METER_CENTS, c));
  const pos = (c: number) => 50 + (clamp(c) / METER_CENTS) * 50;
  const zone = (LOCK_CENTS / METER_CENTS) * 50;
  return (
    <div className="offset-meter">
      <div className="offset-track" />
      <div className="offset-zone" style={{ left: `${50 - zone}%`, width: `${zone * 2}%` }} />
      {[-100, -50, 0, 50, 100].map((t) => (
        <div key={t} className="offset-tick small faint num" style={{ left: `${pos(t)}%` }}>
          {t > 0 ? `+${t}` : t}
        </div>
      ))}
      <div
        className="offset-needle"
        style={{ left: `${cents == null ? 50 : pos(cents)}%`, background: color, opacity: cents == null ? 0.3 : 1 }}
      />
    </div>
  );
}

/** The last few seconds of pitch, with the target note's band. */
function TargetHistory({ target }: { target: TunerTarget }) {
  const ref = useRef<HTMLCanvasElement>(null);
  const centre = useRef<number>(target.reference_midi ?? target.midi ?? 60);
  const c = ref.current;
  if (c) {
    const w = c.clientWidth || 600;
    const h = 96;
    const dpr = window.devicePixelRatio || 1;
    if (c.width !== Math.round(w * dpr)) c.width = Math.round(w * dpr);
    if (c.height !== Math.round(h * dpr)) c.height = Math.round(h * dpr);
    const ctx = c.getContext("2d")!;
    ctx.setTransform(dpr, 0, 0, dpr, 0, 0);
    ctx.clearRect(0, 0, w, h);
    const now = live.now();
    const latest = live.latest(1);
    if (latest?.midi != null) centre.current += (goalFor(target, latest.midi) - centre.current) * 0.08;
    const span = 5;
    const lo = centre.current - span / 2;
    const y = (m: number) => h - ((m - lo) / span) * h;
    const x = (t: number) => 40 + ((t - (now - HISTORY_S)) / HISTORY_S) * (w - 40);
    ctx.font = "11px 'Segoe UI', system-ui, sans-serif";
    ctx.textBaseline = "middle";
    for (let m = Math.ceil(lo); m <= lo + span; m++) {
      const isTarget = target.midi != null ? m === target.midi : ((m % 12) + 12) % 12 === target.pitch_class;
      const yy = y(m);
      if (isTarget) {
        // The lock zone around the target note.
        const band = (LOCK_CENTS / 100) * (h / span);
        ctx.fillStyle = "#e6f4ea";
        ctx.fillRect(40, yy - band, w - 40, band * 2);
      }
      ctx.strokeStyle = isTarget ? "#1a7f37" : "#eef0f3";
      ctx.beginPath();
      ctx.moveTo(40, Math.round(yy) + 0.5);
      ctx.lineTo(w, Math.round(yy) + 0.5);
      ctx.stroke();
      ctx.fillStyle = isTarget ? "#1a7f37" : "#8c959f";
      ctx.fillText(noteDisplay(m), 4, yy);
    }
    ctx.strokeStyle = "#1f2328";
    ctx.lineWidth = 2;
    ctx.beginPath();
    let drawing = false;
    let lastT = 0;
    for (const pt of live.since(now - HISTORY_S)) {
      if (pt.midi == null || pt.t - lastT > 0.05) drawing = false;
      if (pt.midi != null) {
        const xx = Math.max(40, x(pt.t));
        if (!drawing) ctx.moveTo(xx, y(pt.midi));
        else ctx.lineTo(xx, y(pt.midi));
        drawing = true;
      }
      lastT = pt.t;
    }
    ctx.stroke();
  }
  return <canvas ref={ref} style={{ width: "100%", height: 96 }} />;
}
