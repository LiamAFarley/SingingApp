import { useCallback, useEffect, useState } from "react";
import { api, useBackendEvent } from "../api";
import { TodayButton } from "../components/TodayButton";
import type { AnchorPoint } from "../bindings/AnchorPoint";
import type { ApDashboard } from "../bindings/ApDashboard";
import type { AptitudeProfile } from "../bindings/AptitudeProfile";
import type { ClassProgress } from "../bindings/ClassProgress";
import type { ReferenceLine } from "../bindings/ReferenceLine";
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
  const [aptitude, setAptitude] = useState<AptitudeProfile | null>(null);
  const [set, setSet] = useState<DailySet | null>(null);
  const [setProblem, setSetProblem] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [starting, setStarting] = useState<string | null>(null);

  const refresh = useCallback(() => {
    api("apDashboard")
      .then(setDash)
      .catch((e) => setError(e.message));
    api("aptitudeProfile")
      .then(setAptitude)
      .catch(() => setAptitude(null));
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
  // In the plan's order, which is the order of today's set.
  const planIndex = (id: string) => {
    const i = dash?.plan.courses.findIndex((p) => p.course === id) ?? -1;
    return i < 0 ? Number.MAX_SAFE_INTEGER : i;
  };
  const ladders = own.filter((c) => c.kind === "ladder").sort((a, b) => planIndex(a.id) - planIndex(b.id));
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
                {dash.classes_mastered} of 12 named (90% within 2 s over the recent trials, every method counted); {dash.classes_known} known,
                also sung within a quarter tone
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
            <AnchorTrend points={dash.anchor} />
          </div>

          {aptitude && <AptitudeSection p={aptitude} />}

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
                  onStart={(level, minutes, checks) =>
                    start(`${c.id}-${level}-${checks ? "level" : "drill"}`, {
                      source: "course_level",
                      course: c.id,
                      level,
                      minutes,
                      checks,
                    })
                  }
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
  const sources = c.naming_sources.map((s) => `${s.source} ${s.trials}`).join(", ");
  const seen = c.last_seen ? `Last heard ${fmtDate(c.last_seen)}` : "Not practised yet";
  return (
    <div className={`note-tile ${c.mastered ? "mastered" : ""}`} title={sources ? `${seen}. Naming trials from: ${sources}.` : seen}>
      <div className="row" style={{ justifyContent: "space-between" }}>
        <span className="note-name">{c.name}</span>
        {c.known ? (
          <span className="badge correct" title="Named within 2 s and sung within a quarter tone">
            known
          </span>
        ) : (
          c.mastered && <span className="badge correct">named</span>
        )}
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
      <div className="small muted" style={{ marginTop: 4 }} title="Sung within a quarter tone, any octave. Reliable at 80% of 8 or more.">
        Sung {c.singing_trials > 0 ? `${pct(c.singing_accuracy)} of ${c.singing_trials}` : "–"}
        {c.sung_reliably ? " ✓" : ""}
        {c.detection_d_prime != null ? ` · d′ ${c.detection_d_prime.toFixed(1)}` : ""}
      </div>
    </div>
  );
}

/** Internal anchor accuracy: cents from a real C, each day it was sung from memory. */
function AnchorTrend({ points }: { points: AnchorPoint[] }) {
  if (points.length < 2) return null;
  const w = 320;
  const h = 80;
  const max = Math.max(50, ...points.map((p) => Math.abs(p.cents)));
  const x = (i: number) => (i / (points.length - 1)) * (w - 12) + 6;
  const y = (c: number) => h / 2 - (c / max) * (h / 2 - 6);
  const path = points.map((p, i) => `${i === 0 ? "M" : "L"}${x(i).toFixed(1)},${y(p.cents).toFixed(1)}`).join(" ");
  return (
    <div className="anchor-trend">
      <div className="small muted">Your C from memory, cents from a real C (sharp up)</div>
      <svg viewBox={`0 0 ${w} ${h}`} width={w} height={h} role="img" aria-label="Cents from C over time">
        <rect x={0} y={y(25)} width={w} height={y(-25) - y(25)} className="anchor-band" />
        <line x1={0} x2={w} y1={y(0)} y2={y(0)} className="anchor-zero" />
        <path d={path} className="anchor-line" />
        {points.map((p, i) => (
          <circle key={i} cx={x(i)} cy={y(p.cents)} r={3} className="anchor-dot">
            <title>{`${fmtDate(p.at)}: ${p.cents > 0 ? "+" : ""}${Math.round(p.cents)} cents`}</title>
          </circle>
        ))}
      </svg>
    </div>
  );
}

const BAND_TEXT: Record<string, string> = {
  near_ceiling: "Near ceiling on both note memory and 3-back",
  mixed: "Near ceiling on one of note memory and 3-back",
  lower: "Below the thresholds on note memory and 3-back",
};

/** The aptitude screening: each measure's latest run, its trend, and the research reference lines. */
function AptitudeSection({ p }: { p: AptitudeProfile }) {
  const runs = p.inm.length + p.n_back.length + p.span.length;
  const nb = (n: number) => p.n_back.filter((r) => r.n === n);
  const rows: { name: string; unit: string; values: { at: string; v: number }[]; refs: ReferenceLine[]; lowerBetter: boolean; digits: number }[] = [
    {
      name: "Note memory error",
      unit: "cents",
      values: p.inm.map((r) => ({ at: r.at, v: r.mean_abs_error_cents })),
      refs: p.inm_reference.map((r) => ({ ...r, value: r.value * 10, se: r.se != null ? r.se * 10 : null })),
      lowerBetter: true,
      digits: 0,
    },
    { name: "2-back d′", unit: "", values: nb(2).map((r) => ({ at: r.at, v: r.d_prime })), refs: p.n_back_2_reference, lowerBetter: false, digits: 2 },
    { name: "3-back d′", unit: "", values: nb(3).map((r) => ({ at: r.at, v: r.d_prime })), refs: p.n_back_3_reference, lowerBetter: false, digits: 2 },
    {
      name: p.span.length > 0 && p.span[p.span.length - 1].mode === "tones" ? "Span (tone pad)" : "Digit span",
      unit: "",
      values: p.span.map((r) => ({ at: r.at, v: r.span })),
      refs: p.span_reference,
      lowerBetter: false,
      digits: 0,
    },
  ];
  return (
    <div className="section">
      <div className="section-head">
        <h2>Aptitude screening</h2>
        <span className="small faint">
          {runs === 0 ? "Not run yet" : p.capacity_band ? BAND_TEXT[p.capacity_band] ?? p.capacity_band : "Run both note memory and 3-back for a band"}
        </span>
      </div>
      {runs > 0 && (
        <div className="card flush table-scroll">
          <table>
            <thead>
              <tr>
                <th>Measure</th>
                <th>Latest</th>
                <th>Earlier runs</th>
                <th>Reference</th>
              </tr>
            </thead>
            <tbody>
              {rows.map((r) => {
                const last = r.values[r.values.length - 1];
                const fmt = (v: number) => `${v.toFixed(r.digits)}${r.unit ? ` ${r.unit}` : ""}`;
                return (
                  <tr key={r.name}>
                    <td>{r.name}</td>
                    <td className="num">{last ? <span title={fmtDate(last.at)}>{fmt(last.v)}</span> : "–"}</td>
                    <td className="num small muted">
                      {r.values
                        .slice(0, -1)
                        .slice(-4)
                        .map((x) => `${fmt(x.v)} (${fmtDate(x.at)})`)
                        .join(" · ") || "–"}
                    </td>
                    <td className="small muted">
                      {r.refs.map((ref) => `${ref.group}: ${+ref.value.toFixed(2)}${r.unit ? ` ${r.unit}` : ""}`).join("; ")}
                      {r.lowerBetter ? " (lower is better)" : ""}
                    </td>
                  </tr>
                );
              })}
            </tbody>
          </table>
        </div>
      )}
      {p.notes.length > 0 && (
        <ul className="muted small test-notes">
          {p.notes.map((n) => (
            <li key={n}>{n}</li>
          ))}
        </ul>
      )}
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
  onStart: (level: number, minutes: number, checks: boolean) => void;
  onSetLevel: (level: number) => void;
}) {
  const entry = plan.courses.find((p) => p.course === c.id);
  const [minutes, setMinutes] = useState(entry?.minutes ?? 10);
  useEffect(() => setMinutes(entry?.minutes ?? 10), [entry?.minutes]);
  const checks = entry?.checks ?? true;
  const startMinutes = Number.isFinite(minutes) ? Math.min(90, Math.max(1, minutes)) : 10;
  const update = (active: boolean, m: number, check: boolean) =>
    onPlan({
      ...plan,
      courses: plan.courses.some((p) => p.course === c.id)
        ? plan.courses.map((p) => (p.course === c.id ? { ...p, active, minutes: m, checks: check } : p))
        : [...plan.courses, { course: c.id, active, minutes: m, checks: check }],
    });
  const level = Math.min(c.level, c.levels);
  // Today's set runs the active courses in the plan's order; move this one past its active neighbour.
  const move = (dir: -1 | 1) => {
    const list = [...plan.courses];
    const at = list.findIndex((p) => p.course === c.id);
    if (at < 0) return;
    let to = at + dir;
    while (to >= 0 && to < list.length && !list[to].active) to += dir;
    if (to < 0 || to >= list.length) return;
    const [item] = list.splice(at, 1);
    list.splice(to, 0, item);
    onPlan({ ...plan, courses: list });
  };
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
            <input type="checkbox" checked={entry?.active ?? false} onChange={(e) => update(e.target.checked, startMinutes, checks)} />
            Daily
          </label>
          {c.can_drill && (
            <label className="row small" style={{ gap: 6 }} title="End the level with its no-feedback check. Off makes Start level a drill: practice blocks only, and nothing counts towards a pass.">
              <input type="checkbox" checked={checks} onChange={(e) => update(entry?.active ?? false, startMinutes, e.target.checked)} />
              Check
            </label>
          )}
          <input
            className="num"
            type="number"
            min={1}
            max={90}
            value={minutes}
            style={{ width: 56 }}
            onChange={(e) => setMinutes(Number(e.target.value))}
            onBlur={() => minutes !== entry?.minutes && update(entry?.active ?? false, startMinutes, checks)}
            aria-label="Minutes"
            title="Minutes a day, and the length when starting a level from this card"
          />
          <span className="small faint">min</span>
          {entry?.active && (
            <span className="row" style={{ gap: 2 }}>
              <button className="btn small quiet" onClick={() => move(-1)} title="Earlier in today's set" aria-label="Earlier in today's set">
                ↑
              </button>
              <button className="btn small quiet" onClick={() => move(1)} title="Later in today's set" aria-label="Later in today's set">
                ↓
              </button>
            </span>
          )}
          <select value={c.level} onChange={(e) => onSetLevel(Number(e.target.value))} aria-label="Level" title="Set the level by hand" style={{ maxWidth: 170 }}>
            {c.level_titles.map((t, i) => (
              <option key={i} value={i + 1}>
                {i + 1}. {t}
              </option>
            ))}
            <option value={c.levels + 1}>Complete</option>
          </select>
        </div>
        <button
          className="btn small"
          disabled={busy}
          onClick={() => onStart(level, startMinutes, checks)}
          title={checks ? "Run this level with its no-feedback check" : "Practice only: no no-feedback check, and nothing counts towards a level pass"}
        >
          {starting === `${c.id}-${level}-level` || starting === `${c.id}-${level}-drill` ? "Starting…" : c.complete ? `Level ${level}` : checks ? "Start level" : "Drill"}
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
