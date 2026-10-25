# Singing Trainer

A Windows desktop trainer for absolute pitch, ear training, singing and sight singing. It works with an ASIO interface and headphones. An LLM coach reads your results and queues sessions over MCP. See `adaptive_music_training_app_plan.md` for the full design.

- **Engine:** Rust (cpal with ASIO, MPM/pYIN pitch tracking, grading, SQLite).
- **Desktop shell:** Tauri 2.
- **UI:** React.
- **MCP server:** built into the same executable, started with `--mcp`.

## Exercises

- **Absolute pitch:**
  - **Today's set** (Absolute pitch page): one daily session composed from the active courses, each at its current level and fitted to its minutes, with any test that is due first.
  - Courses:
    - **Note naming**: ten levels from three notes to all twelve. Notes are spread over four octaves (C3–B6), each two octaves from the one before, with Other for notes outside the set. Each level ends with a check without feedback; the last level needs four checks at 90%, the fourth at least 12 h after the third.
    - **Fixed C anchor** (Brady): all twelve notes from the start, C on 40% of tones fading to an even share, tones arriving at a steady pace from every 5 s to every 1.4 s. Each session starts by singing C from memory.
    - Tests, first in the day when due, 5 s per note, no replay: the **Benchmark** (12 notes sung from memory, then 60 piano notes; every two weeks), the **Weekly test** (120 notes on five instruments), the **Transfer test** (untrained instruments and Shepard tones; every four weeks) and the **UCSF-style test** (every four weeks). **Maintenance** runs twice a week once naming is complete. `docs/AP_TESTING.md` explains each test and how to read it.
  - A course moves to its next level when the level's checks pass. The page shows what each rule still needs and when the next check counts. Levels can also be set by hand.
  - Progress per note pools every method: a note is mastered at 90% named within 2 s. Also shown: each test's results (exact with its 95% range, within 2 s, UCSF-style points, mean error in semitones, Shepard, sung notes, per instrument, cautions) and how far your remembered C is from a real C.
  - Before a check without feedback, a 20-second endlessly falling Shepard–Risset tone clears pitch memory. It plays automatically when you heard a named note in the last five minutes.
  - After answering any identification question, click an answer to hear it: the other notes, intervals, chords, scales, keys or degrees, in the same register and sound.
  - Singing a named note. After a miss, a tuner aimed at that note opens so you can practise locking on (hold it within 15 cents for a second), with an optional drone.
  - Passive pairing and key finding.
- **Relative pitch** (its own page and Today's set): workshops in the pattern of EarMaster's general workshops and Functional Ear Trainer's levels.
  - Activities: scale degrees, interval comparison, interval identification, intervals in a key, interval singing, chord identification, chord inversions, scale identification, melodic sight singing and melodic dictation.
  - Each activity opens a page of modules and lessons with scores; one run at 90% passes a lesson. A Custom levels tab saves your own settings for the activity, with their scores.
  - See `docs/RELATIVE_PITCH.md`.
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
  - Free part-writing: any voice of a two-voice or SATB piece, sung exactly as written with any intervals, tritones included. The coach can write these into a session or add them to the library. You choose which other voices play with you, e.g. only the bass first, then everyone.
  - Practice mode plays your own part along and shows its notes. Practice attempts are not counted in accuracy.
  - The **Harmony intervals** course has one interval per level: three trials hearing your part, then six by ear.
  - Optional just-intonation grading against the other voice.
- **Sight singing:** melodies read from staff notation.
  - The key and first note are played, a bar is counted in, then you sing.
  - Your voice is drawn on the staff as you sing, from the first attempt: green within 25 cents of the written note, yellow within 50, red beyond. Afterwards each note gets a mark under it.
  - Any octave counts: a melody sung an octave below (or above) the written pitches is graded on its notes, drawn in the written octave, and the result says which octave you sang in. This applies to every sung exercise except harmony, where the octave changes the interval.
  - Melodies are generated, written by the coach, or taken from the melody library.
- **Melodic dictation:** hear a melody and write it in the score editor, then check it note by note.
  - 2–6 set the note length, `.` adds a dot, `0` enters a rest.
  - A–G enter notes in the key, the arrow keys move a note, and you can also click on the staff.
- **Tuner:** live note, cents and pitch history, with a reference note or drone.

Your range is set by hand in **Settings**. Rhythm is trained inside sight singing, where every note is graded for timing.

## Melody library

Every `.json`, `.mid` and `.midi` file in the app's `data\library` folder is read into the **Library** page, and a subfolder is a collection. Changes to the folder are picked up within two seconds.

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

Data lives in the app's `data` folder (`SingingApp\data` when you run the build under `target`), which holds `trainer.sqlite`, `recordings\`, `soundfonts\`, `library\` and `speech\`. Set `SINGING_TRAINER_DATA_DIR` to move it.

## Sounds

You can add extra `.sf2` banks to the app's `data\soundfonts` folder and restart the app. Exercises refer to them as `sf:<file name without .sf2>:<program>`; only presets on bank 0 are reachable. General MIDI programs are `gm:<0-127>`. The built-in sounds are `synth:<wave>` and `shepard`. SFZ and Kontakt libraries are not supported.

A curated set of 16 extra banks (piano, saxophone, guitar, harp, organ, violin, viola, cello, flute, horn, trombone, solo male/female voices and choirs) is installed by `scripts/fetch-extra-soundfonts.ps1`. See `docs/SOUNDFONTS.md` for the timbre IDs, measured pitch ranges, licences and the candidates that were rejected.

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
