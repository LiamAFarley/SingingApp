import { useEffect, useState } from "react";
import { api } from "./api";
import type { LiveBatch } from "./bindings/LiveBatch";
import type { LivePoint } from "./bindings/LivePoint";

/**
 * Live pitch frames from the audio engine plus an estimate of the engine's master clock,
 * interpolated with performance.now() between batches so displays animate smoothly.
 */
class LiveStore {
  points: LivePoint[] = [];
  peak = 0;
  private anchorClock = 0;
  private anchorPerf = 0;
  private lastNow = 0;
  private started = false;
  private listeners = new Set<() => void>();

  start() {
    if (this.started) return;
    this.started = true;
    api("subscribeLive", (b: LiveBatch) => this.push(b)).catch(() => {
      this.started = false;
    });
  }

  private push(b: LiveBatch) {
    this.anchorClock = b.now_s;
    this.anchorPerf = performance.now();
    this.peak = Math.max(b.peak, this.peak * 0.6);
    for (const p of b.points) this.points.push(p);
    const cutoff = b.now_s - 40;
    if (this.points.length > 6000 || (this.points.length && this.points[0].t < cutoff)) {
      let i = 0;
      while (i < this.points.length && this.points[i].t < cutoff) i++;
      this.points.splice(0, Math.max(i, this.points.length - 6000));
    }
    this.listeners.forEach((l) => l());
  }

  /** Estimated master-clock time in seconds (monotonic). */
  now(): number {
    if (!this.anchorPerf) return 0;
    const est = this.anchorClock + (performance.now() - this.anchorPerf) / 1000;
    this.lastNow = Math.max(this.lastNow, est);
    return this.lastNow;
  }

  /** The most recent voiced point within `maxAge` seconds of the clock. */
  latest(maxAge = 0.25): LivePoint | null {
    const now = this.now();
    for (let i = this.points.length - 1; i >= 0; i--) {
      const p = this.points[i];
      if (now - p.t > maxAge) return null;
      if (p.midi != null) return p;
    }
    return null;
  }

  since(t: number): LivePoint[] {
    let lo = 0;
    let hi = this.points.length;
    while (lo < hi) {
      const mid = (lo + hi) >> 1;
      if (this.points[mid].t < t) lo = mid + 1;
      else hi = mid;
    }
    return this.points.slice(lo);
  }

  subscribe(l: () => void): () => void {
    this.listeners.add(l);
    return () => this.listeners.delete(l);
  }
}

export const live = new LiveStore();

/** Re-renders at display rate while mounted (for components that read `live`). */
export function useAnimationFrame(active = true): number {
  const [tick, setTick] = useState(0);
  useEffect(() => {
    if (!active) return;
    let raf = 0;
    const loop = () => {
      setTick((t) => (t + 1) % 1_000_000);
      raf = requestAnimationFrame(loop);
    };
    raf = requestAnimationFrame(loop);
    return () => cancelAnimationFrame(raf);
  }, [active]);
  return tick;
}
