import { useCallback, useEffect, useState } from "react";
import { api, useBackendEvent } from "../api";
import type { AudioView } from "../bindings/AudioView";
import type { PresetView } from "../bindings/PresetView";
import type { RunnerState } from "../bindings/RunnerState";
import type { SessionRow } from "../bindings/SessionRow";
import { fmtDate } from "../music";

const CATEGORIES = ["Absolute pitch", "Ear training", "Singing", "Saved"];

export function PracticeView({
  runner,
  audio,
  onOpenSession,
  onOpenAudio,
}: {
  runner: RunnerState | null;
  audio: AudioView | null;
  onOpenSession: () => void;
  onOpenAudio: () => void;
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
        const items = presets.filter((p) => p.category === cat);
        if (items.length === 0) return null;
        return (
          <div className="section" key={cat}>
            <h2>{cat}</h2>
            <div className="grid cols-3">
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
