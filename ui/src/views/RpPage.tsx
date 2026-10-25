import { type ReactNode, useCallback, useEffect, useState } from "react";
import { api, useBackendEvent } from "../api";
import type { CourseStatus } from "../bindings/CourseStatus";
import type { DailyPlan } from "../bindings/DailyPlan";
import type { DailySet } from "../bindings/DailySet";
import { WorkshopPage } from "./WorkshopPage";

const CATEGORY = "Relative pitch";

type Glyph = "ear" | "compare" | "voice" | "eye" | "pencil" | "search" | "home" | "memory" | "core";

interface Activity {
  /** The workshop course; none for activities not built yet. */
  course?: string;
  title: string;
  desc: string;
  glyph: Glyph;
}

interface Section {
  title: string;
  accent: string;
  activities: Activity[];
}

const SECTIONS: Section[] = [
  {
    title: "Core",
    accent: "#1f6f5c",
    activities: [
      {
        course: "rp_core",
        title: "Core relative pitch",
        desc: "The daily core: tonality first, then tonal patterns and working memory, inner hearing and lines, then structure and error detection, a few short parts each session",
        glyph: "core",
      },
    ],
  },
  {
    title: "Tonal context",
    accent: "#d9822b",
    activities: [
      { course: "rp_home", title: "Find home", desc: "Sing or name the resting tone of a phrase that stops", glyph: "home" },
      { course: "rp_degrees", title: "Scale degrees", desc: "Name each note's degree in a key, after a cadence and a breath", glyph: "ear" },
      { course: "rp_patterns", title: "Tonal patterns", desc: "Echo tonic and dominant patterns after a breath; answer in dialogue", glyph: "voice" },
    ],
  },
  {
    title: "Memory and inner hearing",
    accent: "#0f7c8c",
    activities: [
      { course: "rp_memory", title: "Working memory", desc: "Same or different, probe notes, N-back and canon, changing a melody in your head", glyph: "memory" },
      { course: "rp_inner", title: "Inner hearing", desc: "Silent bars, silent beats, flash scores sung from memory", glyph: "eye" },
    ],
  },
  {
    title: "Intervals",
    accent: "#2d6cdf",
    activities: [
      { course: "rp_intervals_key", title: "Intervals in a key", desc: "Diatonic and chromatic intervals against a cadence", glyph: "ear" },
      { course: "rp_sing", title: "Interval singing", desc: "Sing a degree from do, up or down", glyph: "voice" },
      { course: "rp_intervals", title: "Interval identification", desc: "Name intervals on random notes, no key", glyph: "ear" },
      { course: "rp_compare", title: "Interval comparison", desc: "Which of two intervals is larger", glyph: "compare" },
    ],
  },
  {
    title: "Chords and harmony",
    accent: "#b88a00",
    activities: [
      { course: "rp_harmony", title: "Cadences and bass lines", desc: "How phrases end; sing the bass and the guide tones", glyph: "ear" },
      { course: "rp_chords", title: "Chord identification", desc: "Name triads and seventh chords", glyph: "ear" },
      { course: "rp_inversions", title: "Chord inversions", desc: "Name the chord and its inversion", glyph: "ear" },
    ],
  },
  {
    title: "Scales",
    accent: "#c2417f",
    activities: [{ course: "rp_scales", title: "Scale identification", desc: "Name scales and modes by ear", glyph: "ear" }],
  },
  {
    title: "Melodies",
    accent: "#7a4fd1",
    activities: [
      { course: "rp_singback", title: "Melody singback", desc: "Hear a melody, sing it back", glyph: "voice" },
      { course: "rp_sight", title: "Melodic sight singing", desc: "Sing a melody from the staff", glyph: "eye" },
      { course: "rp_dictation", title: "Melodic dictation", desc: "Write down a melody you hear", glyph: "pencil" },
      { course: "rp_errors", title: "Error detection", desc: "Find the note played wrong, heard once; the wrong syllable", glyph: "search" },
    ],
  },
];

function Icon({ glyph }: { glyph: Glyph }) {
  const common = { fill: "none", stroke: "currentColor", strokeWidth: 2, strokeLinecap: "round" as const, strokeLinejoin: "round" as const };
  const paths: Record<Glyph, ReactNode> = {
    ear: (
      <path {...common} d="M7 10a5 5 0 0 1 10 0c0 3-3 4-3 7a3 3 0 0 1-5 2M10 10a2 2 0 0 1 4 0c0 1.5-2 2-2 3.5" />
    ),
    compare: <path {...common} d="M12 4v16M5 20h14M4 9l3-5 3 5a3 3 0 0 1-6 0zM14 9l3-5 3 5a3 3 0 0 1-6 0zM7 4h10" />,
    voice: <path {...common} d="M12 3a3 3 0 0 0-3 3v6a3 3 0 0 0 6 0V6a3 3 0 0 0-3-3zM6 11a6 6 0 0 0 12 0M12 17v4M9 21h6" />,
    eye: <path {...common} d="M2 12s4-7 10-7 10 7 10 7-4 7-10 7S2 12 2 12zM12 15a3 3 0 1 0 0-6 3 3 0 0 0 0 6z" />,
    pencil: <path {...common} d="M4 20l4-1 11-11-3-3L5 16l-1 4zM14 6l3 3M4 22h16" />,
    home: <path {...common} d="M3 11l9-7 9 7M5 9.5V20h14V9.5M10 20v-6h4v6" />,
    memory: <path {...common} d="M9 4a3 3 0 0 0-3 3 3 3 0 0 0-2 5 3 3 0 0 0 2 5 3 3 0 0 0 3 3h0a3 3 0 0 0 3-3V7a3 3 0 0 0-3-3zM15 4a3 3 0 0 1 3 3 3 3 0 0 1 2 5 3 3 0 0 1-2 5 3 3 0 0 1-3 3h0a3 3 0 0 1-3-3" />,
    core: <path {...common} d="M12 3l2.6 5.3 5.9.9-4.3 4.1 1 5.8L12 16.4 6.8 19.1l1-5.8L3.5 9.2l5.9-.9z" />,
    search: <path {...common} d="M11 18a7 7 0 1 0 0-14 7 7 0 0 0 0 14zM16 16l5 5" />,
  };
  return (
    <svg viewBox="0 0 24 24" width="30" height="30" aria-hidden="true">
      {paths[glyph]}
    </svg>
  );
}

function remembered(): string | null {
  try {
    return localStorage.getItem("rp.open");
  } catch {
    return null;
  }
}

function remember(v: string | null) {
  try {
    if (v) localStorage.setItem("rp.open", v);
    else localStorage.removeItem("rp.open");
  } catch {
    /* private window */
  }
}

/** Relative pitch: today's set, and the activities it is drawn from, each opening its lessons. */
export function RpPage({ sessionActive, onOpenSession }: { sessionActive: boolean; onOpenSession: () => void }) {
  const [courses, setCourses] = useState<CourseStatus[]>([]);
  const [plan, setPlan] = useState<DailyPlan | null>(null);
  const [set, setSet] = useState<DailySet | null>(null);
  const [setProblem, setSetProblem] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [starting, setStarting] = useState<string | null>(null);
  const [open, setOpenState] = useState<string | null>(remembered);
  const setOpen = (v: string | null) => {
    setOpenState(v);
    remember(v);
  };

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

  const openSection = SECTIONS.find((s) => s.activities.some((a) => a.course === open));
  if (open && openSection) {
    return (
      <WorkshopPage
        course={open}
        accent={openSection.accent}
        plan={plan}
        busy={busy}
        onPlan={savePlan}
        onBack={() => setOpen(null)}
        onStart={start}
      />
    );
  }

  return (
    <div className="page">
      <div className="page-head">
        <div>
          <h1>Relative pitch</h1>
          <div className="sub">
            Start with the core: short mixed sessions in four tiers, from tonality to inner hearing and error detection. Every
            activity is also a course of modules and lessons of its own; tick Daily inside one to add its current lesson to
            today's set.
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
            <button className="btn primary" disabled={busy || !set} onClick={() => start("daily", { source: "daily_set", category: CATEGORY })}>
              {starting === "daily" ? "Starting…" : "Start today's set"}
            </button>
          </div>
        </div>
      </div>

      {SECTIONS.map((s) => (
        <div key={s.title} className="activity-section" style={{ ["--activity" as string]: s.accent }}>
          <h2>{s.title}</h2>
          <div className="activity-grid">
            {s.activities.map((a) => {
              const c = a.course ? courses.find((x) => x.id === a.course) : undefined;
              const daily = plan?.courses.find((p) => p.course === a.course)?.active ?? false;
              const lesson = c ? Math.min(c.level, c.levels) : 0;
              return (
                <button key={a.title} className={`activity${a.course ? "" : " soon"}${a.course === "rp_core" ? " wide" : ""}`} disabled={!a.course} onClick={() => a.course && setOpen(a.course)}>
                  <span className="activity-icon">
                    <Icon glyph={a.glyph} />
                  </span>
                  <span className="activity-text">
                    <span className="activity-title">{a.title}</span>
                    <span className="activity-desc">{a.desc}</span>
                    {c ? (
                      <span className="activity-meta">
                        {c.complete ? "Complete" : `Lesson ${lesson} of ${c.levels}`}
                        {c.level_title && !c.complete ? ` · ${c.level_title}` : ""}
                        {daily && <span className="badge accent">daily</span>}
                      </span>
                    ) : (
                      !a.course && <span className="activity-meta">Not built yet</span>
                    )}
                  </span>
                </button>
              );
            })}
          </div>
        </div>
      ))}
    </div>
  );
}
