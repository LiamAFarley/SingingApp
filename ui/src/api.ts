import type { AppInfo } from "./bindings/AppInfo";
import type { AppSettings } from "./bindings/AppSettings";
import type { AudioView } from "./bindings/AudioView";
import type { CalibrationResult } from "./bindings/CalibrationResult";
import type { EngineInfo } from "./bindings/EngineInfo";
import type { HostInfo } from "./bindings/HostInfo";
import type { LiveBatch } from "./bindings/LiveBatch";
import type { PresetView } from "./bindings/PresetView";
import type { RangeProfile } from "./bindings/RangeProfile";
import type { RunnerCommand } from "./bindings/RunnerCommand";
import type { RunnerState } from "./bindings/RunnerState";
import type { SessionReport } from "./bindings/SessionReport";
import type { SessionRow } from "./bindings/SessionRow";
import type { StartRequest } from "./bindings/StartRequest";
import type { StatsQuery } from "./bindings/StatsQuery";
import type { StatsRow } from "./bindings/StatsRow";

export interface Backend {
  appInfo(): Promise<AppInfo>;
  getSettings(): Promise<AppSettings>;
  saveSettings(settings: AppSettings): Promise<AudioView>;
  listAudioHosts(): Promise<HostInfo[]>;
  audioState(): Promise<AudioView>;
  startAudio(): Promise<EngineInfo>;
  stopAudio(): Promise<void>;
  playNote(midi: number, timbre?: string, ms?: number): Promise<void>;
  setDrone(midi: number | null, timbre?: string): Promise<void>;
  calibrate(method: "loopback" | "tap"): Promise<CalibrationResult>;
  listPresets(): Promise<PresetView[]>;
  listQueue(): Promise<SessionRow[]>;
  removeQueued(sessionId: string): Promise<boolean>;
  startSession(request: StartRequest): Promise<string>;
  runnerCommand(command: RunnerCommand): Promise<void>;
  runnerState(): Promise<RunnerState>;
  listSessions(limit?: number): Promise<SessionRow[]>;
  sessionReport(sessionId: string): Promise<SessionReport>;
  trialDetail(trialId: string): Promise<Record<string, unknown>>;
  playRecording(trialId: string): Promise<void>;
  getRangeProfile(): Promise<RangeProfile>;
  setRangeProfile(profile: RangeProfile): Promise<RangeProfile>;
  addComfortReport(sessionId: string | null, phase: string, level: string, note: string | null): Promise<void>;
  getStats(query: StatsQuery): Promise<StatsRow[]>;
  subscribeLive(onBatch: (b: LiveBatch) => void): Promise<void>;
  listen<T>(event: string, cb: (payload: T) => void): Promise<() => void>;
}

export const isTauri = typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;

async function tauriBackend(): Promise<Backend> {
  const core = await import("@tauri-apps/api/core");
  const event = await import("@tauri-apps/api/event");
  const call = <T>(cmd: string, args?: Record<string, unknown>) => core.invoke<T>(cmd, args);
  return {
    appInfo: () => call("app_info"),
    getSettings: () => call("get_settings"),
    saveSettings: (settings) => call("save_settings", { settings }),
    listAudioHosts: () => call("list_audio_hosts"),
    audioState: () => call("audio_state"),
    startAudio: () => call("start_audio"),
    stopAudio: () => call("stop_audio"),
    playNote: (midi, timbre, ms) => call("play_note", { midi, timbre: timbre ?? null, ms: ms ?? null }),
    setDrone: (midi, timbre) => call("set_drone", { midi, timbre: timbre ?? null }),
    calibrate: (method) => call("calibrate", { method }),
    listPresets: () => call("list_presets"),
    listQueue: () => call("list_queue"),
    removeQueued: (sessionId) => call("remove_queued", { sessionId }),
    startSession: (request) => call("start_session", { request }),
    runnerCommand: (command) => call("runner_command", { command }),
    runnerState: () => call("runner_state"),
    listSessions: (limit) => call("list_sessions", { limit: limit ?? null }),
    sessionReport: (sessionId) => call("session_report", { sessionId }),
    trialDetail: (trialId) => call("trial_detail", { trialId }),
    playRecording: (trialId) => call("play_recording", { trialId }),
    getRangeProfile: () => call("get_range_profile"),
    setRangeProfile: (profile) => call("set_range_profile", { profile }),
    addComfortReport: (sessionId, phase, level, note) => call("add_comfort_report", { sessionId, phase, level, note }),
    getStats: (query) => call("get_stats", { query }),
    subscribeLive: async (onBatch) => {
      const channel = new core.Channel<LiveBatch>();
      channel.onmessage = onBatch;
      await call("subscribe_live", { channel });
    },
    listen: async (name, cb) => event.listen(name, (e) => cb(e.payload as never)),
  };
}

let backendPromise: Promise<Backend> | null = null;

export function backend(): Promise<Backend> {
  if (!backendPromise) {
    backendPromise = isTauri ? tauriBackend() : import("./mock").then((m) => m.mockBackend());
  }
  return backendPromise;
}

/** Calls a backend method and turns rejections into thrown Error objects with readable text. */
export async function api<K extends keyof Backend>(
  method: K,
  ...args: Parameters<Backend[K]>
): Promise<Awaited<ReturnType<Backend[K]>>> {
  const b = await backend();
  try {
    // eslint-disable-next-line @typescript-eslint/no-explicit-any
    return await (b[method] as any)(...args);
  } catch (e) {
    throw new Error(typeof e === "string" ? e : e instanceof Error ? e.message : JSON.stringify(e));
  }
}

export function command(c: RunnerCommand) {
  return api("runnerCommand", c).catch(() => undefined);
}
