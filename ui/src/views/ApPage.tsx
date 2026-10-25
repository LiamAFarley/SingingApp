import { useCallback, useEffect, useState } from "react";
import { api, useBackendEvent } from "../api";
import { TodayButton } from "../components/TodayButton";
import type { ApDashboard } from "../bindings/ApDashboard";
import type { ClassProgress } from "../bindings/ClassProgress";
import type { CourseStatus } from "../bindings/CourseStatus";
import type { DailyPlan } from "../bindings/DailyPlan";
import type { DailySet } from "../bindings/DailySet";
import type { TestRun } from "../bindings/TestRun";
import { fmtDate, pct } from "../music";

const DAYS = ["Monday", "Tuesday", "Wednesday", "Thursday", "Friday", "Saturday", "Sunday"];

const TIMBRE_NAMES: Record<string, string> = {
  "gm:0": "Piano",
  "gm:6": "Harpsichord",
  "gm:42": "Cello",
  "gm:71": "Clarinet",
  "synth:square": "Square",
  "synth:sine": "Sine",
};

/** Absolute pitch: today's set, progress per note, the courses and the tests. */
export function ApPage({ sessionActive, onOpenSession }: { sessionActive: boolean; onOpenSession: () => void }) {
  const [dash, setDash] = useState<ApDashboard | null>(null);
  const [set, setSet] = useState<DailySet | null>(null);
  const [setProblem, setSetProblem] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [starting, setStarting] = useState<string | null>(null);

  const refresh = useCallback(() => {
    api("apDashboard")
      .then(setDash)
      .catch((e) => setError(e.message));
    api("previewDailySet")
      .then((s) => {
        setSet(s);
        setSetProblem(null);
      })
      .catch((e) => {
        setSet(null);
        setSetProblem(e.message);
      });
  }, []);
  useEffect(refresh, [refresh]);
  useBackendEvent("courses-changed", refresh);

  const start = async (key: string, req: Parameters<typeof api<"startSession">>[1]) => {
    setError(null);
    setStarting(key);
    try {
      await api("startSession", req);
      onOpenSession();
    } catch (e) {
      setError((e as Error).message);
    } finally {
      setStarting(null);
    }
  };

  const savePlan = (plan: DailyPlan) => {
    api("saveDailyPlan", plan)
      .then(refresh)
      .catch((e) => setError(e.message));
  };

  const busy = sessionActive || starting != null;
  const own = dash?.courses.filter((c) => c.category === "Absolute pitch") ?? [];
  const ladders = own.filter((c) => c.kind === "ladder");
  const tests = own.filter((c) => c.kind === "recurring");

  return (
    <div className="page">
      <div className="page-head">
        <div>
          <h1>Absolute pitch</h1>
          <div className="sub">Today's set, progress on every note, and the courses it is built from.</div>
        </div>
      </div>
      {error && <div className="notice error">{error}</div>}

      <div className="card daily-set">
        <div className="row" style={{ justifyContent: "space-between", alignItems: "flex-start", gap: 16 }}>
          <div className="grow">
            <div className="title">Today's set</div>
            {set ? (
              <ol className="daily-items">
                {set.items.map((i) => (
                  <li key={`${i.course}-${i.level ?? 0}`}>
                    <div className="row" style={{ gap: 8 }}>
                      <strong>{i.title}</strong>
                      {i.test && <span className="badge accent">test</span>}
                      <span className="small faint num">about {Math.max(1, Math.round(i.minutes))} min</span>
                    </div>
                    <div className="desc">{i.reason}</div>
                  </li>
                ))}
              </ol>
            ) : (
              <div className="desc">{setProblem ?? "Working out today's set…"}</div>
            )}
          </div>
          <div style={{ textAlign: "right" }}>
            {set && <div className="big-num num">{Math.round(set.minutes)} min</div>}
            <TodayButton disabled={busy || !set} starting={starting} onStart={start} />
          </div>
        </div>
      </div>

      {dash && (
        <>
          <div className="section">
            <div className="section-head">
              <h2>Notes</h2>
              <span className="small faint">
                {dash.classes_mastered} of 12 mastered: 90% named within 2 s over the recent trials, every method counted
              </span>
            </div>
            <div className="note-grid">
              {dash.classes.map((c) => (
                <NoteTile key={c.pitch_class} c={c} />
              ))}
            </div>
            <div className="row wrap small muted" style={{ gap: 20 }}>
              <span>{dash.minutes_this_week} min of absolute pitch in the last 7 days</span>
              {dash.anchor_median_abs_cents != null && (
                <span>
                  Your C from memory: {Math.round(dash.anchor_median_abs_cents)} cents off (median of the last {Math.min(10, dash.anchor.length)})
                </span>
              )}
            </div>
          </div>

          <div className="section">
            <div className="section-head">
              <h2>Courses</h2>
              <label className="row small" style={{ gap: 8 }}>
                Tests on
                <select
                  value={dash.plan.test_day}
                  onChange={(e) => savePlan({ ...dash.plan, test_day: Number(e.target.value) })}
                >
                  {DAYS.map((d, i) => (
                    <option key={d} value={i}>
                      {d}
                    </option>
                  ))}
                </select>
              </label>
            </div>
            <div className="grid cols-2">
              {ladders.map((c) => (
                <LadderCard
                  key={c.id}
                  c={c}
                  plan={dash.plan}
                  busy={busy}
                  starting={starting}
                  onPlan={savePlan}
                  onStart={(level) => start(`${c.id}-${level}`, { source: "course_level", course: c.id, level })}
                  onSetLevel={(level) =>
                    api("setCourseLevel", c.id, level)
                      .then(refresh)
                      .catch((e) => setError(e.message))
                  }
                />
              ))}
              {tests.map((c) => (
                <div key={c.id} className="card preset">
                  <div>
                    <div className="row" style={{ justifyContent: "space-between" }}>
                      <div className="title">{c.title}</div>
                      {c.due ? <span className="badge accent">due</span> : null}
                    </div>
                    <div className="desc">{c.description}</div>
                    <div className="desc" style={{ marginTop: 6 }}>
                      {c.due_note}
                      {c.last_practised && ` Last run ${fmtDate(c.last_practised)}.`}
                    </div>
                  </div>
                  <div className="row" style={{ justifyContent: "flex-end" }}>
                    <button
                      className="btn small"
                      disabled={busy}
                      onClick={() => start(`${c.id}-1`, { source: "course_level", course: c.id, level: 1 })}
                    >
                      {starting === `${c.id}-1` ? "Starting…" : "Take it now"}
                    </button>
                  </div>
                </div>
              ))}
            </div>
          </div>

          <TestTable title="Benchmark" runs={dash.benchmark} />
          <TestTable title="Weekly test" runs={dash.weekly} />
          <TestTable title="Transfer test" runs={dash.transfer} />
          <TestTable title="UCSF-style test" runs={dash.ucsf_style} />
        </>
      )}
    </div>
  );
}

function NoteTile({ c }: { c: ClassProgress }) {
  const naming = c.naming_in_time;
  return (
    <div className={`note-tile ${c.mastered ? "mastered" : ""}`} title={c.last_seen ? `Last heard ${fmtDate(c.last_seen)}` : "Not practised yet"}>
      <div className="row" style={{ justifyContent: "space-between" }}>
        <span className="note-name">{c.name}</span>
        {c.mastered && <span className="badge correct">mastered</span>}
      </div>
      <div className="small muted">Named in time</div>
      <div className="meter-bar">
        <div style={{ width: `${(naming ?? 0) * 100}%`, background: (naming ?? 0) >= 0.9 ? "var(--correct)" : "var(--accent)" }} />
      </div>
      <div
        className="small num faint"
        title={c.naming_accuracy != null ? `${pct(c.naming_accuracy)} right counting slower answers` : undefined}
      >
        {c.naming_trials > 0 ? `${pct(naming)} of ${c.naming_trials}` : "Not yet"}
      </div>
      <div className="small muted" style={{ marginTop: 4 }}>
        Sung {c.singing_trials > 0 ? `${pct(c.singing_accuracy)} of ${c.singing_trials}` : "–"}
        {c.detection_d_prime != null ? ` · d′ ${c.detection_d_prime.toFixed(1)}` : ""}
      </div>
    </div>
  );
}

export function LadderCard({
  c,
  plan,
  busy,
  starting,
  onPlan,
  onStart,
  onSetLevel,
}: {
  c: CourseStatus;
  plan: DailyPlan;
  busy: boolean;
  starting: string | null;
  onPlan: (p: DailyPlan) => void;
  onStart: (level: number) => void;
  onSetLevel: (level: number) => void;
}) {
  const entry = plan.courses.find((p) => p.course === c.id);
  const [minutes, setMinutes] = useState(entry?.minutes ?? 10);
  useEffect(() => setMinutes(entry?.minutes ?? 10), [entry?.minutes]);
  const update = (active: boolean, m: number) =>
    onPlan({
      ...plan,
      courses: plan.courses.some((p) => p.course === c.id)
        ? plan.courses.map((p) => (p.course === c.id ? { ...p, active, minutes: m } : p))
        : [...plan.courses, { course: c.id, active, minutes: m }],
    });
  const level = Math.min(c.level, c.levels);
  return (
    <div className="card preset">
      <div>
        <div className="row" style={{ justifyContent: "space-between" }}>
          <div className="title">{c.title}</div>
          {c.complete ? <span className="badge correct">complete</span> : <span className="small faint num">Level {c.level} of {c.levels}</span>}
        </div>
        <div className="desc">{c.description}</div>
        {!c.complete && (
          <div style={{ marginTop: 8 }}>
            <strong>{c.level_title}</strong>
            <div className="desc">{c.level_summary}</div>
            {c.pass && <div className="desc" style={{ marginTop: 4 }}>{c.pass.summary}</div>}
          </div>
        )}
      </div>
      <div className="row wrap" style={{ justifyContent: "space-between", gap: 8 }}>
        <div className="row" style={{ gap: 8 }}>
          <label className="row small" style={{ gap: 6 }}>
            <input type="checkbox" checked={entry?.active ?? false} onChange={(e) => update(e.target.checked, minutes)} />
            Daily
          </label>
          <input
            className="num"
            type="number"
            min={1}
            max={90}
            value={minutes}
            style={{ width: 56 }}
            onChange={(e) => setMinutes(Number(e.target.value))}
            onBlur={() => minutes !== entry?.minutes && update(entry?.active ?? false, Math.min(90, Math.max(1, minutes)))}
            aria-label="Minutes a day"
          />
          <span className="small faint">min</span>
          <select value={c.level} onChange={(e) => onSetLevel(Number(e.target.value))} aria-label="Level" title="Set the level by hand" style={{ maxWidth: 170 }}>
            {c.level_titles.map((t, i) => (
              <option key={i} value={i + 1}>
                {i + 1}. {t}
              </option>
            ))}
            <option value={c.levels + 1}>Complete</option>
          </select>
        </div>
        <button className="btn small" disabled={busy} onClick={() => onStart(level)}>
          {starting === `${c.id}-${level}` ? "Starting…" : c.complete ? `Level ${level}` : "Start level"}
        </button>
      </div>
    </div>
  );
}

function TestTable({ title, runs }: { title: string; runs: TestRun[] }) {
  if (runs.length === 0) return null;
  const all = Array.from(new Set(runs.flatMap((r) => r.by_timbre.map((t) => t.timbre)))).filter((t) => t !== "shepard");
  // A column per timbre only while that stays readable.
  const timbres = all.length > 1 && all.length <= 5 ? all : [];
  const shepard = runs.some((r) => r.shepard != null);
  const sung = runs.some((r) => r.sung != null);
  const latest = runs[runs.length - 1];
  return (
    <div className="section">
      <h2>{title}</h2>
      <div className="card flush table-scroll">
        <table>
          <thead>
            <tr>
              <th>Date</th>
              <th>Trials</th>
              <th title="Correct within 5 s, with its 95% range. Guessing gives 8%.">Exact</th>
              <th title="Correct within 2 s of the tone starting">In 2 s</th>
              <th title="1 point for an exact answer, 0.75 for a semitone off">UCSF-style</th>
              <th title="Mean absolute error in semitones: 0 perfect, 3 chance">Mean error</th>
              {shepard && <th title="Shepard tones: note name without height">Shepard</th>}
              {sung && <th title="Notes sung from memory, any octave">Sung</th>}
              {timbres.map((t) => (
                <th key={t}>{TIMBRE_NAMES[t] ?? t}</th>
              ))}
            </tr>
          </thead>
          <tbody>
            {[...runs].reverse().map((r) => (
              <tr key={r.session_id}>
                <td className="num">
                  {fmtDate(r.at)}
                  {r.notes.length > 0 && (
                    <span className="muted" title={r.notes.join("\n")}>
                      {" "}
                      ⚠
                    </span>
                  )}
                </td>
                <td className="num">{r.trials}</td>
                <td className="num" title={`95% range ${pct(r.accuracy_low)}–${pct(r.accuracy_high)}`}>
                  {pct(r.accuracy)}
                  <span className="muted small">
                    {" "}
                    ({pct(r.accuracy_low)}–{pct(r.accuracy_high)})
                  </span>
                </td>
                <td className="num">{pct(r.in_time)}</td>
                <td className="num">{pct(r.ucsf_points)}</td>
                <td className="num">{r.mad == null ? "–" : `${r.mad.toFixed(2)} st`}</td>
                {shepard && <td className="num">{r.shepard == null ? "–" : pct(r.shepard)}</td>}
                {sung && <td className="num">{r.sung == null ? "–" : pct(r.sung.accuracy)}</td>}
                {timbres.map((t) => {
                  const v = r.by_timbre.find((x) => x.timbre === t);
                  return (
                    <td key={t} className="num">
                      {v ? pct(v.accuracy) : "–"}
                    </td>
                  );
                })}
              </tr>
            ))}
          </tbody>
        </table>
      </div>
      {latest.notes.length > 0 && (
        <ul className="muted small test-notes">
          {latest.notes.map((n) => (
            <li key={n}>{n}</li>
          ))}
        </ul>
      )}
    </div>
  );
}
