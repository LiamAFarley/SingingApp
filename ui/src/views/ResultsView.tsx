import { lazy, Suspense, useEffect, useRef, useState } from "react";
import { api } from "../api";
import type { NoteResult } from "../bindings/NoteResult";
import type { PitchRefMode } from "../bindings/PitchRefMode";
import type { Score } from "../bindings/Score";
import type { SessionReport } from "../bindings/SessionReport";
import type { SessionRow } from "../bindings/SessionRow";
import type { TrialBrief } from "../bindings/TrialBrief";
import { fmtCents, fmtDate, fmtDuration, fmtMs, KIND_TITLES, noteDisplay, pct, traceStyle } from "../music";

const Staff = lazy(() => import("../notation/Staff").then((m) => ({ default: m.Staff })));

export function ResultsView({ onRepeat }: { onRepeat: (sessionId: string) => void }) {
  const [sessions, setSessions] = useState<SessionRow[]>([]);
  const [selected, setSelected] = useState<string | null>(null);
  const [report, setReport] = useState<SessionReport | null>(null);

  useEffect(() => {
    api("listSessions", 100).then(setSessions).catch(() => undefined);
  }, []);

  useEffect(() => {
    if (!selected) return setReport(null);
    api("sessionReport", selected).then(setReport).catch(() => setReport(null));
  }, [selected]);

  return (
    <div className="page">
      <div className="page-head">
        <div>
          <h1>Results</h1>
          <div className="sub">Every trial is stored with its stimulus, response and analysis.</div>
        </div>
      </div>
      <div className="card flush">
        {sessions.length === 0 ? (
          <div className="empty">No sessions yet.</div>
        ) : (
          <table>
            <thead>
              <tr>
                <th>Date</th>
                <th>Session</th>
                <th>Source</th>
                <th>Status</th>
                <th>Accuracy</th>
              </tr>
            </thead>
            <tbody>
              {sessions.map((s) => {
                const acc = (s.summary as { accuracy?: number } | null)?.accuracy ?? null;
                return (
                  <tr key={s.id} className={`clickable ${selected === s.id ? "selected" : ""}`} onClick={() => setSelected(s.id === selected ? null : s.id)}>
                    <td className="num">{fmtDate(s.started_at)}</td>
                    <td>{s.title}</td>
                    <td className="muted">{s.source === "llm" ? "Coach" : s.source === "preset" ? "Preset" : s.source}</td>
                    <td className="muted">{s.status}</td>
                    <td className="num">{pct(acc)}</td>
                  </tr>
                );
              })}
            </tbody>
          </table>
        )}
      </div>
      {report && <Report report={report} onRepeat={onRepeat} />}
    </div>
  );
}

function Report({ report, onRepeat }: { report: SessionReport; onRepeat: (id: string) => void }) {
  const [trial, setTrial] = useState<TrialBrief | null>(null);
  return (
    <div className="section">
      <div className="section-head">
        <h2>{report.session.title}</h2>
        <div className="row">
          <span className="small muted">
            {fmtDate(report.session.started_at)} · {fmtDuration(report.duration_s)} · accuracy {pct(report.accuracy)}
          </span>
          <button className="btn small" onClick={() => onRepeat(report.session.id)}>
            Repeat
          </button>
        </div>
      </div>
      <div className="card flush">
        <table>
          <thead>
            <tr>
              <th>Block</th>
              <th>Type</th>
              <th>Trials</th>
              <th>Accuracy</th>
              <th>Median response</th>
              <th>Median error</th>
              <th>Feedback</th>
            </tr>
          </thead>
          <tbody>
            {report.blocks.map((b) => (
              <tr key={b.index}>
                <td>{b.title}</td>
                <td className="muted">{KIND_TITLES[b.kind] ?? b.kind}</td>
                <td className="num">{b.trials}</td>
                <td className="num">{pct(b.accuracy)}</td>
                <td className="num">{fmtMs(b.median_rt_ms)}</td>
                <td className="num">{fmtCents(b.median_abs_cents)}</td>
                <td className="muted">{b.assessment ? "assessment" : b.feedback.replaceAll("_", " ")}</td>
              </tr>
            ))}
          </tbody>
        </table>
      </div>
      {report.by_timbre.length > 1 && (
        <div className="card flush">
          <table>
            <thead>
              <tr>
                <th>Timbre</th>
                <th>Trials</th>
                <th>Accuracy</th>
              </tr>
            </thead>
            <tbody>
              {[...report.by_timbre]
                .sort((a, b) => b.accuracy - a.accuracy)
                .map((t) => (
                  <tr key={t.timbre}>
                    <td>{t.timbre}</td>
                    <td className="num">{t.trials}</td>
                    <td className="num">{pct(t.accuracy)}</td>
                  </tr>
                ))}
            </tbody>
          </table>
        </div>
      )}
      <div className="card flush">
        <table>
          <thead>
            <tr>
              <th>#</th>
              <th>Stimulus</th>
              <th>Result</th>
              <th>Response</th>
            </tr>
          </thead>
          <tbody>
            {report.trials.map((t) => (
              <tr key={t.id} className={`clickable ${trial?.id === t.id ? "selected" : ""}`} onClick={() => setTrial(trial?.id === t.id ? null : t)}>
                <td className="num muted">
                  {t.block_idx + 1}.{t.trial_idx + 1}
                  {t.attempt > 0 ? ` (retry ${t.attempt})` : ""}
                </td>
                <td>{describeStimulus(t)}</td>
                <td>
                  {t.correct == null ? (
                    <span className="badge">passive</span>
                  ) : t.correct ? (
                    <span className="badge correct">correct</span>
                  ) : (
                    <span className="badge wrong">wrong</span>
                  )}{" "}
                  {t.assisted && <span className="badge accent">assisted</span>}
                </td>
                <td className="muted">{t.summary}</td>
              </tr>
            ))}
          </tbody>
        </table>
        {trial && <TrialDetail trial={trial} />}
      </div>
    </div>
  );
}

function describeStimulus(t: TrialBrief): string {
  const s = t.stimulus as Record<string, unknown>;
  const parts: string[] = [];
  if (typeof s.pitch_class === "string") parts.push(String(s.pitch_class) + (typeof s.octave === "number" ? String(s.octave) : ""));
  if (typeof s.interval === "string") parts.push(`${s.interval} ${s.direction ?? ""}`);
  if (typeof s.quality === "string") parts.push(`${s.quality}${s.inversion ? ` inv ${s.inversion}` : ""}`);
  if (typeof s.scale === "string") parts.push(String(s.scale));
  if (typeof s.tonic === "string") parts.push(`key ${s.tonic} ${s.mode ?? ""}`);
  if (typeof s.pattern === "string") parts.push(String(s.pattern));
  if (typeof s.note === "string") parts.push(String(s.note));
  if (typeof s.timbre === "string") parts.push(String(s.timbre));
  return parts.join(" · ") || KIND_TITLES[t.type] || t.type;
}

function TrialDetail({ trial }: { trial: TrialBrief }) {
  const [detail, setDetail] = useState<Record<string, unknown> | null>(null);
  useEffect(() => {
    api("trialDetail", trial.id).then(setDetail).catch(() => setDetail(null));
  }, [trial.id]);
  const rows = ((detail?.trace_20hz as { rows?: [number, number | null][] } | undefined)?.rows ?? []) as [number, number | null][];
  const spec = detail?.spec as
    | { kind?: string; notes?: { midi: number; start_beat: number; beats: number }[]; tempo_bpm?: number; sing_start_beat?: number; score?: Score }
    | undefined;
  const metrics = (detail?.metrics ?? {}) as Record<string, unknown>;
  const beatS = spec?.tempo_bpm ? 60 / spec.tempo_bpm : 1;
  return (
    <div className="drawer section">
      <div className="row" style={{ justifyContent: "space-between" }}>
        <strong>{trial.summary || "Trial"}</strong>
        {Boolean(detail?.has_recording) && (
          <button className="btn small" onClick={() => api("playRecording", trial.id).catch(() => undefined)}>
            Play recording
          </button>
        )}
      </div>
      {spec?.score ? (
        <Suspense fallback={<div className="staff-wrap" style={{ minHeight: 160 }} />}>
          <Staff
            score={spec.score}
            maxHeight={360}
            trace={rows}
            {...traceStyle(metrics.mode as PitchRefMode | undefined)}
            timing={{ singStartS: (spec.sing_start_beat ?? 0) * beatS, beatS }}
            results={(metrics.notes as NoteResult[] | undefined) ?? null}
          />
        </Suspense>
      ) : (
        rows.length > 0 && <TracePlot rows={rows} spec={spec} />
      )}
      <dl className="kv">
        {Object.entries(metrics)
          .filter(([k, v]) => k !== "notes" && (typeof v === "number" || typeof v === "string" || typeof v === "boolean"))
          .slice(0, 16)
          .map(([k, v]) => (
            <div key={k} style={{ display: "contents" }}>
              <dt>{k.replaceAll("_", " ")}</dt>
              <dd className="num">{typeof v === "number" ? (Math.abs(v) < 10 ? v.toFixed(2) : Math.round(v)) : String(v)}</dd>
            </div>
          ))}
      </dl>
    </div>
  );
}

function TracePlot({ rows, spec }: { rows: [number, number | null][]; spec?: { notes?: { midi: number; start_beat: number; beats: number }[]; tempo_bpm?: number; sing_start_beat?: number } }) {
  const ref = useRef<HTMLCanvasElement>(null);
  useEffect(() => {
    const c = ref.current;
    if (!c) return;
    const w = c.clientWidth;
    const h = 200;
    const dpr = window.devicePixelRatio || 1;
    c.width = w * dpr;
    c.height = h * dpr;
    const ctx = c.getContext("2d")!;
    ctx.setTransform(dpr, 0, 0, dpr, 0, 0);
    ctx.fillStyle = "#fff";
    ctx.fillRect(0, 0, w, h);
    const voiced = rows.filter((r) => r[1] != null).map((r) => r[1] as number);
    const beat = spec?.tempo_bpm ? 60 / spec.tempo_bpm : 1;
    const targets = (spec?.notes ?? []).map((n) => ({ midi: n.midi, t: ((spec?.sing_start_beat ?? 0) + n.start_beat) * beat, d: n.beats * beat }));
    const all = voiced.concat(targets.map((t) => t.midi));
    if (all.length === 0) return;
    const lo = Math.floor(Math.min(...all)) - 2;
    const hi = Math.ceil(Math.max(...all)) + 2;
    const t0 = rows[0][0];
    const t1 = rows[rows.length - 1][0];
    const x = (t: number) => 44 + ((t - t0) / Math.max(0.1, t1 - t0)) * (w - 52);
    const y = (m: number) => 8 + ((hi - m) / (hi - lo)) * (h - 16);
    ctx.font = "11px 'Segoe UI', system-ui, sans-serif";
    ctx.textBaseline = "middle";
    for (let m = lo; m <= hi; m++) {
      ctx.strokeStyle = m % 12 === 0 ? "#dde1e6" : "#f0f2f4";
      ctx.beginPath();
      ctx.moveTo(44, Math.round(y(m)) + 0.5);
      ctx.lineTo(w, Math.round(y(m)) + 0.5);
      ctx.stroke();
      if (m % 2 === 0) {
        ctx.fillStyle = "#8c959f";
        ctx.fillText(noteDisplay(m), 4, y(m));
      }
    }
    for (const t of targets) {
      ctx.fillStyle = "#e8f0fd";
      ctx.strokeStyle = "#b8cdf5";
      ctx.fillRect(x(t.t), y(t.midi) - 5, x(t.t + t.d) - x(t.t) - 2, 10);
      ctx.strokeRect(x(t.t), y(t.midi) - 5, x(t.t + t.d) - x(t.t) - 2, 10);
    }
    ctx.strokeStyle = "#1f2328";
    ctx.lineWidth = 2;
    ctx.beginPath();
    let drawing = false;
    for (const [t, m] of rows) {
      if (m == null) {
        drawing = false;
        continue;
      }
      if (!drawing) ctx.moveTo(x(t), y(m));
      else ctx.lineTo(x(t), y(m));
      drawing = true;
    }
    ctx.stroke();
  }, [rows, spec]);
  return (
    <div className="lane-wrap">
      <canvas ref={ref} style={{ width: "100%", height: 200 }} />
    </div>
  );
}
