import { useEffect, useRef } from "react";
import type { Anchor } from "./bindings/Anchor";
import type { ApDashboard } from "./bindings/ApDashboard";
import type { AptitudeProfile } from "./bindings/AptitudeProfile";
import type { DailyPlan } from "./bindings/DailyPlan";
import type { ApPrefs } from "./bindings/ApPrefs";
import type { DailySet } from "./bindings/DailySet";
import type { LabelSet } from "./bindings/LabelSet";
import type { SpeechReport } from "./bindings/SpeechReport";
import type { AppInfo } from "./bindings/AppInfo";
import type { AppSettings } from "./bindings/AppSettings";
import type { AudioView } from "./bindings/AudioView";
import type { CalibrationResult } from "./bindings/CalibrationResult";
import type { EngineInfo } from "./bindings/EngineInfo";
import type { HostInfo } from "./bindings/HostInfo";
import type { ImportFile } from "./bindings/ImportFile";
import type { ImportResult } from "./bindings/ImportResult";
import type { LibraryView } from "./bindings/LibraryView";
import type { LiveBatch } from "./bindings/LiveBatch";
import type { PresetView } from "./bindings/PresetView";
import type { RangeProfile } from "./bindings/RangeProfile";
import type { RunnerCommand } from "./bindings/RunnerCommand";
import type { RunnerState } from "./bindings/RunnerState";
import type { Score } from "./bindings/Score";
import type { SessionReport } from "./bindings/SessionReport";
import type { SessionRow } from "./bindings/SessionRow";
import type { StartRequest } from "./bindings/StartRequest";
import type { StatsQuery } from "./bindings/StatsQuery";
import type { StatsRow } from "./bindings/StatsRow";
import type { CustomLevel } from "./bindings/CustomLevel";
import type { CustomLevelView } from "./bindings/CustomLevelView";
import type { WorkshopView } from "./bindings/WorkshopView";
import type { Resumable } from "./bindings/Resumable";

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
  listAnchors(): Promise<Anchor[]>;
  saveAnchor(anchor: Anchor): Promise<Anchor>;
  removeAnchor(id: string): Promise<boolean>;
  libraryView(): Promise<LibraryView>;
  libraryScores(id: string): Promise<Score[]>;
  libraryImport(files: ImportFile[], collection: string | null): Promise<ImportResult[]>;
  libraryRemoveFile(file: string): Promise<void>;
  libraryOpenFolder(): Promise<void>;
  libraryPlay(id: string, part: number | null): Promise<number>;
  stopPreview(): Promise<void>;
  apDashboard(): Promise<ApDashboard>;
  /** The aptitude screening's runs over time, with reference lines. */
  aptitudeProfile(): Promise<AptitudeProfile>;
  /** Today's set for `category` (absolute pitch when omitted). */
  previewDailySet(category?: string): Promise<DailySet>;
  saveDailyPlan(plan: DailyPlan): Promise<void>;
  setCourseLevel(course: string, level: number): Promise<void>;
  /** A workshop course's lessons by module, with scores. */
  workshopView(course: string): Promise<WorkshopView>;
  customLevels(course: string): Promise<CustomLevelView[]>;
  saveCustomLevel(level: CustomLevel): Promise<CustomLevel>;
  deleteCustomLevel(id: string): Promise<void>;
  /** Today's set for a category, if it was ended before it finished today. */
  resumableDaily(category?: string): Promise<Resumable | null>;
  /** Settings per exercise type applied to course lessons and today's set. */
  exercisePrefs(): Promise<Record<string, Record<string, unknown>>>;
  saveExercisePrefs(kind: string, params: Record<string, unknown>): Promise<void>;
  apPrefs(): Promise<ApPrefs>;
  saveApPrefs(prefs: ApPrefs): Promise<ApPrefs>;
  listLabelSets(): Promise<LabelSet[]>;
  saveLabelSet(set: LabelSet): Promise<LabelSet>;
  speechStatus(): Promise<SpeechReport>;
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
    listAnchors: () => call("list_anchors"),
    saveAnchor: (anchor) => call("save_anchor", { anchor }),
    removeAnchor: (id) => call("remove_anchor", { id }),
    libraryView: () => call("library_view"),
    libraryScores: (id) => call("library_scores", { id }),
    libraryImport: (files, collection) => call("library_import", { files, collection }),
    libraryRemoveFile: (file) => call("library_remove_file", { file }),
    libraryOpenFolder: () => call("library_open_folder"),
    libraryPlay: (id, part) => call("library_play", { id, part }),
    stopPreview: () => call("stop_preview"),
    apDashboard: () => call("ap_dashboard"),
    aptitudeProfile: () => call("aptitude_profile"),
    previewDailySet: (category) => call("preview_daily_set", { category: category ?? null }),
    saveDailyPlan: (plan) => call("save_daily_plan", { plan }),
    setCourseLevel: (course, level) => call("set_course_level", { course, level }),
    workshopView: (course) => call("workshop_view", { course }),
    customLevels: (course) => call("custom_levels", { course }),
    saveCustomLevel: (level) => call("save_custom_level", { level }),
    deleteCustomLevel: (id) => call("delete_custom_level", { id }),
    resumableDaily: (category) => call("resumable_daily", { category: category ?? null }),
    exercisePrefs: () => call("exercise_prefs"),
    saveExercisePrefs: (kind, params) => call("save_exercise_prefs", { kind, params }),
    apPrefs: () => call("ap_prefs"),
    saveApPrefs: (prefs) => call("save_ap_prefs", { prefs }),
    listLabelSets: () => call("list_label_sets"),
    saveLabelSet: (set) => call("save_label_set", { set }),
    speechStatus: () => call("speech_status"),
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

/** Subscribes to a backend event for the lifetime of the calling component. */
export function useBackendEvent(name: string, cb: (payload: unknown) => void) {
  const latest = useRef(cb);
  latest.current = cb;
  useEffect(() => {
    let off: (() => void) | undefined;
    let cancelled = false;
    api("listen", name, (p: unknown) => latest.current(p))
      .then((u) => {
        if (cancelled) u();
        else off = u;
      })
      .catch(() => undefined);
    return () => {
      cancelled = true;
      off?.();
    };
  }, [name]);
}
