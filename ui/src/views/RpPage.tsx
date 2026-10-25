import { useCallback, useEffect, useState } from "react";
import { api, useBackendEvent } from "../api";
import type { CourseStatus } from "../bindings/CourseStatus";
import type { DailyPlan } from "../bindings/DailyPlan";
import type { DailySet } from "../bindings/DailySet";
import { LadderCard } from "./ApPage";

const CATEGORY = "Relative pitch";

/** Relative pitch: its own daily set and the course ladders it is built from. */
export function RpPage({ sessionActive, onOpenSession }: { sessionActive: boolean; onOpenSession: () => void }) {
  const [courses, setCourses] = useState<CourseStatus[]>([]);
  const [plan, setPlan] = useState<DailyPlan | null>(null);
  const [set, setSet] = useState<DailySet | null>(null);
  const [setProblem, setSetProblem] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [starting, setStarting] = useState<string | null>(null);

  const refresh = useCallback(() => {
    api("apDashboard")
      .then((d) => {
        setCourses(d.courses.filter((c) => c.category === CATEGORY));
        setPlan(d.plan);
      })
      .catch((e) => setError(e.message));
    api("previewDailySet", CATEGORY)
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

  const savePlan = (p: DailyPlan) => {
    api("saveDailyPlan", p)
      .then(refresh)
      .catch((e) => setError(e.message));
  };
  const busy = sessionActive || starting != null;

  return (
    <div className="page">
      <div className="page-head">
        <div>
          <h1>Relative pitch</h1>
          <div className="sub">
            Scale degrees and intervals, in a key and without one, heard, compared and sung. Each course climbs a level at a
            time: practice with feedback, then a check without it; two checks in a row at 90% move it on.
          </div>
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
            <button
              className="btn primary"
              disabled={busy || !set}
              onClick={() => start("daily", { source: "daily_set", category: CATEGORY })}
            >
              {starting === "daily" ? "Starting…" : "Start today's set"}
            </button>
          </div>
        </div>
      </div>

      {plan && (
        <div className="section">
          <div className="section-head">
            <h2>Courses</h2>
            <span className="small faint">Tick Daily to put a course in today's set</span>
          </div>
          <div className="grid cols-2">
            {courses.map((c) => (
              <LadderCard
                key={c.id}
                c={c}
                plan={plan}
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
          </div>
        </div>
      )}
    </div>
  );
}
