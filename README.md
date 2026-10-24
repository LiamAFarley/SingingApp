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
- **Ear training:** intervals (melodic and harmonic), chords and inversions, scales and modes.
- **Singing:**
  - Pitch matching, sustained notes, interval singing, scales and warm-up patterns.
  - A follow-along pitch lane that scrolls with the metronome.
  - Backing options: click, drone, chords or the melody.
  - Listen, sing along and retry after each attempt.
- **Sight singing:** melodies read from staff notation.
  - The key and first note are played, a bar is counted in, then you sing.
  - Afterwards your pitch trace is drawn on the staff, with a mark under each note.
  - Melodies are generated, or the coach writes them.
- **Melodic dictation:** hear a melody and write it in the score editor, then check it note by note.
  - 2–6 set the note length, `.` adds a dot, `0` enters a rest.
  - A–G enter notes in the key, the arrow keys move a note, and you can also click on the staff.
- **Tuner:** live note, cents and pitch history, with a reference note or drone.

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

Data lives in `%APPDATA%\SingingTrainer`, which holds `trainer.sqlite`, `recordings\` and `soundfonts\`. Set `SINGING_TRAINER_DATA_DIR` to move it.

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
- `sight` or `sight-feedback`
- `dictation` or `dictation-feedback`
- `summary`
- `notation`, a gallery of generated melodies

TypeScript types in `ui/src/bindings` are generated from the Rust types by `cargo test`. CI fails if they are out of date.

| Path | Contents |
| --- | --- |
| `crates/trainer-core` | Theory, session schema, generators, pitch tracking, grading, storage, stats, presets, reports |
| `crates/trainer-audio` | Real-time engine, cpal/ASIO device layer, SoundFont and synth rendering, calibration |
| `crates/trainer-mcp` | MCP server (22 tools) |
| `crates/trainer-app` | Tauri app: session runner, live pitch forwarding, commands |
| `ui` | React UI |
