import { lazy, Suspense, useEffect, useState } from "react";
import { api, command } from "../api";
import type { BlockReport } from "../bindings/BlockReport";
import type { LaneSpec } from "../bindings/LaneSpec";
import type { RunnerState } from "../bindings/RunnerState";
import type { Score } from "../bindings/Score";
import type { ScoreNote } from "../bindings/ScoreNote";
import type { TrialView } from "../bindings/TrialView";
import { ChoiceGrid, DegreePad, type KeyState, NotesPad, PianoPad } from "../components/AnswerPads";
import { LockOnTuner } from "../components/LockOnTuner";
import { PitchLane } from "../components/PitchLane";
import { SessionOverview } from "../components/SessionOverview";
import { tint } from "../colors";
import { offerKey, useKeyOwner } from "../keyboard";
import { live, useAnimationFrame } from "../live";
import { beatTicks, describeKey } from "../notation/layout";

// VexFlow is large; load it with the first notated exercise.
const Staff = lazy(() => import("../notation/Staff").then((m) => ({ default: m.Staff })));
const ScoreEditor = lazy(() => import("../notation/ScoreEditor").then((m) => ({ default: m.ScoreEditor })));
import { describeCents, fmtCents, fmtMs, KIND_TITLES, noteDisplay, PAIRING_COLORS, pct, traceStyle } from "../music";

const FEEDBACK_TEXT: Record<string, string> = {
  continuous: "Live pitch line and results after each trial",
  after_trial: "Results after each trial",
  after_block: "Results at the end of the block",
  end_of_session: "Results at the end of the session",
  none: "No feedback (assessment)",
  errorless: "The right answer lights up. A miss is not marked.",
};

export function SessionView({ runner, onClose }: { runner: RunnerState; onClose: () => void }) {
  const phase = runner.phase;
  const trial = runner.trial;
  useAnimationFrame(trial?.interaction === "pairing" || trial?.interaction === "detect");
  const [confirmEnd, setConfirmEnd] = useState(false);
  const [overview, setOverview] = useState(false);

  useEffect(() => {
    if (phase === "session_summary" || phase === "idle") setConfirmEnd(false);
  }, [phase]);

  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      const target = e.target as HTMLElement;
      if (target && ["INPUT", "TEXTAREA", "SELECT"].includes(target.tagName)) return;
      const k = e.key.toLowerCase();
      const fb = runner.feedback;
      if (confirmEnd) {
        // Ending takes a click. Esc keeps going; any other key also dismisses the question and
        // then works as usual (so notes typed into the editor are not lost).
        setConfirmEnd(false);
        if (k === "escape") {
          e.preventDefault();
          e.stopImmediatePropagation();
          return;
        }
      }
      // The score editor (dictation) gets first refusal, modifiers and repeats included.
      if (offerKey(e)) {
        e.preventDefault();
        e.stopImmediatePropagation();
        return;
      }
      if (e.repeat) return;
      if (e.ctrlKey || e.metaKey || e.altKey) return;
      if ((k === " " || k === "enter") && trial?.interaction === "detect" && (phase === "listening" || phase === "answering")) {
        e.preventDefault();
        command({ kind: "tap" });
        return;
      }
      if (k === " " || k === "enter") {
        e.preventDefault();
        if (phase === "block_intro") command({ kind: "begin" });
        else if (phase === "feedback" || phase === "block_summary") command({ kind: "next" });
        else if (phase === "session_summary" || phase === "error") command({ kind: "close" });
        return;
      }
      if (k === "escape") {
        if (overview) setOverview(false);
        else if (phase !== "session_summary" && phase !== "error") setConfirmEnd(true);
        return;
      }
      if (k === "v") {
        e.preventDefault();
        setOverview((o) => !o);
        return;
      }
      if (phase === "answering" && trial?.interaction === "choice") {
        if (k === "r" && trial.can_replay) command({ kind: "replay" });
        // Not F: that key answers F on the piano pad.
        if (k === "n" && trial.can_reference) command({ kind: "reference" });
        return;
      }
      if (phase === "feedback" && fb) {
        if (k === "a" && fb.can_show_answer) command({ kind: "show_answer" });
        if (k === "l" && fb.can_listen) command({ kind: "listen" });
        if (k === "g" && fb.can_sing_along) command({ kind: "sing_along" });
        if (k === "t" && fb.can_retry) command({ kind: "retry" });
        if (k === "p" && fb.can_play_answer) command({ kind: "play_answer" });
      }
      if (k === "s" && (phase === "recording" || phase === "listening" || phase === "scrubbing")) command({ kind: "skip" });
    };
    // Capture phase, so the answer pads never see keys meant for the end-session question.
    window.addEventListener("keydown", onKey, true);
    return () => window.removeEventListener("keydown", onKey, true);
  }, [phase, trial, runner.feedback, confirmEnd, overview]);

  const blockProgress =
    phase === "block_summary" || phase === "session_summary"
      ? 1
      : runner.trial_count > 0
        ? (runner.trial_index + (phase === "feedback" ? 1 : 0)) / runner.trial_count
        : 0;

  const flash =
    (phase === "feedback" && runner.feedback?.flash_color) ||
    (trial?.interaction === "pairing" && trial.flash_color && trial.label_at_s != null && live.now() >= trial.label_at_s
      ? trial.flash_color
      : null) ||
    (trial?.interaction === "detect" ? trial.flash_color : null);
  // After a colour answer the whole screen takes the right colour, with readable text.
  const flashing = phase === "feedback" && !!runner.feedback?.flash_color;
  const fill = flashing ? tint(runner.feedback!.flash_color!, "#ffffff", 1) : undefined;
  return (
    <div
      className={`session ${flashing ? "flashing" : ""}`}
      style={fill ?? (flash ? { background: `color-mix(in srgb, ${flash} 16%, var(--bg))` } : undefined)}
    >
      <div className="session-head">
        <div>
          <h1>{runner.title}</h1>
          <div className="muted small" style={{ marginTop: 2 }}>
            {phase === "session_summary" ? (
              `${runner.block_count} ${runner.block_count === 1 ? "block" : "blocks"}`
            ) : (
              <>
                {runner.block_count > 1 && `Block ${runner.block_index + 1} of ${runner.block_count} · `}
                {runner.block_title}
                {phase !== "block_intro" && phase !== "block_summary" && runner.trial_count > 0 && (
                  <> · Trial {Math.min(runner.trial_index + 1, runner.trial_count)} of {runner.trial_count}</>
                )}
              </>
            )}
          </div>
        </div>
        <div className="row">
          {runner.done_in_block > 0 && runner.feedback_mode !== "none" && runner.feedback_mode !== "after_block" && runner.feedback_mode !== "end_of_session" && (
            <span className="badge num">
              {runner.correct_in_block} / {runner.done_in_block} correct
            </span>
          )}
          {runner.outline.length > 0 && !confirmEnd && (
            <button className="btn quiet" onClick={() => setOverview((o) => !o)}>
              Overview <span className="kbd">V</span>
            </button>
          )}
          {runner.block_count > 1 && runner.block_index + 1 < runner.block_count && phase !== "session_summary" && phase !== "error" && !confirmEnd && (
            <button className="btn quiet" onClick={() => command({ kind: "skip_part" })} title="Leave this part and go on to the next">
              Skip part
            </button>
          )}
          {phase !== "session_summary" && !confirmEnd && (
            <button className="btn quiet" onClick={() => setConfirmEnd(true)}>
              End session <span className="kbd">Esc</span>
            </button>
          )}
          {confirmEnd && (
            <>
              <span className="small muted">End this session? Trials so far are kept, and today's set can be resumed.</span>
              <button className="btn" onClick={() => setConfirmEnd(false)}>
                Keep going <span className="kbd">Esc</span>
              </button>
              <button
                className="btn danger"
                onClick={() => {
                  setConfirmEnd(false);
                  command({ kind: "abort" });
                }}
              >
                End session
              </button>
            </>
          )}
        </div>
      </div>
      <div className="progress">
        <div style={{ width: `${Math.min(1, blockProgress) * 100}%` }} />
      </div>
      <div className="stage">
        {overview && <SessionOverview runner={runner} onClose={() => setOverview(false)} />}
        {phase === "block_intro" && <BlockIntro runner={runner} />}
        {phase === "scrubbing" && <Scrubbing runner={runner} />}
        {(phase === "preparing" || phase === "listening" || phase === "answering" || phase === "recording" || phase === "grading" || phase === "feedback") &&
          trial && <TrialStage runner={runner} trial={trial} />}
        {phase === "preparing" && !trial && <div className="prompt-sub">Preparing…</div>}
        {phase === "block_summary" && <BlockSummary report={runner.block_report} />}
        {phase === "session_summary" && <SessionSummary runner={runner} onClose={onClose} />}
        {phase === "error" && (
          <div className="section">
            <div className="notice error">{runner.error}</div>
            <div className="controls">
              <button className="btn primary" onClick={() => command({ kind: "close" })}>
                Close
              </button>
            </div>
          </div>
        )}
      </div>
    </div>
  );
}

function BlockIntro({ runner }: { runner: RunnerState }) {
  return (
    <div className="section" style={{ alignItems: "center", textAlign: "center", gap: 14, margin: "auto 0" }}>
      {runner.block_index === 0 && runner.notes && <div className="notice info" style={{ maxWidth: 620, textAlign: "left" }}>{runner.notes}</div>}
      <h2 style={{ fontSize: 20 }}>{runner.block_title}</h2>
      <div className="muted">
        {[
          runner.block_kind && KIND_TITLES[runner.block_kind] !== runner.block_title ? KIND_TITLES[runner.block_kind] : null,
          `${runner.trial_count} ${runner.trial_count === 1 ? "trial" : "trials"}`,
          FEEDBACK_TEXT[runner.feedback_mode],
        ]
          .filter(Boolean)
          .join(" · ")}
      </div>
      {runner.block_instructions && <p style={{ maxWidth: 560 }}>{runner.block_instructions}</p>}
      <KindHelp kind={runner.block_kind} />
      <div className="controls" style={{ marginTop: 8 }}>
        <button className="btn primary" onClick={() => command({ kind: "begin" })} autoFocus>
          Begin <span className="kbd" style={{ color: "#fff", borderColor: "rgba(255,255,255,.5)", background: "transparent" }}>Space</span>
        </button>
      </div>
    </div>
  );
}

/** The clearing glissando before a block answered without help. */
function Scrubbing({ runner }: { runner: RunnerState }) {
  useAnimationFrame(true);
  const end = runner.scrub_end_s ?? live.now();
  const [total] = useState(() => Math.max(1, end - live.now()));
  const left = Math.max(0, end - live.now());
  return (
    <div className="section" style={{ alignItems: "center", textAlign: "center", gap: 12, margin: "auto 0" }}>
      <h2 style={{ fontSize: 20 }}>Clearing pitch memory</h2>
      <p className="muted" style={{ maxWidth: 520 }}>
        An endlessly falling tone washes out the notes you heard last, so the next answers come from your memory of each
        note rather than from the last one you were told.
      </p>
      <div className="countdown" style={{ width: 320 }}>
        <div style={{ width: `${(left / total) * 100}%` }} />
      </div>
      <div className="small faint num">{Math.ceil(left)} s</div>
      <div className="controls">
        <button className="btn quiet" onClick={() => command({ kind: "skip" })}>
          Skip <span className="kbd">S</span>
        </button>
      </div>
    </div>
  );
}

function KindHelp({ kind }: { kind: string | null }) {
  const text: Record<string, string> = {
    pitch_id: "A tone plays with no reference. Click the note or use the keys A–J (white) and W, E, T, Y, U (black).",
    sing_pitch: "When the prompt appears, sing the named note on a comfortable vowel. Octave does not matter.",
    pitch_pairing: "Listen. Each tone is followed by its name.",
    tonal_center_id: "A short cadence plays. Name its key (the tonic).",
    interval_id: "Two notes play. Name the interval. Number keys pick answers.",
    chord_id: "A chord plays. Name its quality. Number keys pick answers.",
    scale_id: "A scale plays. Name it. Number keys pick answers.",
    pitch_match: "A note plays, then sing it back and hold it.",
    sustain: "Hold the note steadily until the bar ends. Vibrato is fine; the note centre is what counts.",
    sing_interval: "The first note sounds during the count-in. Sing both notes in time with the clicks.",
    pattern: "Follow the bars in time with the metronome. In echo mode you hear the pattern first, then sing it.",
    sight_sing: "Read the melody on the staff. The key and first note are played, then a bar is counted in: sing in time with the clicks.",
    melodic_dictation: "Listen, then write the melody on the staff. 2–6 choose the length, A–G enter notes in the key, Enter checks.",
    anchor_song: "Sing the first note of each song as it sounds on the original recording, or say whether the opening is in its original key.",
    incidental_pairing: "A tone plays and a colour appears. Press the colour as fast as you can with the number keys. Nothing is asked about the tone.",
    scale_degree_id: "A cadence sets the key, then one note plays. Name its scale degree (number keys 1–7). Afterwards the note walks to the tonic so you hear where it wants to go.",
    interval_comparison: "Two intervals play. Is the first or the second larger, or are they the same? Number keys pick answers.",
    harmony: "The key and your first note are played, then a bar is counted in. Sing your part in time while the other voice plays.",
    pitch_detect: "The target note is named and sung. Then a stream of tones. Press Space whenever you hear that note, in any octave.",
    chord_color_id: "A chord plays. Name its colour. Number keys pick colours. Inversions of the same chord have different colours.",
  };
  return kind && text[kind] ? <p className="muted small" style={{ maxWidth: 560 }}>{text[kind]}</p> : null;
}

function keyStates(runner: RunnerState, picked: string | null): Record<string, KeyState> {
  const fb = runner.feedback;
  const states: Record<string, KeyState> = {};
  // A colour flash replaces the marks: the screen itself shows the answer.
  if (runner.phase === "feedback" && fb?.flash_color) return states;
  if (runner.phase === "feedback" && fb && !fb.hidden) {
    if (fb.correct_choice) states[fb.correct_choice] = "correct";
    if (fb.answered && fb.answered !== fb.correct_choice && !fb.errorless) states[fb.answered] = "wrong";
  } else if (picked) {
    states[picked] = "picked";
  }
  return states;
}

function TrialStage({ runner, trial }: { runner: RunnerState; trial: TrialView }) {
  switch (trial.interaction) {
    case "choice":
      return <ChoiceStage runner={runner} trial={trial} />;
    case "sing_pitch":
      return <SingPitchStage runner={runner} trial={trial} />;
    case "pairing":
      return <PairingStage trial={trial} />;
    case "sung":
      return <SungStage runner={runner} trial={trial} />;
    case "dictation":
      return <DictationStage runner={runner} trial={trial} />;
    case "detect":
      return <DetectStage runner={runner} trial={trial} />;
  }
}

function ChoiceStage({ runner, trial }: { runner: RunnerState; trial: TrialView }) {
  const [picked, setPicked] = useState<string | null>(null);
  useEffect(() => setPicked(null), [runner.trial_index, trial.attempt]);
  const phase = runner.phase;
  const fb = runner.feedback;
  const answering = phase === "answering";
  // After answering, every answer can be clicked to hear what it sounds like.
  const hear = phase === "feedback" && !!fb?.can_hear_choices;
  const enabled = answering || hear;
  const pick = (id: string) => {
    if (answering) {
      setPicked(id);
      command({ kind: "answer", choice: id });
    } else if (hear) {
      command({ kind: "play_choice", choice: id });
    }
  };
  const states = keyStates(runner, picked);
  // The answer whose note is sounding now (a degree walking to the tonic lights each step).
  const marks = phase === "feedback" ? (fb?.lit ?? []) : [];
  useAnimationFrame(marks.length > 0);
  const now = live.now();
  const lit = marks.find((m) => now >= m.from_s && now < m.to_s)?.id ?? null;
  // A score read before it plays (error detection): count down to the playing.
  useAnimationFrame(!!trial.score && phase === "listening");
  const playsIn = trial.score && phase === "listening" && trial.response_start_s != null ? trial.response_start_s - live.now() : 0;
  const sub =
    phase === "preparing" || phase === "listening"
      ? playsIn > 0.5
        ? `Read and audiate it: it plays once in ${Math.ceil(playsIn)} s`
        : "Listen…"
      : answering
        ? trial.deadline_ms
          ? <Deadline trial={trial} />
          : "Your answer"
        : hear
          ? "Click any answer to hear it"
          : "";
  return (
    <>
      <div>
        <div className="prompt">{trial.prompt}</div>
        <div className="prompt-sub">{sub}</div>
      </div>
      {trial.score && (
        <Suspense fallback={<div className="staff-wrap" style={{ minHeight: 160 }} />}>
          <Staff score={trial.score} maxHeight={260} />
        </Suspense>
      )}
      {trial.layout === "pitch_classes" && (
        <PianoPad choices={trial.choices} enabled={enabled} states={states} onPick={pick} keyboard={answering} />
      )}
      {trial.layout === "notes" && <NotesPad choices={trial.choices} enabled={enabled} states={states} onPick={pick} />}
      {trial.layout === "list" && <ChoiceGrid choices={trial.choices} enabled={enabled} states={states} onPick={pick} />}
      {trial.layout === "colors" && <ColorTask trial={trial} enabled={enabled} states={states} onPick={pick} />}
      {trial.layout === "degrees" && <DegreePad choices={trial.choices} enabled={enabled} states={states} onPick={pick} lit={lit} />}
      <Result runner={runner} />
      <div className="controls">
        {phase === "answering" && trial.can_replay && (
          <button className="btn" onClick={() => command({ kind: "replay" })}>
            Replay <span className="kbd">R</span>
          </button>
        )}
        {phase === "answering" && trial.can_reference && (
          <button className="btn" onClick={() => command({ kind: "reference" })}>
            Reference <span className="kbd">N</span>
          </button>
        )}
        {phase === "feedback" && fb?.can_show_answer && (
          <button className="btn" onClick={() => command({ kind: "show_answer" })}>
            Play again <span className="kbd">A</span>
          </button>
        )}
        {phase === "feedback" && fb?.can_listen && (
          <button className="btn" onClick={() => command({ kind: "listen" })}>
            {trial.kind === "scale_degree_id" ? "Resolve to the tonic" : "Listen"} <span className="kbd">L</span>
          </button>
        )}
        {phase === "feedback" && <NextButton auto={fb?.auto_advance_ms ?? null} />}
      </div>
    </>
  );
}

function Deadline({ trial }: { trial: TrialView }) {
  useAnimationFrame(true);
  const start = trial.response_start_s ?? 0;
  const total = (trial.deadline_ms ?? 0) / 1000;
  const left = Math.max(0, total - (live.now() - start));
  return (
    <div className="countdown" title="Time left">
      <div style={{ width: `${(left / total) * 100}%` }} />
    </div>
  );
}

function Result({ runner }: { runner: RunnerState }) {
  const fb = runner.feedback;
  if (runner.phase !== "feedback" || !fb) return <div className="result" />;
  if (fb.hidden) return <div className="result muted">Answer recorded.</div>;
  if (fb.flash_color) {
    // Just the colour's name, large: no note names, no marks.
    const c = runner.trial?.choices.find((x) => x.id === fb.correct_choice);
    return <div className="result flash-word">{c?.word ?? c?.label ?? ""}</div>;
  }
  const cls = fb.correct === true ? "correct" : fb.correct === false ? "wrong" : "";
  return <div className={`result ${cls}`}>{fb.summary}</div>;
}

function NextButton({ auto }: { auto: number | null }) {
  return (
    <button className="btn primary" onClick={() => command({ kind: "next" })} autoFocus>
      {auto ? "Next" : "Next"} <span className="kbd" style={{ color: "#fff", borderColor: "rgba(255,255,255,.5)", background: "transparent" }}>Space</span>
    </button>
  );
}

function LivePitchReadout() {
  useAnimationFrame(true);
  const p = live.latest(0.2);
  if (!p || p.midi == null) return <div className="prompt-sub">–</div>;
  const nearest = Math.round(p.midi);
  const cents = (p.midi - nearest) * 100;
  return (
    <div className="prompt-sub num">
      {noteDisplay(nearest)} · {describeCents(cents)}
    </div>
  );
}

function ResponseCountdown({ trial }: { trial: TrialView }) {
  useAnimationFrame(true);
  const a = trial.response_start_s ?? 0;
  const b = trial.response_end_s ?? a + 1;
  const now = live.now();
  const frac = now < a ? 1 : Math.max(0, 1 - (now - a) / (b - a));
  return (
    <div className="countdown">
      <div style={{ width: `${frac * 100}%` }} />
    </div>
  );
}

function SingPitchStage({ runner, trial }: { runner: RunnerState; trial: TrialView }) {
  useAnimationFrame(runner.phase === "recording");
  const fb = runner.feedback;
  const open = runner.phase === "recording" && live.now() >= (trial.response_start_s ?? 0);
  // After a miss the tuner opens to practise locking on to the note; after a hit it is a click away.
  const [tunerOpen, setTunerOpen] = useState(false);
  useEffect(() => setTunerOpen(false), [runner.trial_index, trial.attempt]);
  const showTuner = runner.phase === "feedback" && fb?.tuner != null && (fb.correct === false || tunerOpen);
  return (
    <div className="section" style={{ gap: 18, margin: "auto 0" }}>
      <div className="big-label">{open || runner.phase !== "recording" ? trial.prompt : "…"}</div>
      <div className="prompt-sub">
        {runner.phase === "recording" ? (open ? "Sing now" : "Get ready") : runner.phase === "grading" ? "Analysing…" : ""}
      </div>
      {runner.phase === "recording" && <ResponseCountdown trial={trial} />}
      {runner.phase === "recording" && trial.show_live_pitch && <LivePitchReadout />}
      {runner.phase === "feedback" && fb && (
        <>
          <Result runner={runner} />
          {!fb.hidden && fb.cents != null && (
            <div className="prompt-sub num">
              Error {fmtCents(fb.cents)}
            </div>
          )}
          {showTuner && fb.tuner && <LockOnTuner target={fb.tuner} />}
          <div className="controls">
            {fb.can_listen && (
              <button className="btn" onClick={() => command({ kind: "listen" })}>
                Hear the note <span className="kbd">L</span>
              </button>
            )}
            {fb.tuner && !showTuner && (
              <button className="btn" onClick={() => setTunerOpen(true)}>
                Practise with the tuner
              </button>
            )}
            <NextButton auto={fb.auto_advance_ms} />
          </div>
        </>
      )}
    </div>
  );
}

function PairingStage({ trial }: { trial: TrialView }) {
  useAnimationFrame(true);
  const show = trial.label_at_s != null && live.now() >= trial.label_at_s;
  return (
    <div className="section" style={{ margin: "auto 0" }}>
      <div className="prompt-sub">Listen</div>
      <div className="big-label" style={{ minHeight: 64 }}>
        {show ? trial.label : ""}
      </div>
    </div>
  );
}

/** Incidental pairing: the colour appears shortly after the tone; press it. */
function DetectStage({ runner, trial }: { runner: RunnerState; trial: TrialView }) {
  const phase = runner.phase;
  const listening = phase === "listening" || phase === "answering";
  const fb = runner.feedback;
  const mark =
    trial.detect_mark === "hit" ? "Yes" : trial.detect_mark === "false" ? "Not the target" : trial.detect_mark === "miss" ? "Missed" : "";
  return (
    <div className="section" style={{ alignItems: "center", gap: 18, margin: "auto 0" }}>
      <div className="prompt-sub">Listen for</div>
      <div className="big-label" style={trial.flash_color ? { color: trial.flash_color } : undefined}>
        {trial.label ?? trial.prompt}
      </div>
      {trial.detect_total != null && (
        <div className="prompt-sub num">
          {trial.detect_index && trial.detect_index > 0 ? `Tone ${trial.detect_index} of ${trial.detect_total}` : "Get ready"}
        </div>
      )}
      {listening && (
        <button className="btn primary" style={{ fontSize: 20, padding: "16px 32px" }} onClick={() => command({ kind: "tap" })}>
          Target <span className="kbd">Space</span>
        </button>
      )}
      {mark && <div className="prompt-sub">{mark}</div>}
      {phase === "feedback" && (
        <>
          <Result runner={runner} />
          <NextButton auto={fb?.auto_advance_ms ?? null} />
        </>
      )}
    </div>
  );
}

function ColorTask({ trial, enabled, states, onPick }: { trial: TrialView; enabled: boolean; states: Record<string, KeyState>; onPick: (id: string) => void }) {
  useAnimationFrame(trial.label_at_s != null);
  const shown = trial.label != null && trial.label_at_s != null && live.now() >= trial.label_at_s ? trial.label : null;
  useEffect(() => {
    if (!enabled) return;
    const onKey = (e: KeyboardEvent) => {
      if (e.repeat || e.ctrlKey || e.metaKey || e.altKey) return;
      const n = e.key === "0" ? 10 : parseInt(e.key, 10);
      if (!Number.isNaN(n) && n >= 1 && n <= trial.choices.length && n <= 10) {
        e.preventDefault();
        onPick(trial.choices[n - 1].id);
      }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  });
  return (
    <div className="section" style={{ alignItems: "center", gap: 18 }}>
      {trial.label_at_s != null && <div className="swatch" style={{ background: shown ? PAIRING_COLORS[shown] ?? shown : "var(--surface-muted)" }} />}
      <div className="row wrap" style={{ gap: 10, justifyContent: "center" }}>
        {trial.choices.map((c, i) => (
          <button
            key={c.id}
            className={`color-key ${states[c.id] ?? ""}`}
            disabled={!enabled}
            onClick={() => onPick(c.id)}
            style={states[c.id] ? undefined : tint(c.color ?? PAIRING_COLORS[c.id] ?? "#cccccc", "#ffffff", 1)}
          >
            <span>
              {c.label}
              {c.word && c.word !== c.label ? <span className="hint"> {c.word}</span> : null}
            </span>
            {i < 10 && <span className="kbd">{i === 9 ? 0 : i + 1}</span>}
          </button>
        ))}
      </div>
    </div>
  );
}

function SungStage({ runner, trial }: { runner: RunnerState; trial: TrialView }) {
  if (trial.score) return <SightStage runner={runner} trial={trial} score={trial.score} />;
  const lane = trial.lane!;
  const fb = runner.feedback;
  const graded = runner.phase === "feedback" && fb && !fb.hidden ? { trace: fb.trace, notes: fb.notes } : null;
  return (
    <>
      <div>
        <div className="prompt">{trial.prompt}</div>
        <SungStatus runner={runner} trial={trial} />
      </div>
      {runner.phase === "recording" && <Cue lane={lane} start={trial.timeline_start_s ?? 0} />}
      <PitchLane lane={lane} timelineStart={trial.timeline_start_s} showLive={trial.show_live_pitch} graded={graded} height={320} />
      {runner.phase === "recording" && <LaneReadout trial={trial} />}
      {runner.phase === "feedback" && fb && (
        <>
          <Result runner={runner} />
          {!fb.hidden && fb.notes.length > 0 && <NoteStrip notes={fb.notes} />}
          <SungControls fb={fb} />
        </>
      )}
    </>
  );
}

function SungControls({ fb }: { fb: NonNullable<RunnerState["feedback"]> }) {
  return (
    <div className="controls">
      {fb.can_listen && (
        <button className="btn" onClick={() => command({ kind: "listen" })}>
          Listen <span className="kbd">L</span>
        </button>
      )}
      {fb.can_play_answer && (
        <button className="btn" onClick={() => command({ kind: "play_answer" })}>
          Play my answer <span className="kbd">P</span>
        </button>
      )}
      {fb.can_sing_along && (
        <button className="btn" onClick={() => command({ kind: "sing_along" })}>
          Sing along <span className="kbd">G</span>
        </button>
      )}
      {fb.can_retry && (
        <button className="btn" onClick={() => command({ kind: "retry" })}>
          Retry <span className="kbd">T</span>
        </button>
      )}
      <NextButton auto={fb.auto_advance_ms} />
    </div>
  );
}

/** Sight singing: the score on a staff, the steps before singing, the trace and marks after. */
function SightStage({ runner, trial, score }: { runner: RunnerState; trial: TrialView; score: Score }) {
  const lane = trial.lane!;
  const fb = runner.feedback;
  const recording = runner.phase === "recording";
  useAnimationFrame(recording);
  const start = trial.timeline_start_s ?? 0;
  const t = live.now() - start;
  const graded = runner.phase === "feedback" && fb && !fb.hidden ? fb : null;
  const timing = { singStartS: lane.sing_start_s, beatS: lane.beat_s };
  const ticksPerBeat = beatTicks(score);
  const singing = recording && t >= lane.sing_start_s && t <= lane.total_s;
  const cursorTick = singing && lane.cursor ? ((t - lane.sing_start_s) / lane.beat_s) * ticksPerBeat : null;
  const liveTrace =
    recording && trial.show_live_pitch
      ? live.since(start + lane.sing_start_s - 0.3).map((p) => [p.t - start, p.midi] as [number, number | null])
      : undefined;
  return (
    <>
      <div>
        <div className="prompt">{trial.prompt}</div>
        <div className="prompt-sub">
          {runner.phase === "grading"
            ? "Analysing…"
            : `${describeKey(score)} · ${score.time} · ${Math.round(score.tempo_bpm)} bpm${trial.assisted ? " · sing-along attempt" : ""}`}
        </div>
      </div>
      {recording && <Cue lane={lane} start={start} />}
      {recording && lane.score_until_s != null && t >= lane.score_until_s ? (
        <div className="staff-wrap memory-blank">Sing it from memory</div>
      ) : (
        <Suspense fallback={<div className="staff-wrap" style={{ minHeight: 200 }} />}>
          <Staff
            score={score}
            trace={graded ? graded.trace : liveTrace}
            {...traceStyle(trial.grading_mode)}
            timing={timing}
            results={graded ? graded.notes : null}
            cursorTick={cursorTick}
            noteColors={graded ? undefined : silentColors(score, lane.silent)}
          />
        </Suspense>
      )}
      {recording && <Steps lane={lane} t={t} />}
      {recording && lane.metronome && <BeatDots lane={lane} t={t} />}
      {runner.phase === "feedback" && fb && (
        <>
          <Result runner={runner} />
          <SungControls fb={fb} />
        </>
      )}
    </>
  );
}

const MARK_COLORS: Record<string, string> = { correct: "#1a7f37", wrong: "#cf222e", extra: "#cf222e", missed: "#cf222e" };

/** Melodic dictation: hear the melody, write it in the editor, check it. */
function DictationStage({ runner, trial }: { runner: RunnerState; trial: TrialView }) {
  const given = trial.score!;
  const fb = runner.feedback;
  const answering = runner.phase === "answering";
  const [notes, setNotes] = useState<ScoreNote[]>([]);
  const playsLeft = trial.max_plays > 0 ? Math.max(0, trial.max_plays - trial.plays) : null;
  const replay = () => playsLeft !== 0 && command({ kind: "replay" });
  const playKey = () => command({ kind: "reference" });
  const playMine = () => command({ kind: "play_score", notes });
  const check = () => command({ kind: "submit_score", notes });
  useKeyOwner((e) => {
    if (e.ctrlKey || e.metaKey || e.altKey) return false;
    const k = e.key.toLowerCase();
    if (k === "r") replay();
    else if (k === "n" && trial.can_reference) playKey();
    else if (k === "w") playMine();
    else if (k === "enter") check();
    else return false;
    return true;
  }, answering);

  if (runner.phase === "feedback" && fb) {
    const d = fb.dictation;
    return (
      <>
        <div className="prompt">{trial.prompt}</div>
        {d && (
          <Suspense fallback={<div className="staff-wrap" style={{ minHeight: 160 }} />}>
            <div className="section" style={{ gap: 4 }}>
              <div className="small muted">Your answer</div>
              <Staff score={d.answer} maxHeight={240} noteColors={d.answer_marks.map((m) => (m ? MARK_COLORS[m] : null))} />
              <div className="small muted">Melody</div>
              <Staff score={d.target} maxHeight={240} noteColors={d.target_marks.map((m) => (m && m !== "correct" ? MARK_COLORS[m] : null))} />
            </div>
          </Suspense>
        )}
        <Result runner={runner} />
        <SungControls fb={fb} />
      </>
    );
  }

  return (
    <>
      <div>
        <div className="prompt">{trial.prompt}</div>
        <div className="prompt-sub">
          {describeKey(given)} · {given.time} · {Math.round(given.tempo_bpm)} bpm
          {playsLeft != null && ` · ${playsLeft} ${playsLeft === 1 ? "play" : "plays"} left`}
        </div>
      </div>
      <Suspense fallback={<div className="staff-wrap" style={{ minHeight: 200 }} />}>
        <ScoreEditor key={`${runner.trial_index}-${trial.attempt}`} given={given} disabled={!answering} onChange={setNotes} />
      </Suspense>
      <div className="controls">
        <button className="btn" onClick={replay} disabled={!answering || playsLeft === 0}>
          Play melody <span className="kbd">R</span>
        </button>
        {trial.can_reference && (
          <button className="btn" onClick={playKey} disabled={!answering}>
            Play key <span className="kbd">N</span>
          </button>
        )}
        <button className="btn" onClick={playMine} disabled={!answering}>
          Play what I wrote <span className="kbd">W</span>
        </button>
        <button className="btn primary" onClick={check} disabled={!answering}>
          Check <span className="kbd" style={{ color: "#fff", borderColor: "rgba(255,255,255,.5)", background: "transparent" }}>Enter</span>
        </button>
      </div>
    </>
  );
}

/** The part of the timeline now: listen, breathe, an instruction, sing silently or aloud. */
function Cue({ lane, start }: { lane: LaneSpec; start: number }) {
  useAnimationFrame(true);
  const t = live.now() - start;
  const s = lane.sections.find((x) => t >= x.start_s && t < x.end_s);
  if (!s) return null;
  const l = s.label.toLowerCase();
  const kind = l.includes("silent") ? "silent" : l.startsWith("breathe") ? "breathe" : l.startsWith("sing") || l.startsWith("answer") ? "sing" : l === "listen" || l === "key" || l === "count-in" ? "listen" : "think";
  return (
    <div className={`cue cue-${kind}`} aria-live="polite">
      {kind === "breathe" && <span className="breath-ring" aria-hidden />}
      {s.label}
    </div>
  );
}

/** Notes sung silently, greyed on the staff: indexes count pitched notes, colours all notes. */
function silentColors(score: Score, silent: number[]): (string | null)[] | undefined {
  if (silent.length === 0) return undefined;
  let k = -1;
  return score.notes.map((n) => {
    if (!n.pitch) return null;
    k += 1;
    return silent.includes(k) ? "#a7b0ba" : null;
  });
}

/** EarMaster-style steps: establish key, first note, count-in, sing. */
function Steps({ lane, t }: { lane: LaneSpec; t: number }) {
  if (lane.sections.length === 0) return null;
  const current = lane.sections.findIndex((s) => t >= s.start_s && t < s.end_s);
  const before = t < (lane.sections[0]?.start_s ?? 0);
  return (
    <div className="steps">
      {lane.sections.map((s, i) => {
        const state = i === current ? "active" : !before && (current === -1 ? t >= s.end_s : i < current) ? "done" : "";
        return (
          <div key={`${i}-${s.label}`} className={`step ${state}`}>
            {s.label}
          </div>
        );
      })}
    </div>
  );
}

function BeatDots({ lane, t }: { lane: LaneSpec; t: number }) {
  const beat = lane.beat_s > 0 && t >= 0 ? Math.floor(t / lane.beat_s + 1e-6) : -1;
  const countIn = lane.sections.find((s) => s.label === "Count-in");
  const origin = countIn ? Math.round(countIn.start_s / lane.beat_s) : 0;
  const active = beat >= origin && t <= lane.total_s;
  return (
    <div className="readout">
      <div className="beats" aria-hidden>
        {Array.from({ length: lane.beats_per_bar }, (_, i) => (
          <span key={i} className={active && (beat - origin) % lane.beats_per_bar === i ? "on" : ""} />
        ))}
      </div>
    </div>
  );
}

function SungStatus({ runner, trial }: { runner: RunnerState; trial: TrialView }) {
  useAnimationFrame(runner.phase === "recording");
  if (runner.phase === "grading") return <div className="prompt-sub">Analysing…</div>;
  if (runner.phase !== "recording") return <div className="prompt-sub">{trial.assisted ? "Sing-along attempt" : ""}</div>;
  const lane = trial.lane!;
  const t = live.now() - (trial.timeline_start_s ?? 0);
  let text = "Sing";
  if (t < 0) text = "Get ready";
  else if (t < lane.sing_start_s) {
    const refEnd = lane.reference.reduce((m, n) => Math.max(m, n.start_s + n.dur_s), 0);
    text = t < refEnd && lane.reference.length > 1 ? "Listen" : "Count-in";
  } else if (t > lane.total_s) text = "Done";
  return <div className="prompt-sub">{trial.assisted ? `${text} · with guide` : text}</div>;
}

/** Beat, current target and live pitch under the lane while a sung trial records. */
function LaneReadout({ trial }: { trial: TrialView }) {
  useAnimationFrame(true);
  const lane = trial.lane!;
  const t = live.now() - (trial.timeline_start_s ?? 0);
  const beat = lane.beat_s > 0 && t >= 0 ? Math.floor(t / lane.beat_s + 1e-6) : -1;
  const singing = t >= lane.sing_start_s && t <= lane.total_s;
  const target = lane.targets_hidden
    ? undefined
    : (lane.notes.find((n) => t >= n.start_s && t < n.start_s + n.dur_s) ?? (t < lane.sing_start_s ? lane.notes[0] : undefined));
  const p = live.latest(0.15);
  let you = "–";
  let cls = "";
  if (p && p.midi != null) {
    const nearest = Math.round(p.midi);
    const cents = (p.midi - nearest) * 100;
    you = `${noteDisplay(nearest)} ${cents >= 0 ? "+" : "−"}${Math.abs(Math.round(cents))}¢`;
    if (target && singing) cls = nearest !== target.midi ? "wrong" : Math.abs(cents) <= 25 ? "correct" : "marginal";
  }
  return (
    <div className="readout num">
      {lane.metronome && lane.beats_per_bar > 0 && (
        <div className="beats" aria-hidden>
          {Array.from({ length: lane.beats_per_bar }, (_, i) => (
            <span key={i} className={beat >= 0 && beat % lane.beats_per_bar === i ? "on" : ""} />
          ))}
        </div>
      )}
      {!lane.targets_hidden && (
        <span>
          {singing ? "Target" : "First note"} <strong>{target ? noteDisplay(target.midi) : "–"}</strong>
        </span>
      )}
      <span>
        You <strong className={cls}>{you}</strong>
      </span>
    </div>
  );
}

function NoteStrip({ notes }: { notes: NonNullable<RunnerState["feedback"]>["notes"] }) {
  return (
    <div className="row wrap" style={{ justifyContent: "center", gap: 6 }}>
      {notes.map((n, i) => {
        if (n.silent) {
          return (
            <span key={i} className="badge num faint" title="Sung silently">
              {noteDisplay(n.target_midi)} silent
            </span>
          );
        }
        const cls = !n.matched ? "" : !n.correct ? "wrong" : n.in_tune ? "correct" : "marginal";
        return (
          <span key={i} className={`badge num ${cls}`} title={n.onset_error_ms != null ? `Onset ${Math.round(n.onset_error_ms)} ms` : undefined}>
            {noteDisplay(n.target_midi)} {n.matched && n.cents_error != null ? `${n.cents_error > 0 ? "+" : ""}${Math.round(n.cents_error)}` : "missed"}
          </span>
        );
      })}
    </div>
  );
}

function Stat({ label, value }: { label: string; value: string }) {
  return (
    <div className="card muted-card" style={{ padding: 12 }}>
      <div className="small muted">{label}</div>
      <div className="num" style={{ fontSize: 20, fontWeight: 600, marginTop: 2 }}>
        {value}
      </div>
    </div>
  );
}

function BlockSummary({ report }: { report: BlockReport | null }) {
  return (
    <div className="section" style={{ gap: 18 }}>
      <h2>Block complete</h2>
      {report && report.trials === 0 && report.assisted > 0 ? (
        <p className="muted">
          Practice: your part played along, so these {report.assisted} attempts are not scored. Sing it on your own to
          see your accuracy.
        </p>
      ) : report && report.kind === "pitch_detect" ? (
        <div className="grid cols-3" style={{ gridTemplateColumns: "repeat(3, minmax(0, 1fr))" }}>
          <Stat label="Mean d′" value={report.mean_score == null ? "–" : report.mean_score.toFixed(2)} />
          <Stat label="Streams" value={String(report.trials)} />
          <Stat label="Median hit" value={fmtMs(report.median_rt_ms)} />
        </div>
      ) : report ? (
        <>
          <div className="grid cols-3" style={{ gridTemplateColumns: "repeat(4, minmax(0, 1fr))" }}>
            <Stat label="Accuracy" value={pct(report.accuracy)} />
            <Stat label="Correct" value={`${report.correct} / ${report.graded}`} />
            <Stat label="Median response" value={fmtMs(report.median_rt_ms)} />
            <Stat label="Median error" value={fmtCents(report.median_abs_cents)} />
          </div>
          {report.pass_mark != null && (
            <div className={`notice ${report.passed ? "info" : ""}`}>
              Pass mark {pct(report.pass_mark)}: {report.passed ? "passed" : "not yet"}.
            </div>
          )}
        </>
      ) : (
        <p className="muted">Results are shown at the end of the session.</p>
      )}
      <div className="controls">
        <NextButton auto={null} />
      </div>
    </div>
  );
}

const COMFORT = [
  ["comfortable", "Comfortable"],
  ["mild_fatigue", "Mild fatigue"],
  ["discomfort", "Discomfort"],
  ["pain", "Pain or hoarseness"],
] as const;

function SessionSummary({ runner, onClose }: { runner: RunnerState; onClose: () => void }) {
  const r = runner.session_report;
  const [comfort, setComfort] = useState<string | null>(null);
  return (
    <div className="section" style={{ gap: 18 }}>
      <h2>Session complete</h2>
      {r && (
        <>
          <div className="grid cols-3">
            <Stat label="Accuracy" value={pct(r.accuracy)} />
            <Stat label="Trials" value={String(r.trials.filter((t) => !t.assisted).length)} />
            <Stat label="Duration" value={r.duration_s != null ? `${Math.round(r.duration_s / 60)} min` : "–"} />
          </div>
          <div className="card flush">
            <table>
              <thead>
                <tr>
                  <th>Block</th>
                  <th>Trials</th>
                  <th>Accuracy</th>
                  <th>Median response</th>
                  <th>Median error</th>
                  <th>Pass</th>
                </tr>
              </thead>
              <tbody>
                {r.blocks.map((b) => (
                  <tr key={b.index}>
                    <td>{b.title}</td>
                    <td className="num">{b.trials}</td>
                    <td className="num">{pct(b.accuracy)}</td>
                    <td className="num">{fmtMs(b.median_rt_ms)}</td>
                    <td className="num">{fmtCents(b.median_abs_cents)}</td>
                    <td>{b.passed == null ? "–" : b.passed ? <span className="badge correct">Passed</span> : <span className="badge">Not yet</span>}</td>
                  </tr>
                ))}
              </tbody>
            </table>
          </div>
        </>
      )}
      <div className="section">
        <div className="small muted">How does your voice feel? (optional, shared with your coach)</div>
        <div className="row wrap">
          {COMFORT.map(([id, label]) => (
            <button
              key={id}
              className={`btn small ${comfort === id ? "primary" : ""}`}
              onClick={() => {
                setComfort(id);
                api("addComfortReport", runner.session_id, "post", id, null).catch(() => undefined);
              }}
            >
              {label}
            </button>
          ))}
        </div>
        {comfort === "pain" && (
          <div className="notice">Rest your voice. If pain or hoarseness persists, see a voice professional before demanding singing.</div>
        )}
      </div>
      <div className="controls">
        <button
          className="btn primary"
          onClick={() => {
            command({ kind: "close" });
            onClose();
          }}
        >
          Done <span className="kbd" style={{ color: "#fff", borderColor: "rgba(255,255,255,.5)", background: "transparent" }}>Space</span>
        </button>
      </div>
    </div>
  );
}
