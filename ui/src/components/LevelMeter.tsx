import { live, useAnimationFrame } from "../live";

/** Input level from the live peak, -60 to 0 dBFS. */
export function LevelMeter({ width = 220 }: { width?: number }) {
  useAnimationFrame(true);
  const peak = live.peak;
  const db = peak > 0 ? 20 * Math.log10(peak) : -100;
  const frac = Math.max(0, Math.min(1, (db + 60) / 60));
  const clip = peak >= 0.98;
  return (
    <div className="row" style={{ gap: 10 }}>
      <div className={`meter-bar ${clip ? "clip" : ""}`} style={{ width }}>
        <div style={{ width: `${frac * 100}%` }} />
      </div>
      <span className="small muted num" style={{ width: 64 }}>
        {db <= -99 ? "–" : `${db.toFixed(0)} dBFS`}
      </span>
      {clip && <span className="badge wrong">Clipping</span>}
    </div>
  );
}
