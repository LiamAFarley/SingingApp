import { useEffect, useState } from "react";
import { api } from "../api";
import type { Anchor } from "../bindings/Anchor";
import type { AppInfo } from "../bindings/AppInfo";
import type { AppSettings } from "../bindings/AppSettings";
import type { ApPrefs } from "../bindings/ApPrefs";
import type { LabelSet } from "../bindings/LabelSet";
import type { SpeechReport } from "../bindings/SpeechReport";
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

      <ApPractice />

      <AnchorSongs />

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

const PITCH_CLASSES = ["C", "C#", "D", "D#", "E", "F", "F#", "G", "G#", "A", "A#", "B"];

function ApPractice() {
  const [prefs, setPrefs] = useState<ApPrefs | null>(null);
  const [sets, setSets] = useState<LabelSet[]>([]);
  const [speech, setSpeech] = useState<SpeechReport | null>(null);
  const [which, setWhich] = useState("my_colors");
  const [note, setNote] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    api("apPrefs").then(setPrefs).catch((e) => setError(e.message));
    api("listLabelSets").then((rows) => {
      setSets(rows);
      if (rows.some((s) => s.id === "my_colors")) setWhich("my_colors");
      else if (rows[0]) setWhich(rows[0].id);
    }).catch((e) => setError(e.message));
    api("speechStatus").then(setSpeech).catch((e) => setError(e.message));
  }, []);

  const current = sets.find((s) => s.id === which);
  const keys = current?.kind === "pitch_class" ? PITCH_CLASSES : Object.keys(current?.entries ?? {});

  const patch = (key: string, field: "color" | "word", value: string) => {
    if (!current) return;
    const entries = { ...current.entries };
    const prev = entries[key] ?? {};
    const next = { ...prev, [field]: value.trim() ? value.trim() : null };
    if (!next.color && !next.word && !next.name && !next.speech) delete entries[key];
    else entries[key] = next;
    setSets(sets.map((s) => (s.id === current.id ? { ...current, entries } : s)));
  };

  const save = async () => {
    if (!prefs || !current) return;
    setError(null);
    try {
      const saved = await api("saveApPrefs", prefs);
      setPrefs(saved);
      const set = await api("saveLabelSet", current);
      setSets(sets.map((s) => (s.id === set.id ? set : s)));
      setNote("Saved.");
      setTimeout(() => setNote(null), 2000);
    } catch (e) {
      setError((e as Error).message);
    }
  };

  if (!prefs) return null;
  return (
    <div className="card section">
      <div className="row" style={{ justifyContent: "space-between" }}>
        <h3>Colours, names and speech</h3>
        <div className="row">
          {note && <span className="small muted">{note}</span>}
          <button className="btn small" onClick={save}>
            Save
          </button>
        </div>
      </div>
      {error && <div className="notice error">{error}</div>}
      <label className="check">
        <input type="checkbox" checked={prefs.errorless_early} onChange={(e) => setPrefs({ ...prefs, errorless_early: e.target.checked })} />
        Errorless early levels
      </label>
      <p className="small muted">
        The first levels of note colours, chord colours and contrast light up the right answer without marking a miss. What is stored is still whether you were right.
      </p>
      <label className="check">
        <input
          type="checkbox"
          checked={prefs.daily_anchor_check}
          onChange={(e) => setPrefs({ ...prefs, daily_anchor_check: e.target.checked })}
        />
        Start today's set by singing C from memory
      </label>
      <p className="small muted">
        One note before anything is heard, on days no test starts the set. It tracks how far your remembered C is from a real one, shown as a trend on the Absolute pitch page.
      </p>
      <label className="field" style={{ maxWidth: 280 }}>
        Contrast course
        <select value={prefs.contrast_labels} onChange={(e) => setPrefs({ ...prefs, contrast_labels: e.target.value })}>
          <option value="gervain_names">Gervain names</option>
          <option value="my_colors">My colours</option>
          <option value="taneda_colors">Taneda colours</option>
          <option value="note_names">Note names</option>
        </select>
      </label>

      {speech && (
        <div style={{ marginTop: 8 }}>
          <div className="small">Speech folder</div>
          <div className="small muted" style={{ userSelect: "text" }}>{speech.dir}</div>
          {speech.disabled.length > 0 ? (
            <p className="small muted">
              Speaking is off for: {speech.disabled.join(", ")}. {speech.missing.length} files are not there yet. Everything else still runs.
            </p>
          ) : (
            <p className="small muted">Every expected speech file is in place.</p>
          )}
        </div>
      )}

      {sets.length > 0 && current && (
        <div style={{ marginTop: 12 }}>
          <label className="field" style={{ maxWidth: 280 }}>
            Label set
            <select value={current.id} onChange={(e) => setWhich(e.target.value)}>
              {sets.map((s) => (
                <option key={s.id} value={s.id}>
                  {s.title}
                </option>
              ))}
            </select>
          </label>
          <p className="small muted">{current.description}</p>
          <div className="grid cols-2">
            {keys.map((key) => {
              const entry = current.entries[key];
              return (
                <div key={key} className="row" style={{ gap: 8 }}>
                  <span className="num" style={{ width: 28 }}>{key}</span>
                  <input
                    type="text"
                    placeholder="Word"
                    value={entry?.word ?? ""}
                    onChange={(e) => patch(key, "word", e.target.value)}
                    style={{ width: 120 }}
                  />
                  <input
                    type="color"
                    aria-label={`Colour for ${key}`}
                    value={entry?.color && /^#[0-9a-fA-F]{6}$/.test(entry.color) ? entry.color : "#cccccc"}
                    onChange={(e) => patch(key, "color", e.target.value)}
                  />
                </div>
              );
            })}
          </div>
        </div>
      )}
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

/** Songs the user knows in their original key, for the anchor-song exercise. */
function AnchorSongs() {
  const [anchors, setAnchors] = useState<Anchor[]>([]);
  const [title, setTitle] = useState("");
  const [first, setFirst] = useState(55);
  const [error, setError] = useState<string | null>(null);
  const load = () => api("listAnchors").then(setAnchors).catch((e) => setError(e.message));
  useEffect(() => {
    load();
  }, []);

  const add = async () => {
    setError(null);
    try {
      await api("saveAnchor", { id: "", title, first_note: noteId(first), melody: null, created_at: "", source: "" });
      setTitle("");
      load();
    } catch (e) {
      setError((e as Error).message);
    }
  };

  const practise = async (mode: "sing_first_note" | "key_check") => {
    setError(null);
    const n = mode === "key_check" ? anchors.filter((a) => a.melody).length : anchors.length;
    const spec = {
      title: mode === "key_check" ? "Anchor songs: original key?" : "Anchor songs: first notes",
      blocks: [{ type: "anchor_song", count: Math.max(8, n * 2), feedback: "after_trial", params: { mode } }],
    };
    try {
      await api("startSession", { source: "spec", spec });
    } catch (e) {
      setError((e as Error).message);
    }
  };

  const withMelody = anchors.some((a) => a.melody);
  return (
    <div className="card section">
      <div className="section-head">
        <h3>Anchor songs</h3>
        <div className="row">
          <button className="btn small" disabled={anchors.length === 0} onClick={() => practise("sing_first_note")}>
            Practise first notes
          </button>
          <button className="btn small" disabled={!withMelody} onClick={() => practise("key_check")} title={withMelody ? "" : "Needs an opening phrase, which your coach can add"}>
            Original key or shifted?
          </button>
        </div>
      </div>
      <p className="small muted">
        Songs you know well in their original key. Practice asks you to sing each song's first note from memory. When an opening phrase is saved (your coach
        can add one with <code>save_anchor</code>), it also plays the opening in or out of the original key.
      </p>
      {error && <div className="notice error">{error}</div>}
      {anchors.length > 0 && (
        <table>
          <thead>
            <tr>
              <th>Song</th>
              <th>First note</th>
              <th>Opening phrase</th>
              <th />
            </tr>
          </thead>
          <tbody>
            {anchors.map((a) => (
              <tr key={a.id}>
                <td>{a.title}</td>
                <td className="num">{a.first_note.replace("#", "♯").replace(/b(?=-?\d)/, "♭")}</td>
                <td className="muted">{a.melody ? `${a.melody.notes.length} notes` : "–"}</td>
                <td style={{ textAlign: "right" }}>
                  <button className="btn small quiet" onClick={() => api("removeAnchor", a.id).then(load).catch(() => undefined)}>
                    Remove
                  </button>
                </td>
              </tr>
            ))}
          </tbody>
        </table>
      )}
      <div className="row" style={{ gap: 12, alignItems: "flex-end" }}>
        <label className="field grow">
          Song
          <input type="text" value={title} placeholder="Title (artist)" onChange={(e) => setTitle(e.target.value)} />
        </label>
        <label className="field">
          First note
          <select value={first} onChange={(e) => setFirst(parseInt(e.target.value, 10))}>
            {Array.from({ length: 49 }, (_, i) => 36 + i).map((m) => (
              <option key={m} value={m}>
                {noteDisplay(m)}
              </option>
            ))}
          </select>
        </label>
        <button className="btn" disabled={!title.trim()} onClick={add}>
          Add
        </button>
      </div>
    </div>
  );
}
