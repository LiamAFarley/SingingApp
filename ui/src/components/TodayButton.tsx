import { useCallback, useEffect, useState } from "react";
import { api, useBackendEvent } from "../api";
import type { Resumable } from "../bindings/Resumable";

/** Start today's set, or carry on with it when it was ended part-way today. */
export function TodayButton({
  category,
  disabled,
  starting,
  onStart,
}: {
  category?: string;
  disabled: boolean;
  starting: string | null;
  onStart: (key: string, req: Parameters<typeof api<"startSession">>[1]) => void;
}) {
  const [resumable, setResumable] = useState<Resumable | null>(null);
  const refresh = useCallback(() => {
    api("resumableDaily", category)
      .then(setResumable)
      .catch(() => setResumable(null));
  }, [category]);
  useEffect(refresh, [refresh]);
  useBackendEvent("courses-changed", refresh);
  const fresh = () => onStart("daily", category ? { source: "daily_set", category } : { source: "daily_set" });
  if (!resumable) {
    return (
      <button className="btn primary" disabled={disabled} onClick={fresh}>
        {starting === "daily" ? "Starting…" : "Start today's set"}
      </button>
    );
  }
  return (
    <div className="today-buttons">
      <button className="btn primary" disabled={disabled} onClick={() => onStart("resume", { source: "resume", session_id: resumable.session_id })}>
        {starting === "resume" ? "Resuming…" : `Resume: part ${resumable.block + 1} of ${resumable.blocks}`}
      </button>
      <button className="btn quiet small" disabled={disabled} onClick={fresh}>
        {starting === "daily" ? "Starting…" : "Start over"}
      </button>
    </div>
  );
}
