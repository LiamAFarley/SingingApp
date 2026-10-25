import { useEffect, useState } from "react";
import { api } from "../api";

type Field =
  | { key: string; label: string; type: "number"; min: number; max: number; step?: number; unit: string; placeholder: number }
  | { key: string; label: string; type: "choice"; options: { value: string; label: string }[] };

const tempo = (label = "Tempo", placeholder = 90): Field => ({ key: "tempo_bpm", label, type: "number", min: 40, max: 220, unit: "bpm", placeholder });
const labels: Field = {
  key: "labels",
  label: "Note names",
  type: "choice",
  options: [
    { value: "fixed_do", label: "Fixed do" },
    { value: "solfege", label: "Movable do" },
    { value: "numbers", label: "Numbers" },
  ],
};
const cadence: Field = { key: "cadence_bpm", label: "Cadence tempo", type: "number", min: 40, max: 220, unit: "bpm", placeholder: 100 };

/** The settings worth changing per exercise, applied to every lesson and today's set. */
const FIELDS: Record<string, { title: string; fields: Field[] }> = {
  scale_degree_id: {
    title: "Scale degrees",
    fields: [
      tempo("Note tempo", 100),
      cadence,
      labels,
      {
        key: "resolution",
        label: "Resolution",
        type: "choice",
        options: [
          { value: "nearest", label: "To the nearer tonic" },
          { value: "ascending", label: "Up" },
          { value: "descending", label: "Down" },
          { value: "none", label: "None" },
        ],
      },
    ],
  },
  interval_id: { title: "Intervals", fields: [{ key: "note_ms", label: "Note length", type: "number", min: 200, max: 3000, step: 50, unit: "ms", placeholder: 900 }, cadence, labels] },
  interval_comparison: { title: "Interval comparison", fields: [{ key: "note_ms", label: "Note length", type: "number", min: 200, max: 3000, step: 50, unit: "ms", placeholder: 700 }] },
  sing_interval: { title: "Interval singing", fields: [tempo("Tempo", 80), labels] },
  chord_id: { title: "Chords", fields: [{ key: "duration_ms", label: "Chord length", type: "number", min: 300, max: 5000, step: 100, unit: "ms", placeholder: 1500 }] },
  scale_id: { title: "Scales", fields: [tempo("Tempo", 160)] },
  resting_tone: { title: "Find home", fields: [tempo("Tempo", 96)] },
  tonal_pattern: { title: "Tonal patterns", fields: [tempo("Tempo", 90), { key: "breath_beats", label: "Breath", type: "number", min: 0, max: 8, step: 0.5, unit: "beats", placeholder: 1 }, labels] },
  pattern_dialogue: { title: "Pattern dialogue", fields: [tempo("Tempo", 90), labels] },
  n_back: { title: "N-back", fields: [{ key: "ioi_ms", label: "Time between notes", type: "number", min: 800, max: 5000, step: 100, unit: "ms", placeholder: 2000 }] },
  melodic_updating: { title: "Melodic updating", fields: [tempo("Tempo", 80), { key: "think_beats", label: "Time to rehearse", type: "number", min: 1, max: 16, unit: "beats", placeholder: 4 }, labels] },
  pattern_memory: { title: "Same or different", fields: [tempo("Tempo", 96)] },
  syllable_check: { title: "Mislabeled syllables", fields: [tempo("Tempo", 90), labels] },
  error_detection: { title: "Error detection", fields: [tempo("Tempo", 80), { key: "pre_audiation_s", label: "Reading time", type: "number", min: 0, max: 60, unit: "s", placeholder: 15 }] },
  cadence_id: { title: "Cadences", fields: [tempo("Tempo", 72)] },
  bass_line: { title: "Bass and guide tones", fields: [tempo("Tempo", 80), labels] },
  sight_sing: { title: "Sight singing and singback", fields: [tempo("Tempo", 72)] },
  melodic_dictation: { title: "Dictation", fields: [tempo("Tempo", 80)] },
};

/** Settings for the exercises of an activity. Empty means each lesson's own value. */
export function ExerciseSettings({ kinds, onClose }: { kinds: string[]; onClose: () => void }) {
  const [prefs, setPrefs] = useState<Record<string, Record<string, unknown>>>({});
  const [error, setError] = useState<string | null>(null);
  const [saved, setSaved] = useState<string | null>(null);
  useEffect(() => {
    api("exercisePrefs")
      .then(setPrefs)
      .catch((e) => setError(e.message));
  }, []);
  const shown = kinds.filter((k, i) => FIELDS[k] && kinds.indexOf(k) === i);
  const save = (kind: string, next: Record<string, unknown>) => {
    setPrefs((p) => ({ ...p, [kind]: next }));
    setError(null);
    api("saveExercisePrefs", kind, next)
      .then(() => setSaved(kind))
      .catch((e) => setError(e.message));
  };
  const put = (kind: string, key: string, value: unknown) => {
    const next = { ...(prefs[kind] ?? {}) };
    if (value === "" || value === undefined || (typeof value === "number" && Number.isNaN(value))) delete next[key];
    else next[key] = value;
    save(kind, next);
  };
  return (
    <div className="card level-editor">
      <div className="row" style={{ justifyContent: "space-between" }}>
        <div>
          <div className="title">Settings</div>
          <div className="small faint">Used by every lesson here and in today's set. Leave a field empty for the lesson's own value. Custom levels keep theirs.</div>
        </div>
        <button className="btn quiet small" onClick={onClose}>
          Done
        </button>
      </div>
      {error && <div className="notice error">{error}</div>}
      {shown.map((kind) => {
        const p = prefs[kind] ?? {};
        return (
          <div key={kind} className="section" style={{ gap: 8 }}>
            <div className="row" style={{ justifyContent: "space-between" }}>
              <h3>{FIELDS[kind].title}</h3>
              {Object.keys(p).length > 0 && (
                <button className="btn quiet small" onClick={() => save(kind, {})}>
                  Reset
                </button>
              )}
            </div>
            {FIELDS[kind].fields.map((f) => (
              <div key={f.key} className="level-field">
                <div className="level-field-label">{f.label}</div>
                <div className="level-field-control">
                  {f.type === "number" ? (
                    <span className="row" style={{ gap: 6 }}>
                      <input
                        className="num"
                        type="number"
                        min={f.min}
                        max={f.max}
                        step={f.step ?? 1}
                        placeholder={String(f.placeholder)}
                        value={typeof p[f.key] === "number" ? (p[f.key] as number) : ""}
                        onChange={(e) => setPrefs((all) => ({ ...all, [kind]: { ...p, [f.key]: e.target.value === "" ? undefined : Number(e.target.value) } }))}
                        onBlur={(e) => put(kind, f.key, e.target.value === "" ? "" : Math.min(f.max, Math.max(f.min, Number(e.target.value))))}
                        style={{ width: 80 }}
                      />
                      <span className="small faint">{f.unit}</span>
                    </span>
                  ) : (
                    <select value={typeof p[f.key] === "string" ? (p[f.key] as string) : ""} onChange={(e) => put(kind, f.key, e.target.value)}>
                      <option value="">Lesson's own</option>
                      {f.options.map((o) => (
                        <option key={o.value} value={o.value}>
                          {o.label}
                        </option>
                      ))}
                    </select>
                  )}
                </div>
              </div>
            ))}
            {saved === kind && <div className="small faint">Saved.</div>}
          </div>
        );
      })}
    </div>
  );
}
