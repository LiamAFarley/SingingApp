# Singing Trainer

A Windows desktop trainer for absolute pitch, ear training, singing and sight singing. It works with an ASIO interface and headphones. An LLM coach reads your results and queues sessions over MCP. See `adaptive_music_training_app_plan.md` for the full design.

- **Engine:** Rust (cpal with ASIO, MPM/pYIN pitch tracking, grading, SQLite).
- **Desktop shell:** Tauri 2.
- **UI:** React.
- **MCP server:** built into the same executable, started with `--mcp`.

## Exercises

- **Absolute pitch:**
  - Note naming in progressive stages, with out-of-set distractors, several octaves and timbres, and a deadline.
  - Singing a named note.
  - Passive pairing and key finding.
  - A holdout assessment battery.
- **Ear training:**
  - Intervals, melodic and harmonic, either without a key or inside a key after a cadence.
  - Interval comparison: which of two intervals is larger.
  - Scale degrees in the style of Functional Ear Trainer. A cadence sets the key, then you name a note's degree, and the note walks to the tonic (nearest, up or down). Eight graded levels take you from three degrees in C to all twelve in changing keys.
  - Chords and inversions, scales and modes.
- **Singing:**
  - Pitch matching, sustained notes, interval singing, scales and warm-up patterns.
  - A follow-along pitch lane that scrolls with the metronome.
  - Backing options: click, drone, chords or the melody.
  - Listen, sing along and retry after each attempt.
- **Harmony singing:** sing one voice while the app plays another.
  - A third, fourth, fifth or sixth below or above a melody, by ear or from the staff, or the melody over that voice.
    - The melody can be generated, written by the coach, or taken from the library.
    - Where the interval would be a tritone (F against B), the harmony note becomes a perfect fourth instead.
  - Any part of a two-part (or larger) library piece.
  - Practice mode plays your own part along and shows its notes. Practice attempts are not counted in accuracy.
  - The **Harmony intervals** course has one interval per level: three trials hearing your part, then six by ear.
  - Optional just-intonation grading against the other voice.
- **Sight singing:** melodies read from staff notation.
  - The key and first note are played, a bar is counted in, then you sing.
  - Afterwards your pitch trace is drawn on the staff, with a mark under each note.
  - Melodies are generated, written by the coach, or taken from the melody library.
- **Melodic dictation:** hear a melody and write it in the score editor, then check it note by note.
  - 2–6 set the note length, `.` adds a dot, `0` enters a rest.
  - A–G enter notes in the key, the arrow keys move a note, and you can also click on the staff.
- **Tuner:** live note, cents and pitch history, with a reference note or drone.

Your range is set by hand in **Settings**. Rhythm is trained inside sight singing, where every note is graded for timing.

## Melody library

Every `.json`, `.mid` and `.midi` file in `%APPDATA%\SingingTrainer\library` is read into the **Library** page, and a subfolder is a collection. Changes to the folder are picked up within two seconds.

From the Library page you can:

- Preview and play pieces.
- Sight-sing a piece or a collection, take it as dictation, or sing one part of a multi-part piece.
- Import files with **Import files…**, which copies them into the folder.

The coach can use the same pieces in sessions, and can add pieces over MCP: for example, melodies it transcribes from your books.

A melody in JSON:

```json
{
  "title": "Exercise 12",
  "key": "D major",
  "time": "3/4",
  "tempo_bpm": 72,
  "notes": "A4:1 | D5:2 C#5:1 | B4:1 A4 G4 | F#4:3"
}
```

- `pitch:beats` is counted in the time signature's unit, and a length carries over to the next note.
- `R` is a rest and `~` ties to the next note.
- Every `|` is a barline that is checked. A short first bar is a pickup.
- A file can hold a list of pieces, or `{"collection": "…", "pieces": […]}`.
- Several voices go in `"parts": [{"name": "Upper", "notes": "…"}, …]`, top part first.

For MIDI files:

- Each track is a part, and chords keep their top note.
- Times are rounded to sixteenths.
- The key comes from the key signature, or is estimated when there is none.

## Install

Each push builds a Windows installer in CI. Open the latest run of the **CI** workflow, download the `singing-trainer-windows` artifact, and run the NSIS `.exe`. It installs per user, with no admin rights needed. The installer is unsigned, so SmartScreen will ask for confirmation the first time.

The default General MIDI SoundFont (GeneralUser GS) is bundled.

## First run

1. **Audio:** pick the ASIO driver (Focusrite USB ASIO), the microphone input and the headphone output pair, then press **Apply**.
   - Use the interface's direct monitoring for your own voice. The app never plays the microphone back.
   - Set the buffer size in Focusrite Control.
2. **Audio → Latency calibration:** run loopback (a cable from an output to a line input) or tap-along once per device, sample rate and buffer size. Timing grades use the measured round trip.
3. **Settings:** set your comfortable range. Sung exercises stay inside it.
4. **Practice:** start a preset, or a session your coach queued under **Up next**.

## Connect a coach (MCP)

The app executable is also a stdio MCP server:

```
"<install dir>\singing-trainer.exe" --mcp [--data-dir <folder>]
```

It reads and writes the same database as the app, whether or not the window is open. **Settings → Coach** shows the exact command with ready-to-copy snippets:

- **OpenCode:** `opencode.json`, under `mcp` with `"type": "local"`.
- **Hermes Agent:** `config.yaml`, under `mcp_servers`.

Data lives in `%APPDATA%\SingingTrainer`, which holds `trainer.sqlite`, `recordings\`, `soundfonts\` and `library\`. Set `SINGING_TRAINER_DATA_DIR` to move it.

## Sounds

You can add extra `.sf2` banks to `%APPDATA%\SingingTrainer\soundfonts` and restart the app. Exercises refer to them as `sf:<file name without .sf2>:<program>`. General MIDI programs are `gm:<0-127>`. The built-in sounds are `synth:<wave>` and `shepard`. SFZ and Kontakt libraries are not supported.

## Development

You need:

- Rust (stable).
- Node 22.
- On Windows, LLVM with `LIBCLANG_PATH` pointing at its `bin` folder. The ASIO SDK is downloaded automatically during the build.

```
npm ci
scripts/fetch-soundfont.ps1    # or scripts/fetch-soundfont.sh
npx tauri dev                  # run the app
npx tauri build                # installers in target/release/bundle
cargo test --workspace
```

To work on the UI in a browser without the desktop shell, run `npm run dev` and open `http://localhost:1420/`. A mock backend stands in for the app. Add `?scenario=` with one of these values to jump to a specific screen:

- `pitch`
- `sung` or `sung-feedback`
- `harmony`, `harmony-practice` or `harmony-feedback`
- `sight` or `sight-feedback`
- `dictation` or `dictation-feedback`
- `degrees` or `degrees-feedback`
- `colors`
- `summary`
- `notation`, a gallery of generated melodies

TypeScript types in `ui/src/bindings` are generated from the Rust types by `cargo test`. CI fails if they are out of date.

| Path | Contents |
| --- | --- |
| `crates/trainer-core` | Theory, session schema, generators, pitch tracking, grading, storage, stats, presets, reports |
| `crates/trainer-audio` | Real-time engine, cpal/ASIO device layer, SoundFont and synth rendering, calibration |
| `crates/trainer-mcp` | MCP server (27 tools) |
| `crates/trainer-app` | Tauri app: session runner, live pitch forwarding, commands |
| `ui` | React UI |
