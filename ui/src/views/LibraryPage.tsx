import { lazy, Suspense, useCallback, useEffect, useMemo, useRef, useState } from "react";
import { api, useBackendEvent } from "../api";
import type { ImportResult } from "../bindings/ImportResult";
import type { LibraryEntry } from "../bindings/LibraryEntry";
import type { LibraryView } from "../bindings/LibraryView";
import type { Score } from "../bindings/Score";

const Staff = lazy(() => import("../notation/Staff").then((m) => ({ default: m.Staff })));

const UNSORTED = "Unsorted";
/** Longest session started from a collection. */
const MAX_TRIALS = 40;

const EXAMPLE = `{
  "title": "Exercise 12",
  "key": "D major",
  "time": "3/4",
  "tempo_bpm": 72,
  "notes": "A4:1 | D5:2 C#5:1 | B4:1 A4 G4 | F#4:3"
}`;

type Kind = "sight_sing" | "melodic_dictation";

interface Group {
  name: string;
  collection: string | null;
  pieces: LibraryEntry[];
}

export function LibraryPage({ sessionActive, onOpenSession }: { sessionActive: boolean; onOpenSession: () => void }) {
  const [view, setView] = useState<LibraryView | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [imported, setImported] = useState<ImportResult[] | null>(null);
  const [importTo, setImportTo] = useState("");
  const [filter, setFilter] = useState("");
  const [open, setOpen] = useState<string | null>(null);
  const [playing, setPlaying] = useState<string | null>(null);
  const [deleting, setDeleting] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const fileInput = useRef<HTMLInputElement>(null);
  const playTimer = useRef<number | undefined>(undefined);

  const refresh = useCallback(() => {
    api("libraryView")
      .then(setView)
      .catch((e) => setError(e.message));
  }, []);
  useEffect(refresh, [refresh]);
  useBackendEvent("library-changed", refresh);
  // The coach's MCP server writes to the library too.
  useBackendEvent("queue-changed", refresh);
  useEffect(() => () => window.clearTimeout(playTimer.current), []);

  const groups = useMemo(() => groupPieces(view?.pieces ?? [], filter), [view, filter]);
  const collections = useMemo(
    () => [...new Set((view?.pieces ?? []).map((p) => p.collection).filter((c): c is string => c != null))].sort(),
    [view],
  );

  const importFiles = async (list: FileList | null) => {
    if (!list || list.length === 0) return;
    setError(null);
    setBusy(true);
    try {
      const files = await Promise.all(
        [...list].map(async (f) => ({ name: f.name, data: [...new Uint8Array(await f.arrayBuffer())] })),
      );
      setImported(await api("libraryImport", files, importTo.trim() || null));
      refresh();
    } catch (e) {
      setError((e as Error).message);
    } finally {
      setBusy(false);
      if (fileInput.current) fileInput.current.value = "";
    }
  };

  const start = async (kind: Kind, title: string, library: { ids: string[] }, count: number) => {
    setError(null);
    const what = kind === "sight_sing" ? "Sight singing" : "Dictation";
    const spec = {
      title: `${what}: ${title}`,
      blocks: [{ type: kind, title, count: Math.min(count, MAX_TRIALS), params: { melody: { library } } }],
    };
    try {
      await api("startSession", { source: "spec", spec });
      onOpenSession();
    } catch (e) {
      setError((e as Error).message);
    }
  };

  const play = async (id: string) => {
    window.clearTimeout(playTimer.current);
    if (playing === id) {
      setPlaying(null);
      await api("stopPreview").catch(() => undefined);
      return;
    }
    try {
      const ms = await api("libraryPlay", id, null);
      setPlaying(id);
      playTimer.current = window.setTimeout(() => setPlaying(null), ms + 300);
    } catch (e) {
      setError((e as Error).message);
    }
  };

  const remove = async (file: string) => {
    setDeleting(null);
    try {
      await api("libraryRemoveFile", file);
      refresh();
    } catch (e) {
      setError((e as Error).message);
    }
  };

  const pieces = view?.pieces ?? [];
  const failed = imported?.filter((r) => r.error) ?? [];

  return (
    <div className="page">
      <div className="page-head">
        <div>
          <h1>Library</h1>
          <div className="sub">Melodies read from JSON and MIDI files in the library folder. A subfolder is a collection.</div>
        </div>
        <div className="row">
          <button className="btn" onClick={() => api("libraryOpenFolder").catch((e) => setError(e.message))}>
            Open folder
          </button>
        </div>
      </div>

      {error && <div className="notice error">{error}</div>}

      <div className="card library-import">
        <div className="row" style={{ gap: 12, flexWrap: "wrap" }}>
          <button className="btn primary" disabled={busy} onClick={() => fileInput.current?.click()}>
            {busy ? "Importing…" : "Import files…"}
          </button>
          <label className="row small muted" style={{ gap: 8 }}>
            into
            <input
              type="text"
              list="library-collections"
              placeholder="Collection (optional)"
              value={importTo}
              onChange={(e) => setImportTo(e.target.value)}
            />
          </label>
          <datalist id="library-collections">
            {collections.map((c) => (
              <option key={c} value={c} />
            ))}
          </datalist>
          <span className="small faint">.json, .mid or .midi. Files are copied into the folder, which is watched for changes.</span>
          <input
            ref={fileInput}
            type="file"
            multiple
            accept=".json,.mid,.midi"
            style={{ display: "none" }}
            onChange={(e) => importFiles(e.target.files)}
          />
        </div>
        {view && <div className="small faint folder-path">{view.folder}</div>}
        {imported && (
          <div className={`notice ${failed.length > 0 ? "error" : "info"}`} style={{ marginTop: 12, marginBottom: 0 }}>
            {imported.length - failed.length > 0 && (
              <div>
                Imported {imported.length - failed.length} file{imported.length - failed.length === 1 ? "" : "s"}.
              </div>
            )}
            {failed.map((r) => (
              <div key={r.name}>
                <strong>{r.name}</strong>: {r.error}
              </div>
            ))}
          </div>
        )}
      </div>

      {view && view.errors.length > 0 && (
        <div className="notice error">
          <div>
            {view.errors.length === 1 ? "A file" : `${view.errors.length} files`} in the folder could not be read:
          </div>
          {view.errors.map((f) => (
            <div key={f.file}>
              <strong>{f.file}</strong>: {f.error}
            </div>
          ))}
        </div>
      )}

      {view && pieces.length === 0 && <EmptyLibrary />}

      {pieces.length > 0 && (
        <div className="row" style={{ marginBottom: 8 }}>
          <input
            type="text"
            className="grow"
            placeholder="Filter by title, collection or tag"
            value={filter}
            onChange={(e) => setFilter(e.target.value)}
          />
          <span className="small faint num">
            {pieces.length} piece{pieces.length === 1 ? "" : "s"}
          </span>
        </div>
      )}

      {groups.map((g) => (
        <div className="section" key={g.name}>
          <div className="section-head">
            <h2>{g.name}</h2>
            <div className="row">
              <span className="small faint num">
                {g.pieces.length} piece{g.pieces.length === 1 ? "" : "s"}
              </span>
              <button
                className="btn small"
                disabled={sessionActive}
                onClick={() => start("sight_sing", g.name, selection(g), g.pieces.length)}
              >
                Sight-sing
              </button>
              <button
                className="btn small"
                disabled={sessionActive}
                onClick={() => start("melodic_dictation", g.name, selection(g), g.pieces.length)}
              >
                Dictation
              </button>
            </div>
          </div>
          <div className="card flush">
            <table className="library-table">
              <colgroup>
                <col style={{ width: "32%" }} />
                <col style={{ width: "11%" }} />
                <col style={{ width: "7%" }} />
                <col style={{ width: "6%" }} />
                <col style={{ width: "10%" }} />
                <col style={{ width: "12%" }} />
                <col style={{ width: "22%" }} />
              </colgroup>
              <thead>
                <tr>
                  <th>Title</th>
                  <th>Key</th>
                  <th>Time</th>
                  <th className="num">Bars</th>
                  <th>Range</th>
                  <th>Parts</th>
                  <th />
                </tr>
              </thead>
              <tbody>
                {g.pieces.map((p) => (
                  <PieceRow
                    key={p.id}
                    piece={p}
                    open={open === p.id}
                    playing={playing === p.id}
                    confirmDelete={deleting === p.id}
                    sessionActive={sessionActive}
                    sameFile={pieces.filter((x) => x.file === p.file).length}
                    onToggle={() => setOpen(open === p.id ? null : p.id)}
                    onPlay={() => play(p.id)}
                    onSing={() => start("sight_sing", p.title, { ids: [p.id] }, 1)}
                    onDelete={() => setDeleting(p.id)}
                    onCancelDelete={() => setDeleting(null)}
                    onConfirmDelete={() => remove(p.file)}
                  />
                ))}
              </tbody>
            </table>
          </div>
        </div>
      ))}
    </div>
  );
}

/** The pieces listed under a heading (sub-collections have their own). */
function selection(g: Group): { ids: string[] } {
  return { ids: g.pieces.map((p) => p.id) };
}

function groupPieces(pieces: LibraryEntry[], filter: string): Group[] {
  const q = filter.trim().toLowerCase();
  const match = (p: LibraryEntry) =>
    !q ||
    p.title.toLowerCase().includes(q) ||
    p.id.toLowerCase().includes(q) ||
    (p.collection ?? "").toLowerCase().includes(q) ||
    p.tags.some((t) => t.toLowerCase().includes(q));
  const map = new Map<string, Group>();
  for (const p of pieces.filter(match)) {
    const name = p.collection ?? UNSORTED;
    const g = map.get(name) ?? { name, collection: p.collection, pieces: [] };
    g.pieces.push(p);
    map.set(name, g);
  }
  return [...map.values()].sort((a, b) =>
    a.name === UNSORTED ? 1 : b.name === UNSORTED ? -1 : a.name.localeCompare(b.name, undefined, { numeric: true }),
  );
}

function PieceRow({
  piece: p,
  open,
  playing,
  confirmDelete,
  sessionActive,
  sameFile,
  onToggle,
  onPlay,
  onSing,
  onDelete,
  onCancelDelete,
  onConfirmDelete,
}: {
  piece: LibraryEntry;
  open: boolean;
  playing: boolean;
  confirmDelete: boolean;
  sessionActive: boolean;
  sameFile: number;
  onToggle: () => void;
  onPlay: () => void;
  onSing: () => void;
  onDelete: () => void;
  onCancelDelete: () => void;
  onConfirmDelete: () => void;
}) {
  return (
    <>
      <tr className={`clickable ${open ? "selected" : ""}`} onClick={onToggle}>
        <td>
          <div>{p.title}</div>
          {(p.id !== p.title || p.tags.length > 0) && (
            <div className="small faint">{[p.id !== p.title ? p.id : null, ...p.tags].filter(Boolean).join(" · ")}</div>
          )}
        </td>
        <td>{p.key}</td>
        <td className="num">{p.time}</td>
        <td className="num">{p.bars}</td>
        <td className="num">{p.low && p.high ? `${p.low}–${p.high}` : ""}</td>
        <td className="muted">{p.parts.length > 1 ? p.parts.join(", ") : ""}</td>
        <td onClick={(e) => e.stopPropagation()}>
          <div className="row" style={{ justifyContent: "flex-end", gap: 6 }}>
            <button className="btn small quiet" onClick={onPlay}>
              {playing ? "Stop" : "Play"}
            </button>
            <button className="btn small" disabled={sessionActive} onClick={onSing}>
              Sing
            </button>
            <button className="btn small quiet" onClick={onDelete} title={`Delete ${p.file}`}>
              Delete
            </button>
          </div>
        </td>
      </tr>
      {confirmDelete && (
        <tr className="library-confirm">
          <td colSpan={7}>
            <div className="row" style={{ justifyContent: "space-between" }}>
              <span>
                Delete <strong>{p.file}</strong> from the library folder
                {sameFile > 1 ? `, with all ${sameFile} pieces in it` : ""}?
              </span>
              <div className="row" style={{ gap: 6 }}>
                <button className="btn small quiet" onClick={onCancelDelete}>
                  Cancel
                </button>
                <button className="btn small danger" onClick={onConfirmDelete}>
                  Delete file
                </button>
              </div>
            </div>
          </td>
        </tr>
      )}
      {open && (
        <tr className="library-preview">
          <td colSpan={7}>
            <Preview id={p.id} parts={p.parts} />
          </td>
        </tr>
      )}
    </>
  );
}

function Preview({ id, parts }: { id: string; parts: string[] }) {
  const [scores, setScores] = useState<Score[] | null>(null);
  const [error, setError] = useState<string | null>(null);
  useEffect(() => {
    let alive = true;
    api("libraryScores", id)
      .then((s) => alive && setScores(s))
      .catch((e) => alive && setError(e.message));
    return () => {
      alive = false;
    };
  }, [id]);
  if (error) return <div className="small muted">{error}</div>;
  if (!scores) return <div className="small faint">Loading…</div>;
  return (
    <Suspense fallback={<div className="small faint">Loading…</div>}>
      {scores.map((s, k) => (
        <div key={k} className="library-part">
          {scores.length > 1 && <div className="small muted">{parts[k]}</div>}
          {/* One scale for every part, so the voices line up by eye. */}
          <Staff score={s} scale={scores.length > 1 ? 1.3 : undefined} maxHeight={320} />
        </div>
      ))}
    </Suspense>
  );
}

function EmptyLibrary() {
  return (
    <div className="card library-help">
      <div className="title">The library is empty</div>
      <p className="desc">
        Put JSON or MIDI files in the library folder, import them above, or ask your coach to transcribe pieces with{" "}
        <code>add_library_pieces</code>. Pieces can then be sight-sung or taken as dictation, and your coach can use them
        in sessions.
      </p>
      <div className="kv-help">
        <div>
          <div className="small muted">A melody in JSON</div>
          <pre className="code">{EXAMPLE}</pre>
        </div>
        <ul className="small muted">
          <li>
            <code>pitch:beats</code>, counted in the time signature's unit. A length carries over to the next note.
          </li>
          <li>
            <code>R</code> is a rest, <code>~</code> ties to the next note, and every <code>|</code> is a barline that
            is checked. A short first bar is a pickup.
          </li>
          <li>
            Two or more voices go in <code>"parts": [{"{"}"name": "Upper", "notes": "…"{"}"}, …]</code>.
          </li>
          <li>MIDI: one part per track, times rounded to sixteenths, key from the key signature or estimated.</li>
        </ul>
      </div>
    </div>
  );
}
