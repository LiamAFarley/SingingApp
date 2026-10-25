import { lazy, Suspense, useCallback, useEffect, useRef, useState } from "react";
import { api, isTauri, useBackendEvent } from "./api";
import type { AudioView } from "./bindings/AudioView";
import type { RunnerState } from "./bindings/RunnerState";
import { live } from "./live";
import { ApPage } from "./views/ApPage";
import { RpPage } from "./views/RpPage";
import { AudioPage } from "./views/AudioPage";
import { LibraryPage } from "./views/LibraryPage";
import { PracticeView } from "./views/PracticeView";
import { ResultsView } from "./views/ResultsView";
import { SessionView } from "./views/SessionView";
import { SettingsPage } from "./views/SettingsPage";
import { TunerView } from "./views/TunerView";

type View = "practice" | "ap" | "rp" | "session" | "library" | "tuner" | "results" | "audio" | "settings";

// Browser-only renderer check page; never part of the desktop app.
const NotationGallery = lazy(() => import("./views/NotationGallery").then((m) => ({ default: m.NotationGallery })));
const galleryMode = !isTauri && typeof location !== "undefined" && new URLSearchParams(location.search).get("scenario") === "notation";

const NAV: [View, string][] = [
  ["practice", "Practice"],
  ["ap", "Absolute pitch"],
  ["rp", "Relative pitch"],
  ["library", "Library"],
  ["tuner", "Tuner"],
  ["results", "Results"],
  ["audio", "Audio"],
  ["settings", "Settings"],
];

export function App() {
  const [view, setView] = useState<View>("practice");
  const [runner, setRunner] = useState<RunnerState | null>(null);
  const [audio, setAudio] = useState<AudioView | null>(null);
  const [a4, setA4] = useState(440);
  const [notice, setNotice] = useState<string | null>(null);
  const prevPhase = useRef<string>("idle");

  const refreshAudio = useCallback(() => {
    api("audioState").then(setAudio).catch(() => undefined);
  }, []);

  useEffect(() => {
    live.start();
    api("runnerState").then(setRunner).catch(() => undefined);
    api("getSettings").then((s) => setA4(s.a4_hz)).catch(() => undefined);
    refreshAudio();
    const t = setInterval(refreshAudio, 2000);
    return () => clearInterval(t);
  }, [refreshAudio]);
  useBackendEvent("runner-state", (s) => {
    const next = s as RunnerState;
    setRunner((prev) => (prev && prev.seq > next.seq && prev.session_id === next.session_id ? prev : next));
  });
  useBackendEvent("audio-changed", refreshAudio);

  // Jump to the session screen when a session starts; leave it when it ends.
  useEffect(() => {
    const phase = runner?.phase ?? "idle";
    if (prevPhase.current === "idle" && phase !== "idle") setView("session");
    if (prevPhase.current !== "idle" && phase === "idle") setView((v) => (v === "session" ? "practice" : v));
    prevPhase.current = phase;
  }, [runner?.phase]);

  const active = runner != null && runner.phase !== "idle";
  const running = audio?.status?.running ?? false;

  return (
    <div className="app">
      <nav className="nav">
        <div className="nav-title">Singing Trainer</div>
        {active && (
          <button className={view === "session" ? "active" : ""} onClick={() => setView("session")}>
            Session
            <span className="dot on" />
          </button>
        )}
        {NAV.map(([id, label]) => (
          <button key={id} className={view === id ? "active" : ""} onClick={() => setView(id)}>
            {label}
          </button>
        ))}
        <div className="spacer" />
        <div className="status-line">
          <span className={`dot ${running ? "on" : audio ? "off" : ""}`} />
          {running ? `${audio?.info?.host ?? ""} · ${(audio?.info?.sample_rate ?? 48000) / 1000} kHz` : "Audio stopped"}
        </div>
        {running && audio?.info && <div className="status-line">{audio.info.output_device}</div>}
      </nav>
      <main className="main">
        {galleryMode && (
          <Suspense fallback={null}>
            <NotationGallery />
          </Suspense>
        )}
        {notice && (
          <div className="notice error app-notice">
            <span>{notice}</span>
            <button className="btn small quiet" onClick={() => setNotice(null)}>
              Dismiss
            </button>
          </div>
        )}
        {view === "session" && runner && active && <SessionView runner={runner} onClose={() => setView("practice")} />}
        {view === "session" && !active && (
          <PracticeView runner={runner} audio={audio} onOpenSession={() => setView("session")} onOpenAudio={() => setView("audio")} onOpenAp={() => setView("ap")} onOpenRp={() => setView("rp")} />
        )}
        {view === "practice" && !galleryMode && (
          <PracticeView runner={runner} audio={audio} onOpenSession={() => setView("session")} onOpenAudio={() => setView("audio")} onOpenAp={() => setView("ap")} onOpenRp={() => setView("rp")} />
        )}
        {view === "ap" && <ApPage sessionActive={active} onOpenSession={() => setView("session")} />}
        {view === "rp" && <RpPage sessionActive={active} onOpenSession={() => setView("session")} />}
        {view === "library" && <LibraryPage sessionActive={active} onOpenSession={() => setView("session")} />}
        {view === "tuner" && <TunerView a4={a4} />}
        {view === "results" && (
          <ResultsView
            onRepeat={(id) => {
              api("startSession", { source: "repeat", session_id: id })
                .then(() => setView("session"))
                .catch((e) => setNotice(e.message));
            }}
          />
        )}
        {view === "audio" && <AudioPage audio={audio} onChanged={refreshAudio} />}
        {view === "settings" && <SettingsPage onSaved={(s) => setA4(s.a4_hz)} />}
      </main>
    </div>
  );
}
