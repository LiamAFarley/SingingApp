import { useEffect, useState } from "react";
import { api } from "../api";
import type { AppSettings } from "../bindings/AppSettings";
import type { AudioView } from "../bindings/AudioView";
import type { CalibrationResult } from "../bindings/CalibrationResult";
import type { EngineSettings } from "../bindings/EngineSettings";
import type { HostInfo } from "../bindings/HostInfo";
import { LevelMeter } from "../components/LevelMeter";
import { live, useAnimationFrame } from "../live";
import { describeCents, noteDisplay } from "../music";

export function AudioPage({ audio, onChanged }: { audio: AudioView | null; onChanged: () => void }) {
  const [hosts, setHosts] = useState<HostInfo[]>([]);
  const [settings, setSettings] = useState<AppSettings | null>(null);
  const [draft, setDraft] = useState<EngineSettings | null>(null);
  const [busy, setBusy] = useState<string | null>(null);
  const [message, setMessage] = useState<{ kind: "info" | "error"; text: string } | null>(null);
  const [calib, setCalib] = useState<CalibrationResult | null>(null);

  useEffect(() => {
    api("listAudioHosts").then(setHosts).catch(() => undefined);
    api("getSettings").then((s) => {
      setSettings(s);
      setDraft(s.audio);
    });
  }, []);

  const status = audio?.status;
  const info = audio?.info;
  const running = status?.running ?? false;
  const hostName = draft?.host ?? info?.host ?? hosts[0]?.name ?? "";
  const host = hosts.find((h) => h.name === hostName);
  const inputs = host?.devices.filter((d) => d.input_channels > 0) ?? [];
  const outputs = host?.devices.filter((d) => d.output_channels > 0) ?? [];
  const isAsio = hostName === "ASIO";
  const inDev = inputs.find((d) => d.id === (draft?.input_device ?? info?.input_device_id)) ?? inputs[0];
  const outDev = outputs.find((d) => d.id === (draft?.output_device ?? info?.output_device_id)) ?? outputs[0];
  const inChannels = isAsio ? outDev?.input_channels ?? info?.input_channels ?? 2 : inDev?.input_channels ?? info?.input_channels ?? 2;
  const outChannels = outDev?.output_channels ?? info?.output_channels ?? 2;

  const apply = async () => {
    if (!settings || !draft) return;
    setBusy("apply");
    setMessage(null);
    try {
      const next = { ...settings, audio: draft };
      await api("saveSettings", next);
      setSettings(next);
      await api("startAudio");
      setMessage({ kind: "info", text: "Audio restarted with the new settings." });
    } catch (e) {
      setMessage({ kind: "error", text: (e as Error).message });
    } finally {
      setBusy(null);
      onChanged();
    }
  };

  const calibrate = async (method: "loopback" | "tap") => {
    setBusy(method);
    setMessage(null);
    setCalib(null);
    try {
      const r = await api("calibrate", method);
      setCalib(r);
      setMessage({ kind: "info", text: `Calibrated: ${r.matched} of ${r.expected} clicks found, spread ${r.spread_ms.toFixed(1)} ms.` });
    } catch (e) {
      setMessage({ kind: "error", text: (e as Error).message });
    } finally {
      setBusy(null);
      onChanged();
    }
  };

  const set = (patch: Partial<EngineSettings>) => draft && setDraft({ ...draft, ...patch });

  return (
    <div className="page">
      <div className="page-head">
        <div>
          <h1>Audio</h1>
          <div className="sub">Interface, channels and latency. Use headphones and your interface's direct monitoring.</div>
        </div>
        <div className="row">
          {running ? <span className="badge correct">Running</span> : <span className="badge wrong">Stopped</span>}
          <button
            className="btn"
            disabled={busy != null}
            onClick={async () => {
              setBusy("restart");
              try {
                await api("startAudio");
                setMessage(null);
              } catch (e) {
                setMessage({ kind: "error", text: (e as Error).message });
              }
              setBusy(null);
              onChanged();
            }}
          >
            {running ? "Restart" : "Start"}
          </button>
        </div>
      </div>
      {audio?.error && !running && <div className="notice error">{audio.error}</div>}
      {message && <div className={`notice ${message.kind}`}>{message.text}</div>}

      <div className="grid cols-2">
        <div className="card section">
          <h3>Device</h3>
          <label className="field">
            Driver
            <select value={hostName} onChange={(e) => set({ host: e.target.value, input_device: null, output_device: null })}>
              {hosts.map((h) => (
                <option key={h.name} value={h.name}>
                  {h.name}
                </option>
              ))}
            </select>
          </label>
          {isAsio ? (
            <label className="field">
              ASIO driver
              <select value={outDev?.id ?? ""} onChange={(e) => set({ output_device: e.target.value, input_device: e.target.value })}>
                {outputs.map((d) => (
                  <option key={d.id} value={d.id}>
                    {d.name}
                  </option>
                ))}
              </select>
            </label>
          ) : (
            <>
              <label className="field">
                Input device
                <select value={inDev?.id ?? ""} onChange={(e) => set({ input_device: e.target.value })}>
                  {inputs.map((d) => (
                    <option key={d.id} value={d.id}>
                      {d.name}
                    </option>
                  ))}
                </select>
              </label>
              <label className="field">
                Output device
                <select value={outDev?.id ?? ""} onChange={(e) => set({ output_device: e.target.value })}>
                  {outputs.map((d) => (
                    <option key={d.id} value={d.id}>
                      {d.name}
                    </option>
                  ))}
                </select>
              </label>
            </>
          )}
          <div className="grid cols-2">
            <label className="field">
              Microphone input
              <select value={draft?.input_channel ?? 0} onChange={(e) => set({ input_channel: parseInt(e.target.value, 10) })}>
                {Array.from({ length: Math.max(1, inChannels) }, (_, i) => (
                  <option key={i} value={i}>
                    Input {i + 1}
                  </option>
                ))}
              </select>
            </label>
            <label className="field">
              Headphone output
              <select value={draft?.output_pair ?? 0} onChange={(e) => set({ output_pair: parseInt(e.target.value, 10) })}>
                {Array.from({ length: Math.max(1, Math.floor(outChannels / 2)) }, (_, i) => (
                  <option key={i} value={i * 2}>
                    Outputs {i * 2 + 1}–{i * 2 + 2}
                  </option>
                ))}
              </select>
            </label>
          </div>
          <div className="grid cols-2">
            <label className="field">
              Sample rate
              <select value={draft?.sample_rate ?? 48000} onChange={(e) => set({ sample_rate: parseInt(e.target.value, 10) })}>
                <option value={48000}>48 kHz</option>
                <option value={44100}>44.1 kHz</option>
              </select>
            </label>
            <label className="field">
              Buffer size
              <select
                value={draft?.buffer_frames ?? 0}
                onChange={(e) => {
                  const v = parseInt(e.target.value, 10);
                  set({ buffer_frames: v === 0 ? null : v });
                }}
              >
                <option value={0}>Driver setting</option>
                {[64, 128, 256, 512].map((n) => (
                  <option key={n} value={n}>
                    {n} samples
                  </option>
                ))}
              </select>
            </label>
          </div>
          {isAsio && <p className="small faint">With ASIO the buffer size is usually set in Focusrite Control; the app uses whatever the driver reports.</p>}
          <div className="row">
            <button className="btn primary" disabled={busy != null || !draft} onClick={apply}>
              {busy === "apply" ? "Applying…" : "Apply"}
            </button>
          </div>
        </div>

        <div className="card section">
          <h3>Status</h3>
          <dl className="kv">
            <dt>Driver</dt>
            <dd>{info ? `${info.host} · ${info.output_device}` : "–"}</dd>
            <dt>Format</dt>
            <dd className="num">{info ? `${info.sample_rate / 1000} kHz · ${status?.buffer_frames ?? "?"} samples · ${info.input_format}` : "–"}</dd>
            <dt>Round trip</dt>
            <dd className="num">
              {audio?.roundtrip_ms != null ? `${audio.roundtrip_ms.toFixed(1)} ms` : "–"}{" "}
              {audio?.calibration ? <span className="badge correct">calibrated</span> : <span className="badge">estimated</span>}
            </dd>
            <dt>Load</dt>
            <dd className="num">{status ? `${status.load_percent.toFixed(1)}%` : "–"}</dd>
            <dt>Dropouts</dt>
            <dd className="num">{status ? `${status.xruns}` : "–"}</dd>
          </dl>
          <div className="divider" />
          <h3>Input</h3>
          <LevelMeter width={260} />
          <InputPitch />
          <div className="row">
            <button className="btn" onClick={() => api("playNote", 69, "synth:sine", 1000).catch(() => undefined)}>
              Test tone (A4)
            </button>
          </div>
        </div>
      </div>

      <div className="card section">
        <h3>Latency calibration</h3>
        <p className="muted small">
          Timing grades (sight singing, patterns, rhythm) subtract the measured round trip. Calibrate once per device, sample rate and buffer size.
        </p>
        <div className="grid cols-2">
          <div className="section">
            <strong>Loopback (most accurate)</strong>
            <p className="small muted">Connect a cable from an output of the interface to a line input, set that input as the microphone input, then run.</p>
            <div>
              <button className="btn" disabled={busy != null || !running} onClick={() => calibrate("loopback")}>
                {busy === "loopback" ? "Measuring…" : "Run loopback"}
              </button>
            </div>
          </div>
          <div className="section">
            <strong>Tap along</strong>
            <p className="small muted">Ten clicks play through the headphones. Tap the microphone (or say "ta") exactly on each click.</p>
            <div>
              <button className="btn" disabled={busy != null || !running} onClick={() => calibrate("tap")}>
                {busy === "tap" ? "Listening…" : "Run tap-along"}
              </button>
            </div>
          </div>
        </div>
        {calib && <p className="small num">Correction applied: {calib.residual_frames} samples.</p>}
      </div>
    </div>
  );
}

function InputPitch() {
  useAnimationFrame(true);
  const p = live.latest(0.25);
  if (!p || p.midi == null) return <div className="small faint">No pitch detected</div>;
  const n = Math.round(p.midi);
  return (
    <div className="small num">
      Detected {noteDisplay(n)} · {describeCents((p.midi - n) * 100)}
    </div>
  );
}
