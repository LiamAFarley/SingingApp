import { useEffect, useState } from "react";
import { api } from "../api";
import type { AppInfo } from "../bindings/AppInfo";
import type { AppSettings } from "../bindings/AppSettings";
import type { RangeProfile } from "../bindings/RangeProfile";
import { noteDisplay, noteId, parseNote } from "../music";

const VOICES: [string, number, number][] = [
  ["Bass", 43, 60],
  ["Baritone", 45, 64],
  ["Tenor", 48, 67],
  ["Alto", 53, 72],
  ["Mezzo", 57, 76],
  ["Soprano", 60, 79],
];

export function SettingsPage({ onSaved }: { onSaved: (s: AppSettings) => void }) {
  const [settings, setSettings] = useState<AppSettings | null>(null);
  const [range, setRange] = useState<RangeProfile | null>(null);
  const [info, setInfo] = useState<AppInfo | null>(null);
  const [saved, setSaved] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    api("getSettings").then(setSettings);
    api("getRangeProfile").then(setRange);
    api("appInfo").then(setInfo);
  }, []);

  if (!settings || !range) return <div className="page" />;

  const low = parseNote(range.comfortable_low) ?? 48;
  const high = parseNote(range.comfortable_high) ?? 67;

  const save = async () => {
    setError(null);
    try {
      await api("saveSettings", settings);
      const r = await api("setRangeProfile", range);
      setRange(r);
      onSaved(settings);
      setSaved("Saved.");
      setTimeout(() => setSaved(null), 2000);
    } catch (e) {
      setError((e as Error).message);
    }
  };

  const notes = Array.from({ length: 50 }, (_, i) => 31 + i);
  const mcp = info ? { command: info.mcp_command, args: info.mcp_args } : null;
  const openCode = mcp
    ? JSON.stringify({ $schema: "https://opencode.ai/config.json", mcp: { "singing-trainer": { type: "local", command: [mcp.command, ...mcp.args], enabled: true } } }, null, 2)
    : "";
  const hermes = mcp ? `mcp_servers:\n  singing-trainer:\n    command: '${mcp.command}'\n    args: [${mcp.args.map((a) => `"${a}"`).join(", ")}]` : "";

  return (
    <div className="page">
      <div className="page-head">
        <div>
          <h1>Settings</h1>
          <div className="sub">Voice range, tuning, practice behaviour and the coach connection.</div>
        </div>
        <div className="row">
          {saved && <span className="small muted">{saved}</span>}
          <button className="btn primary" onClick={save}>
            Save
          </button>
        </div>
      </div>
      {error && <div className="notice error">{error}</div>}

      <div className="card section">
        <h3>Voice range</h3>
        <p className="small muted">Sung exercises stay inside your comfortable range. Pick a starting point, then adjust.</p>
        <div className="row wrap">
          {VOICES.map(([name, lo, hi]) => (
            <button
              key={name}
              className={`btn small ${range.voice_type === name.toLowerCase() ? "primary" : ""}`}
              onClick={() => setRange({ ...range, comfortable_low: noteId(lo), comfortable_high: noteId(hi), voice_type: name.toLowerCase() })}
            >
              {name}
            </button>
          ))}
        </div>
        <div className="row" style={{ gap: 16 }}>
          <label className="field">
            Comfortable low
            <select value={low} onChange={(e) => setRange({ ...range, comfortable_low: noteId(parseInt(e.target.value, 10)) })}>
              {notes.map((m) => (
                <option key={m} value={m}>
                  {noteDisplay(m)}
                </option>
              ))}
            </select>
          </label>
          <label className="field">
            Comfortable high
            <select value={high} onChange={(e) => setRange({ ...range, comfortable_high: noteId(parseInt(e.target.value, 10)) })}>
              {notes.map((m) => (
                <option key={m} value={m}>
                  {noteDisplay(m)}
                </option>
              ))}
            </select>
          </label>
          <span className="small faint" style={{ alignSelf: "flex-end", paddingBottom: 6 }}>
            {high - low} semitones
          </span>
        </div>
      </div>

      <div className="grid cols-2">
        <div className="card section">
          <h3>Tuning</h3>
          <label className="field" style={{ maxWidth: 200 }}>
            A4 reference (Hz)
            <input
              type="number"
              min={415}
              max={466}
              step={0.5}
              value={settings.a4_hz}
              onChange={(e) => setSettings({ ...settings, a4_hz: parseFloat(e.target.value) || 440 })}
            />
          </label>
        </div>
        <div className="card section">
          <h3>Practice</h3>
          <label className="check">
            <input
              type="checkbox"
              checked={settings.auto_advance_correct}
              onChange={(e) => setSettings({ ...settings, auto_advance_correct: e.target.checked })}
            />
            Move on automatically after a correct answer
          </label>
          <label className="check">
            <input type="checkbox" checked={settings.keep_recordings} onChange={(e) => setSettings({ ...settings, keep_recordings: e.target.checked })} />
            Keep recordings of sung trials
          </label>
          <label className="field" style={{ maxWidth: 200 }}>
            Delete recordings after (days)
            <input
              type="number"
              min={1}
              max={3650}
              value={settings.recording_retention_days}
              onChange={(e) => setSettings({ ...settings, recording_retention_days: parseInt(e.target.value, 10) || 60 })}
            />
          </label>
        </div>
      </div>

      <div className="card section">
        <h3>Coach (MCP)</h3>
        <p className="small muted">
          Your agent reads results and queues sessions through this app's MCP server. It runs from the app executable with <code>--mcp</code> and works
          whether or not the window is open.
        </p>
        {mcp && (
          <>
            <dl className="kv">
              <dt>Command</dt>
              <dd style={{ userSelect: "text" }}>{mcp.command}</dd>
              <dt>Arguments</dt>
              <dd>{mcp.args.join(" ")}</dd>
              <dt>Data folder</dt>
              <dd style={{ userSelect: "text" }}>{info?.data_dir}</dd>
            </dl>
            <div className="grid cols-2">
              <Snippet title="OpenCode (opencode.json)" text={openCode} />
              <Snippet title="Hermes Agent (config.yaml)" text={hermes} />
            </div>
          </>
        )}
      </div>

      <div className="card section">
        <h3>Sound library</h3>
        <dl className="kv">
          <dt>General MIDI bank</dt>
          <dd>{info?.soundfont ?? "Not found (built-in synth sounds are used instead)"}</dd>
          <dt>Banks</dt>
          <dd>{info?.banks.length ? info.banks.join(", ") : "–"}</dd>
        </dl>
        <p className="small muted">Add SF2 files to the "soundfonts" folder inside the data folder and restart the app to use them.</p>
      </div>
    </div>
  );
}

function Snippet({ title, text }: { title: string; text: string }) {
  const [copied, setCopied] = useState(false);
  return (
    <div className="section">
      <div className="row" style={{ justifyContent: "space-between" }}>
        <span className="small muted">{title}</span>
        <button
          className="btn small quiet"
          onClick={() => {
            navigator.clipboard?.writeText(text).then(() => {
              setCopied(true);
              setTimeout(() => setCopied(false), 1500);
            });
          }}
        >
          {copied ? "Copied" : "Copy"}
        </button>
      </div>
      <pre className="code">{text}</pre>
    </div>
  );
}
