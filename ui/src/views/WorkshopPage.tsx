import { useCallback, useEffect, useMemo, useState } from "react";
import { api, useBackendEvent } from "../api";
import type { CustomLevel } from "../bindings/CustomLevel";
import type { CustomLevelView } from "../bindings/CustomLevelView";
import type { DailyPlan } from "../bindings/DailyPlan";
import type { LessonView } from "../bindings/LessonView";
import type { WorkshopView } from "../bindings/WorkshopView";
import { LevelForm } from "../components/LevelForm";
import { ExerciseSettings } from "../components/ExerciseSettings";
import { fmtDate } from "../music";

const pct = (x: number | null | undefined) => (x == null ? "–" : `${Math.round(x * 100)}%`);

function scoreClass(x: number | null | undefined, mark: number) {
  if (x == null) return "";
  return x + 1e-9 >= mark ? "good" : x >= mark - 0.15 ? "close" : "low";
}

function remembered(key: string): string | null {
  try {
    return localStorage.getItem(key);
  } catch {
    return null;
  }
}

function remember(key: string, value: string) {
  try {
    localStorage.setItem(key, value);
  } catch {
    /* private window */
  }
}

/** One activity: its lessons by module, its custom levels, and its place in today's set. */
export function WorkshopPage({
  course,
  accent,
  plan,
  busy,
  onPlan,
  onBack,
  onStart,
}: {
  course: string;
  accent: string;
  plan: DailyPlan | null;
  busy: boolean;
  onPlan: (p: DailyPlan) => void;
  onBack: () => void;
  onStart: (key: string, req: Parameters<typeof api<"startSession">>[1]) => void;
}) {
  const [view, setView] = useState<WorkshopView | null>(null);
  const [custom, setCustom] = useState<CustomLevelView[]>([]);
  const [error, setError] = useState<string | null>(null);
  const [tabChoice, setTab] = useState<"lessons" | "custom">(() => (remembered(`rp.tab.${course}`) === "custom" ? "custom" : "lessons"));
  const [selected, setSelected] = useState<number | null>(null);
  const [open, setOpen] = useState<Set<string>>(new Set());
  const [editing, setEditing] = useState<CustomLevel | null>(null);
  const [settings, setSettings] = useState(false);

  const refresh = useCallback(() => {
    api("workshopView", course)
      .then((v) => {
        setView(v);
        setError(null);
      })
      .catch((e) => setError(e.message));
    api("customLevels", course)
      .then(setCustom)
      .catch((e) => setError(e.message));
  }, [course]);
  useEffect(refresh, [refresh]);
  useBackendEvent("courses-changed", refresh);

  const tab = view && !view.custom_levels ? "lessons" : tabChoice;
  const lessons = useMemo(() => view?.modules.flatMap((m) => m.lessons) ?? [], [view]);
  const current = view ? Math.min(view.current, view.lessons) : 1;
  const chosen = selected ?? current;
  const lesson: LessonView | undefined = lessons.find((l) => l.level === chosen);
  // Open the module holding the current lesson the first time the view arrives.
  useEffect(() => {
    if (!view || open.size > 0) return;
    const m = view.modules.find((m) => m.lessons.some((l) => l.level === current));
    if (m) setOpen(new Set([m.title]));
  }, [view, current, open.size]);

  const switchTab = (t: "lessons" | "custom") => {
    setTab(t);
    remember(`rp.tab.${course}`, t);
  };

  const entry = plan?.courses.find((p) => p.course === course);
  const [minutes, setMinutes] = useState(entry?.minutes ?? 8);
  useEffect(() => setMinutes(entry?.minutes ?? 8), [entry?.minutes]);
  const updatePlan = (active: boolean, m: number) => {
    if (!plan) return;
    onPlan({
      ...plan,
      courses: plan.courses.some((p) => p.course === course)
        ? plan.courses.map((p) => (p.course === course ? { ...p, active, minutes: m } : p))
        : [...plan.courses, { course, active, minutes: m }],
    });
  };

  const numberOf = (level: number) => {
    if (!view) return "";
    let mi = 0;
    for (const m of view.modules) {
      mi += 1;
      const k = m.lessons.findIndex((l) => l.level === level);
      if (k >= 0) return `${mi}.${k + 1}`;
    }
    return "";
  };

  const startLesson = (level: number) => onStart(`${course}-${level}`, { source: "course_level", course, level });

  const newLevel = (from?: LessonView) =>
    setEditing({
      id: "",
      course,
      title: from ? `${view?.modules.find((m) => m.lessons.includes(from))?.title ?? ""} – ${from.title} (custom)`.replace(/^ – /, "") : "New level",
      count: from?.questions ?? 20,
      params: structuredClone(from?.params ?? {}),
    });

  const save = async (startAfter: boolean) => {
    if (!editing) return;
    setError(null);
    try {
      const saved = await api("saveCustomLevel", editing);
      setEditing(null);
      switchTab("custom");
      refresh();
      if (startAfter) onStart(`custom-${saved.id}`, { source: "custom", level: saved });
    } catch (e) {
      setError((e as Error).message);
    }
  };

  if (!view) {
    return (
      <div className="page">
        <button className="btn quiet small back" onClick={onBack}>
          ← All activities
        </button>
        {error ? <div className="notice error">{error}</div> : <div className="small faint">Loading…</div>}
      </div>
    );
  }

  return (
    <div className="page workshop" style={{ ["--activity" as string]: accent }}>
      <div>
        <button className="btn quiet small back" onClick={onBack}>
          ← All activities
        </button>
        <div className="page-head" style={{ marginTop: 8 }}>
          <div>
            <h1>{view.title}</h1>
            <div className="sub">{view.description}</div>
          </div>
          <div style={{ textAlign: "right" }}>
            <div className="big-num num">
              {view.passed}/{view.lessons}
            </div>
            <div className="small faint">lessons passed</div>
          </div>
        </div>
      </div>
      {error && <div className="notice error">{error}</div>}

      <div className="tabs">
        <button className={tab === "lessons" ? "on" : ""} onClick={() => switchTab("lessons")}>
          Lessons
        </button>
        {view.custom_levels && (
          <button className={tab === "custom" ? "on" : ""} onClick={() => switchTab("custom")}>
            Custom levels{custom.length > 0 ? ` (${custom.length})` : ""}
          </button>
        )}
        <span className="grow" />
        <button className={settings ? "on" : ""} onClick={() => setSettings((x) => !x)} title="Tempo and other settings for every lesson and today's set">
          Settings
        </button>
      </div>

      {settings && <ExerciseSettings kinds={view.exercises} onClose={() => setSettings(false)} />}

      {editing ? (
        <div className="card level-editor">
          <div className="row" style={{ justifyContent: "space-between", gap: 12 }}>
            <input
              className="level-title-input"
              value={editing.title}
              onChange={(e) => setEditing({ ...editing, title: e.target.value })}
              aria-label="Level name"
            />
            <label className="row small" style={{ gap: 6 }}>
              Questions
              <input
                className="num"
                type="number"
                min={1}
                max={200}
                value={editing.count}
                onChange={(e) => setEditing({ ...editing, count: Number(e.target.value) })}
                style={{ width: 64 }}
              />
            </label>
          </div>
          <LevelForm exercise={view.exercise} params={editing.params} onChange={(params) => setEditing({ ...editing, params })} />
          <div className="row" style={{ justifyContent: "flex-end", gap: 8 }}>
            <button className="btn quiet" onClick={() => setEditing(null)}>
              Cancel
            </button>
            <button
              className="btn"
              disabled={busy}
              onClick={() => onStart("custom-try", { source: "custom", level: { ...editing, id: "" } })}
              title="Play these settings once without saving them"
            >
              Try once
            </button>
            <button className="btn" onClick={() => save(false)}>
              Save
            </button>
            <button className="btn primary" disabled={busy} onClick={() => save(true)}>
              Save and start
            </button>
          </div>
        </div>
      ) : tab === "lessons" ? (
        <>
          <div className="card flush lesson-tree" role="tree">
            {view.modules.map((m, mi) => {
              const isOpen = open.has(m.title);
              const share = m.lessons.length ? m.passed / m.lessons.length : 0;
              const hasCurrent = m.lessons.some((l) => l.level === current);
              return (
                <div key={m.title} className="module" role="treeitem" aria-expanded={isOpen}>
                  <button
                    className={`module-row${hasCurrent ? " has-current" : ""}`}
                    onClick={() => {
                      const next = new Set(open);
                      if (isOpen) next.delete(m.title);
                      else next.add(m.title);
                      setOpen(next);
                    }}
                  >
                    <span className="caret">{isOpen ? "▾" : "▸"}</span>
                    <span className={`mark ${share === 1 ? "done" : m.passed > 0 || hasCurrent ? "doing" : ""}`}>{share === 1 ? "✓" : m.passed > 0 || hasCurrent ? "→" : ""}</span>
                    <span className="grow">
                      {mi + 1}: {m.title}
                    </span>
                    {m.passed > 0 && <span className={`small num ${share === 1 ? "good" : "close"}`}>Completed {Math.round(share * 100)}%</span>}
                  </button>
                  {isOpen &&
                    m.lessons.map((l, li) => (
                      <button
                        key={l.level}
                        className={`lesson-row${l.level === chosen ? " selected" : ""}${l.level === current ? " current" : ""}`}
                        onClick={() => setSelected(l.level)}
                        onDoubleClick={() => !busy && startLesson(l.level)}
                        title={l.summary}
                      >
                        <span className={`mark ${l.passed ? "done" : l.level === current ? "doing" : ""}`}>{l.passed ? "✓" : l.level === current ? "→" : ""}</span>
                        <span className="grow">
                          {mi + 1}.{li + 1} {l.title}
                          {l.test && <span className="badge" style={{ marginLeft: 8 }}>test</span>}
                        </span>
                        {l.runs > 0 && l.best_d_prime != null && (
                          <span className={`small num ${l.passed ? "good" : "close"}`}>d′ {l.best_d_prime.toFixed(1)}</span>
                        )}
                        {l.runs > 0 && l.best_d_prime == null && <span className={`small num ${scoreClass(l.best, view.pass_mark)}`}>Score {pct(l.best)}</span>}
                      </button>
                    ))}
                </div>
              );
            })}
          </div>
        </>
      ) : (
        <div className="section">
          <div className="row" style={{ justifyContent: "space-between" }}>
            <span className="small faint">Your own levels: any settings, scored like lessons. They are not part of the course.</span>
            <button className="btn primary small" onClick={() => newLevel()}>
              New level
            </button>
          </div>
          {custom.length === 0 ? (
            <div className="card muted-card small faint">
              No custom levels yet. Start one from scratch, or pick a lesson and choose “Customise this lesson”.
            </div>
          ) : (
            <div className="grid cols-2">
              {custom.map((c) => (
                <div key={c.level.id} className="card custom-level">
                  <div className="row" style={{ justifyContent: "space-between" }}>
                    <div className="title">{c.level.title}</div>
                    {c.best != null && <span className={`score-pill ${scoreClass(c.best, view.pass_mark)}`}>{pct(c.best)}</span>}
                  </div>
                  <div className="desc">
                    {c.level.count} questions{c.runs > 0 ? ` · ${c.runs} run${c.runs > 1 ? "s" : ""}, last ${pct(c.last)}${c.last_at ? ` on ${fmtDate(c.last_at)}` : ""}` : " · not played yet"}
                  </div>
                  <div className="row" style={{ justifyContent: "flex-end", gap: 6, marginTop: 10 }}>
                    <button
                      className="btn quiet small"
                      onClick={() =>
                        api("deleteCustomLevel", c.level.id)
                          .then(refresh)
                          .catch((e) => setError(e.message))
                      }
                    >
                      Delete
                    </button>
                    <button className="btn small" onClick={() => setEditing(structuredClone(c.level))}>
                      Edit
                    </button>
                    <button className="btn small primary" disabled={busy} onClick={() => onStart(`custom-${c.level.id}`, { source: "custom", level: c.level })}>
                      Start
                    </button>
                  </div>
                </div>
              ))}
            </div>
          )}
        </div>
      )}

      {!editing && tab === "lessons" && lesson && (
        <div className="lesson-bar">
          <div className="grow">
            <div className="small faint">{chosen === current ? "Current lesson" : "Selected lesson"}</div>
            <div className="lesson-bar-title">
              {numberOf(chosen)} {lesson.title}
            </div>
            <div className="small faint">
              {lesson.summary}{" "}
              {lesson.parts > 1
                ? ""
                : lesson.detection
                  ? `${lesson.questions} stream${lesson.questions > 1 ? "s" : ""}, d′ 2 to pass.`
                  : `${lesson.questions} questions, ${Math.round(view.pass_mark * 100)}% to pass${lesson.test ? ", no feedback until the end" : ""}.`}
              {lesson.runs > 0 && lesson.best_d_prime != null && ` Best d′ ${lesson.best_d_prime.toFixed(1)}, last ${lesson.last_d_prime?.toFixed(1)}.`}
              {lesson.runs > 0 && lesson.best_d_prime == null && ` Best ${pct(lesson.best)}, last ${pct(lesson.last)}.`}
            </div>
          </div>
          <div className="lesson-bar-actions">
            {plan && (
              <label className="row small" style={{ gap: 6 }} title="Put this activity's current lesson in today's set">
                <input type="checkbox" checked={entry?.active ?? false} onChange={(e) => updatePlan(e.target.checked, minutes)} />
                Daily
                <input
                  className="num"
                  type="number"
                  min={1}
                  max={90}
                  value={minutes}
                  style={{ width: 52 }}
                  onChange={(e) => setMinutes(Number(e.target.value))}
                  onBlur={() => minutes !== entry?.minutes && updatePlan(entry?.active ?? false, Math.min(90, Math.max(1, minutes)))}
                  aria-label="Minutes a day"
                />
                min
              </label>
            )}
            {view.custom_levels && (
              <button className="btn quiet small" onClick={() => newLevel(lesson)}>
                Customise this lesson
              </button>
            )}
            {chosen !== current && (
              <button
                className="btn small"
                onClick={() =>
                  api("setCourseLevel", course, chosen)
                    .then(refresh)
                    .catch((e) => setError(e.message))
                }
                title="Today's set plays the current lesson"
              >
                Make current
              </button>
            )}
            <button className="btn primary" disabled={busy} onClick={() => startLesson(chosen)}>
              Start
            </button>
          </div>
        </div>
      )}
    </div>
  );
}
