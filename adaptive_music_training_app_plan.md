# Singing, Ear-Training and Absolute-Pitch Trainer
## Implementation plan (revision 2)

**Purpose:** A Windows desktop app for singing, ear training and absolute-pitch (AP) training. It plays stimuli and captures the voice through ASIO on a Focusrite Scarlett, grades every attempt deterministically, logs everything, and exposes the history plus exercise and session generation to an LLM through MCP. The LLM reads the results and writes the next sessions; the app runs and grades them.

**Status (October 2026):**

- **Built:** phases 1–3.
- **Phase 4, built:**
  - Interval, chord and scale identification.
  - Intervals inside a key (after a cadence, both notes diatonic) as well as without a key.
  - Interval comparison.
  - Scale degrees in the style of Functional Ear Trainer: a graded eight-level course, fixed or changing keys, and the walk to the tonic (nearest, up or down).
  - Pitch matching, sustain, interval singing and patterns, with the follow-along lane and backing options.
  - Harmony singing: parallel thirds or sixths to a generated melody, or one part of a multi-part library piece, with equal or just-intonation grading.
  - The range profile, set by hand.
- **Phase 5, built:**
  - Sight singing from staff notation (VexFlow) and melodic dictation with a monophonic score editor.
  - Melodies are generated, written by the coach, or taken from the melody library.
  - The library is a watched folder of JSON and MIDI files, with MCP tools to list, add and import pieces.
- **All six §9 absolute-pitch types are in, including anchor songs and incidental pairing.**
- **Phase 4, still to do:** progressions, echo phrases, own-voice matching.
- **Phase 5, still to do:** `regrade` and `get_trial_plot` over MCP.
- **Dropped:**
  - The range finder: the singer knows their range and sets it in Settings.
  - The rhythm types (§10): rhythm is practised inside sight singing.
  - MusicXML import: the coach converts the user's scores into library JSON or MIDI files instead.
- **Not started:** Part 2 and §17.
- **Testing:** everything runs in CI, which builds the Windows installer with ASIO. The audio engine has not yet been run on the Scarlett.

**Core loop:**

```text
LLM (MCP) reads results → writes session JSON → app validates and queues it
→ user practises (headphones, Scarlett) → engine plays and captures → deterministic grading
→ trials logged → LLM reads results → …
```

**Principles:**

- The app measures and grades; the LLM plans. Grading is always deterministic code.
- Every trial is logged append-only with its full conditions (stimulus, timbre, pool, feedback mode, mask, protocol, calibration).
- Intonation is judged on the note centre; vibrato is described separately.
- AP progress is judged on holdout stimuli, transfer (octave, timbre, context) and retention, never on training-set accuracy alone.
- Nothing that can block runs in the audio callback.

---

## 0. Decisions

| Topic | Decision |
|---|---|
| Platform | Windows 10/11 desktop only. No mobile app, no sync. |
| Licensing | Personal tool. Any licence is acceptable, GPL included; getting it running and functional comes first. What can be redistributed is decided at the end. |
| UI style | Clean, professional light mode with a sans-serif system font (Segoe UI). No marketing-site tropes (§15). |
| Exercise conventions | Modelled on EarMaster's tools, which follow the standard aural-skills curriculum (§8). |
| Audio | ASIO first (Focusrite Scarlett), WASAPI fallback. 48 kHz. |
| Listening | Always headphones. Playback never reaches the mic, so sing-along, drone and accompaniment exercises are graded without bleed handling. |
| Monitoring | The singer hears their own voice through the Scarlett's direct monitor (zero latency). The app outputs prompts, references, drones, clicks and accompaniment. Optional software monitoring, off by default. |
| Language | Rust for the audio engine, DSP, grading, storage and MCP server. TypeScript/React for the UI only (Tauri 2 webview). Python only in Part 2, for ML models. |
| Adaptivity | No built-in adaptive-difficulty engine or learner-state model. The LLM decides progression from logged results; the app provides parameterised generators, deterministic grading and aggregate queries. |
| Exchange formats | Sessions and exercises: JSON validated against the Rust types. Notation: MusicXML. The LLM writes and edits scores and transcriptions as MusicXML or compact note JSON via MCP. |
| LLM agent | OpenCode or Hermes Agent, connected to the app's MCP server (any MCP client works). For now the agent runs in its own window; hosting it inside the app via ACP is a future addition (§17). |
| Part 1 (V1) | Foundation → full AP suite → MCP → relative pitch and singing → sight singing and dictation (generated melodies + open MusicXML/MIDI). |
| Part 2 (V2) | Song import → vocal separation → vocal F0 and transcription → LLM-corrected MusicXML → repertoire exercises. |
| Future app (§17) | In-app coach panel via ACP, hands-free mode for practice away from the PC, phone client. |
| Deferred | Drum and other-instrument transcription, full-mix multi-instrument transcription. |
| Outside the app | The Physical Vocal Practice Reference is a document given directly to the LLM for drafting off-app physical practice. The app does not implement or serve it. |

---

## 1. Scope

### Part 1 (V1)

- ASIO/WASAPI engine, device page, latency calibration
- Live tuner and real-time pitch display
- Post-trial pitch analysis and grading
- Full AP suite: identification, production, cross-octave and cross-timbre transfer, speeded identification, passive pairing, incidental pairing, anchor songs, tonal-centre identification, assessment battery
- Relative pitch: intervals, chords, scale degrees, cadences and progressions, tonic finding
- Singing: pitch matching, sustain, interval singing, patterns, harmony against a second voice, echo, range profile (set by hand), own-voice references
- Sight singing from generated, LLM-authored and library melodies (JSON and MIDI files); melodic and harmonic dictation
- MCP server: read results, generate and queue sessions, author scores, keep coach notes

### Part 2 (V2)

- Song import and vocal/accompaniment separation
- Vocal F0 and note transcription → MusicXML with per-note confidence
- LLM correction of transcriptions via MCP; user verification by listening
- Repertoire exercises: sing-along, karaoke, echo, phrase sight-reading; grading against corrected notes or directly against the original vocal contour
- Passage search by musical features

### Future app (§17)

- In-app coach panel: OpenCode or Hermes hosted in the app window over ACP
- Hands-free mode: audio-only sessions with spoken prompts and feedback, for practice away from the PC
- Spoken note-name answers (speech recognition)
- Phone client

### Deferred (not planned)

- Drum separation and transcription, kit-piece MIDI
- Other-instrument and full-mix multi-instrument transcription
- Pitch-shifted own-voice references
- Formant and resonance analytics

---

## 2. Architecture

```text
┌────────────────────────┐   stdio (MCP)   ┌──────────────────────────────┐
│ LLM agent              │◄───────────────►│ trainer-mcp (Rust, rmcp)     │
│ OpenCode / Hermes      │                 │ links trainer-core           │
└────────────────────────┘                 └──────────────┬───────────────┘
                                                          │
                                       SQLite (WAL) + data directory
                                                          │
┌─────────────────────────────────────────────────────────┴────────────────┐
│ trainer-app (Tauri 2, Rust): session runner, queue watcher, library      │
│                                                                          │
│  ┌──────────────────────────┐  commands,   ┌──────────────────────────┐  │
│  │ trainer-audio (Rust)     │  events,     │ UI (React + TS webview)  │  │
│  │ cpal ASIO → Scarlett     │◄────────────►│ tuner, trial views       │  │
│  │ scheduler, recorder      │  pitch and   │ OSMD scores, charts      │  │
│  │ live pitch, synth        │  meter data  │                          │  │
│  └──────────────────────────┘              └──────────────────────────┘  │
└──────────────────────────────────────────────────────────────────────────┘
         Part 2: ml/ Python worker (subprocess, JSON job in, files out)
```

### Processes

- **trainer-app** — the desktop app. Owns the audio device and runs sessions.
- **trainer-mcp** — stdio MCP server launched by the LLM agent (OpenCode, Hermes or any other MCP client). Reads and writes the same SQLite database and data directory and links the same `trainer-core` crate (schema, generators, grading, analysis, stats). Works whether or not the app is open.
- The app polls `PRAGMA data_version` (~500 ms) and refreshes its queue when the MCP server writes a session.
- Part 2: the `ml/` Python worker is run as a subprocess with a JSON job manifest and writes results into the data directory.
- Audio never passes through the webview. The UI receives compact pitch and meter frames (Tauri `ipc::Channel`, batched per ~16 ms) and renders at display rate.

### Language choice

Rust for everything except the UI:

- ASIO needs native code; `cpal`'s `asio` feature gives Rust a direct ASIO host.
- The audio callback must be free of GC pauses, locks and allocation; Rust plus `rtrb` ring buffers makes that straightforward.
- Tauri's backend is already Rust, so the engine, app and MCP server share one `trainer-core` crate and one set of schema types. MCP tool schemas (`schemars`) and UI types (`ts-rs`) are generated from the same structs.
- Pitch detection itself is cheap in any language; the gains are ASIO access, real-time safety and one shared codebase.
- The UI stays TypeScript because MusicXML rendering (OSMD) and canvas drawing live there.

### Libraries

| Concern | Choice |
|---|---|
| Audio I/O | `cpal` with the `asio` feature (ASIO host on Windows, WASAPI fallback, other hosts for development elsewhere) |
| Real-time queues | `rtrb` SPSC ring buffers |
| FFT | `rustfft` |
| Live pitch | McLeod Pitch Method (NSDF via FFT): `pitch-detection` crate or in-house |
| Post-trial pitch | In-house pYIN-style tracker: per-frame MPM period candidates + Viterbi smoothing |
| Synthesis | `rustysynth` (SoundFont, General MIDI; default bank GeneralUser GS) + in-house oscillators and Shepard tones |
| MIDI files / MIDI input | `midly` / `midir` |
| MusicXML | `roxmltree` (read), `quick-xml` (write), `zip` (.mxl) |
| Audio files | `hound` (WAV); Part 2: `symphonia` (decode), `rubato` (resample) |
| Storage | `rusqlite` (bundled, JSON1) + `rusqlite_migration` |
| Schema and validation | `serde`, `schemars`, `serde_path_to_error`, `ts-rs` |
| MCP | `rmcp` (features `server`, `transport-io`, `macros`, `schemars`) |
| Plots for the LLM | `plotters` (PNG pitch trace vs target) |
| App shell | Tauri 2 |
| UI | React + TypeScript + Vite, `opensheetmusicdisplay`, custom canvas pitch view, `uplot` |
| Part 2 ML | Python (uv): `audio-separator`, `torchcrepe` or RMVPE, `basic-pitch`, `beat-this` |

**Build prerequisites (Windows):** Rust stable (MSVC toolchain), Node 22, LLVM/Clang (for cpal's ASIO bindings), ASIO SDK (cpal fetches it automatically or uses `CPAL_ASIO_DIR`), Focusrite ASIO driver.

---

## 3. Audio engine

### Device

- Host preference: ASIO → WASAPI → other cpal hosts (development only).
- 48 kHz; 44.1 kHz accepted (all processing is sample-rate agnostic).
- Buffer size: the driver's size by default. On the Scarlett the ASIO buffer size is set in Focusrite Control; the app displays it and requests a fixed size only when the driver accepts one. Target 64–128 frames.
- Input: one mono mic channel (Scarlett input 1 or 2, selectable). Output: stereo headphones.

The device page shows: host, device, sample rate, buffer size, input channel, input meter with clip indicator, output test tone, measured round-trip latency, calibration status, suspected xruns (callback timing gaps) and callback load (% of buffer period).

### Real-time rules

The audio callback never allocates, locks, does I/O, logs, makes system calls or runs analysis. It only:

- copies input samples into an SPSC ring, tagged with the master frame counter,
- mixes scheduled pre-rendered buffers and the click into the output,
- reads commands from an SPSC command ring (play buffer at frame N, stop, set gain),
- writes compact events to an event ring (buffer started or finished at frame N, timing gap detected).

**Master clock:** one frame counter advanced by the output callback. ASIO delivers input and output in the same buffer switch, so input blocks carry the same counter. On WASAPI, input blocks are mapped to the counter using stream timestamps. Calibration absorbs the remaining constant offset. Every prompt, click, capture and response timestamp is stored in master frames.

### Playback

- Prompts, references, drones, masks, clicks and accompaniment are rendered off the audio thread (rustysynth, oscillators) into pre-allocated buffers before they are needed, then scheduled at exact frames.
- On-demand sounds (replay reference, on-screen keyboard) render on a worker thread in a few milliseconds and are scheduled immediately.
- Buses: prompt, reference/accompaniment, click, optional software voice monitoring.
- Answer input never makes sound unless the block's feedback mode allows it (use a MIDI controller with local sound off), so answering cannot leak a reference.

### Capture and recording

- An analysis thread drains the input ring, feeds the live pitch tracker and appends to the current trial's capture buffer.
- Each trial's response window is saved as 16-bit 48 kHz mono WAV with its start frame.
- Stream errors (device unplugged, driver reset after a buffer or sample-rate change in Focusrite Control) tear down and rebuild the streams; the current trial is marked `interrupted`.

---

## 4. Latency calibration

Needed for every timing grade: rhythm, sight-singing onsets and sung-response latency.

- **Loopback (preferred):** cable from a Scarlett output to a line input. The app plays a click train, detects the clicks in the input and stores the median round-trip offset in frames.
- **Tap-along (fallback):** the user taps the mic or says "ta" on the clicks; median offset after outlier rejection. Stored as lower precision.
- Stored per (device, host, sample rate, buffer size). If the configuration changes without a new calibration, the app warns and timing metrics are flagged `uncalibrated` instead of being silently shifted.
- A performed onset is compared to its scheduled frame after subtracting the round-trip offset.

---

## 5. Pitch analysis

Two passes over the same audio. The live pass drives the display; the post-trial pass produces the grade. The grade always comes from the post-trial pass, so the two never conflict.

### Live (display)

- MPM/NSDF on the analysis thread, 2048-sample window (~43 ms at 48 kHz, covers F0 down to ~50 Hz), 480-sample hop (100 updates/s).
- Frames: `{frame, f0_hz, clarity, rms_db}`.
- Voicing gate on clarity and RMS; octave-jump guard (ignore jumps over ~700 cents unless sustained for 3 frames).
- Target display latency ~40–50 ms end to end at 128-frame buffers.

### Post-trial (authoritative)

- pYIN-style tracking over the trial capture: per-frame MPM period candidates with clarity, then Viterbi smoothing across frames (pitch-jump and voicing-switch costs) to remove octave errors and spurious frames. 10 ms hop, fmin/fmax from the singer's range profile plus a margin. Takes well under 100 ms for a typical trial, so after-trial feedback is effectively immediate.
- Output: dense trace (cents relative to A4 as f32, voicing probability as u8) stored as a compact blob.
- Optional later: a neural F0 backend (CREPE, RMVPE or FCPE as ONNX via `ort`) behind the same `F0Estimator` trait, if pYIN struggles on the user's voice. A dry, close-miked vocal with headphones should not need it.

---

## 6. Grading

Grading is a pure function: `grade(block_spec, stimulus, trace, calibration, responses) → metrics`. It lives in `trainer-core`, is used by the app and by `trainer-mcp` (re-grading), and is covered by golden tests.

Units: `midi = 69 + 12·log2(f / A4)`, cents = 100 × midi. A4 defaults to 440 Hz (configurable).

### Note segmentation (sung responses)

- Voiced regions from pYIN voicing, split at sustained pitch changes (> ~60 cents held > 60 ms) and at energy re-onsets (repeated same-pitch notes).
- Melodies are sung on a syllable with a consonant ("da", "la", solfège) so repeated notes separate cleanly; exercise instructions say so.

### Per-note metrics

| Metric | Definition |
|---|---|
| onset latency | prompt end (or expected onset) → first reliably voiced frame |
| initial error | median cents error over the first 50–100 ms of voicing |
| settling time | time until the note centre enters `stable_tol` and stays there ≥ `hold_ms` |
| centre error | median of the note centre over the settled region |
| drift | slope of the note centre over the settled region (cents/s) |
| stability | SD of the note centre over the settled region |
| vibrato | rate (Hz), extent (± cents) and regularity (cycle-to-cycle CV) of the residual, when it is periodic in ~4–8 Hz |
| duration and release | performed vs expected (timed exercises) |
| continuity | unvoiced gaps inside the note |

**Note centre vs vibrato:** the centre is a zero-phase low-pass (~3 Hz) of the trace; vibrato is the residual. Intonation is scored on the centre only, so a well-centred note with ±50-cent vibrato counts as in tune. Vibrato is reported, never scored as good or bad.

### Pitch reference modes

| Mode | Use |
|---|---|
| `absolute` | error vs the target pitch (pitch matching, sustain, AP production with octave) |
| `pitch_class` | absolute with octave ignored (AP production; octave reported separately) |
| `relative_to_first` | targets re-anchored to the singer's first note (interval singing, melodies without a given pitch) |
| `relative_drift_tolerant` | a global offset and a drift line are fitted across the phrase and reported separately from per-note errors (long unaccompanied melodies) |

`tuning: equal | just` — for harmony against a drone or chord, targets can be just-intonation ratios relative to the sounding root.

### Melodies (sight singing, echo, sung dictation)

- Detect performed note candidates and align them to the expected notes with dynamic programming. Costs: pitch mismatch (in the chosen reference mode), onset error, duration error, insertion, deletion. One missed note does not shift every later score.
- Per expected note: matched or missed, pitch error, onset error. Per attempt: extra notes, overall offset, drift (cents per bar).

### Identification responses

Correct or incorrect, signed category error (answer − actual; for pitch classes wrapped to −6…+5 semitones), response time from stimulus onset on the audio clock, timeout flag.

### Defaults (overridable per block)

`category_tol 50¢`, `in_tune_tol 20¢`, `stable_tol 25¢`, `hold_ms 400`, `onset_tol 60 ms`, response deadline per exercise type.

Loudness is relative only: the Scarlett's gain setting is arbitrary, so no SPL claims are made.

---

## 7. Stimuli

### Timbres

- `gm:<program>` — General MIDI SoundFont via rustysynth (default bank GeneralUser GS, bundled into the Windows build by CI): piano, strings, winds, brass, organ, guitar, choir "aahs", voice "oohs", synths.
- `synth:<sine|triangle|saw|square|pulse>` with optional filter and envelope.
- `shepard` — octave-ambiguous Shepard tones (chroma without height).
- `own_voice` — the user's best stored takes of that exact pitch.
- Extra SF2 banks (e.g. free instrument banks from music-production libraries) go in the library's `soundfonts/` folder and show up as `sf:<bank>:<program>`. SFZ libraries would need an SFZ player (e.g. sfizz) and are added only if needed.

Per-stimulus parameters: pitch, timbre, duration, loudness, attack shape, optional detune (default 0 cents).

### Training and holdout pools

- Each (timbre, octave) combination is tagged `training` or `holdout` in a pool configuration. Defaults: a fixed set of holdout timbres never used in training (e.g. four GM programs plus `synth:pulse`) and holdout octaves 2 and 6.
- Training blocks never draw holdout stimuli; assessment blocks draw only holdout stimuli. The validator rejects violations unless the session explicitly overrides them.

### Masks between trials

A mask between AP trials stops the next trial being solved by relative pitch from the previous, labelled one: `none | noise | random_tones | chromatic_cluster`. The default is `random_tones` (~1.5 s of random pitches across the range) whenever feedback reveals answers.

### Reference-exposure log

Each trial records `secs_since_labeled_reference`: time since the app last played a tone whose name the user was told. Assessment blocks warn when it is under 5 minutes. The LLM uses it to place assessments at the start of sessions.

---

## 8. Exercise and session model

### Concepts

- **Session** — an ordered list of blocks, written by the LLM (via MCP), taken from a preset, or set up by the user.
- **Block** — one exercise type + generator params (or explicit items) + count + feedback mode + grading overrides + seed + tags.
- **Trial** — one concrete stimulus and response, generated deterministically from (block, seed, index) in `trainer-core`, so `validate_session` previews exactly what the app will play. Logged append-only.
- New exercise types are added in code. The LLM composes existing types through params and explicit items.

### Feedback modes (per block)

`continuous` (live pitch line visible) · `after_trial` · `after_block` · `end_of_session` · `none`.
Fading feedback is the LLM's job: it picks the mode per block, and every trial logs the mode it ran under.

### Backing and guide (sung multi-note exercises)

Every sung exercise with more than one note (pitch matching sequences, patterns, interval singing, echo, harmony, sight singing, repertoire phrases) runs against the metronome and can play a backing:

- `backing`: `none | click | drone | chords | melody | melody_and_chords`, with `backing_volume`, `tempo_bpm` and `count_in_bars`. Chords come from the score's harmony when it has any, otherwise from a simple diatonic harmonisation of the melody per bar.
- After an attempt, the trial view offers **Listen** (play the target), **Sing along** (retry with the melody guide audible) and **Retry** (unassisted). This is the path after a failed sight-singing attempt.
- Assisted attempts are logged as separate trials with `assisted: true` and the backing used, so statistics always separate assisted from unassisted performance.

### Exercise conventions (EarMaster-style)

- **Question controls** on every trial: Replay, Reference (A4, tonic, or cadence as the exercise defines), Show answer (plays and displays the correct answer), Next. All have keyboard shortcuts.
- **Answer inputs**, chosen per exercise: on-screen piano, note-name or solfège buttons, interval/chord/scale name buttons, MIDI keyboard, microphone (sung answers), tap (rhythm via keyboard, MIDI pad or mic).
- **Lessons:** a block can set `pass_mark` (fraction correct) and `min_trials`; results show pass or fail. The LLM decides what happens next; the app never auto-advances.
- **Sing in your range:** melodies and patterns are transposed into the singer's comfortable range unless a block pins the key.
- **Statistics** per exercise type and per answer category (e.g. accuracy per interval), available in the app and through MCP.

### Example session

```json
{
  "title": "AP stage 2 + no-reference production",
  "protocol": "ap_progressive",
  "planned_minutes": 20,
  "blocks": [
    {
      "type": "pitch_id",
      "count": 60,
      "seed": 1842,
      "feedback": "after_trial",
      "params": {
        "classes": ["C", "E", "G#"],
        "out_of_set_rate": 0.15,
        "octaves": [3, 4, 5],
        "timbres": ["gm:0", "gm:48", "synth:triangle"],
        "pool": "training",
        "duration_ms": 1000,
        "deadline_ms": 3000,
        "mask": "random_tones",
        "answer": "pitch_class"
      }
    },
    {
      "type": "sing_pitch",
      "count": 12,
      "feedback": "after_trial",
      "params": {
        "classes": "all",
        "prompt": "text",
        "response_ms": 4000,
        "pre_mask": "random_tones",
        "reveal_target_after": true
      },
      "grading": { "mode": "pitch_class", "category_tol_cents": 50, "in_tune_tol_cents": 20 }
    }
  ]
}
```

### Example block with an LLM-authored melody

```json
{
  "type": "sight_sing",
  "count": 1,
  "feedback": "after_trial",
  "items": [
    {
      "score": {
        "notes_json": {
          "key": "D", "mode": "major", "time": "4/4", "tempo_bpm": 84, "clef": "treble",
          "notes": [["D4","q"], ["F#4","q"], ["A4","h"], ["G4","q"], ["E4","q"], ["C#4","q"], ["D4","q"]]
        }
      },
      "given_start_pitch": false,
      "count_in_bars": 1
    }
  ],
  "grading": { "mode": "relative_drift_tolerant" }
}
```

**Compact note JSON:** `[pitch, duration]` pairs. Pitch is scientific notation with `#`/`b` (`##`/`bb` allowed) or `r` for a rest. Duration codes: `w h q e s`, `.` for dotted, `~` to tie into the next note. An optional third element is a lyric or solfège syllable. Items can also reference a stored score by `score_id`.

### Trial record

```text
trial_id, session_id, block_idx, trial_idx, type, protocol, tags[],
stimulus (resolved JSON), pool, feedback_mode, mask,
response (answer, rt_ms | recording ref), metrics (JSON), correct (bool | null),
trace_id, recording_path, started_at (UTC), start_frame,
device_profile_id, calibrated (bool), secs_since_labeled_reference, interrupted (bool)
```

All statistics are computed from this log on demand. Nothing else stores "skill state".

### Skill tags

Dotted tags for filtering and grouping, e.g. `ap.id`, `ap.id.octave_transfer`, `ap.id.timbre_transfer`, `ap.production`, `ap.context`, `rp.interval.melodic`, `rp.interval.harmonic`, `rp.chord`, `rp.scale_degree`, `rp.function`, `voice.pitch_match`, `voice.sustain`, `voice.interval`, `voice.harmony`, `voice.sight_singing`, `rhythm.reproduction`, `dictation.melodic`. Each exercise type has default tags; blocks can add more.

### Validation

Sessions deserialize into strongly typed Rust structs (`serde_path_to_error` gives exact JSON paths for errors), then pass semantic checks: known timbres, pool rules, notes within the range profile (warning), consistent params, estimated duration. The app and the MCP server use the same validator.

---

## 9. Absolute-pitch suite (first feature set)

### Skill dimensions (tagged and reported separately)

Pitch-class identification · note labelling with octave · AP production · cross-octave transfer · cross-timbre transfer · AP in musical context · response latency · retention. Relative pitch is a separate family. The app never reports a single "you have perfect pitch" verdict.

### Exercise types

| Type | What happens | Key params |
|---|---|---|
| `pitch_id` | Tone plays → user names it (12 on-screen buttons, computer keys in piano layout, or a silent MIDI keyboard). Optional "not in set" answer for out-of-set distractors. | classes, out_of_set_rate, octaves, timbres, pool, duration, loudness range, deadline_ms, mask, answer = `pitch_class` \| `note_with_octave` |
| `sing_pitch` | Named prompt ("Sing E♭"), no reference → user sings; graded by pitch class, octave reported. | classes, prompt (on-screen text; speech arrives with hands-free mode, §17), response_ms, delay_ms, pre_mask, distractor (tones or music before the prompt), reveal_target_after |
| `pitch_pairing` | Passive: tone → pause → on-screen label (spoken labels arrive with hands-free mode, §17). No response. Logged as exposure, never counted as AP evidence. | classes, timbres, octaves, gap_ms, rate |
| `incidental_pairing` | Secondary visual task (e.g. press the colour shown) in which each colour is consistently accompanied by one pitch class. Logged as exposure; transfer is measured only with ordinary `pitch_id`. | class-to-colour map, timbres, octaves |
| `anchor_song` | User-defined anchors (song + original-key first pitch, optional MIDI). Mode A: "Sing the first note of <song>", graded absolute. Mode B: "Original key or shifted?" for a melody played at 0/±1/±2 semitones. | anchors, mode, shift set |
| `tonal_center_id` | Short unfamiliar progression or melody (generated, or a library piece transposed to a random key) → name the tonic or key, or a chord's root. Without reference it is AP-in-context; with `reference: true` it is relative tonic finding. | source, keys, length, timbres, reference, answer |

Identification response times are measured from stimulus onset on the audio clock; a missed deadline logs a `timeout` trial.

### Assessment battery (preset `ap_assessment`)

All holdout stimuli, no feedback, masks between trials, placed first in a session:

1. 12-class identification, octaves 3–5, holdout piano-like timbre — 48 trials
2. Cross-octave identification, octaves 2 and 6 — 24 trials
3. Cross-timbre identification, all holdout timbres including `shepard` and `synth:sine` — 36 trials
4. Speeded identification, 1500 ms deadline — 24 trials
5. No-reference production, each pitch class once in random order, masked — 12 trials
6. Tonal-centre identification on unfamiliar excerpts, no reference — 12 trials

The report (`get_ap_profile`) gives: exact and ±1-semitone accuracy per test, median response time, 12×12 confusion matrix, signed-error histogram (systematic sharp or flat category bias), accuracy by timbre and by octave, production category accuracy and median |cents|, and comparison with earlier battery runs (retention when the gap is long).

### Presets shipped with the app

- `ap_assessment` — the battery above.
- `ap_progressive/stage_NN` — starts with three classes spread across the octave (e.g. C, E, G♯) plus out-of-set distractors; each stage adds one class; deadlines tighten from 4 s to 2 s. Suggested criterion: ≥ 90 % over two sessions. Stats tools report the criterion; the LLM (or the user) moves to the next stage.
- `ap_daily` — short mixed identification and production block.

Presets and "repeat last session" work without the LLM.

---

## 10. Relative pitch, harmony and rhythm

| Type | Description |
|---|---|
| `interval_id` | Melodic ascending/descending or harmonic intervals → name the interval. Params: interval set, directions, register, root range, timbres. With `in_key` a cadence plays first and both notes come from the key's scale (fixed key, or a new key per block or per question); without it the intervals have no key. |
| `chord_id` | Quality (major, minor, diminished, augmented, suspended, sevenths, …) and/or inversion; close or open voicing; register. |
| `scale_degree_id` | After Functional Ear Trainer: a cadence sets the key, a tone plays → name its degree (numbers or solfège, chromatic degrees optional). After the answer the tone walks the scale to the tonic: to the nearer tonic, always up or always down. Keys fixed or changing per block or per question; presets `degrees_01`–`08` form the course. |
| `interval_comparison` | Two intervals → which is larger (or are they the same). |
| `scale_id` | Scale or mode played → name it (major, natural/harmonic/melodic minor, modes, pentatonics, blues, …). |
| `progression_id` | Identify a cadence type or Roman-numeral progression. |
| `melody_error_detection` | See a melody and hear a performance of it → mark the notes that differ in pitch or rhythm. |

Tonic finding uses `tonal_center_id` with `reference: true`. Answer labels per block: note names, movable-do, fixed-do or scale-degree numbers.

Separate rhythm types (reproduction, reading, error detection) are dropped: rhythm is trained inside sight singing, where every note is graded for timing.

---

## 11. Singing exercises

| Type | Description | Grading mode |
|---|---|---|
| `pitch_match` | Hear a reference → sing it (together with it or after it). | absolute |
| `sustain` | Hold a note for N seconds, optionally over a drone. | absolute; centre error, drift, stability, vibrato |
| `sing_interval` | From a start note (played or self-chosen), sing an interval up or down. | relative_to_first or absolute |
| `pattern` | Scales, arpeggios, slides and vocalise patterns, transposed stepwise through a range. | absolute or relative |
| `harmony` | Sing one voice while the app plays another: a diatonic third or sixth below or above a generated melody (by ear, or read from the staff), the melody over that voice, or one part of a multi-part library piece. The other voice shows in the lane. | absolute; equal or just tuning (each note against the pure interval over the other voice) |
| `echo` | Hear a phrase → sing it back. | melody alignment |
| `own_voice_match` | Match a stored best take of your own voice. | absolute |

### Live tuner and follow-along display

- **Tuner** (its own view, and available in any free-singing moment): large note name, cents meter, frequency, a scrolling pitch history, and an optional reference tone or drone. Uses the live pitch pass (§5).
- **Follow-along pitch lane** for every sung exercise with more than one note: target notes are bars on a semitone grid that scroll past a fixed "now" line in time with the metronome; the live pitch trace is drawn over them; the current note is highlighted; a count-in precedes the first note. After the attempt the bars are coloured by grade (§15 colours).
- The lane follows the block's feedback mode: `continuous` shows the live trace during the attempt; `after_trial` reveals the trace and grades afterwards; `none` hides both. For sight singing, `lane_targets: hidden_until_done` keeps the target bars hidden so the lane does not give the pitches away; the score is the only guide.

### Range profile

`lowest_reachable`, `highest_reachable`, `comfortable_low`, `comfortable_high` (MIDI), plus `updated_at` and `source` (`manual`, `llm`). The singer sets it in Settings; there is no range finder. Generators keep melodies inside the comfortable range unless a block opts out. The LLM can read and set it. A single extreme note never updates it automatically.

---

## 12. Sight singing and dictation

### Melody sources

1. **Generator** (`trainer-core`, deterministic from a seed). Params: key, mode, meter, bars, tempo, range (defaults to the comfortable range), rhythm vocabulary, max leap, step/leap mix, interval weights (e.g. emphasise descending minor sixths), chromatic-note rate, start and end degrees, final cadence, phrase structure (e.g. AABA). Shared by sight singing and dictation.
2. **LLM-authored**: compact note JSON or MusicXML via MCP (`create_score`).
3. **Library**: a watched folder (`%APPDATA%\SingingTrainer\library`) of JSON and MIDI files; a subfolder is a collection. The coach converts the user's existing scores into these files (or adds pieces over MCP with `add_library_pieces`); the app does not import MusicXML.
   - JSON: one piece, a list, or a collection. A piece has one part (`notes`) or several (`parts`, top first). Notes are `{note, beats}` / `{rest}` objects or compact text, `"Eb4:1 F4:.5 G4 | Ab4:2~ | Ab4:1 R:1"`, with durations that carry over, ties, and checked barlines (a short first bar is a pickup).
   - MIDI: one part per track, chords reduced to the top line, times rounded to sixteenths, a short first time signature kept as a pickup, the key from the key-signature event or estimated (Krumhansl–Kessler).
   - Exercises select pieces by id, collection or tag (`{"library": {…}}`), move them by octaves into the comfortable range, and record the piece id with each trial.

### Internal representation

`Score { meta, key/time/tempo maps, parts[ notes { pitch, spelling, onset_ticks, dur_ticks, tie, lyric? } ] }`, converted to and from MusicXML (rendering, LLM editing) and compact note JSON (LLM authoring). Grading uses the internal form.

### Rendering

OSMD renders MusicXML. During an attempt the cursor follows the click; after grading, noteheads are coloured from the grading result (green correct, yellow marginal, red wrong, grey missed). The renderer never grades.

### Exercise types

| Type | Description |
|---|---|
| `sight_sing` | Score shown → count-in → sing. Start pitch given or not (no-reference sight singing doubles as AP practice). Optional click, drone or accompaniment. |
| `melodic_dictation` | Melody played N times → answer on a MIDI or on-screen keyboard (pitches in order, optionally rhythm by playing in time to the click) or by singing it back. |
| `harmonic_dictation` | Progression played → bass line plus chord qualities or Roman numerals. |
| `audiation` | Score shown, tonic given, silence → sing a requested note or bar on cue. |

---

## 13. MCP server

### Process and setup

`trainer-mcp.exe` is a stdio server built on `rmcp`. Any MCP-capable agent can use it; the main candidates are OpenCode and Hermes Agent.

OpenCode (`opencode.json`):

```json
{
  "$schema": "https://opencode.ai/config.json",
  "mcp": {
    "singing-trainer": {
      "type": "local",
      "command": ["C:\\path\\to\\trainer-mcp.exe"],
      "enabled": true
    }
  }
}
```

Hermes Agent (`config.yaml`, in `%LOCALAPPDATA%\hermes\` on native Windows or `~/.hermes/` under WSL2):

```yaml
mcp_servers:
  singing-trainer:
    command: 'C:\path\to\trainer-mcp.exe'
```

- Other MCP clients (Claude Code, Claude Desktop, …) take the same command.
- The server uses the app's data directory by default; `--data-dir` overrides it.
- An agent running under WSL2 can launch the Windows executable through its `/mnt/c/...` path; it still runs as a Windows process and uses the Windows data directory.

### Design

- Tools return compact JSON plus a short deterministic text summary. No raw SQL and no arbitrary file access.
- Tool input schemas are generated from the same Rust types the app uses, so the agent sees exact field names and enums.
- Outputs are bounded: pagination, downsampled traces, top-N lists.
- The same server will back the in-app coach panel (§17), so nothing here changes when ACP is added.

### Read tools

| Tool | Returns |
|---|---|
| `get_overview` | recent sessions, practice totals by tag over 7 and 30 days, latest comfort reports, range profile, coach-notes digest, queued sessions |
| `get_session_report(session_id)` | per-block summaries: accuracy, response times, cents-error statistics, notable trials |
| `query_trials(filters, limit, cursor)` | trial rows with stimulus, response and key metrics |
| `get_stats(filters, group_by[])` | counts, accuracy, median response time, median and mean \|cents\|, signed bias, grouped by any stimulus, param or tag field (e.g. `stimulus.pitch_class`, `stimulus.timbre`, `params.interval`, `params.direction`, `feedback_mode`, `protocol`, `week`). This replaces a built-in error-clustering model. |
| `get_confusion_matrix(filters, field)` | confusion matrix for identification tasks (pitch class, interval, chord) |
| `get_trial_detail(trial_id)` | full metrics, per-note results, alignment, downsampled trace (cents vs target at 20 Hz), text summary |
| `get_trial_plot(trial_id)` | PNG image: pitch trace, note centre, target band, onsets |
| `get_ap_profile(battery_run_id?)` | battery report (§9) and comparison with earlier runs |
| `get_range_profile` | current range profile |
| `list_exercise_types` | every type with its params schema, defaults, tags and an example |
| `list_timbres` | timbres with their pool (training or holdout) |
| `list_library(collection?, tag?, query?)` / `get_library_piece(id)` | the melody library: folder, one summary per piece (key, metre, bars, parts, range), unreadable files; one piece in full |
| `get_coach_notes(tags?)` | notes the LLM saved in earlier conversations |

### Write tools

| Tool | Effect |
|---|---|
| `validate_session(session)` | errors with JSON paths, warnings, estimated duration, preview of the first trials |
| `queue_session(session, start_now?)` | validates and adds to the app's "Up next"; `start_now` brings the app forward with a Start prompt |
| `save_preset(name, session_or_block)` | stores a reusable template |
| `generate_melodies(params, n)` | generator candidates (note JSON + summary) to review before use |
| `add_library_pieces(pieces, collection?, file_name?)` | validates the pieces and writes them as one JSON file in the library folder (the same file name replaces it) |
| `import_library_files(paths, collection?)` | copies JSON or MIDI files into the library folder and reads them |
| `remove_library_file(file)` | deletes a library file and its pieces |
| `regrade(filters, grading_overrides, persist?)` | re-scores stored traces with different tolerances; results are returned and stored only when `persist` is set |
| `set_range_profile(...)` | updates the range profile with `source = llm` |
| `save_coach_note(text, tags)` | persists plans and observations across conversations, stored with the training data whichever agent is used (complements the agent's own memory) |

Part 2 adds `import_audio`, `separate`, `transcribe_vocals`, `get_transcription`, `update_transcription`, `create_excerpt` and `search_passages`.

### Server instructions (sent on connect)

- The app grades. Use its metrics; never re-judge pitch or rhythm yourself.
- Use holdout pools only for assessment and schedule assessments at the start of a session. Fade feedback (continuous → after_trial → none) as accuracy stabilises. Test transfer (new octaves, timbres, keys) before calling something learned, and retest after gaps for retention.
- Passive and incidental exposure is not evidence of AP.
- Respect comfort reports: mild fatigue → fewer sung blocks; discomfort → no range or intensity work; pain or new hoarseness → no demanding vocal work, and suggest professional evaluation if it persists. Never diagnose; a correct pitch is not evidence of healthy technique.
- Keep melodies inside the comfortable range unless deliberately extending it.

### Typical loop

```text
get_overview → get_stats / get_confusion_matrix (weak spots)
→ generate_melodies / create_score (if needed) → validate_session → queue_session
→ [user practises] → get_session_report → save_coach_note
```

---

## 14. Data

SQLite (WAL mode, JSON1) in the app data directory (`%APPDATA%\SingingTrainer\`), plus files.

**Tables:** `sessions`, `blocks`, `trials`, `traces`, `recordings`, `regrades`, `queue`, `presets`, `scores`, `score_versions`, `timbres`, `stimulus_pools`, `range_profile`, `anchors`, `comfort_reports`, `coach_notes`, `device_profiles`, `calibrations`, `battery_runs`.
Part 2 adds `assets`, `stems`, `analysis_runs`.

**Provenance:** `score_versions.source` is `generated | imported | llm | model | user_verified`, with `supersedes` and `created_at`. `analysis_runs` records model name, version, checkpoint hash, configuration, input hash and timestamp.

**Files:** `recordings/YYYY/MM/<trial_id>.wav`, `scores/<id>/<version>.musicxml`, `soundfonts/`, `assets/` (Part 2).

**Retention:** traces are kept forever (a few KB per trial). WAV recordings are kept for 60 days by default, except pinned takes (own-voice references and takes the user flags).

**Append-only:** trials are never edited. Persisted re-grades go to `regrades (trial_id, grading_config, metrics, created_at)`.

**Comfort:** an optional one-tap check before and after a session (comfortable / mild fatigue / discomfort / pain / new hoarseness) plus a free-text note to the coach.

---

## 15. UI

| View | Contents |
|---|---|
| Audio setup | device page (§3) and calibration |
| Tuner | live pitch line over a note grid, cents readout, voicing, optional drone |
| Up next | queued sessions from the LLM and presets; start, skip, delete |
| Session runner | per-type trial views (answer pads, score view, pitch display according to feedback mode), progress, pause |
| Results | session summary, trial list, trial detail (trace plot, per-note table, replay recording against target) |
| History | accuracy, response-time and cents trends by tag |
| Library | scores, presets, timbres and pools, anchor songs |
| Settings | A4, note spelling, solfège system, keyboard mapping, MIDI device, retention |

All 12-class answers have computer-keyboard shortcuts in piano layout; a MIDI keyboard works wherever the answer is a note.

### Visual style

Clean, professional and quiet: it should look like a well-made desktop tool, not a marketing site.

- **Theme:** light mode. App background `#F6F7F9`, content surfaces `#FFFFFF`, 1 px borders `#DDE1E6`, primary text `#1F2328`, secondary text `#59636E`. One accent, a muted blue `#2D6CDF`, for primary actions, selection and focus rings.
- **Type:** `"Segoe UI Variable Text", "Segoe UI", system-ui, sans-serif`; 14 px base, 13 px secondary, headings 600 weight at modest sizes; tabular figures for cents, Hz, percentages and timers.
- **Layout:** left navigation rail (Practice, Tuner, Results, Library, Audio, Settings), content in a centred column, 8 px spacing grid, 6 px corner radius.
- **Avoid:** gradients, glows, glass effects, heavy shadows, emoji, oversized hero text, decorative illustrations, marketing copy. Plain labels and sentence case. Shadows only on popovers and menus.
- **Motion:** only where it carries information (tuner, pitch lane, playhead at display rate); other transitions ≤ 120 ms.
- **Grading colours** (also used in score colouring and the pitch lane): correct `#1A7F37`, marginal `#9A6700`, wrong `#CF222E`, missed `#8C959F`, current `#2D6CDF`.
- **Pitch lane:** white canvas, light semitone grid with slightly darker C rows and note labels on the left, target bars in light blue with a darker outline, live trace as a 2 px dark line.

---

## 16. Part 2 — Repertoire pipeline

```text
audio file → decode and normalise (symphonia, rubato) → hash
  → vocal separation (ml/: audio-separator; RoFormer vocal model by default, Demucs fallback)
  → vocal F0 (torchcrepe or RMVPE), dense trace kept
  → beats and downbeats (beat-this) → tempo and bar grid
  → note segmentation (F0 + onsets; basic-pitch on the stem as a second opinion) → notes with confidence
  → quantise to the grid → MusicXML + per-note confidence and F0 summary
  → LLM review and correction via MCP (MusicXML)
  → user listens (transcription synth over the stem) and marks it verified
  → excerpts → repertoire exercises
```

- Every run is logged in `analysis_runs`.
- Trust order: user-verified > LLM-corrected > model output. Re-running a model never overwrites a higher-trust version.
- `get_transcription` gives the LLM per-note pitch, timing, confidence and F0 median/spread, and flags low-confidence regions, so corrections rest on evidence (the LLM cannot hear the audio).
- Exercise modes: sing along with the original (vocal at reduced level), karaoke (instrumental stem), echo, sight-read the phrase from notation.
- Grading: against the corrected notes (melody alignment) and/or directly against the original singer's F0 contour (DTW-aligned cents error with optional key and octave normalisation). Contour grading needs no transcription at all.
- `search_passages` filters excerpts by range, contained intervals, tempo, length and confidence, e.g. "vocal phrases, 2–8 bars, containing a descending minor sixth, inside the comfortable range, confidence ≥ 0.8".

---

## 17. Future app

Not part of V1 or V2. Each item reuses the engine, session runner, MCP server and data model as they are.

### In-app coach panel (ACP)

Hosts the agent inside the app window instead of a separate terminal or chat window, using the Agent Client Protocol (ACP), the protocol Zed, JetBrains IDEs and several Neovim plugins use to host external agents.

- **Agents:** OpenCode (`opencode acp`) and Hermes Agent (`hermes acp`) both run as ACP servers over stdio. Hosted this way, Hermes keeps its own memory, skills and provider setup. The agent command is a setting, so any other ACP agent can be swapped in.
- **Client:** `trainer-app` acts as the ACP client using the Rust `agent-client-protocol` crate and spawns the agent as a subprocess.
- **Tools:** `session/new` passes `trainer-mcp` in `mcpServers` (Hermes adds client-provided servers to each session); alternatively the agent keeps it in its own MCP config (§13). `cwd` is a coach folder the user picks, where reference documents can live.
- **Permissions:** the client advertises no file-system or terminal capabilities. `trainer-mcp` tool calls are approved according to a user setting; any other permission request is shown to the user. The agent's own tools can be trimmed for coach sessions in its config (e.g. Hermes `platform_toolsets.acp`).
- **Panel:** streams messages, tool calls and plans. Context buttons ("ask about this trial", "plan my next session") insert trial or session ids into the prompt. Sessions the agent queues reach "Up next" through the existing queue watcher.
- **References:** Zed's agent panel and the example clients in the `agent-client-protocol` crate. Zed's GPL code can be reused directly; licensing is settled at the end.

### Hands-free mode

For practice away from the PC (around the house, and later on the phone client). The same session runner with audio-only prompts and feedback.

- **Flow:** prompt → response window → mic capture → spoken feedback → next. "Sing F-sharp." → mic opens → "Correct. Twelve cents sharp."
- **Spoken prompts:** Windows `SpeechSynthesizer` (via the `windows` crate) renders text to WAV, cached by text hash in `tts-cache/` and played through the engine (same clock, same headphones). Covers note names (spelled for the context), interval and chord names, numbers and feedback phrases. This also enables speech prompts for `sing_pitch` and spoken labels for `pitch_pairing`.
- **Responses:** sung answers are graded as usual; a MIDI pedal or keys handle skip and repeat. Spoken note-name answers via speech recognition are optional, with tap or key answers kept for noisy rooms.
- **Passive exposure:** `pitch_pairing` blocks with spoken labels (preset `ap_passive`) are logged as exposure and never credited like active trials.
- A queued session can be flagged `hands_free`; once started, it needs no screen interaction.

### Phone client

For hands-free practice and short drills away from the PC:

- Runs sessions prepared on the PC offline and syncs trials back; because the trial log is append-only, sync is just appending.
- Native audio for background headphone playback (Oboe/AAudio on Android, AVAudioSession/Core Audio on iOS), not a browser tab.
- AP and ear-training drills, simple mic pitch feedback, hands-free sessions.
- Heavy analysis (separation, transcription) stays on the PC.
- Grading and generators live in `trainer-core`, so the same Rust code can be built for the phone.

---

## 18. Testing

- **Golden synthetic signals**, generated in the tests: harmonic tones with known F0 offsets, onset delays, glides, drift, vibrato (e.g. 5.5 Hz ±50 cents), breath noise and gaps. Assert: centre error within ±1 cent, settling time within ±20 ms, vibrato rate within ±0.2 Hz, onset within ±5 ms.
- **Alignment:** melodies with inserted, deleted and shifted notes; transposed and drifting performances.
- **Calibration:** simulated loopback with a known delay.
- **Generators:** property tests (range, leap limits, determinism from seed, pool rules).
- **Validator:** valid and invalid sessions, with the expected error paths.
- **MusicXML and MIDI:** round-trips on library samples.
- **MCP:** contract tests calling each tool against a temporary database; snapshot outputs.
- **Real voice:** a small set of the user's own recordings with hand-checked notes as regression tests.
- **Engine checklist on the Scarlett (manual):** unplug and replug, buffer and sample-rate changes in Focusrite Control, a 60-minute session without xruns or clock drift, recording while the UI renders.
- **CI:** Windows build with the `asio` feature; `trainer-core` tests on every push.

---

## 19. Build phases

Each phase ends with something usable. "Done when" items are acceptance checks.

| Phase | Deliverables | Done when |
|---|---|---|
| 1. Foundation | Cargo workspace, Tauri shell, `trainer-core` types, SQLite and migrations, cpal ASIO engine, device page, calibration, standalone tuner, post-trial pitch tracker, single-note grading, golden tests | The tuner runs on the Scarlett via ASIO at 128 frames with no xruns; golden tests pass; a loopback calibration is stored |
| 2. AP suite | Stimulus engine (SoundFont, synth, Shepard, masks, pools), session runner, every §9 exercise type, assessment battery, presets, results view | A full `ap_assessment` and an `ap_progressive` stage run end to end and log trials |
| 3. MCP | `trainer-mcp` with the read and write tools, server instructions, queue watcher in the app | From OpenCode or Hermes: read the last session, queue a new AP session, run it in the app, read its report |
| 4. Relative pitch and singing | §10 and §11 types, follow-along pitch lane with metronome, backing options, range profile, own-voice references | Each type runs from an MCP-queued session and grades correctly on golden tests |
| 5. Sight singing and dictation | Melody generator, score model, melody library (JSON and MIDI files), staff rendering with colouring, melody alignment, §12 types, Listen / Sing along / Retry flow | Generated, LLM-authored and library melodies are sung and graded; a MIDI file and a coach-written JSON file import |
| 6. Part 2 ingest | `ml/` worker, import, separation, vocal F0, beat grid | A song produces a vocal stem, an F0 trace and a bar grid |
| 7. Part 2 transcription | Notes → MusicXML with confidence, MCP transcription tools, listen-and-verify view | The LLM corrects a transcription via MCP and the user marks it verified |
| 8. Part 2 repertoire | Excerpts, sing-along/karaoke/echo/sight-read modes, contour grading, passage search | A phrase from an imported song is practised and graded against the original vocal |

V1 = phases 1–5. V2 = phases 6–8.

Future app items (§17) have no fixed position:

| Item | Deliverables | Done when |
|---|---|---|
| F1. In-app coach | ACP client, coach panel, permission handling | OpenCode or Hermes runs in the app window, reads the last session and queues the next one |
| F2. Hands-free | TTS cache, audio-only runner, passive blocks, `ap_passive` preset | A hands-free AP session runs start to finish without touching the screen |
| F3. Phone client | Offline session player, trial sync, native audio | A session prepared on the PC runs on the phone and its trials appear in the PC history |

---

## 20. Repository layout

```text
SingingApp/
├─ Cargo.toml              workspace
├─ crates/
│  ├─ trainer-core/        schema, theory, generators, scores (MusicXML/MIDI), analysis
│  │                       (pYIN, segmentation, alignment), grading, storage, stats
│  ├─ trainer-audio/       cpal engine, scheduler, recorder, live pitch, synth
│  ├─ trainer-app/         Tauri 2 app: commands, session runner, queue watcher
│  └─ trainer-mcp/         stdio MCP server (rmcp)
├─ ui/                     React + TypeScript + Vite (OSMD, canvas tuner, uPlot)
├─ presets/                ap_assessment.json, ap_progressive/*.json, ap_daily.json, …
├─ assets/                 soundfonts (FluidR3_GM), default stimulus pools
├─ ml/                     Part 2 Python worker (uv project)
└─ docs/
```
