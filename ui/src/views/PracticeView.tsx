import { useCallback, useEffect, useState } from "react";
import { api, useBackendEvent } from "../api";
import type { AudioView } from "../bindings/AudioView";
import type { PresetView } from "../bindings/PresetView";
import type { RunnerState } from "../bindings/RunnerState";
import type { SessionRow } from "../bindings/SessionRow";
import { fmtDate } from "../music";

const CATEGORIES = ["Absolute pitch", "Ear training", "Singing", "Sight singing and dictation", "Saved"];

/** Graded courses shown as one card with a stage picker. */
interface Course {
  pattern: RegExp;
  title: string;
  description: string;
  unit: string;
  storageKey: string;
  /** Picker label; defaults to the unit and number ("Level 3"). */
  label?: (p: PresetView) => string;
}

const COURSES: Course[] = [
  {
    pattern: /^degrees_(\d+)$/,
    title: "Scale degrees",
    description: "A cadence sets the key, a note plays, you name its degree, and it walks to the tonic.",
    unit: "Level",
    storageKey: "practice.degreeLevel",
  },
  {
    pattern: /^harmony_(\d+)$/,
    title: "Harmony intervals",
    description: "Sing a third, fourth, fifth or sixth from a melody: first with your part playing along, then by ear.",
    unit: "Interval",
    storageKey: "practice.harmonyInterval",
    label: (p) => {
      const name = p.title.replace(/^Harmony:\s*/, "");
      return name.charAt(0).toUpperCase() + name.slice(1);
    },
  },
];

const inCourse = (p: PresetView) => COURSES.some((c) => c.pattern.test(p.id));

function remembered(key: string): string | null {
  try {
    return localStorage.getItem(key);
  } catch {
    return null;
  }
}

export function PracticeView({
  runner,
  audio,
  onOpenSession,
  onOpenAudio,
  onOpenAp,
}: {
  runner: RunnerState | null;
  audio: AudioView | null;
  onOpenSession: () => void;
  onOpenAudio: () => void;
  onOpenAp: () => void;
}) {
  const [presets, setPresets] = useState<PresetView[]>([]);
  const [queue, setQueue] = useState<SessionRow[]>([]);
  const [error, setError] = useState<string | null>(null);
  const [starting, setStarting] = useState<string | null>(null);
  const active = runner != null && runner.phase !== "idle";

  const refresh = useCallback(() => {
    api("listQueue").then(setQueue).catch(() => undefined);
  }, []);

  useEffect(() => {
    api("listPresets").then(setPresets).catch((e) => setError(e.message));
    refresh();
  }, [refresh]);
  useBackendEvent("queue-changed", refresh);

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
      refresh();
    }
  };

  const audioDown = audio != null && !(audio.status?.running ?? false);

  return (
    <div className="page">
      <div className="page-head">
        <div>
          <h1>Practice</h1>
          <div className="sub">Start a session your coach queued, or pick a preset.</div>
        </div>
      </div>

      {active && (
        <div className="notice info row" style={{ justifyContent: "space-between" }}>
          <span>
            <strong>{runner!.title}</strong> is in progress.
          </span>
          <button className="btn small primary" onClick={onOpenSession}>
            Return to session
          </button>
        </div>
      )}
      {audioDown && (
        <div className="notice error row" style={{ justifyContent: "space-between" }}>
          <span>{audio?.error ? `Audio is not running: ${audio.error}` : "Audio is not running."}</span>
          <button className="btn small" onClick={onOpenAudio}>
            Audio settings
          </button>
        </div>
      )}
      {error && <div className="notice error">{error}</div>}

      <div className="section">
        <div className="section-head">
          <h2>Up next</h2>
          <span className="small faint">Sessions queued by your coach over MCP</span>
        </div>
        {queue.length === 0 ? (
          <div className="card empty">Nothing queued. Sessions your coach queues appear here.</div>
        ) : (
          <div className="grid">
            {queue.map((q) => (
              <div key={q.id} className="card row" style={{ alignItems: "flex-start", gap: 16 }}>
                <div className="grow">
                  <div className="title">{q.title}</div>
                  {q.queue_note && <div className="desc">{q.queue_note}</div>}
                  <div className="meta">Queued {fmtDate(q.queued_at)}</div>
                </div>
                <div className="row">
                  <button
                    className="btn quiet small"
                    onClick={() => api("removeQueued", q.id).then(refresh).catch(() => undefined)}
                  >
                    Remove
                  </button>
                  <button
                    className="btn primary"
                    disabled={active || starting != null}
                    onClick={() => start(q.id, { source: "queued", session_id: q.id })}
                  >
                    {starting === q.id ? "Starting…" : "Start"}
                  </button>
                </div>
              </div>
            ))}
          </div>
        )}
      </div>

      {CATEGORIES.map((cat) => {
        const items = presets.filter((p) => p.category === cat && !inCourse(p));
        const courses = COURSES.map((c) => ({
          course: c,
          stages: presets.filter((p) => p.category === cat && c.pattern.test(p.id)),
        })).filter((c) => c.stages.length > 0);
        if (items.length === 0 && courses.length === 0) return null;
        return (
          <div className="section" key={cat}>
            <h2>{cat}</h2>
            <div className="grid cols-3">
              {cat === "Absolute pitch" && (
                <div className="card preset">
                  <div>
                    <div className="title">Today's set</div>
                    <div className="desc">
                      The absolute pitch courses (note naming, the fixed C anchor and the tests) composed into one daily session,
                      with progress on every note.
                    </div>
                  </div>
                  <div className="row" style={{ justifyContent: "space-between" }}>
                    <button className="btn small quiet" onClick={onOpenAp}>
                      Progress and courses
                    </button>
                    <button
                      className="btn small primary"
                      disabled={active || starting != null}
                      onClick={() => start("daily", { source: "daily_set" })}
                    >
                      {starting === "daily" ? "Starting…" : "Start"}
                    </button>
                  </div>
                </div>
              )}
              {courses.map(({ course, stages }) => (
                <StageCard
                  key={course.storageKey}
                  course={course}
                  stages={stages}
                  disabled={active || starting != null}
                  starting={starting}
                  onStart={(id) => start(id, { source: "preset", id })}
                />
              ))}
              {items.map((p) => (
                <div key={p.id} className="card preset">
                  <div>
                    <div className="title">{p.title}</div>
                    <div className="desc">{p.description}</div>
                  </div>
                  <div className="row" style={{ justifyContent: "space-between" }}>
                    <span className="small faint num">
                      {p.trial_count} trials · about {Math.max(1, Math.round(p.estimated_minutes))} min
                    </span>
                    <button
                      className="btn small"
                      disabled={active || starting != null}
                      onClick={() => start(p.id, { source: "preset", id: p.id })}
                    >
                      {starting === p.id ? "Starting…" : "Start"}
                    </button>
                  </div>
                </div>
              ))}
            </div>
          </div>
        );
      })}
    </div>
  );
}

/** A graded course (scale degrees, harmony intervals) as one card with a stage picker. */
function StageCard({
  course,
  stages,
  disabled,
  starting,
  onStart,
}: {
  course: Course;
  stages: PresetView[];
  disabled: boolean;
  starting: string | null;
  onStart: (id: string) => void;
}) {
  const sorted = [...stages].sort((a, b) => a.id.localeCompare(b.id));
  const [id, setId] = useState(() => {
    const saved = remembered(course.storageKey);
    return sorted.some((p) => p.id === saved) ? saved! : sorted[0].id;
  });
  const p = sorted.find((x) => x.id === id) ?? sorted[0];
  const stageNo = (x: PresetView) => parseInt(course.pattern.exec(x.id)?.[1] ?? "0", 10);
  return (
    <div className="card preset">
      <div>
        <div className="title">{course.title}</div>
        <div className="desc">{course.description}</div>
        <label className="field" style={{ marginTop: 10 }}>
          <select
            value={p.id}
            onChange={(e) => {
              setId(e.target.value);
              try {
                localStorage.setItem(course.storageKey, e.target.value);
              } catch {
                // Remembering the stage is only a convenience.
              }
            }}
          >
            {sorted.map((x) => (
              <option key={x.id} value={x.id}>
                {course.label ? course.label(x) : `${course.unit} ${stageNo(x)}`}
              </option>
            ))}
          </select>
        </label>
        <div className="desc" style={{ marginTop: 6 }}>
          {p.description}
        </div>
      </div>
      <div className="row" style={{ justifyContent: "space-between" }}>
        <span className="small faint num">
          {p.trial_count} trials · about {Math.max(1, Math.round(p.estimated_minutes))} min
        </span>
        <button className="btn small" disabled={disabled} onClick={() => onStart(p.id)}>
          {starting === p.id ? "Starting…" : "Start"}
        </button>
      </div>
    </div>
  );
}
