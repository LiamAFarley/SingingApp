import { useEffect, useState } from "react";
import { api } from "../api";
import type { BlockOutline } from "../bindings/BlockOutline";
import type { RunnerState } from "../bindings/RunnerState";
import type { SessionRow } from "../bindings/SessionRow";
import { pct } from "../music";

/** Results may be shown for a block once it is done, unless they are kept for the session's end. */
function score(b: BlockOutline): string | null {
  if (b.done === 0) return null;
  const hidden = b.feedback === "end_of_session" || (b.status === "current" && (b.feedback === "none" || b.feedback === "after_block"));
  return hidden ? `${b.done} answered` : `${b.correct} / ${b.done} · ${pct(b.correct / b.done)}`;
}

function sameDay(iso: string | null, day: Date): boolean {
  if (!iso) return false;
  const d = new Date(iso);
  return d.getFullYear() === day.getFullYear() && d.getMonth() === day.getMonth() && d.getDate() === day.getDate();
}

/** What this session is made of, block by block, and what else was done today. */
export function SessionOverview({ runner, onClose }: { runner: RunnerState; onClose: () => void }) {
  const [earlier, setEarlier] = useState<SessionRow[] | null>(null);
  useEffect(() => {
    const today = new Date();
    api("listSessions", 60)
      .then((rows: SessionRow[]) => setEarlier(rows.filter((r) => r.id !== runner.session_id && sameDay(r.started_at, today))))
      .catch(() => setEarlier([]));
  }, [runner.session_id]);

  return (
    <div className="overview card">
      <div className="row" style={{ justifyContent: "space-between" }}>
        <h2 style={{ margin: 0 }}>This session</h2>
        <button className="btn quiet" onClick={onClose}>
          Close <span className="kbd">V</span>
        </button>
      </div>
      <ol className="overview-blocks">
        {runner.outline.map((b, i) => {
          const s = score(b);
          return (
            <li key={i} className={`overview-block ${b.status}`}>
              <div className="row" style={{ justifyContent: "space-between", gap: 8 }}>
                <strong>
                  {i + 1}. {b.title}
                </strong>
                <span className={`badge ${b.status === "current" ? "accent" : ""}`}>
                  {b.status === "current" ? "Now" : b.status === "done" ? "Done" : "Next"}
                  {s && ` · ${s}`}
                </span>
              </div>
              <div className="small muted">
                {b.kind} · {b.trials} {b.trials === 1 ? "trial" : "trials"}
                {b.check ? " · counts as a check or test" : ""}
                {b.feedback === "none" ? " · no feedback" : b.feedback === "errorless" ? " · errorless" : ""}
              </div>
              {b.course && (
                <div className="small" style={{ marginTop: 4 }}>
                  <strong>{b.course}</strong>
                  {b.level != null && `, level ${b.level}`}
                  {b.level_summary && `: ${b.level_summary}`}
                </div>
              )}
              {b.course_about && <div className="small muted">{b.course_about}</div>}
              {b.instructions && <div className="small" style={{ marginTop: 4 }}>{b.instructions}</div>}
            </li>
          );
        })}
      </ol>
      <h3 style={{ marginBottom: 6 }}>Earlier today</h3>
      {earlier == null ? (
        <div className="small muted">Loading…</div>
      ) : earlier.length === 0 ? (
        <div className="small muted">Nothing else yet today.</div>
      ) : (
        <ul className="overview-earlier">
          {earlier.map((r) => {
            const sum = (r.summary ?? {}) as { accuracy?: number | null; trials?: number; blocks?: { title?: string; accuracy?: number | null }[] };
            return (
              <li key={r.id}>
                <div className="row" style={{ justifyContent: "space-between", gap: 8 }}>
                  <strong>{r.title}</strong>
                  <span className="small muted num">
                    {r.started_at ? new Date(r.started_at).toLocaleTimeString([], { hour: "2-digit", minute: "2-digit" }) : ""}
                    {sum.trials != null && ` · ${sum.trials} trials`}
                    {sum.accuracy != null && ` · ${pct(sum.accuracy)}`}
                    {r.status !== "completed" && ` · ${r.status}`}
                  </span>
                </div>
                {sum.blocks && sum.blocks.length > 0 && (
                  <div className="small muted">
                    {sum.blocks.map((b) => `${b.title ?? ""}${b.accuracy != null ? ` ${pct(b.accuracy)}` : ""}`).join(" · ")}
                  </div>
                )}
              </li>
            );
          })}
        </ul>
      )}
    </div>
  );
}
