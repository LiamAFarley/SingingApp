# App extensions for song practice packages and the exercise corpus

**Date:** 24 October 2026 (revised the same day with the owner's priorities)
**Inputs:** `song_practice_ingestion_playback_pipeline.md` (song packages) and `EXERCISE_CORPUS_PLAN.md` (corpus, courses, conversion).
**Scope:** what the app (trainer-core, trainer-audio, trainer-app, UI, trainer-mcp) must be able to do to *use* what those two pipelines produce.

Out of scope:

- separation, beat tracking, transcription and F0 extraction;
- OMR, MusicXML conversion and the corpus build.

Those run outside the app and hand it files. The app's side of that boundary is a file contract, a validator and the features that consume the files.

---

## 0. Bottom line

**Priorities:**

1. **Courses and warm-ups first.**
   - Collection pages and course manifests with progress.
   - Warm-up vocalises with generated backing tracks.
   - Instructions shown before an exercise.
   - The 315 Crowell melodies that already convert need nothing more from the notation.
2. **Then the shared notation work:**
   - a finer time grid (960 ticks per quarter) for tuplets and 32nds;
   - tempo maps and fermatas;
   - metre and key changes;
   - rhythm on a percussion staff, read by tapping;
   - alto and tenor clefs, aligned multi-part systems, grace notes and lyrics or solfège under the notes.
3. **Then the rest of the courses:** the material that needs that notation, i.e. rhythm courses, chorales, duets and canons, and the remaining Trained Ear books.
4. **Songs are deferred.** The plan for them is kept (§3) so the shared work fits them later.

**Out of the app entirely:**

- **Identity and rights.** The corpus build decides what goes into the library and courses. Everything is for personal use.
- **Breathing and bodywork routines.** They live with the coach agent, not in the app.

**Deferred indefinitely:** MIDI keyboard input (wanted eventually) and `sing_and_play`.

---

## 1. What the app has today (facts this plan builds on)

| Area | Current state | Where |
|---|---|---|
| Score grid | `TICKS_PER_QUARTER = 4` (16th grid); `from_compact` rejects anything finer | `trainer-core/src/score.rs` |
| Clefs | `treble`, `treble8vb`, `bass` | `score.rs` `Clef`/`ClefChoice` |
| Notes | `CompactNote::Note{note, beats}` / `Rest{rest}`; `ScoreNote.pitch: None` = rest | `score.rs` |
| Score shape | one key, one metre, one clef, one tempo | `score.rs` `Score` |
| Library | watched folder `%APPDATA%\SingingTrainer\library`; `.json`, `.mid`, `.midi`; subfolder = collection; ≤ 4 parts; one key/metre/tempo per piece | `trainer-core/src/library.rs` |
| Library selection | `{"library": {ids \| collection \| tags, shuffle, part, fit_range, piece_tempo}}` in sight singing, dictation, harmony | `schema.rs` `LibrarySelect` |
| Courses | hardcoded regex over preset ids | `ui/src/views/PracticeView.tsx` `COURSES` |
| Audio engine | cpal (ASIO on Windows); pre-rendered `SoundBuffer`s scheduled at absolute output frames on 4 buses (`prompt`, `backing`, `guide`, `click`) with bus gains | `trainer-audio/src/engine.rs` |
| Audio files | `hound` (WAV) for recordings; no other decoders, no resampler, no time-stretch | `Cargo.toml` |
| Backing | generated from score/key: `none`, `click`, `drone`, `chords`, `melody`, `melody_and_chords`; harmony `play_parts` | `schema.rs` `Backing` |
| Live pitch | `LiveFrame` every 10 ms with `in_frame`, `hz`, `midi`, `clarity`, `rms_db`; latency calibration and `capture_offset_s` | `trainer-audio/src/analysis.rs` |
| Grading | `grade_sung`: segments → Needleman–Wunsch alignment → per-note cents/onset; modes `absolute`, `any_octave`, `pitch_class`, `relative_to_first`, `relative_drift_tolerant`; just tuning | `trainer-core/src/grade.rs` |
| Expected notes | built from `start_beat × 60/tempo` (one constant tempo) | `grade.rs` `Expected` |
| Trials | SQLite `trials` row with `stimulus_json`, `spec_json`, `metrics_json`, `assisted`, `start_frame`; library melodies record `stimulus.piece`; no indexed item or course id | `trainer-core/src/storage.rs` |
| Staff | VexFlow 5, one score per `Staff`; multi-part pieces are stacked separate staves; live/graded trace with octave folding and tint | `ui/src/notation/Staff.tsx` |
| MIDI keyboard input | planned in the main plan, **not implemented** | — |
| MCP | 27 tools; library read/write; sessions; stats | `trainer-mcp/src/lib.rs` |

The pipeline document assumes a C++/JUCE engine. Here the equivalents are:

- the existing Rust/cpal engine;
- `signalsmith-stretch`, a Rust wrapper of the MIT C++ library, for stretching and pitch-shifting;
- `symphonia` for FLAC and other decoding;
- `rubato` for resampling.

---

## 2. Shared foundations

These serve both documents. Each lists the change, why it is needed and how to tell it is done.

### 2.1 Time grid: 960 ticks per quarter

- **Why.**
  - The corpus needs tuplets and 32nds: 26 and 11 Crowell files, plus the Aural Training compound and triplet material.
  - The song pipeline specifies PPQ 960 for its musical time.
  - One grid for both avoids a second conversion layer.
  - 960 = 2⁶·3·5, so it covers triplets, quintuplets, sextuplets and 64ths.
- **Change.**
  - Set `TICKS_PER_QUARTER = 960`.
  - Audit every tick constant: the generator's rhythm cells, the dictation editor's 16th steps, the Staff tick ↔ x maps, `beat_ticks`, `bar_ticks`.
  - Compact durations accept rationals as well as decimals: `"C4:1/3"`, `{"note": "C4", "beats": "1/3"}`.
  - Conversion snaps to the nearest tick and errors only when the error is more than one tick.
  - `Score` stores `ppq` so stored trials stay readable. Old specs at PPQ 4 are scaled on load.
- **Tuplet notation.**
  - A group marker so the staff can draw brackets, e.g. `{"tuplet": "3:2", "notes": [...]}` or compact `3:2(C4:.5 D4:.5 E4:.5)`.
  - Durations inside are the sounding durations. The group only informs rendering.
- **Done when:**
  - tuplet, 32nd and 64th fixtures round-trip through compact → `Score` → compact;
  - every existing library piece and preset produces identical note onsets in seconds, checked by a regression test comparing old and new;
  - the dictation editor still enters 16ths, and grading results on the existing test signals are unchanged.

### 2.2 Tempo maps

- **Why.**
  - Songs are a beat map, not a BPM. The pipeline's §8–§10 say to "not reduce a song to one BPM number."
  - Corpus items need fermatas as timed holds, and MusicXML tempo changes.
- **Change.**
  - Add a `TempoMap` in trainer-core:
    - either a list of `(beat, seconds)` anchors with linear interpolation, which is the pipeline's `BeatGrid`;
    - or `(beat, bpm)` segments, which is what scores use.
  - Functions: `beat_to_s`, `s_to_beat`, `scaled(speed)` and `bpm_at(beat)`.
  - `Score` gains an optional `tempo_map`. When it is absent, the constant `tempo_bpm` applies, as now.
  - Fermata: a note flag. The map adds a hold, by default ×2 the note's length or a set number of beats, and the click pauses through it.
  - Everything that turns beats into time uses the map. That covers:
    - `SungTrial` rendering (backing, click, guide);
    - `lane_for` sections;
    - `grade_sung`'s expected notes;
    - the Staff cursor.
- **Done when:**
  - a piece with a ritardando and a fermata plays, clicks, scrolls and grades consistently;
  - a 4-minute beat grid round-trips `beat_to_s(s_to_beat(t)) == t` within 0.1 ms.

### 2.3 Metre and key changes

- **Why.**
  - 3 of 427 Crowell files change metre.
  - Song transcriptions report time-signature changes.
  - The corpus plan offers splitting at the change as a fallback. Supporting changes in the app is better, because songs need them too.
- **Change.**
  - `Score.changes: Vec<{tick, key?, time?}>`.
  - Bar arithmetic walks the changes.
  - The staff draws new key and time signatures at the bar where they occur.
  - Compact form: an inline `{"time": "3/4"}` / `{"key": "G major"}` item between bars.
  - Generators keep producing single-metre scores.
- **Done when** the 3 Crowell metre-change pieces render with correct bar counts and play correctly.

### 2.4 Multi-track mixer

- **Why.**
  - Corpus P2 asks for MIDI backing tracks with roles and gains, and recorded stems per course item.
  - Songs need 5–7 channels: instrumental, backing vocals, lead vocal, piano melody, piano accompaniment, metronome, monitor.
  - Both should use one mixer.
- **Change, trainer-audio.**
  - Replace the fixed 4-bus model with **channels**: an id, role, gain, mute, solo and pan.
  - The current buses become default channels, so existing code keeps working.
  - Solo logic and fades apply per channel in the callback. Mute and gain changes are instant and click-free, using the existing 256-frame fade.
- **Change, schema.**
  - `SungTrial.backing_tracks: Vec<Track>`. A `Track` is `{role, events | audio_ref, timbre, gain_db, default: on/off}`.
  - Roles: `click`, `pulse`, `chordal`, `bass`, `guide_melody`, `counterpart`, `accompaniment_audio`, `guide_audio`.
  - The existing `Backing` enum is rendered into tracks, keeping compatibility.
- **Change, UI.**
  - A mixer strip, shared by the session view and the song player, with mute/solo/level per channel.
  - Changes during a trial take effect live.
- **Done when** a harmony or sight-singing trial with three generated tracks can solo and mute each one live, and two simultaneous trials of the same spec produce sample-identical output.

### 2.5 Audio files: decode, resample, memory

- **Why.**
  - Song stems are WAV at 48 kHz.
  - Corpus course assets are FLAC (`assets/<item>/<stem>.flac`).
  - The device may run at 44.1 kHz.
- **Change.**
  - `symphonia` for decoding (WAV, FLAC, plus MP3 for the corpus's demo media) and `rubato` for resampling to the engine rate on load.
  - Short course stems are preloaded and resampled in memory (a few MB).
  - **Song stems are streamed** (§3.3). At 4 minutes, 48 kHz stereo f32 is about 92 MB per stem, and six stems would be about 550 MB. They are read from disk into ring buffers by a feeder thread instead.
  - A cache of resampled stems under `songs/<id>/cache/` (the package already reserves `cache/`) avoids resampling on every open when the device rate differs.
- **Done when:**
  - a 48 kHz package plays correctly on a 44.1 kHz device;
  - opening a cached song takes under 1 s;
  - memory stays under 100 MB while playing six stems.

### 2.6 Progress per item

- **Why.** Courses need to know which pieces have been attempted and passed.
- **Change.**
  - No identity, provenance or rights system in the app.
  - Trials of library pieces already record `stimulus.piece`.
  - Course blocks also record the course and unit ids in the block spec, so they can be found in `spec_json`.
  - Progress queries read these fields with SQLite `json_extract`. An expression index is added only if it proves slow at corpus scale.
- **Done when** a course page shows per-item attempted and passed status from stored trials.

### 2.7 Grading against explicit note times and confidence

- **Why.**
  - Songs give note onsets in source seconds, already tempo-mapped.
  - Low-confidence or unverified notes should not count as strictly as verified ones (pipeline §18, §51).
- **Change.**
  - `grade_sung` takes a list of `ExpectedNote {midi, onset_s, dur_s, weight, strict}` built by the caller.
  - Sung trials build it through the tempo map, and songs build it from vocal notes through the transport speed.
  - `weight` is 0..1, from confidence and verification. Notes with `strict = false` are graded and shown but left out of `correct` and accuracy, marked "unverified" in results.
  - Alignment is unchanged, so the NW cost model still applies.
- **Done when** a song phrase whose notes are half verified reports accuracy over the verified notes only and shows the others in grey.

### 2.8 Library scale

- **Why.** The Trained Ear is 5,177 exercises; the corpus plan groups them as one JSON per source section, giving a few hundred files. The current scanner reads and checks every file on load.
- **Change.**
  - Cache each file's parsed pieces in SQLite, keyed by path, mtime and size. Only changed files are parsed again.
  - Keep a lightweight index (id, title, collection, tags, key, time, bars, range, kind) for the UI.
  - The Library page becomes a virtualised list with search and filters on key, metre, range, tags, kind and status.
  - The 40-trial cap on starting a session stays.
- **Done when** 5,177 pieces start up in under 1 s from cache and in under 10 s from cold.

### 2.9 Validator CLI

- **Why.** Both external pipelines must prove their output loads before anyone opens the app.
- **Change.** A new `trainer-cli` binary in the workspace with these commands:
  - `trainer-cli validate library <dir|file>`: parse every piece and render scores. Report per-piece errors as JSON in the same shape as `validation.json`.
  - `trainer-cli validate song <song-dir>`: check the package contract (§3.1).
  - `trainer-cli validate course <file>`: check that every item id resolves in the library.
  - `trainer-cli render <piece-id> --svg` is optional; it could share Verovio QA with the corpus pipeline later.
- **Done when** the converter's CI or a local script can run it, and failures say which item failed and why.

---

## 3. Song practice packages (deferred)

Deferred until after the courses and notation work (§6). The pipeline is built separately and writes packages straight into the app's songs folder. Songs stay in their recorded keys; the user can raise or lower the key (§3.3), and grading is always absolute against the targets in the chosen key.

### 3.1 The package contract (what the app reads)

The app watches `%APPDATA%\SingingTrainer\songs\<song-id>\`, the same pattern as the library folder. The pipeline's §41 layout is the contract, with these additions so the app never has to guess:

| File | Required | App use | Notes |
|---|---|---|---|
| `metadata.json` | **yes** | title, artist, duration, sample rate, nominal BPM, meter, key, `package_version`, `verification_state` | add `package_version: 1` |
| `audio/source_master.wav` | yes | full-mix channel | 48 kHz, frame count = `source_frames` |
| `audio/instrumental.wav` | yes | pure-backing channel | |
| `audio/lead_vocal.wav` | yes | original-singer reference | |
| `audio/backing_vocals.wav` | optional | channel | |
| `audio/accompaniment_with_backing.wav` | optional | karaoke preset | the app can sum instrumental + backing itself if absent |
| `timing/beat_grid.json` | **yes** | tempo map, bars, metronome, loops | pipeline §10 shape; beats in source seconds |
| `timing/sections.json` | optional | section markers, region suggestions | `{start_beat, end_beat, label, confidence}` |
| `transcription/vocal_notes.json` | **yes** | targets, piano melody, notation, phrase loops | pipeline §42 `VocalNote`; **source seconds authoritative** |
| `transcription/vocal_f0.bin` | optional | singer's contour, imitation mode | layout fixed below |
| `transcription/full_notes.json` | optional | piano roll, chord display, future harmony | pipeline §12 note shape |
| `transcription/chords.json` | optional | harmonic display, harmonic reduction fallback | pipeline §34 shape |
| `transcription/phrases.json` | optional | automatic phrase loops | `{start_beat, end_beat, label}` |
| `transcription/lyrics.json` | optional, later | lyric underlay | word/syllable with source times |
| `generated/piano_reduction.mid`, `generated/harmonic_reduction.mid` | optional | piano accompaniment channels | the app renders them with its SoundFont; tick timing must follow `tempo_map.mid` |
| `edits/user_overrides.json` | written by the app | corrections and regions | §3.7 |

**Rules for the app:**

- **Source seconds are the truth.** The beat fields in note files are derived. The app recomputes beats from source seconds through the current, possibly user-corrected, beat grid, so correcting the grid never requires re-running the pipeline.
- **Stems must match.** The app checks that every stem has the master's sample rate and frame count within ±1 frame, pads or crops on load, and flags a mismatch on the song page.
- **`vocal_f0.bin` layout (version 1, little-endian):**
  - header: magic `F0V1`, `u32 hop_samples`, `u32 sample_rate`, `u64 start_frame`, `u64 count`;
  - then `count` × `f32 midi` (NaN when unvoiced);
  - then `count` × `f32 confidence`.

  A `vocal_f0.json` with the same fields as arrays is accepted as well.
- **Versioning.** An unknown `package_version` opens read-only with a warning. Missing optional files disable only the features that need them.

### 3.2 Song model (trainer-core)

- `SongAsset`: metadata, stems (role → path), `BeatGrid` (wrapping `TempoMap` from §2.2 plus bars and downbeats), sections, regions, `VocalNote`s, optional F0 contour, full notes, chords and reductions.
- `VocalNote { id, midi, onset_s, offset_s, pitch_center_cents, confidence, state, ornament?, breath_after? }`. `state` is `raw | model_consensus | review_needed | user_verified`.
- Conversions:
  - `source_s ↔ beat`, through the grid;
  - `beat ↔ tick`, at 960 PPQ;
  - `source_s → practice_s`, through the transport speed.
- `Region { id, label, start_beat, end_beat, kind: section | phrase | user | error_loop }`. Stored by beat, resolved to seconds through the grid.
- Phrase suggestion when `phrases.json` is absent: vocal rests of ≥ 400 ms, snapped to the nearest beat (pipeline §39).
- **Quantised view:** `SongAsset::score(region, transpose)` gives a `Score` from vocal notes snapped to the grid. It is used for the staff display, export to the library (§3.9) and the piano melody.
- Storage tables:
  - `songs`: id, path, title, hash, verification state;
  - `song_note_stats`: per song and vocal-note id, attempts, mean |cents|, hit rate and last seen, for "weak notes" (§3.4).

### 3.3 Practice transport (trainer-audio)

This is the core new subsystem. The design follows pipeline §20–§28 and §47, adapted to the current engine.

- **Feeder thread.** For each active stem it reads source frames from disk and runs one `signalsmith-stretch` instance per stem (pipeline Strategy B). Every stretcher gets the same speed, seek and frame counts. It writes into per-channel ring buffers about 100–150 ms ahead of output.
- **Audio callback.** It mixes the ring buffers with per-channel gain, mute and solo (§2.4) alongside the existing scheduled voices, so prompts and clicks still work. Gain and mute are therefore instant, while speed and transpose changes land within the look-ahead.
- **Source clock.** The feeder publishes anchor pairs `(output_frame, source_frame, speed)` every block, in a lock-free ring. `source_time_at(output_frame)` interpolates between anchors. This is the single clock that all of the following read:
  - the score cursor;
  - the lane;
  - live-pitch → target lookup;
  - grading;
  - the metronome.
- **Speed.**
  - `set_speed(factor)` accepts 0.5–1.5, which is the stretcher's good range.
  - `set_target_bpm(bpm)` sets `factor = bpm / nominal_bpm`, keeping the song's local tempo fluctuations (pipeline §25).
  - An optional "fixed tempo" mode is deferred.
- **Transpose.** `set_transpose(semitones)` uses the same stretcher's pitch shift on audio stems. Note targets, piano references, MIDI reductions and displayed note names add the same offset. The microphone is never shifted.
- **Seek and loop.**
  - `seek_beat(b)` / `seek_s(t)` reset every stretcher at the same source frame, with a pre-roll of the stretcher's reported latency. The click scheduler and the grading window are reset in the same step.
  - Loops are defined by beats, so the loop end is computed from the source frame, never from wall-clock time. Rollover is a seek, so nothing accumulates.
  - A count-in, if set, is generated clicks in output time before the loop start.
- **Metronome.** Clicks are scheduled at `beat_to_s(beat) / speed` through the anchors, so they follow the recording's actual beats, with accented downbeats.
- **Latency.** The existing `capture_offset_s` maps a live frame's `in_frame` to an output frame, then through the anchors to source time. Recorded timestamps keep the corrected time, and only the display may smooth.
- **Recording.** A take records the microphone plus the transport anchor log, so a stored pass can be re-graded and replayed against the stems later.
- **API** (Tauri commands mirroring pipeline §47):
  - `song_load`, `play`, `pause`;
  - `seek_beat`, `seek_s`, `set_loop`, `clear_loop`;
  - `set_speed`, `set_target_bpm`, `set_transpose`;
  - `set_channel {gain, mute, solo}`, `set_metronome`, `set_count_in`;
  - a 30 Hz `transport` event `{source_s, beat, bar, practice_s, playing, loop}`.
- **Acceptance (pipeline §56).**
  - At 50, 75, 90, 100 and 125% speed, audio, clicks, cursor and targets stay within ±10 ms.
  - A four-bar loop repeats 100 times with no drift (anchor error < 1 ms).
  - Changing transpose moves audio, targets and names together.
  - Switching references never moves the position.

### 3.4 Reference and accompaniment renderers

- **Melody reference** (pipeline §31, §48): `none | original_vocal | piano_full_notes | piano_onsets | phrase_preview | weak_notes_only`.
  - **Piano modes** render vocal notes as SoundFont events into a dedicated channel. They are scheduled per loop pass through the anchors, not pre-rendered for the whole song.
  - **`phrase_preview`** plays the region once with the reference, then the backing alone.
  - **`weak_notes_only`** plays only notes whose `song_note_stats` hit rate is below a threshold, default under 60% over the last 5 attempts.
- **Original-singer reference.** Lead-vocal channel level and balance, plus optional "duck while singing", which lowers the lead stem by a set amount while the user's voice is detected.
- **Accompaniment** (pipeline §30, §48): `instrumental | instrumental_plus_backing_vocals | full_mix | literal_piano_reduction | harmonic_piano_reduction`. The piano reductions are MIDI files rendered through the SoundFont on the tempo map; they don't stretch.
- **Mixer presets** (pipeline §30): Original, Karaoke, Pure backing, Learn melody, Pitch drill, Test. Each is a named set of channel gains and mutes.

### 3.5 Live targets and grading

- **Live target.**
  - Each 10 ms frame goes `in_frame → source_s → current VocalNote → target midi (+ transpose) → cents`.
  - Emitted to the UI as pipeline §49's update: `{practice_s, source_s, beat, expected_note, user_midi, error_cents, confidence}`.
  - The live trace tint from the sight-singing work (green ≤ 25 cents, yellow ≤ 50) reuses this.
- **Musical-accuracy grading.**
  - A pass over a region is graded by `grade_sung` with explicit expected notes (§2.7) in practice time.
  - Results are per note. Each note's id goes into `song_note_stats` for the weak-note reference and error loops.
  - Pitch mode: always `absolute`, against the targets in the song's recorded key moved by any key change the user chose.
- **Imitation grading.**
  - A new grader compares the user's contour with the singer's F0 over the region: time-aligned cents error with a small DTW window (±80 ms).
  - It reports scoop timing, glide shape and vibrato onset/rate/extent against the singer's.
  - Reported separately; never mixed into intonation accuracy (pipeline §36).
- **Unverified notes** are shown and graded but not counted, per §2.7.

### 3.6 Song practice screen (UI)

A new **Songs** tab with a list, and a song page laid out per pipeline §50:

1. **Overview strip:** waveform, section markers and loop handles, drag to loop, click to seek.
2. **Notes view**, switchable between:
   - a piano roll on the pitch lane: target note centres as bars, the singer's F0 as a thin grey line, the user's live trace tinted;
   - a staff of the quantised vocal line with the cursor (§3.2 `score()`);
   - lyrics later.
3. **Mixer strip** (§2.4) with presets.
4. **Transport bar:** play/pause, speed % or target BPM, transpose, metronome, count-in, loop, reference mode, accompaniment mode.
5. **After a pass:** a per-note result row (the same marks as sight singing), "loop my misses", and "practise this phrase in a session".

### 3.7 Editing and verification in the app

The pipeline wants human-corrected data to be authoritative. The app owns these edits and writes them to `edits/user_overrides.json`. It never edits the pipeline's files.

- **Beat-grid fixes** (pipeline §10):
  - first downbeat;
  - half or double tempo;
  - beats per bar;
  - nudging a single beat;
  - tempo-section boundaries.

  Stored as operations and re-applied on load.
- **Vocal-note editor** (pipeline §19), on the piano-roll view with the F0 line:
  - change pitch;
  - split, merge;
  - move onset or offset;
  - mark breath or rest;
  - mark ornament;
  - mark uncertain;
  - mark verified;
  - "verify phrase" in one click (pipeline §51).
- **Regions:** create, rename and delete. Section suggestions are copied into regions on demand.
- **Export:** corrected `vocal_melody.mid` and MusicXML (pipeline V2 §54) can be written from the corrected model. Neither exists yet (midly can write MIDI; MusicXML export would be new), so this can wait.

The Python pipeline can read `user_overrides.json` too, for example to re-transcribe with a corrected grid. That is its choice.

### 3.8 Sessions and the coach

- **New exercise kind `song_phrase`:**

  ```json
  {
    "type": "song_phrase",
    "params": {
      "song": "id",
      "region": "id | {start_beat, end_beat}",
      "accompaniment": "...",
      "reference": "...",
      "speed": 0.75,
      "transpose": 0,
      "count_in_bars": 1,
      "grading": "accuracy | imitation",
      "reps": 4,
      "ladder": "lead_then_fade"
    }
  }
  ```

  - Each rep is a trial. It runs on the transport rather than on pre-rendered audio, so the runner gets a `run_song_phrase` path that drives the transport and collects frames.
  - The trial is stored with `song_id` and `region_id`.
- **Repetition ladder** (pipeline §32): `lead_then_fade` gives:
  1. lead vocal + backing;
  2. quiet lead + backing;
  3. piano melody + backing;
  4. backing only.

  Reps with a reference are stored as `assisted` and excluded from accuracy, as practice mode does today.
- **Tempo ladder:** an optional `speed_ladder: [0.75, 0.9, 1.0]`, advancing when a rep passes the block's pass mark.
- **Error loops** (pipeline V2): after a pass, regions around missed notes (the note ± 1 beat, snapped to bar) become `error_loop` regions that the coach or user can queue.
- **MCP tools:**
  - read: `list_songs`, `get_song` (sections, regions, phrases, verification share, nominal BPM, key, range of the vocal line), `song_note_stats(song, region?)`;
  - write: `create_region`, and `song_phrase` blocks through the existing `queue_session`.
  - The coach can then say "loop the bridge at 80% with the piano melody, then without."

### 3.9 Song → exercises (bridges into existing kinds)

- **To library:** "Save phrase to library" writes the quantised vocal line of a region as a `LibraryPiece`, tagged `source: song`. The phrase can then be used in:
  - sight singing;
  - melodic dictation;
  - pattern-style drilling with any transposition.
- **Dictation from the recording:** a `melodic_dictation` variant whose prompt is the lead-vocal stem over the region rather than a synth, checked against the corrected vocal notes. It needs a `prompt_audio` source on `DictationTrial`.
- **Harmony (later):**
  - backing-vocal stems and `full_notes` can supply `given` harmony parts;
  - chords can drive `chords` backing for parallel harmony over the song's melody.
  - It needs a reliable backing-vocal transcription, so it waits for V2 data.

---

## 4. Exercise corpus outputs

### 4.1 What the corpus build hands the app

Per the corpus plan §3 and §6:

- **Library JSON:** one file per source section, generated and never hand-edited. It is copied into `library/<collection path>/`. The build includes only items it has approved, so the app doesn't track review status.
- **Course manifests:** in `courses/<course-id>.json`.
- **Per-piece teaching data** (instructions, vowel, solfège, cautions): embedded in the library JSON. Provenance, extraction and rights records stay in the corpus folder; the app doesn't read them.
- **Optional assets, later:** `assets/<item>/<stem>.flac` plus `audio.json`. Generated MIDI backing comes first (§2.4).

The app does not read MusicXML, `exercise.json`, `validation.json` or source crops.

### 4.2 Notation breadth

| Need | Change | Done when |
|---|---|---|
| **Rhythm / unpitched** (42 Smey, 26 Crowell, 268 Aural R) | `CompactNote::Hit { beats }` (compact `x:1`); `ScoreNote.kind = pitched \| hit \| rest`; `Clef::Percussion`; the staff draws a one-line percussion staff with x or normal heads; piece kind `rhythm` | Smey 42/42 load, render and play as clicks or woodblock on the right onsets |
| **Tuplets, 32nds** | §2.1, plus VexFlow `Tuplet` and beaming by beat group | the 26 tuplet and 11 32nd Crowell pieces render and play correctly |
| **Alto and tenor clefs** | `Clef::Alto`, `Clef::Tenor` (and `ClefChoice`); `lineFor` uses the C-clef line; VexFlow `alto`/`tenor`; range auto-fit may still choose to re-clef for display | the 16 C-clef Crowell pieces render at correct pitch |
| **Multi-part systems** | Staff renders N parts as one system (connected barlines, bracket or brace, aligned bars); the singer's part highlighted; `play_parts` mute/solo per part through §2.4; parts beyond 4 allowed for display | a four-part chorale shows as one system; any part can be sung while chosen others play |
| **Metre and key changes** | §2.3 | the 3 Crowell metre-change pieces |
| **Grace notes** | `grace: true` on a note: rendered small, played as a short note stolen from the next note, not graded (excluded from alignment) | 8 Crowell grace pieces render; grading ignores the graces |
| **Fermata** | `fermata: true`: rendered, with a hold through the tempo map (§2.2) | the 3 fermata Crowell pieces play with holds and grade |
| **Repeats** | the converter expands them, so the app sees plain expanded music; drawing repeat barlines is optional later | expanded pieces play in the printed order |
| **Lyrics / solfège underlay** | `lyric` per note (syllable, hyphen and extender flags); rendered under the staff; also used later for song lyrics | a vocalise with vowels and a solfège melody display correctly |

### 4.3 Library piece metadata

These optional fields are added to `LibraryPiece`. Generators ignore them; the library filters, the pre-exercise screen and courses use them.

```json
{
  "kind": "sight_singing | rhythm | vocalise | chorale | duet | canon",
  "pedagogy": {
    "instructions": "...",
    "solfege": null,
    "vowel": null,
    "cautions": []
  }
}
```

- `pedagogy.instructions`, `vowel` and `cautions` show before the first trial of a piece, using the existing block-intro screen.
- `kind` filters the library and chooses the default exercise when "Start" is pressed.
- Ordering, the source citation and dedup links stay in the corpus build and its course manifests.

### 4.4 Exercise kinds

| Kind | Status | What the app needs |
|---|---|---|
| `vocalise` | new, built on `pattern`; **first priority** | `pattern` plus warm-up fields: vowel, register, reps, tempo ladder (advance when a rep passes), transposition plan stepping through the singer's range, cautions shown before starting. Backing from generated tracks (§2.4): click, pulse, chord pads, bass, guide melody. Graded on pitch and onset as `pattern` is; never on "vocal health". |
| guided singing | exists in part | Sight singing already has sing-along and practice (assisted) modes. Formalise the corpus plan's assessment policy as fields on `SightSingParams`: `preview_melody` (play once first), `reference_pitch`, `guide` (`off`, `on`, `fade_after_n`), `answer_visible`, `max_plays`. |
| `rhythm_reading` | new | Show a rhythm score with click and count-in; the user **taps** it (space bar or an on-screen pad). Tap times are mapped to the master clock, with a one-off tap latency calibration in Settings. Grading: onset-only alignment (the existing alignment with the pitch cost removed), per-note early or late ms, tolerance scaled by tempo. Clapping or speaking through the microphone is not planned. |
| `melodic_dictation` | extend, later | `prompt_audio` (a course stem or song segment) instead of synth playback; answer checked against the score. |

**Not in the app:** breathing, SOVT and bodywork routines. The coach agent handles them directly with the user.

**Deferred indefinitely:** `sing_and_play` (sing one part while playing another). It needs MIDI keyboard input, which is wanted eventually: `midir`, a device setting, timestamped events and its own latency calibration. When MIDI input arrives it also serves tapping in `rhythm_reading` and note entry in `pitch_id` and dictation.

### 4.5 Courses

- **Step 1: collection pages, no new schema.**
  - A **Courses** tab lists library collections as cards (count, level from tags, progress).
  - A collection page shows the units as sub-collections, the item rows (number, title, bars, key, metre, tags, status) and filters, with preview and start.
  - Starting uses the existing library selection.
- **Step 2: course manifests.**
  - Load `courses/*.json` (corpus §6.2): units → items, each item referencing a library id with overrides (`kind`, `start`, `tempo_bpm`, `instructions`, `tags`), plus optional prerequisites and pass criteria.
  - A course item turns into a block, so `kind` + overrides become an `Exercise` with `{"library": {"ids": [id]}}`.
  - The `trainer-cli validate course` command checks that every item id resolves.
- **Progress.**
  - Per item: unseen, attempted, or passed (block pass mark met, not one lucky attempt). It is derived from stored trials (§2.6).
  - Course page actions: Start next 10, Start unit, Resume.
- **Practice page.**
  - It stays "Quick practice" with the presets.
  - The `COURSES` regex in `PracticeView.tsx` is removed once the shipped preset courses become course manifests too. The AP stages and harmony intervals can be expressed as items that point at presets.
  - "Up next" (the coach queue) stays on Practice.

### 4.6 Coach and MCP

- **Read:**
  - `list_courses`, `get_course`;
  - `course_progress(course_id)`, giving per-item status and the next unseen items;
  - `search_library(filters)`, using the §2.8 index, because 5,000 items cannot be dumped into context.
- **Write, once the schema is stable:** `save_course`, `add_course_items`.
- **Session specs accept** `{"course": {"id", "unit"?, "next": 10}}` as a library selection, so the coach can say "next 10 items in this unit" without restating specs.

---

- **Warm-ups:** the coach can queue `vocalise` blocks and course units. Breathing and bodywork stay in its conversation with the user.

---

## 5. How the inputs share the work

| Foundation | Courses and warm-ups use it for | Corpus notation uses it for | Songs (deferred) use it for |
|---|---|---|---|
| Mixer channels | warm-up backing tracks | chorale part mute/solo | stems, piano references |
| 960 PPQ | — | tuplets, 32nds | musical time, quantised vocal score |
| Tempo map | tempo ladders | fermatas, tempo changes | the beat grid |
| Metre/key changes | — | Crowell metre changes, books | transcribed metre changes |
| Library index | course and collection pages | 5,000+ pieces | — |
| Validator CLI | course manifests | converter round-trip | package check |
| Lyrics underlay | vowels, solfège | text underlay | lyrics |
| Explicit expected notes | — | — | song passes, unverified notes |

---

## 6. Phases

Sizes: S is a few days of work, M one to two weeks, L more.

### Phase A: courses and warm-ups (M)

- Library cache and index, plus a virtualised, filterable Library page (§2.8).
- `kind` and teaching fields on `LibraryPiece`, shown before an exercise (§4.3).
- Courses tab: collection pages first, then course manifests and per-item progress (§4.5, §2.6).
- Practice page migration: the shipped preset courses become manifests, and the `COURSES` regex goes.
- Mixer channels and generated multi-track backing with roles (§2.4).
- The `vocalise` kind with tempo ladder and transposition plan, and the guided-singing fields (§4.4).
- Course and library-search MCP tools (§4.6).
- `trainer-cli validate library|course` (§2.9).

**Exit:**

- the 315 convertible Crowell pieces load through the validator and appear as a collection page;
- a curated stepwise-melody course runs with progress;
- a warm-up course of vocalises runs with generated backing and steps through the singer's range;
- the coach can read course progress and queue the next items.

### Phase B: shared notation (M–L)

- 960 PPQ with rational durations and tuplets (§2.1), with the old/new regression test.
- Tempo map with fermatas (§2.2).
- Metre and key changes (§2.3).
- Rhythm and unpitched notation, the percussion staff, and `rhythm_reading` by tapping (§4.2, §4.4).
- Alto, tenor and percussion clefs.
- Multi-part systems: one aligned system, with part mute and solo.
- Grace notes.
- Lyrics and solfège underlay.

**Exit:**

- all 427 Crowell pieces (minus the 19 for source review) and 42/42 Smey load, render and play;
- rhythm blocks grade tap onsets;
- a four-part chorale can be sung from any part.

### Phase C: the rest of the courses (M)

- Rhythm courses (Smey, the Aural Training R sections once converted).
- Chorale, duet and canon units using the multi-part systems.
- The remaining Trained Ear books as collections and curated cross-source courses, as the corpus build produces them.
- Course write tools for the coach (`save_course`, `add_course_items`), once the manifest format has settled.
- Startup and search performance checked at full corpus scale (§2.8).

**Exit:** every approved item the build produces loads, and each book has a collection page; curated rhythm and chorale courses run with progress.

### Deferred: songs

In this order when they are picked up:

1. **Song playback core:**
   - package loader and `trainer-cli validate song` (§3.1);
   - song model (§3.2);
   - decode and resample (§2.5);
   - transport at 100% speed with streaming, seek, bar loops, count-in, beat-grid metronome and one source clock (§3.3);
   - live targets and accuracy grading (§3.5, §2.7);
   - Songs tab (§3.6).
2. **Practice tools:**
   - speed (`signalsmith-stretch`) and raising or lowering the key;
   - reference and accompaniment modes, and mixer presets (§3.4);
   - the `song_phrase` exercise with the repetition and tempo ladders (§3.8);
   - song MCP tools;
   - "Save phrase to library" (§3.9).
3. **Later:**
   - beat-grid and vocal-note correction (§3.7);
   - imitation grading;
   - weak-note references and error loops;
   - recorded stems for course items;
   - dictation from recorded prompts;
   - song lyrics.

Each step's acceptance is in §3.3 and §8.

### Deferred indefinitely

- MIDI keyboard input and `sing_and_play` (§4.4).

---

## 7. Decisions (resolved 24 October 2026)

1. **Order:** courses and warm-ups, then shared notation, then the rest of the courses. Songs deferred.
2. **Rhythm response:** tapping only.
3. **Stretch backend:** `signalsmith-stretch`.
4. **Rights:** personal use; the app manages no identity or rights.
5. **Song packages:** the separately built pipeline writes them straight into the app's songs folder.
6. **Song keys:** songs stay in their recorded keys, graded absolutely; the user can raise or lower the key.
7. **Breathing and bodywork:** handled by the coach, not the app.
8. **MIDI input:** wanted eventually; `sing_and_play` deferred indefinitely.

---

## 8. Acceptance tests to automate

- **Grid:** old vs new note onsets in seconds are identical for every preset and library piece; tuplet/32nd fixtures round-trip.
- **Tempo map:** `beat_to_s ∘ s_to_beat` is identity within 0.1 ms over a 4-minute grid; a fermata extends the timeline by the hold.
- **Transport** (offline render with a fake device):
  - 100 loop iterations end within 1 ms of the expected source frame;
  - click onsets match `beat_to_s / speed` within 1 ms at 0.5, 0.75, 1.0 and 1.25;
  - stem phase coherence: two stems from the same source cancel at 100% speed (instrumental + vocals − master ≈ 0 for a synthetic package).
- **Live target:** a synthetic package with a known vocal line, sung by the test tone generator at known offsets, reports the right `expected_note` and cents at every 10 ms.
- **Grading:** unverified notes are excluded from accuracy; imitation and accuracy scores stay separate.
- **Validator:** Smey, Crowell fixtures and a synthetic song package pass; malformed fixtures fail with the right reason.
- **Courses:** a manifest whose items include an unknown id fails validation with the item named; per-item progress matches a fixture set of stored trials.
- **Vocalise:** a tempo ladder advances only after a passing rep; the transposition plan never leaves the singer's comfortable range.
- **Tapping:** synthetic tap streams at known offsets report the right early/late ms after the tap calibration is applied.
