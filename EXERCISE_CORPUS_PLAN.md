# Exercise Corpus, Courses and Conversion — Build Plan

**Date:** 24 October 2026  
**Scope:** Turning the open sources and the target books into a progressive, course-based exercise library for `SingingApp`; auditing the current exercise format; defining the schema expansions, extraction toolchain, and phased execution.  
**Related documents:** `Modern_Singing_SightSinging_Acquisition_and_Digitization_Guide.md` (bibliographic catalog — not duplicated here), `SingingApp/adaptive_music_training_app_plan.md` (app architecture), `corpus/manifest.csv` (tracker).  
**Status:** Plan plus verification probe. No source has been declared converted or approved yet.

---

## 0. Bottom line

1. **Two web sources are immediately usable and large.**
   - **The Trained Ear**: 14 historical source books (mostly public domain), **5,177 exercises**, with MusicXML 3.0, MIDI, MP3, PNG and SVG downloads, licensed **CC BY-SA 4.0**.
   - **Eyes and Ears** (Benjamin Crowell): **455 LilyPond files**; The Trained Ear provides 427 Crowell MusicXML files that overlap heavily (not yet reconciled 1:1). CC BY-SA.
2. **The local Aural Training supplement is real but not born-symbolic.** It has **147 pages, 505 numbered exercises (268 rhythm, 237 melody), and 605 image objects (mostly notation systems, plus cover/logo)**. The notation is raster at roughly 150–250 ppi; the labels are vector text. It can be segmented automatically, but needs OMR plus review. It is **CC BY-NC-SA 4.0**, so personal/non-commercial use only without permission.
3. **The Krueger EPUB on disk cannot supply notation.** It is a `pdftohtml` conversion: notation survives only as unrelated glyph characters (`o. o. o.`, `?.`) with no staff lines and no embedded music font. It is useful for prose and chapter structure only. A real PDF or scan is required. The single 4e worksheet PDF is vector and clean.
4. **The app's current runtime cannot ingest all of this yet.** A throwaway converter run against the 427 Crowell MusicXML files found:
   - **318 / 427** convert structurally under the current pitched, single-key, single-metre, 16th-note-grid model; **315** of those are clean for a first import once repeats are expanded, and **3** need a fermata/timing decision;
   - **90** more are blocked only by known schema gaps (rhythm/unpitched, tuplets, C clefs, multi-part, metre changes, grace notes, 32nd notes);
   - **19** need source review (irregular bar lengths in the source).
   - **0 / 42** Smey rhythm exercises convert, because every one is unpitched.
5. **The first real engineering work is therefore schema, not scraping.** The minimum set is rhythm/unpitched notation, a finer tick grid, C clefs, multi-part handling, and a course/collection layer. After that the bulk ingest is mostly deterministic conversion and review.
6. **Recommended order:** Trained Ear (pitched subset first, then all), Eyes and Ears reconciliation, Aural Training PDF pilot and review, Krueger, then purchased books. Build collection pages immediately; build curated course manifests on top of the same library.

---

## 1. Verified source audit

### 1.1 The Trained Ear

**What is there**

| Item | Finding |
|---|---|
| Sources | 14 books, 1884–2005 |
| Exercise count | **5,177** (sum of source counts on the downloads page) |
| Formats | MusicXML 3.0 (Sibelius export), MIDI, MP3, PNG, SVG, PDF, Sibelius |
| Hierarchy | Book → part/book number → step/section → exercise; IDs are stable, e.g. `Cole-Lewis-1904_1-01-001`, `Batchellor-Charmbury-1884b_2-1-036-3` |
| Metadata on site | Source, pickup, bar count, and computed filters: voice count, exercise type, metre, tuplets, ties, key mode, largest interval, chromaticism, arpeggiated triads/sevenths, starting/ending scale degree |
| XML titles | **Empty** in the Crowell files (`work-title` blank, no credits); titles must come from the site hierarchy or the source book |
| Audio | MP3 per exercise; MIDI is preferable for our own playback and avoids shipping large audio |
| Licence | **CC BY-SA 4.0** for the site's editions/audio; underlying books are public domain, but some melodies descend from Mutopia/CPDL editions with their own attribution/share-alike terms |

**Crowell audit (427 files, representative of the melodic corpus)**

- 420 single-part, 7 two-part.
- Median 10 bars; range 2–59 bars; 5,229 bars total.
- 26 unpitched/percussion-clef rhythm exercises.
- 26 with tuplets; 16 in alto/tenor (C) clef; 3 with metre changes; 2 with backup voices.
- 50 with repeats; 90 with ties; 8 with grace notes; 15 with articulations; 6 with fermatas; 1 with dynamics.
- 49 32nd notes in 11 files (sub-16th grid).
- 19 files have a short *middle* bar (e.g. 3 beats then 1 beat in 4/4), which is a source irregularity to review, not silently repair.
- 138 files end with a short final bar, which is normal and supported.

**Smey audit (42 files)**

- **42 / 42 are unpitched rhythm exercises** on a percussion clef.
- No tuplets or key changes; one tie.

This is enough to say the Trained Ear corpus is a strong first target, but the app needs rhythm support before it can use the rhythm third of the material.

### 1.2 Eyes and Ears

- The source archive contains **455 `.ly` files**; The Trained Ear has **427 Crowell XML files**, so the two overlap heavily but not completely.
- The book source uses a deliberately restricted LilyPond subset, with a `list` file giving chapter/section order and flags (`r` rhythm-only, `h` no header, `d` didactic, `x_` ignored by the index).
- LilyPond 2.24 has **no supported MusicXML export** (only `musicxml2ly` import; export is a rudimentary Frescobaldi feature). Do not plan a LilyPond→MusicXML conversion as the main path.
- **Use The Trained Ear's MusicXML as canonical for the 427 overlapping melodies**, then reconcile the residual `.ly` files against `list` and either parse the restricted subset or render/OMR the remainder. Never transcribe by hand first.

### 1.3 Aural Training and Sight Singing Supplement

This is **not** Merritt/Castro's *Comprehensive Aural Skills*. It is the UNI open textbook supplement by Harlow, Peyton, Schwabe and Swilley (2022).

| Item | Finding |
|---|---|
| Pages | 147 |
| Numbered exercises | **505**: 268 `R` (rhythm) + 237 `M` (melody) |
| Notation | 605 image objects, mostly one or more per exercise (plus cover/logo); effective resolution mostly **150–250 ppi** (253 at 150–199, 335 at 200–249, 16 at 300+) |
| Text layer | Real vector text: headings, labels (`R 1.9`, `M 3.4`), instructions |
| Layout | Each label sits above its system; on page 8, for example, label bottom edges meet the next image top (98→113, 340→355, 505→520, 596→611), so a label→image crop map is direct with PyMuPDF |
| Licence | **CC BY-NC-SA 4.0** |
| Printed/PDF offset | Printed page 5 = PDF page 7 (2 pages of front matter); verify before committing a page map |
| Content | Rhythm sections 1–18; melody sections 1–17; canons, duets and four-part chorales; deliberately no dynamics/articulations |

The notation images are line art and look clean at 130 dpi, but 150–250 ppi is below the 300–400 dpi normally recommended for OMR. **Pilot before batch.**

### 1.4 Krueger

| File | Finding |
|---|---|
| `Progressive Sight Singing … 3rd, 2016 … .epub` | `pdftohtml 0.36` conversion, 17 HTML splits, 64 PNGs that are not page scans (the one inspected is a decorative section photo). Notation is **glyph text**, e.g. `"o. o. o. o o o ?."`, with no staff lines and no embedded font. Unusable for notation extraction. |
| `krueger4e_intervalandchordworksheets_01.pdf` | 1 page, Finale 2004b, vector, tagged. Clean and usable, but only one worksheet. |
| Needed | A print PDF or a clean scan of the 3rd/4th edition. The 4th edition is the guide's recommendation; the EPUB is 3rd. |

### 1.5 Current app format — what actually exists

**Library piece** (`trainer-core/src/library.rs`)

```json
{
  "id": "Crowell-2004_027",
  "title": "…",
  "collection": "Trained Ear/Crowell 2004",
  "tags": ["sight_singing"],
  "key": "C major",
  "time": "4/4",
  "tempo_bpm": 72,
  "pickup": 1,
  "parts": [
    { "name": "Voice", "clef": "treble", "notes": "C4:1.5 D4:.5 E4:1 | …" }
  ]
}
```

- One key, one metre, one tempo per piece; up to 4 parts.
- Notes are `{note, beats}` / `{rest}` or compact text; durations are in the time signature's beat unit; barlines can be checked; ties merge; a short first bar is a pickup.
- The folder is the library; a subfolder is a collection; ids/titles default from the file path; pieces can be selected by id, collection or tags.
- **No MusicXML import.** The watched folder accepts `.json`, `.mid`, `.midi` only.
- MIDI import reduces chords to the top line, rounds to sixteenths, keeps a short first time signature as a pickup, and estimates the key if absent.

**Score** (`trainer-core/src/score.rs`)

- One key, one metre, one clef, one tempo, one staff.
- **4 ticks per quarter note (16th-note grid).** `Score::from_compact` rejects anything that is not a whole number of sixteenths, with the message “not a whole number of sixteenth notes”. **Tuplets and 32nd notes cannot be represented.**
- Clefs: `treble`, `treble8vb`, `bass` only. **No alto, tenor or percussion clef.**
- Pitched notes only; `pitch: None` means a rest, so an unpitched hit cannot be distinguished from a rest.
- No lyrics, dynamics, articulations, grace notes, repeats, or mid-piece key/metre changes.
- `Staff` renders one score; the library page stacks multiple parts as separate staves rather than a system.

**Exercises and sessions**

- 18 exercise kinds; relevant ones: `sight_sing`, `melodic_dictation`, `harmony`, `pattern`, `sing_interval`.
- `SightSingParams`: melody source (generated / given / library), tempo, start (key + first note / key only / first note / none), count-in, backing (`click`, `drone`, `chords`, `melody`, `melody_and_chords`), timbre, cursor.
- `Backing` is generated by the engine from the score or key. There is no external audio-file backing in a trial and no per-track mix (stored trial recordings can be replayed as WAV).
- Each trial records `stimulus.piece` for library melodies. That is enough to compute per-piece attempts, but there is no indexed `item_id`/`course_id`.
- Presets are `SessionSpec` JSON files in `crates/trainer-core/presets/`; category is a string. Courses in the UI are a hardcoded regex over preset IDs (`PracticeView.tsx`), not a data model.
- The library page can start sight-singing, dictation or harmony from selected pieces, with a 40-trial cap.

**Consequences**

| Source need | Current state |
|---|---|
| Rhythm/unpitched | Not representable |
| Tuplets | Rejected |
| 32nd notes | Rejected |
| Alto/tenor clef | Not representable |
| Four-part chorales | 4 parts exist, but only stacked staves; part selection works for harmony |
| Metre changes | Not representable |
| Repeats | Ignored, not expanded |
| Grace notes | Not representable |
| Per-exercise instructions | Only `BlockSpec.instructions`, not per library item |
| Course ordering | No course entity |
| Audio backing | Generated only; no stems or per-track mix |

### 1.6 Conversion probe (current runtime)

The probe (`corpus/_probe/xml_audit.py`) is deliberately throwaway, but it applies the app's rules (beat unit, pickup, ties, 16th grid) and reports:

```
Crowell: files=427 convertible_now=318 phase0_no_fermata=315 blocked=109
  percussion 26, tuplet 26, c_clef 16, multi_part 7, meter_change 3,
  grace 8, sub_sixteenth 13, bar_length 19, repeat 50, fermata 6
  schema-fixable only: 90
  source review: 19

Smey: files=42 convertible_now=0 (all percussion/unpitched)
```

The 318 are structurally representable, not notation-complete: slurs, articulations and dynamics are not modelled by the app and must be recorded as unrepresented. Fermatas change timing and are held back. It also caught a real conversion trap: **the app's beat is the time signature's denominator unit**, so 6/8 has 6 beats per bar, not 1.5. Using `4/beat_type` instead of `beat_type/4` shrinks every compound-metre piece to a quarter of its beat count. The probe was corrected; the production converter must encode this with a test.

The probe's minor-key naming was also initially wrong (it printed C minor for A minor). The production converter must use the app's `KeySig::from_fifths(fifths, mode)` semantics, not a naive major-name lookup.

---

## 2. Acquisition goals

### 2.1 Rules

1. **Symbolic first.** MusicXML, MuseScore, LilyPond, MEI or MIDI beats a PDF. A PDF beats a scan. A scan beats manual transcription only when the music is simple.
2. **Keep the source edition and page.** Every exercise must resolve to source + edition + volume/part + section + exercise label + printed page (and PDF page when from a file).
3. **Rights are data, not memory.** Record `rights_status`, jurisdiction/evidence, and whether the item may be redistributed or only used locally.
4. **Do not defeat DRM or access controls.** A university copy or a purchased ebook is not a redistribution licence.
5. **Do not flatten pedagogical instructions into notation.** Vowels, registration, breath, SOVT, bodywork and cautions are first-class fields or protocols.
6. **Companion media is separate.** Track number, purpose (prompt/accompaniment/demo/answer), and its own licence.

### 2.2 Ready now

| ID | Source | What we get | Rights | Status |
|---|---|---|---|---|
| TE-* | The Trained Ear, 14 books | 5,177 exercises; MusicXML/MIDI/MP3 | CC BY-SA 4.0 | Downloaded samples; full fetch pending |
| EE | Eyes and Ears, 455 `.ly` | 427 overlapping with TE-CRO; residual melodies and hierarchy | CC BY-SA (book); per-file source notes | Source archive downloaded |
| AT-2022 | Aural Training and Sight Singing Supplement | 505 exercises; text labels and images | CC BY-NC-SA 4.0 | Local PDF; OMR not started |
| KR-4E-WS | Krueger 4e worksheets | 1 vector worksheet | Copyrighted; local use | On disk |
| KR-3E | Krueger 3rd ed EPUB | Prose/chapter structure only | Copyrighted; local use | **Unusable for notation** |

### 2.3 Target purchases

**Group A — notation-rich aural skills (highest extraction value)**

| Priority | Title | Edition | Why | Target format |
|---|---|---|---|---|
| A1 | **Comprehensive Aural Skills**, Merritt & Castro | 2nd, 2020 | Modular rhythm/melody/harmony with recorded homework; the closest fit to a progressive course | Vector PDF + audio access; MusicXML if obtainable |
| A2 | **Music for Ear Training**, Horvit, Koozin & Nelson | 4th | Large recorded dictation/interval/chord bank; complements sight-singing | Vector PDF + audio; answer keys |
| A3 | **Strategies and Patterns for Ear Training**, Marcozzi | 1st | Progressive dictation across classical/jazz/pop; prompts vs answers | Vector PDF + audio |

**Group B — vocal exercise and routine sources**

| Priority | Title | Edition | Why | Target format |
|---|---|---|---|---|
| B1 | **The Vocal Athlete: Application and Technique for the Hybrid Singer**, Rosenberg & LeBorgne | 3rd, 2026 | 108 targeted exercises; contemporary styles | PDF + media; protocol text is essential |
| B2 | **A Systematic Approach to Voice**, Ragan | 2nd, 2026 | 100+ corrective exercises, 12 routines, videos | PDF + video index; protocols |
| B3 | **Berklee in the Pocket: Singer's Handbook**, Peckham | — | Compact workout sequencing | PDF; routine templates |
| B4 | **Your Singing Voice**, Gagné | — | Contemporary technique and expression | PDF |
| B5 | **The Contemporary Singer**, Peckham | 2nd | Technique with exercises | PDF |

**Group C — reference and curriculum design (not score extraction)**

| Priority | Title | Edition | Use |
|---|---|---|---|
| C1 | **The Vocal Athlete** (theory volume), LeBorgne & Rosenberg | 3rd, 2026 | Voice science, cautions, exercise selection |
| C2 | **Complete Vocal Fitness**, Friedlander | 2018 | Alignment, breathing, physical training |
| C3 | **What Every Singer Needs to Know About the Body**, Malde, Allen & Zeller | 4th, 2020 | Body mapping and movement cues |
| C4 | **Vocal Technique: A Physiologic Approach**, Bickel | 2nd | Physiology-informed technique |
| C5 | **Aural Skills Acquisition**, Karpinski | — | Learning sequence and assessment design |

**Acquisition note:** for every purchase, get the **print or publisher PDF**, not an image-only ebook. If a publisher PDF is not available, buy print and scan at 400 dpi. Record the ISBN and copyright page. Do not strip DRM.

### 2.4 What “acquired” means

For each source, create `corpus/sources/<id>/source.json` with:

- title, authors, edition, year, ISBN, publisher;
- access type (`open`, `purchased`, `library`, `permission`);
- rights status and evidence (licence URL, receipt, permission email);
- local paths for PDF/scan/audio;
- table of contents and page map;
- media manifest (track → purpose → rights);
- extraction status and known defects.

An item is not “ready” because a PDF exists. It is ready when the TOC, page map, rights record, and a sample extraction have been checked.

---

## 3. Canonical corpus format

The existing guide proposes a per-exercise bundle. That is the right archival shape; the app's runtime is different. Keep both.

```text
corpus/
  manifest.csv
  sources/
    TE-CRO/
      source.json
      toc.json
      raw/                 # downloaded zip/xml/mid (never edited)
      pages/               # source crops or page renders (rights-controlled)
      media/               # audio, only when needed
  exercises/
    TE-CRO/
      Crowell-2004_027/
        original.musicxml
        source.png         # crop or page reference
        exercise.json      # provenance + pedagogy + app mapping
        validation.json
        render.svg
        playback.mid
  build/
    library/               # app library JSON, grouped by source/section
    courses/               # course manifests
    assets/                # generated audio/MIDI, if any
```

**Rules**

- `original.musicxml` is the archival score; it is never overwritten by the app or by a re-run. Corrections go to a new revision with a note.
- `exercise.json` is authoritative for provenance and pedagogy.
- The app library JSON is **generated**; do not hand-edit it.
- Keep copyrighted scans and purchased media out of the app repository. Only generated library JSON and rights metadata are copied into `%APPDATA%\SingingTrainer`.
- One library JSON file per source/section (not one file per exercise). The library scanner reads every file when the library is loaded; 5,177 tiny files would be wasteful.

### 3.1 Exercise record (proposed)

```json
{
  "id": "TE-CRO/Crowell-2004_027",
  "source": {
    "source_id": "TE-CRO",
    "work": "Eyes and Ears",
    "authors": "Benjamin Crowell",
    "edition": "2004",
    "url": "https://www.trainedear.net/",
    "section": "Melody / …",
    "exercise_label": "Crowell-2004_027",
    "printed_page": null,
    "pdf_page": null,
    "rights_ref": "sources/TE-CRO/source.json"
  },
  "kind": "sight_singing",
  "notation": {
    "musicxml": "original.musicxml",
    "clef": "treble",
    "key": "C major",
    "time": "4/4",
    "bars": 8,
    "pickup": null,
    "parts": 1,
    "has_tuplets": false,
    "has_repeats": false
  },
  "pedagogy": {
    "source_sequence": 27,
    "skill_tags": ["meter.simple", "steps", "tonic_triad"],
    "instructions": "Sing at sight. Key and first note are given.",
    "preferred_solfege": null,
    "vowel": null,
    "breath_cues": [],
    "cautions": [],
    "assessment": "pitch_and_rhythm"
  },
  "extraction": {
    "method": "native_musicxml",
    "tool": "Sibelius export via The Trained Ear",
    "review_status": "unreviewed",
    "pitch_verified": false,
    "rhythm_verified": false,
    "source_verified": false,
    "notes": "No site title; title comes from TOC/source book."
  }
}
```

`review_status` moves `unreviewed → candidate → approved → rejected`. Only `approved` items appear in courses.

### 3.2 Identity and duplicates

- **Source identity is never merged:** `source + edition + volume/part + section + exercise label`.
- Compute a **content fingerprint** for dedup: interval sequence, duration sequence, contour, metrical pattern, transposition-invariant pitch classes.
- Link variants (“same tune, different rhythm/range/arrangement”) without deleting either source. The Trained Ear books share many folk tunes; 5,177 is an exercise count, not a unique-tune count.

---

## 4. Runtime expansion wishlist

### 4.1 P0 — required before bulk ingest

| Feature | Why (evidence) | Proposed change | Acceptance |
|---|---|---|---|
| **Rhythm / unpitched notation** | 42/42 Smey; 26/427 Crowell; 268 Aural rhythm exercises | Add `CompactNote::Hit { beats }` and a `rhythm` piece kind; `ScoreNote` distinguishes rest from unpitched hit; Staff renders a one-line percussion staff | Smey 42/42 load and play with correct onsets; a rhythm block grades onsets only |
| **Finer tick grid** | 26 Crowell tuplets; 11 with 32nds; Aural compound/triplet material | Raise `Score` from 4 ticks/quarter to **24 or 48 ticks/quarter**; convert beats rationally; store PPQ | Tuplet and 32nd fixtures round-trip; bar sums remain exact; grading timing unchanged within tolerance |
| **C clefs** | 16/427 Crowell; chorales | Add `Clef::Alto`, `Clef::Tenor`, `ClefChoice` variants; VexFlow clef mapping; range auto-fit understands them | 16 Crowell C-clef pieces render at correct pitch |
| **Multi-part systems** | 7/427 Crowell; Aural canons/duets/chorales | At minimum, part selection and stacked staves; ideally one system with aligned parts | A four-part chorale can be displayed and one part selected for singing |
| **Metre/key changes** | 3/427 Crowell; possible in books | Either support change events or split at the change into segments with a provenance note | A metre-change piece imports as segments with correct bar counts |
| **Grace/32nd/fermata policy** | 8 grace; 11 sub-16th; 6 fermatas | Support, expand to a short note, or quarantine with a visible reason; never silently drop. Fermatas affect timing, so they need an explicit hold or exclusion | Every unsupported symbol is recorded in `validation.json` |
| **Repeats** | 50/427 Crowell | Expand playback order at conversion; preserve original and a `repeat_expanded` flag | Bar count and playback match the printed repeat; source XML unchanged |
| **Course/item identity** | Needed for progress | Add optional `course_id`/`item_id` to `BlockSpec` or trial extras; index them | Per-item attempts and completion can be queried |
| **Collection browsing** | “Everything is condensed” | New `Courses` tab that lists collections as cards and opens a collection page | A user can open `Trained Ear / Crowell 2004` and see all pieces |

Do **not** encode rhythm as a single repeated pitch. That produces fake notation, wrong staff rendering, and misleading grading.

### 4.2 P1 — pedagogical completeness

| Feature | Why | Change |
|---|---|---|
| Per-item instructions | Books specify vowels, breath, solfège, cautions | `exercise.json` fields; a library-side `pedagogy` map; show before start |
| Lyrics / solfège underlay | Vocalises, songs, diction | Add lyric syllables to notes; render under the staff |
| Source/provenance in UI | Trust and review | Show source, edition, page, review status on item pages |
| Transposition and tempo ladder | Warmups and range safety | Per-item transpose plan; tempo ladder with pass criteria |
| Assessment policy | Prompt vs answer | Explicit `preview_melody`, `reference_pitch`, `guide`, `answer_visible`, `max_plays` |
| Skill tags | Progression | Preserve source sequence and compute tags independently |
| Non-timing markings | Slurs, articulation, dynamics are in the source but not the app model | Keep them in the original XML; record them as unrendered on the item; do not pretend they were converted |
| Multi-staff display | Chorales | One system, aligned parts, mute/solo |

### 4.3 P2 — audio and backing tracks

Two different problems; do not conflate them.

**A. Generated MIDI backing (first)**

- Extend `SungTrial` with `backing_tracks: Vec<Track>`: events, timbre, gain, mute/solo, role.
- Roles: `click`, `pulse`, `chordal`, `bass`, `guide_melody`, `counterpart`.
- Reuse the existing sample-accurate scheduler; mix tracks with per-track gain.
- This covers most warmups, vocalises, harmony, and generated courses without shipping audio.

**B. Recorded audio stems (later)**

- Decode with `symphonia`, resample to the engine rate, preload into buffers, schedule at a sample-accurate frame.
- Stems: accompaniment, guide vocal, click, demo; per-stem gain, mute, solo, loop, offset.
- `sync`: score start, count-in bars, tempo, offset; reject files whose tempo does not match the score unless explicitly marked “free”.
- Package per course: `assets/<item>/<stem>.flac` + `audio.json`; record rights per stem.
- Do not ship purchased book audio; generate our own MIDI/piano realisations where possible.

Sketch:

```json
"audio": {
  "sync": { "mode": "score", "count_in_bars": 1, "offset_ms": 0 },
  "stems": [
    { "role": "accompaniment", "path": "assets/te-cro-027/piano.flac", "gain_db": -6, "default": "on" },
    { "role": "guide", "path": "assets/te-cro-027/guide.flac", "gain_db": -3, "default": "off" },
    { "role": "click", "path": "assets/te-cro-027/click.flac", "gain_db": -12, "default": "on" }
  ]
}
```

### 4.4 New exercise kinds

| Kind | Payload | Grading | Notes |
|---|---|---|---|
| `rhythm_reading` | Rhythm score or library piece; mode: speak/tap/sing; syllable system | Onset timing only; no pitch | Needed for Smey and the R sections |
| `vocalise` | Pattern/notation + vowel, register, reps, tempo ladder, range, cautions | Pitch/onset partial; never “vocal health” | Protocols are first-class; no fake MusicXML for SOVT/bodywork |
| `guided_sing` (or `sight_sing` + `guide`) | Melody + guide/accompaniment policy | Pitch/onset; mark assisted | The user asked for guided singing with reference notes/accompaniment |
| `sing_and_play` | Two parts; one sung, one played | Lead part only | Later; Trained Ear has sing-and-play filters |
| `dictation` (exists) | Stimulus + answer | Note/rhythm edit distance | Link prompt audio separately from the answer score |

For warmups, the existing `pattern` exercise plus `vocalise` metadata covers most needs; add a `protocol` record for breathing/bodywork that has no notation and is graded only by completion.

---

## 5. Conversion toolchain

### 5.1 Verified installed

| Tool | Status |
|---|---|
| Python 3.10 | Installed |
| PyMuPDF 1.24, pypdf 6.7, OpenCV, Pillow, lxml, pandas | Installed |
| onnxruntime 1.23, torch 2.11 (CUDA) | Installed |
| Poppler (`pdftotext`, `pdfinfo`, `pdfimages`, `pdftoppm`) | Installed |
| ImageMagick, ffmpeg, 7-Zip, Node | Installed |
| Tesseract | **Not installed** |
| Java / Audiveris | **Not installed** |
| MuseScore | **Not installed** (winget has 4.7.5) |
| LilyPond | **Not installed** (winget has 2.24.4) |
| music21 | **Not installed** |
| Rust toolchain | rustc 1.88; **cannot build the current lockfile** (`rusqlite_migration 2.6` needs 1.95, `uuid 1.29` needs 1.89). The existing release binary does not provide a test path for library changes. Update the toolchain or pin dependencies before Phase 0 code changes. |

### 5.2 Install list

```powershell
winget install --id Musescore.Musescore -e
winget install --id LilyPond.LilyPond -e
python -m pip install music21
# Audiveris: install Temurin Java 17, then the Audiveris release zip
# Tesseract: UB Mannheim build, only for scanned text
```

**OMR choice**

- **Audiveris** is the primary recommendation: printed common notation, MusicXML export, GUI correction, batch CLI.
- **oemer** is a useful cross-check for simple single-staff systems, but it is unmaintained (last PyPI release 0.1.8, 2024) and has install/runtime issues. Do not make it the only path.
- **HOMR** is newer but AGPL-3.0 and requires Python 3.11+; only if Audiveris/oemer fail the pilot.
- **MuseScore** is for inspection and correction, not OMR.
- **Verovio** can render MusicXML to SVG for visual QA.

### 5.3 Pipelines

```text
Trained Ear XML  -> converter -> library JSON -> app (collection, then course)
Eyes and Ears .ly -> (match TE XML) or restricted parser -> library JSON
Aural PDF        -> label/crop map -> OMR -> MusicXML -> review -> converter
Krueger / books  -> PDF or scan -> OMR/manual -> MusicXML -> review -> converter
Course manifest  -> app course page (references library ids)
```

**A. Trained Ear MusicXML → app library JSON (bulk, deterministic)**

1. Download the per-source XML zip from `source-files/zip-archives/<source>_xml-files.zip`.
2. Parse with `lxml`/`ElementTree`; preserve the original XML.
3. Extract: key (fifths + mode), time, divisions, notes/rests, ties, tuplets, clefs, parts, repeats, grace, tempo, pickup.
4. Convert to beats in the **time signature's denominator unit**; merge ties; expand repeats if requested; split at metre/key changes; choose part(s).
5. Write one library JSON per source/section with explicit ids and collection.
6. Validate: bar sums (except pickup/final), tie integrity, range, supported features; write `validation.json`.
7. Round-trip: load the generated JSON through the app's own parser and render it.
8. Human sample audit; only then mark `approved`.

**B. Eyes and Ears residual LilyPond**

- Match `.ly` files to Trained Ear XML by title/source metadata in `list`.
- For the residual, parse the restricted subset: `\key`, `\time`, `\relative`, note tokens, `|` checks, `\acciaccatura`, ties. Reject anything outside the subset.
- Do not attempt a generic LilyPond→MusicXML converter.

**C. Aural Training PDF → OMR**

1. Use PyMuPDF to extract text blocks matching `^[RM]\s+\d+\.\d+$` and their bboxes.
2. For each label, associate the image block(s) below it on the same page; crop at native resolution; save a source PNG and a bbox record.
3. Build `segments.json`: label, printed page, PDF page, bbox, image path.
4. Run Audiveris on each crop (or a 2× nearest-neighbour upscale if the model needs it).
5. Inspect in MuseScore; correct against the crop; export MusicXML.
6. Convert to library JSON; mark `review_status`.
7. For rhythm exercises, extract onsets/durations and use the rhythm model directly; do not force pitches.

**D. Krueger**

- Acquire a vector PDF or scan. If vector: extract text/notation; if scan: 400 dpi, deskew, OMR.
- The EPUB can supply chapter headings and prose, not notation.
- Worksheet PDFs: convert directly if Finale vector; otherwise OMR.

**E. Purchased books**

- Same as D, but keep all scans local, record rights, and never include them in a distributable export.
- For protocol-first books (Vocal Athlete, Ragan, Peckham), extract **text + media pointers** first; notation is secondary.

### 5.4 Converter rules and traps

| Trap | Rule |
|---|---|
| Beat unit | In 6/8 the app has 6 beats/bar; beats = quarters × `beat_type/4`. Test 2/2, 3/4, 6/8, 9/8, 12/8. |
| Minor keys | Use `KeySig::from_fifths(fifths, mode)`; fifths 0 + minor = A minor, not C minor. |
| Pickup | A short first bar becomes `pickup`; do not insert a rest manually. |
| Final short bar | Allowed; do not flag as an error. |
| Middle short bar | Quarantine for review; do not auto-pad. |
| Ties | Merge only same pitch; a tie to nothing is an error. |
| Repeats | Expand into playback order and record it; never alter the original. |
| Tuplets | Preserve ratio; do not round to the 16th grid. |
| 32nds/grace | Support or quarantine with a reason; never drop. |
| Clefs | Map G/F/C/percussion; C clef line 3 = alto, line 4 = tenor. |
| Multiple voices | Split by voice; `<backup>` sets the cursor back. Do not sum voices as sequential notes. |
| Chords | Keep the requested part/top line; record that reduction happened. |
| Tempo | Prefer the score's tempo; a practice tempo is a separate field. |
| Titles | MusicXML titles are often empty; use the TOC/site hierarchy, not the filename. |
| Source page | Printed page and PDF page are different; record both. |

### 5.5 QA gates

1. **Structural:** valid XML; every measure's duration equals the metre except pickup/final; ties resolve; no overlapping voices; bar count matches the source.
2. **Musical:** pitch/rhythm diff against a gold transcription; range; key; accidental spelling.
3. **Visual:** render the corrected score and compare with the source crop, system by system. Check clefs, key signatures, accidentals, rests, dots, ties, beams, tuplets, slurs, repeats, text underlay.
4. **Playback:** listen through; the melody must be recognisable and metrically aligned.
5. **Human sign-off:** one pass per exercise for the pilot; sampling plus 100% review of quarantined items thereafter.
6. **Metrics:** per source, report pitch accuracy, rhythm accuracy, segmentation recall, and median correction time. A 99% note score can still be pedagogically wrong.

**Pilot gate:** do not batch-OMR a book until a 20-exercise stratified sample has been corrected and the per-exercise correction time is acceptable. For the Aural Training PDF, include rhythm, simple melody, compound metre, C clef, canon/duet, and chorale in the pilot.

### 5.6 Dedup

- Fingerprint each approved melody.
- Cluster exact/near duplicates; keep every source citation.
- Do not merge different arrangements, rhythms, texts, or pedagogical instructions.
- Report duplicate rate per source; 5,177 exercises will not be 5,177 unique tunes.

---

## 6. Courses and UI

### 6.1 Two-step UI plan

**Step 1 — Collection pages (no new backend model).** The library already has nested collections. Add a `Courses` tab that lists collections as cards; opening a card shows every piece in that collection/sub-collections, with search, filters, preview, and “start”. This immediately fixes “everything is condensed into the main page” and requires no course schema.

**Step 2 — Curated course manifests.** Add ordered units, item overrides, prerequisites, instructions, and progress. A course references library pieces by id; it does not duplicate notes.

### 6.2 Course manifest (proposed)

`%APPDATA%\SingingTrainer\courses\<course-id>.json`

```json
{
  "id": "trainedear/crowell-2004",
  "title": "Eyes and Ears",
  "subtitle": "An Anthology of Melodies for Sight-Singing",
  "source": { "source_id": "TE-CRO", "edition": "2004", "rights": "CC BY-SA 4.0" },
  "level": "beginner",
  "description": "427 melodies, from stepwise tunes to chromatic and ensemble pieces.",
  "units": [
    {
      "id": "rhythm",
      "title": "Rhythm",
      "items": [
        {
          "id": "Crowell-2004_001",
          "kind": "rhythm_reading",
          "tempo_bpm": 72,
          "tags": ["meter.simple", "rhythm.quarters"],
          "instructions": "Tap the pulse, then speak the rhythm on ta."
        }
      ]
    },
    {
      "id": "steps",
      "title": "Stepwise melodies",
      "items": [
        {
          "id": "Crowell-2004_027",
          "kind": "sight_sing",
          "start": "key_and_first_note",
          "tempo_bpm": 72,
          "tags": ["melody.steps", "meter.simple"]
        }
      ]
    }
  ]
}
```

- `kind` selects the exercise; defaults come from the course/item, not from a separate preset per item.
- `tags` are for filtering and stats; the source sequence is preserved separately.
- `rights` is displayed and can block export.

### 6.3 Navigation and pages

```text
Courses
  ├─ collection card: Trained Ear / Crowell 2004      [427] [Beginner]
  ├─ collection card: Aural Training / Rhythm          [268] [Beginner]
  └─ curated course: Warm-up · SOVT and onset          [12 routines]

Course page
  ├─ header: source, edition, rights, level, progress
  ├─ filters: rhythm / melody / harmony, metre, key, range, skill tag
  ├─ units (accordion)
  │    └─ item row: number · title · bars · key · metre · tags · status · [preview] [start]
  └─ actions: Start next 10 · Start unit · Start item · Resume
```

### 6.4 Progress

- Derive item attempts from trials whose `stimulus.piece` matches the library id.
- Add explicit `course_id`/`item_id` for curated courses so renamed ids do not break progress.
- Status: unseen / attempted / passed (block pass mark) / needs review.
- Do not auto-advance on a single lucky attempt; the app already has pass marks but no mastery model. Keep “passed” explicit.

### 6.5 Migration of the current Practice page

- Keep Practice as **Quick practice** (the existing presets and stage cards).
- Move book/source browsing to **Courses**.
- Remove the `COURSES` regex hack once presets carry a `course`/`unit` field or once course items reference exercise specs directly.
- Keep “Up next” (coach queue) on Practice, not buried in Courses.

### 6.6 Authoring and MCP

- Add MCP read tools: `list_courses`, `get_course`, `list_course_items`.
- Add write tools only after the schema is stable: `save_course`, `add_course_items`.
- The coach can then recommend “next 10 items in this unit” without inventing exercise specs.
- Course manifests should be generated by the corpus build, not typed by hand.

---

## 7. Phased execution

### Phase 0 — Prove the loop (small, no schema change)

**Goal:** get real music into the app and a real collection page, before touching the schema.

- Fix the Rust build first: rustc 1.88 cannot compile the current lockfile (see 5.1). Update the toolchain or pin the two offending dependencies, and confirm `cargo test -p trainer-core` is green.
- Build the production converter for the subset the app already supports.
- Import **up to 315** of the 318 structurally convertible Crowell melodies as `Trained Ear/Crowell 2004`; expand repeats; hold the 3 with fermatas.
- Add the collection/course page (Step 1 above).
- Validate through the app's own parser, render, play, and start a session.
- Write the converter tests for beat unit, pickup, ties, final short bar, minor keys.
- Record the 109 blocked files with reasons; do not hide them.

**Exit criteria:** 315 pieces visible in a collection page; 20 randomly sampled pieces match their source; a sight-singing session starts and grades; no app regressions.  
**Effort:** S–M.

### Phase 1 — P0 schema + the rest of the open corpus

- Rhythm/unpitched model and `rhythm_reading`.
- 24/48 PPQ grid; tuplets and 32nds.
- Alto/tenor clefs.
- Multi-part systems/part selection.
- Metre/key-change splitting or support.
- Grace/repeat policy.
- Course/item identity.

Then:

- Import all **427 Crowell** and **42 Smey**.
- Audit 100 stratified pieces (rhythm, tuplets, C clef, two-part, repeats, compound metre).
- Fetch and import the remaining 13 Trained Ear books (**4,750** exercises).
- Build collection pages per book and section; curated starter courses for rhythm, stepwise melody, tonic triad, and interval leaps.
- Reconcile Eyes and Ears residual files.

**Exit criteria:** 100% of structurally valid exercises imported with source ids; every blocked file has a machine-readable reason; 100-piece audit meets the agreed pitch/rhythm threshold; courses open and start.  
**Effort:** M–L.

### Phase 2 — Aural Training PDF pilot and batch

- Build the label→crop map (505 labels, 605 images).
- Run a 20-exercise stratified OMR pilot; correct in MuseScore; report pitch/rhythm accuracy and median correction time.
- Decide: Audiveris, oemer cross-check, or manual entry for the worst classes.
- If the pilot passes, batch the rhythm sections first (single-line, no pitch), then simple melodies, then canons/duets/chorales.
- Record every exercise's source crop and review status.
- Respect CC BY-NC-SA: personal/non-commercial only; no redistribution without permission.

**Exit criteria:** pilot report with measured accuracy and time; approved exercises have source crops and human sign-off; no batch run before the gate.  
**Effort:** M–L, dominated by review.

### Phase 3 — Krueger

- Acquire a vector PDF or clean scan; the EPUB is not a notation source.
- Extract TOC, headings, and instructions; map exercises to units.
- OMR in batches by chapter; correct; import.
- Use the 4e worksheets as a cross-check for interval/chord material.
- Keep all scans local; copyrighted.

**Exit criteria:** a chapter-level pilot (e.g. rhythmic reading + tonic-triad melodies) approved; page map and rights record complete.  
**Effort:** M–L.

### Phase 4 — Purchased books

- Acquire Group A first (Comprehensive Aural Skills, Music for Ear Training, Strategies and Patterns), then Group B (vocal exercise books), then Group C references.
- For A: extract notation + answer keys + audio mapping; build dictation and sight-singing courses.
- For B: extract protocols, vowels, cautions, tempo ladders, and routine sequences before notation; many exercises are protocol-first.
- For C: no score extraction; use to define tags, progressions, and safety rules.

**Exit criteria:** per-book extraction report; rights record; approved pilot; no restricted content in any distributable export.  
**Effort:** L.

### Phase 5 — Generated warmups, guided singing, audio

- `vocalise` kind with vowel/register/tempo ladder/reps/cautions.
- Generated patterns (scales, arpeggios, SOVT-friendly contours) transposed by range.
- Guided singing mode: play the melody once, then sing with accompaniment at reduced gain; mark assisted attempts.
- Multi-track MIDI backing first (roles, gains, mute/solo).
- Recorded stems later, with per-stem rights and sync metadata.

**Exit criteria:** a 12-routine warmup course works with generated backing; guided mode records assisted attempts separately; no pitch grading is claimed for SOVT/bodywork.  
**Effort:** M–L.

### Why this order

- The open sources need no permission and give the largest immediate volume.
- The pitched Crowell subset proves the entire loop before schema work.
- Rhythm support is the single largest blocker; it unlocks Smey, 26 Crowell pieces, and all 268 Aural rhythm exercises.
- The Aural Training PDF is the best OMR pilot because it has vector labels and clean line art, but its raster resolution is the risk; test before committing.
- Krueger is high value but needs a source we do not yet have.
- Purchased books are the slowest and most rights-constrained; they should not block the open corpus.

---

## 8. Risks and decisions

| Risk | Impact | Mitigation |
|---|---|---|
| Rights | A CC BY-NC-SA or purchased book cannot ship in a commercial product | Rights field per source; export filter; personal-only build now |
| OMR accuracy at 150–250 ppi | Wrong notes/rhythms teach the wrong thing | Pilot; source-crop comparison; human sign-off; quarantine |
| Tick grid change | Grading and rendering regressions | 24/48 PPQ fixtures; compare old/new on 318 pieces |
| Rhythm rendering | A fake pitched rhythm staff is misleading | One-line percussion staff; grade onsets only |
| Multi-part display | Chorales unusable | Start with part selection; add aligned systems |
| Library startup cost | 5,000+ pieces slow the app | Group JSON per section; measure; add a cached index if needed |
| Duplicates | 5,177 is not 5,177 unique tunes | Fingerprints; link, never merge sources |
| Audio licensing | Purchased play-alongs cannot be redistributed | Generate MIDI/piano; keep purchased audio local |
| Scope creep | Schema work delays music | Phase 0 ships 315 pieces first; P0 is the minimum for the rest |
| Source defects | 19 Crowell files with short middle bars | Quarantine and review; never auto-pad |
| Rust MSRV drift | The app cannot be rebuilt with the installed 1.88 toolchain | Update Rust or pin deps in Phase 0 before schema work |

### Decisions needed

1. **Rights posture:** is this corpus personal-only, or intended for eventual distribution? This determines whether CC BY-NC-SA and purchased material can be included in shipped builds.
2. **Course granularity:** per source book/section (e.g. “Crowell: stepwise melodies”), per skill (e.g. “tonic triad leaps”), or both? The plan assumes both: source collections plus curated cross-source courses.
3. **Rhythm scope:** do we need spoken/tapped rhythm grading in the first release, or is rhythm notation enough to start?
4. **Krueger source:** buy the 4th edition print and scan, or try to obtain a publisher PDF? The EPUB cannot be used.
5. **Audio priority:** generated MIDI backing first, or wait for recorded stems? The plan recommends generated MIDI first.

---

## Appendix A — Trained Ear inventory

| ID | Book | Count | Notes |
|---|---|---:|---|
| TE-BAT-1884a | Batchellor & Charmbury, *Tonic Sol-fa Music Course*, Vol. 1 | 120 | Steps, staff notation supplement |
| TE-BAT-1884b | same, Vol. 2 | 137 | Includes elementary rhythms and multi-part variants |
| TE-BAT-1884c | same, Vol. 3 | 86 | |
| TE-BAT-1887a | same, Vol. 4 | 95 | Includes intermediate rhythms |
| TE-BOI-1876 | Boissière, *Solfège élémentaire* | 122 | 110 one-voice + 12 two-voice |
| TE-COL-1904 | Cole & Lewis, *Melodia* | 1,511 | Largest source; systematic |
| TE-CON-1897 | Concone, *The School of Sight-Singing* | 90 | Classical vocalises |
| TE-CRI-1901 | Cringan, *Conservatory Sight-Singing Method* | 129 | |
| TE-CRO-2004 | Crowell, *Eyes and Ears* | 427 | Also 455 `.ly`; 26 rhythm, 26 tuplets, 16 C clef |
| TE-HOL-1915 | Hollis Dann, *First Studies in Sight Singing* | 373 | |
| TE-SME-2005 | Smey, *Easy Rhythms* | 42 | All unpitched |
| TE-SOM-1911 | Somervell, *One Thousand Exercises* | 456 | Historical progressive bank |
| TE-SUL | Sullivan, miscellaneous exercises | 314 | No source facsimile |
| TE-WED-1922 | Wedge, *Advanced Ear-Training and Sight Singing* | 1,275 | Advanced/harmony |
| **Total** | | **5,177** | |

## Appendix B — Probe output and sample

Probe: `corpus/_probe/xml_audit.py`  
Samples: `corpus/_probe/converted/`

```
Crowell: files=427 convertible_now=318 phase0_no_fermata=315 blocked=109
  percussion 26, tuplet 26, c_clef 16, multi_part 7, meter_change 3,
  grace 8, sub_sixteenth 13, bar_length 19, repeat 50, fermata 6
  schema-fixable only: 90
  source review: 19
Smey: files=42 convertible_now=0 (all percussion/unpitched)
```

The probe is not the production converter. It exists to size the schema work and to catch conversion traps early. It found the compound-metre beat-unit bug and a grid-check bug: the tick check must use the app's ticks per beat, not `beats × 4`, or 32nds in compound metre slip through.

## Appendix C — First ten concrete tasks

1. Decide rights posture and record it in `corpus/manifest.csv`.
2. Install MuseScore, Audiveris + Java 17, and (optionally) Tesseract; verify each CLI.
3. Write the production MusicXML→library converter with tests for beat unit, pickup, ties, final short bar, minor keys, 32nds, tuplets, C clefs, and multi-part.
4. Import up to 315 convertible Crowell pieces (repeat-expanded; fermatas held); sample-audit 20 against the source.
5. Add the `Courses` tab and collection page; open `Trained Ear / Crowell 2004`; start a session.
6. Raise `Score` to 24/48 PPQ; add tuplet and 32nd fixtures; re-run the 318-piece regression.
7. Add rhythm/unpitched notation and `rhythm_reading`; import Smey 42/42; grade onsets.
8. Add alto/tenor clefs and multi-part part selection; import the remaining 109 Crowell pieces except the 19 source-review cases.
9. Fetch all 14 Trained Ear zips; import 5,177; build course manifests by book/section.
10. Build the Aural Training label→crop map and run the 20-exercise OMR pilot; report accuracy and correction time before batching.
