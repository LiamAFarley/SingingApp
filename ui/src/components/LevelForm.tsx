import type { ReactNode } from "react";
import {
  CHROMATIC_DEGREES,
  COMPOUND_INTERVALS,
  DIATONIC_DEGREES,
  KEYS,
  MINOR_DEGREES,
  type Opt,
  SCALES,
  SEVENTHS,
  SIMPLE_INTERVALS,
  TRIADS,
  degreeLabel,
} from "../exerciseOptions";

type P = Record<string, unknown>;

function get<T>(p: P, k: string, d: T): T {
  return p[k] === undefined || p[k] === null ? d : (p[k] as T);
}

function Field({ label, hint, children }: { label: string; hint?: string; children: ReactNode }) {
  return (
    <div className="level-field">
      <div className="level-field-label">{label}</div>
      <div className="level-field-control">{children}</div>
      {hint && <div className="small faint">{hint}</div>}
    </div>
  );
}

/** A row of on/off buttons; at least `min` stay on. */
function Toggles({ options, value, onChange, min = 1 }: { options: Opt[]; value: string[]; onChange: (v: string[]) => void; min?: number }) {
  return (
    <div className="toggle-grid">
      {options.map((o) => {
        const on = value.includes(o.id);
        return (
          <button
            key={o.id}
            type="button"
            className={`toggle${on ? " on" : ""}`}
            aria-pressed={on}
            onClick={() => {
              if (on && value.length <= min) return;
              onChange(on ? value.filter((v) => v !== o.id) : options.filter((x) => x.id === o.id || value.includes(x.id)).map((x) => x.id));
            }}
          >
            {o.label}
          </button>
        );
      })}
    </div>
  );
}

/** One of several, as a segmented control. */
function Choice<T extends string>({ options, value, onChange }: { options: { value: T; label: string }[]; value: T; onChange: (v: T) => void }) {
  return (
    <div className="segmented">
      {options.map((o) => (
        <button key={o.value} type="button" className={o.value === value ? "on" : ""} aria-pressed={o.value === value} onClick={() => onChange(o.value)}>
          {o.label}
        </button>
      ))}
    </div>
  );
}

// --- Keys ------------------------------------------------------------------------------------

type KeyMode = "fixed" | "session" | "each";

function keyState(p: P): { mode: KeyMode; tonic: string } {
  const keys = p.keys;
  if (keys === "all") return { mode: get<string>(p, "key_change", "per_block") === "per_trial" ? "each" : "session", tonic: "C" };
  if (Array.isArray(keys) && keys.length === 1) return { mode: "fixed", tonic: String(keys[0]) };
  if (Array.isArray(keys) && keys.length > 1) return { mode: get<string>(p, "key_change", "per_block") === "per_trial" ? "each" : "session", tonic: "C" };
  return { mode: "fixed", tonic: "C" };
}

function KeyFields({ p, set }: { p: P; set: (patch: P) => void }) {
  const k = keyState(p);
  const modes = get<string[]>(p, "modes", ["major"]);
  const modeValue = modes.length > 1 ? "both" : modes[0] === "minor" ? "minor" : "major";
  return (
    <>
      <Field label="Tonic">
        <select
          value={k.mode === "fixed" ? k.tonic : k.mode}
          onChange={(e) => {
            const v = e.target.value;
            if (v === "session") set({ keys: "all", key_change: "per_block" });
            else if (v === "each") set({ keys: "all", key_change: "per_trial" });
            else set({ keys: [v], key_change: "per_block" });
          }}
        >
          {KEYS.map((o) => (
            <option key={o.id} value={o.id}>
              {o.label}
            </option>
          ))}
          <option value="session">One random key for the session</option>
          <option value="each">A new key every question</option>
        </select>
      </Field>
      <Field label="Scale">
        <Choice
          options={[
            { value: "major", label: "Major" },
            { value: "minor", label: "Minor" },
            { value: "both", label: "Both" },
          ]}
          value={modeValue}
          onChange={(v) => set({ modes: v === "both" ? ["major", "minor"] : [v] })}
        />
      </Field>
    </>
  );
}

function CadenceFields({ p, set }: { p: P; set: (patch: P) => void }) {
  const each = get<string>(p, "key_change", "per_block") === "per_trial";
  return (
    <>
      <Field label="Play cadence">
        <Choice
          options={[
            { value: "start", label: "Once at the start" },
            { value: "key_change", label: "When the key changes" },
            { value: "every", label: "Before every question" },
          ]}
          value={get<string>(p, "cadence", each ? "key_change" : "start")}
          onChange={(v) => set({ cadence: v })}
        />
      </Field>
      <Field label="Cadence tempo">
        <span className="row" style={{ gap: 6 }}>
          <input
            className="num"
            type="number"
            min={40}
            max={220}
            value={get<number>(p, "cadence_bpm", 100)}
            onChange={(e) => set({ cadence_bpm: Number(e.target.value) })}
            style={{ width: 72 }}
          />
          <span className="small faint">bpm</span>
        </span>
      </Field>
    </>
  );
}

function LabelField({ p, set }: { p: P; set: (patch: P) => void }) {
  return (
    <Field label="Note names">
      <Choice
        options={[
          { value: "fixed_do", label: "Fixed do (C is do)" },
          { value: "solfege", label: "Movable do" },
          { value: "numbers", label: "Numbers" },
        ]}
        value={get<string>(p, "labels", "fixed_do")}
        onChange={(v) => set({ labels: v })}
      />
    </Field>
  );
}

// --- Per exercise ----------------------------------------------------------------------------

function DegreeForm({ p, set }: { p: P; set: (patch: P) => void }) {
  const modes = get<string[]>(p, "modes", ["major"]);
  const minor = modes.length === 1 && modes[0] === "minor";
  const both = modes.length > 1;
  const explicit = get<string[]>(p, "degrees", []);
  const chromatic = explicit.some((d) => !(minor ? MINOR_DEGREES : DIATONIC_DEGREES).includes(d)) || explicit.length > 8;
  const pool = chromatic ? CHROMATIC_DEGREES : minor ? MINOR_DEGREES : DIATONIC_DEGREES;
  const upper = get<boolean>(p, "upper_tonic", true);
  const selected = explicit.length > 0 ? Array.from(new Set([...explicit, ...(upper && explicit.includes("1") ? ["8"] : [])])) : pool.filter((d) => d !== "8" || upper);
  const octaves = get<number[]>(p, "octaves", [0]);
  const setTones = (tones: string[]) => set({ degrees: tones.filter((t) => t !== "8" || !tones.includes("1")), upper_tonic: tones.includes("8") });
  return (
    <>
      <KeyFields p={p} set={set} />
      <Field label="Range">
        <Choice
          options={[
            { value: "1", label: "1 octave" },
            { value: "2", label: "2 octaves" },
            { value: "3", label: "Many octaves" },
          ]}
          value={String(Math.min(3, octaves.length))}
          onChange={(v) => set({ octaves: v === "1" ? [0] : v === "2" ? [-1, 0] : [-1, 0, 1] })}
        />
      </Field>
      <Field label="Chromatics">
        <label className="row small" style={{ gap: 6 }}>
          <input
            type="checkbox"
            checked={chromatic}
            onChange={(e) => setTones(e.target.checked ? CHROMATIC_DEGREES : minor ? MINOR_DEGREES : DIATONIC_DEGREES)}
          />
          Every degree, in and out of the key
        </label>
      </Field>
      <Field label="Enabled tones" hint={both && !chromatic ? "With both scales, each key uses its own seven degrees." : undefined}>
        {both && !chromatic ? (
          <div className="small faint">All degrees of each key</div>
        ) : (
          <Toggles options={pool.map((d) => ({ id: d, label: degreeLabel(d) }))} value={selected} onChange={setTones} min={2} />
        )}
      </Field>
      <details className="level-advanced">
        <summary>Advanced settings</summary>
        <CadenceFields p={p} set={set} />
        <LabelField p={p} set={set} />
        <Field label="Resolution">
          <Choice
            options={[
              { value: "nearest", label: "To the nearer tonic" },
              { value: "ascending", label: "Up" },
              { value: "descending", label: "Down" },
              { value: "none", label: "None" },
            ]}
            value={get<string>(p, "resolution", "nearest")}
            onChange={(v) => set({ resolution: v })}
          />
        </Field>
      </details>
    </>
  );
}

const DIRECTIONS: Opt[] = [
  { id: "ascending", label: "Ascending" },
  { id: "descending", label: "Descending" },
  { id: "harmonic", label: "Harmonic" },
];

function IntervalForm({ p, set }: { p: P; set: (patch: P) => void }) {
  const inKey = get<boolean>(p, "in_key", false);
  const octaves = get<number[]>(p, "start_octaves", [0]);
  return (
    <>
      <Field label="Intervals">
        <Toggles options={SIMPLE_INTERVALS} value={get<string[]>(p, "intervals", [])} onChange={(v) => set({ intervals: [...v, ...get<string[]>(p, "intervals", []).filter((i) => COMPOUND_INTERVALS.some((c) => c.id === i))] })} min={0} />
        {!inKey && (
          <Toggles
            options={COMPOUND_INTERVALS}
            value={get<string[]>(p, "intervals", [])}
            onChange={(v) => set({ intervals: [...get<string[]>(p, "intervals", []).filter((i) => SIMPLE_INTERVALS.some((c) => c.id === i)), ...v] })}
            min={0}
          />
        )}
      </Field>
      <Field label="Played">
        <Toggles options={DIRECTIONS} value={get<string[]>(p, "directions", ["ascending", "descending"])} onChange={(v) => set({ directions: v })} />
      </Field>
      <Field label="Context">
        <Choice
          options={[
            { value: "free", label: "No key" },
            { value: "key", label: "In a key" },
          ]}
          value={inKey ? "key" : "free"}
          onChange={(v) => set(v === "key" ? { in_key: true, start: "tonic" } : { in_key: false, start: "random" })}
        />
      </Field>
      {inKey && (
        <>
          <KeyFields p={p} set={set} />
          <Field label="First note">
            <Choice
              options={[
                { value: "tonic", label: "The tonic (from do)" },
                { value: "random", label: "Any note of the key" },
              ]}
              value={get<string>(p, "start", "random") === "tonic" ? "tonic" : "random"}
              onChange={(v) => set({ start: v })}
            />
          </Field>
          <Field label="Out-of-key notes">
            <label className="row small" style={{ gap: 6 }}>
              <input type="checkbox" checked={get<boolean>(p, "chromatic", false)} onChange={(e) => set({ chromatic: e.target.checked })} />
              The second note may leave the scale
            </label>
          </Field>
        </>
      )}
      <Field label="Octaves of the first note">
        <Choice
          options={[
            { value: "one", label: "One octave" },
            { value: "three", label: "An octave either way" },
          ]}
          value={octaves.length > 1 ? "three" : "one"}
          onChange={(v) => set({ start_octaves: v === "one" ? [0] : [-1, 0, 1] })}
        />
      </Field>
      <Field label="Other" hint="Intervals outside the set, answered Other, so one interval can be drilled alone.">
        <Choice
          options={[
            { value: "0", label: "None" },
            { value: "0.15", label: "Some (15%)" },
            { value: "0.3", label: "More (30%)" },
          ]}
          value={String(get<number>(p, "other_rate", 0))}
          onChange={(v) => set({ other_rate: Number(v) })}
        />
      </Field>
      {inKey && (
        <details className="level-advanced">
          <summary>Advanced settings</summary>
          <CadenceFields p={p} set={set} />
          <LabelField p={p} set={set} />
        </details>
      )}
    </>
  );
}

function CompareForm({ p, set }: { p: P; set: (patch: P) => void }) {
  const fromPool = get<boolean>(p, "pairs_from_pool", false);
  return (
    <>
      <Field label="Intervals">
        <Toggles options={[...SIMPLE_INTERVALS.slice(1), ...COMPOUND_INTERVALS]} value={get<string[]>(p, "intervals", [])} onChange={(v) => set({ intervals: v })} min={fromPool ? 2 : 1} />
      </Field>
      <Field label="Played">
        <Toggles options={DIRECTIONS} value={get<string[]>(p, "directions", ["ascending", "descending"])} onChange={(v) => set({ directions: v })} />
      </Field>
      <Field label="Second interval">
        <Choice
          options={[
            { value: "pool", label: "Another from the set" },
            { value: "near", label: "A few semitones from the first" },
          ]}
          value={fromPool ? "pool" : "near"}
          onChange={(v) => set({ pairs_from_pool: v === "pool" })}
        />
      </Field>
      {!fromPool && (
        <Field label="Size difference" hint="0 means the same interval: the answer is Same.">
          <Toggles
            options={[0, 1, 2, 3, 4].map((d) => ({ id: String(d), label: d === 0 ? "Same" : `${d} semitone${d > 1 ? "s" : ""}` }))}
            value={get<number[]>(p, "differences", [0, 1, 2]).map(String)}
            onChange={(v) => set({ differences: v.map(Number) })}
          />
        </Field>
      )}
      <Field label="Shared note">
        <Toggles
          options={[
            { id: "first", label: "Same first note" },
            { id: "second", label: "Same last note" },
            { id: "near", label: "Nearby first notes" },
            { id: "none", label: "Nothing shared" },
          ]}
          value={get<string[]>(p, "shared", ["none"])}
          onChange={(v) => set({ shared: v })}
        />
      </Field>
    </>
  );
}

function SingForm({ p, set }: { p: P; set: (patch: P) => void }) {
  const inKey = get<boolean>(p, "in_key", false);
  return (
    <>
      <Field label="Intervals">
        <Toggles options={SIMPLE_INTERVALS.slice(1)} value={get<string[]>(p, "intervals", [])} onChange={(v) => set({ intervals: v })} />
      </Field>
      <Field label="Sung">
        <Toggles
          options={[
            { id: "up", label: "Up" },
            { id: "down", label: "Down" },
          ]}
          value={get<string[]>(p, "directions", ["up", "down"])}
          onChange={(v) => set({ directions: v })}
        />
      </Field>
      <Field label="Context">
        <Choice
          options={[
            { value: "free", label: "No key" },
            { value: "key", label: "In a key" },
          ]}
          value={inKey ? "key" : "free"}
          onChange={(v) => set({ in_key: v === "key" })}
        />
      </Field>
      {inKey && (
        <>
          <KeyFields p={p} set={set} />
          <Field label="First note">
            <Choice
              options={[
                { value: "tonic", label: "The tonic (from do)" },
                { value: "random", label: "Any note of the key" },
              ]}
              value={get<string>(p, "start", "random") === "tonic" ? "tonic" : "random"}
              onChange={(v) => set({ start: v })}
            />
          </Field>
          <Field label="Out-of-key notes">
            <label className="row small" style={{ gap: 6 }}>
              <input type="checkbox" checked={get<boolean>(p, "chromatic", false)} onChange={(e) => set({ chromatic: e.target.checked })} />
              The sung note may leave the scale
            </label>
          </Field>
          <LabelField p={p} set={set} />
        </>
      )}
    </>
  );
}

function ChordForm({ p, set }: { p: P; set: (patch: P) => void }) {
  const inversion = get<boolean>(p, "answer_inversion", false);
  const qualities = get<string[]>(p, "qualities", ["major", "minor", "diminished", "augmented"]);
  const styles = get<string[]>(p, "styles", []);
  return (
    <>
      <Field label="Triads">
        <Toggles options={TRIADS} value={qualities} onChange={(v) => set({ qualities: [...v, ...qualities.filter((q) => SEVENTHS.some((s) => s.id === q))] })} min={0} />
      </Field>
      <Field label="Four-note chords">
        <Toggles options={SEVENTHS} value={qualities} onChange={(v) => set({ qualities: [...qualities.filter((q) => TRIADS.some((s) => s.id === q)), ...v] })} min={0} />
      </Field>
      <Field label="Played">
        <Toggles
          options={[
            { id: "arpeggio", label: "Ascending" },
            { id: "arpeggio_down", label: "Descending" },
            { id: "block", label: "Harmonic" },
          ]}
          value={styles.length ? styles : [get<string>(p, "style", "block")]}
          onChange={(v) => set({ styles: v })}
        />
      </Field>
      <Field label="Inversions">
        <label className="row small" style={{ gap: 6 }}>
          <input type="checkbox" checked={inversion} onChange={(e) => set({ answer_inversion: e.target.checked })} />
          Name the inversion too
        </label>
        <Toggles
          options={[
            { id: "0", label: "Root position" },
            { id: "1", label: "1st" },
            { id: "2", label: "2nd" },
            { id: "3", label: "3rd (sevenths)" },
          ]}
          value={get<number[]>(p, "inversions", [0]).map(String)}
          onChange={(v) => set({ inversions: v.map(Number) })}
        />
      </Field>
    </>
  );
}

function ScaleForm({ p, set }: { p: P; set: (patch: P) => void }) {
  return (
    <>
      <Field label="Scales">
        <Toggles options={SCALES} value={get<string[]>(p, "scales", ["major", "natural_minor", "harmonic_minor", "melodic_minor"])} onChange={(v) => set({ scales: v })} />
      </Field>
      <Field label="Played">
        <Choice
          options={[
            { value: "ascending", label: "Ascending" },
            { value: "descending", label: "Descending" },
            { value: "up_down", label: "Up and down" },
          ]}
          value={get<string>(p, "direction", "ascending")}
          onChange={(v) => set({ direction: v })}
        />
      </Field>
    </>
  );
}

function MelodyForm({ p, set }: { p: P; set: (patch: P) => void }) {
  const melody = get<P>(p, "melody", {});
  const g = get<P>(melody, "generate", {});
  const put = (patch: P) => set({ melody: { generate: { ...g, ...patch } } });
  const keys = get<string[]>(g, "keys", []);
  const minor = get<string[]>(g, "modes", ["major"]).includes("minor");
  const keyValue = minor ? (keys.length === 1 ? "a_minor" : "any_minor") : keys.length === 1 && keys[0] === "C" ? "c_major" : "any_major";
  const notes = g.notes as number | undefined;
  const degrees = get<number[]>(g, "degrees", []);
  const names = minor ? ["do", "re", "me", "fa", "sol", "le", "te", "do'"] : ["do", "re", "mi", "fa", "sol", "la", "ti", "do'"];
  return (
    <>
      <Field label="Key">
        <Choice
          options={[
            { value: "c_major", label: "C major" },
            { value: "any_major", label: "Any major key" },
            { value: "a_minor", label: "A minor" },
            { value: "any_minor", label: "Any minor key" },
          ]}
          value={keyValue}
          onChange={(v) =>
            put(
              v === "c_major"
                ? { keys: ["C"], modes: ["major"] }
                : v === "any_major"
                  ? { keys: [], modes: ["major"] }
                  : v === "a_minor"
                    ? { keys: ["A"], modes: ["minor"] }
                    : { keys: [], modes: ["minor"] },
            )
          }
        />
      </Field>
      <Field label="Notes used" hint="None selected: every note in the range.">
        <Toggles
          options={names.map((n, i) => ({ id: String(i + 1), label: n }))}
          value={degrees.map(String)}
          onChange={(v) => put({ degrees: v.map(Number) })}
          min={0}
        />
      </Field>
      <Field label="Length">
        <select
          value={notes ? `n${notes}` : `b${get<number>(g, "bars", 2)}`}
          onChange={(e) => {
            const v = e.target.value;
            if (v.startsWith("n")) put({ notes: Number(v.slice(1)) });
            else put({ notes: undefined, bars: Number(v.slice(1)) });
          }}
        >
          {[3, 4, 5, 6, 7, 8].map((n) => (
            <option key={`n${n}`} value={`n${n}`}>
              {n} notes, no rhythm
            </option>
          ))}
          {[1, 2, 4].map((b) => (
            <option key={`b${b}`} value={`b${b}`}>
              {b} bar{b > 1 ? "s" : ""} with rhythm
            </option>
          ))}
        </select>
      </Field>
      {!notes && (
        <>
          <Field label="Time signature">
            <Choice
              options={[
                { value: "4/4", label: "4/4" },
                { value: "3/4", label: "3/4" },
                { value: "6/8", label: "6/8" },
              ]}
              value={get<string[]>(g, "meters", ["4/4"])[0] ?? "4/4"}
              onChange={(v) => put({ meters: [v] })}
            />
          </Field>
          <Field label="Note values">
            <Toggles
              options={[
                { id: "whole", label: "Whole" },
                { id: "dotted_half", label: "Dotted half" },
                { id: "half", label: "Half" },
                { id: "dotted_quarter", label: "Dotted quarter" },
                { id: "quarter", label: "Quarter" },
                { id: "eighth", label: "Eighth" },
              ]}
              value={get<string[]>(g, "rhythms", ["half", "quarter"])}
              onChange={(v) => put({ rhythms: v })}
            />
          </Field>
        </>
      )}
      <Field label="Largest leap">
        <select value={get<number>(g, "max_leap", 2)} onChange={(e) => put({ max_leap: Number(e.target.value) })}>
          {[1, 2, 3, 4, 5, 7].map((n) => (
            <option key={n} value={n}>
              {["", "Steps only", "A third", "A fourth", "A fifth", "A sixth", "", "An octave"][n]}
            </option>
          ))}
        </select>
      </Field>
    </>
  );
}

/** The settings of one exercise, as a form. Unknown params are kept as they are. */
export function LevelForm({ exercise, params, onChange }: { exercise: string; params: P; onChange: (p: P) => void }) {
  const set = (patch: P) => {
    const next = { ...params, ...patch };
    for (const [k, v] of Object.entries(patch)) if (v === undefined) delete next[k];
    onChange(next);
  };
  switch (exercise) {
    case "scale_degree_id":
      return <DegreeForm p={params} set={set} />;
    case "interval_id":
      return <IntervalForm p={params} set={set} />;
    case "interval_comparison":
      return <CompareForm p={params} set={set} />;
    case "sing_interval":
      return <SingForm p={params} set={set} />;
    case "chord_id":
      return <ChordForm p={params} set={set} />;
    case "scale_id":
      return <ScaleForm p={params} set={set} />;
    case "melodic_dictation":
    case "sight_sing":
      return <MelodyForm p={params} set={set} />;
    default:
      return <div className="small faint">No form for {exercise} yet.</div>;
  }
}
