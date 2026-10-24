import { useEffect, useRef, useState } from "react";
import { api } from "../api";
import { LevelMeter } from "../components/LevelMeter";
import { live, useAnimationFrame } from "../live";
import { describeCents, midiToHz, noteDisplay, octave, pcDisplay } from "../music";

const HISTORY_S = 8;

export function TunerView({ a4 }: { a4: number }) {
  useAnimationFrame(true);
  const p = live.latest(0.25);
  const midi = p?.midi ?? null;
  const nearest = midi != null ? Math.round(midi) : null;
  const cents = midi != null && nearest != null ? (midi - nearest) * 100 : null;
  const [refNote, setRefNote] = useState(57);
  const [timbre, setTimbre] = useState("gm:0");
  const [drone, setDrone] = useState(false);

  useEffect(() => () => void api("setDrone", null).catch(() => undefined), []);

  // A piano decays, so the drone uses a sustained sound in its place.
  const droneTimbre = timbre === "gm:0" ? "gm:52" : timbre;

  // A running drone follows changes of note and sound.
  useEffect(() => {
    if (drone) api("setDrone", refNote, droneTimbre).catch(() => setDrone(false));
  }, [drone, refNote, droneTimbre]);

  const toggleDrone = async () => {
    if (drone) {
      setDrone(false);
      await api("setDrone", null).catch(() => undefined);
    } else {
      setDrone(true);
    }
  };

  const cls = cents == null ? "" : Math.abs(cents) <= 10 ? "correct" : Math.abs(cents) <= 25 ? "marginal" : "wrong";
  const color = cls === "correct" ? "var(--correct)" : cls === "marginal" ? "var(--marginal)" : cls === "wrong" ? "var(--wrong)" : "var(--text-faint)";

  return (
    <div className="page">
      <div className="page-head">
        <div>
          <h1>Tuner</h1>
          <div className="sub">Live pitch from the microphone input. A4 = {a4} Hz.</div>
        </div>
        <LevelMeter />
      </div>

      <div className="card" style={{ padding: 28 }}>
        <div className="grid" style={{ gridTemplateColumns: "260px 1fr", alignItems: "center", gap: 28 }}>
          <div>
            <div className="tuner-note" style={{ color: nearest == null ? "var(--text-faint)" : "var(--text)" }}>
              {nearest == null ? "–" : pcDisplay(nearest)}
              {nearest != null && <span className="oct">{octave(nearest)}</span>}
            </div>
            <div className="prompt-sub num" style={{ marginTop: 10, color }}>
              {cents == null ? "No pitch" : describeCents(cents)}
            </div>
            <div className="prompt-sub num small">{midi != null ? `${midiToHz(midi, a4).toFixed(1)} Hz` : ""}</div>
          </div>
          <div className="section">
            <CentsMeter cents={cents} color={color} />
            <History a4={a4} />
          </div>
        </div>
      </div>

      <div className="card">
        <div className="row wrap" style={{ gap: 16, alignItems: "flex-end" }}>
          <label className="field">
            Reference note
            <select value={refNote} onChange={(e) => setRefNote(parseInt(e.target.value, 10))}>
              {Array.from({ length: 37 }, (_, i) => 40 + i).map((m) => (
                <option key={m} value={m}>
                  {noteDisplay(m)}
                </option>
              ))}
            </select>
          </label>
          <label className="field">
            Sound
            <select value={timbre} onChange={(e) => setTimbre(e.target.value)}>
              <option value="gm:0">Piano</option>
              <option value="gm:52">Choir</option>
              <option value="gm:48">Strings</option>
              <option value="gm:73">Flute</option>
              <option value="synth:sine">Sine</option>
            </select>
          </label>
          <button className="btn" onClick={() => api("playNote", refNote, timbre, 1500).catch(() => undefined)}>
            Play note
          </button>
          <button className={`btn ${drone ? "primary" : ""}`} onClick={toggleDrone}>
            {drone ? "Stop drone" : "Drone"}
          </button>
          <span className="small faint">The drone keeps sounding while you match it.</span>
        </div>
      </div>
    </div>
  );
}

function CentsMeter({ cents, color }: { cents: number | null; color: string }) {
  const pos = cents == null ? 50 : 50 + Math.max(-50, Math.min(50, cents));
  return (
    <div style={{ position: "relative", height: 40 }}>
      <div style={{ position: "absolute", left: 0, right: 0, top: 18, height: 4, background: "var(--border)", borderRadius: 2 }} />
      <div style={{ position: "absolute", left: "40%", width: "20%", top: 16, height: 8, background: "var(--correct-soft)", borderRadius: 3 }} />
      {[-50, -25, 0, 25, 50].map((t) => (
        <div key={t} style={{ position: "absolute", left: `${50 + t}%`, top: 30, transform: "translateX(-50%)" }} className="small faint num">
          {t > 0 ? `+${t}` : t}
        </div>
      ))}
      <div
        style={{
          position: "absolute",
          left: `${pos}%`,
          top: 6,
          width: 3,
          height: 28,
          marginLeft: -1.5,
          borderRadius: 2,
          background: color,
          transition: "left 60ms linear",
          opacity: cents == null ? 0.3 : 1,
        }}
      />
    </div>
  );
}

function History({ a4 }: { a4: number }) {
  const ref = useRef<HTMLCanvasElement>(null);
  const centre = useRef<number>(57);
  const c = ref.current;
  if (c) {
    const w = c.clientWidth || 600;
    const h = 180;
    const dpr = window.devicePixelRatio || 1;
    if (c.width !== Math.round(w * dpr)) c.width = Math.round(w * dpr);
    if (c.height !== Math.round(h * dpr)) c.height = Math.round(h * dpr);
    const ctx = c.getContext("2d")!;
    ctx.setTransform(dpr, 0, 0, dpr, 0, 0);
    ctx.clearRect(0, 0, w, h);
    const now = live.now();
    const pts = live.since(now - HISTORY_S);
    const latest = live.latest(1);
    if (latest?.midi != null) centre.current += (latest.midi - centre.current) * 0.05;
    const span = 7;
    const lo = centre.current - span / 2;
    const y = (m: number) => h - ((m - lo) / span) * h;
    const x = (t: number) => ((t - (now - HISTORY_S)) / HISTORY_S) * w;
    ctx.font = "11px 'Segoe UI', system-ui, sans-serif";
    ctx.textBaseline = "middle";
    for (let m = Math.ceil(lo); m <= lo + span; m++) {
      const yy = Math.round(y(m)) + 0.5;
      ctx.strokeStyle = m % 12 === 0 ? "#dde1e6" : "#eef0f3";
      ctx.beginPath();
      ctx.moveTo(40, yy);
      ctx.lineTo(w, yy);
      ctx.stroke();
      ctx.fillStyle = "#8c959f";
      ctx.fillText(noteDisplay(m), 4, yy);
    }
    ctx.strokeStyle = "#1f2328";
    ctx.lineWidth = 2;
    ctx.beginPath();
    let drawing = false;
    let lastT = 0;
    for (const p of pts) {
      if (p.midi == null || p.t - lastT > 0.05) {
        drawing = false;
      }
      if (p.midi != null) {
        const xx = Math.max(40, x(p.t));
        if (!drawing) ctx.moveTo(xx, y(p.midi));
        else ctx.lineTo(xx, y(p.midi));
        drawing = true;
      }
      lastT = p.t;
    }
    ctx.stroke();
  }
  void a4;
  return <canvas ref={ref} style={{ width: "100%", height: 180 }} />;
}
