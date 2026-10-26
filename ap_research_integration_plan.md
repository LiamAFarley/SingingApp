# Absolute pitch: integrating the acquisition research

**Date:** 24 October 2026 (revised the same day with the owner's answers)
**Input:** the deep-research report "Advanced Absolute Pitch Training: Empirical Protocols, Experimental Biohacks, and Architectural Specifications". It is quoted by its section numbers (R§1–R§7).
**Builds on:** `adaptive_music_training_app_plan.md` §7 (stimuli, masks, exposure log) and §9 (absolute-pitch suite).
**Priority:** this is the top priority, ahead of the courses, notation and songs work in `song_and_corpus_app_plan.md`. The minimal course machinery it needs (manifests, progress, gates) is pulled forward into AP-1.
**Scope:** every exercise and tracking idea in the research that looks useful. The pharmacological, neurostimulation, sleep-cueing and infant-exposure material is left out.

---

## 0. Bottom line

1. **Everything useful becomes a course.** The course types are:
   - multi-octave naming;
   - fast detection;
   - Brady's anchor fading;
   - chord colours;
   - note colours and names;
   - contrast drills;
   - inner voices;
   - sung-note production;
   - anchors;
   - screening;
   - weekly and maintenance tests.

   Each course is a ladder of levels with a pass rule.
2. **A daily set is composed from all active courses.** The app picks each course's current level, fits them into a time budget, and adds the weekly test or maintenance on their days. The coach can adjust it, but it works without the coach.
3. **Progress is tracked as one combined picture.** Per-pitch-class mastery pools all methods (naming, detection, chord extraction, production), plus each course's level. Per-method detail is kept in the data but isn't the headline.
4. **Gates are shown, not enforced.** Examples are "4 passes, the last at least 12 h after the third" and "14 days before the next chord". The course page and daily set show when an item is due; nothing locks.
5. **Out-of-set "Other" comes back for subset training,** as the research recommends (Wong safeguard 4), alongside the octave spread and jump rules.
6. **Spoken audio comes from files you generate** (Breeze TTS). The app reads a speech folder of digits, letters, note names and custom label names. Uses include:
   - spoken digit span;
   - letter n-back;
   - spoken labels in pairing and detection.

---

## 1. What is in and what is out

| Research item | Decision | Where |
|---|---|---|
| Working-memory screening: implicit note memory, n-back, digit span (R§1) | **In**; spoken digits and letters from the speech folder | §4.4, §3.7 |
| Simple Speed go/no-go with a sung prompt (R§1) | **In**; choir prompt | §4.1 |
| Weekly 120-note five-timbre test; UCSF-style test and scoring (R§1) | **In** ("UCSF-style": the real test's stimuli aren't reproduced) | §6, §5.3 |
| Wong safeguards: octave span, scrubber, 13-semitone jumps, 2 s answers, out-of-set distractors, 4 passes with an overnight gap, maintenance (R§1) | **In, all of them** | §3.1, §3.2, §5.2 |
| Cuddy single reference; Brady probability fading and paced streams (R§2) | **In** | §3.1, §4.1, §6 |
| Eguchi chord colours, then single notes (R§3) | **In** | §4.2 |
| Taneda colour discovery, errorless (R§3) | **In**: colour sets, discovery mode, errorless feedback | §3.3, §3.4, §4.5 |
| Beato inner-voice singing (R§3) | **In** | §4.3 |
| Age-related drift monitoring (R§3) | **In**, as the drift detector | §5.6 |
| Burge pitch colour; Gervain names; your own words or colours per note (R§4) | **In** | §3.3, §6 |
| Shepard–Risset glissando (R§6) | **In**, as the scrubber | §3.2 |
| Mental jukebox flash anchors; hot/cold memorised-piece scan (R§6) | **In**; clips from audio files now, from song packages later | §3.6, §4.1 |
| Sung-note production feel (proprioception) and anchor-note singing | **In** | §4.5 |
| Drugs, supplements, psychedelics, tDCS, carbamazepine, sleep cueing, pink noise, infant exposure (R§3, R§5) | **Out** | — |

---

## 2. Principles

- **The app composes and tracks; the coach steers.**
  - The daily set is a deterministic rule: each course's current level, within a time budget.
  - The coach can change budgets, pause courses, add sessions or override levels.
  - There is still no hidden learner model. The levels come from stored results and the course pass rules.
- **One combined progress picture.** All methods feed per-class mastery, because this is a personal tool. Per-method numbers stay queryable for the coach.
- **Training and holdout pools stay.** Weekly and UCSF-style tests use holdout timbres and octaves.
- **The reference-exposure log stays, and counts every labelled sound:**
  - the reference tones;
  - hearing answers after a trial;
  - the sung prompt in detection;
  - chord colour reveals;
  - anchor clips;
  - spoken labels.

  A scrubber resets the echoic clock; the log records both `secs_since_labeled_reference` and `scrubbed_since_reference`.
- **The app never declares "you have perfect pitch."** It reports classes mastered, within the time limit, across octaves and timbres.

---

## 3. Changes to existing pieces

### 3.1 Sequencing and answer options for `pitch_id` (and the other AP kinds)

New optional fields, reused by `sing_pitch`, `pitch_detect`, `chord_color_id` single-note trials and the screening tasks where they apply:

| Field | Meaning | Research default |
|---|---|---|
| `min_jump_semitones` | Each stimulus is at least this far from the previous one, across the block and across masks | 13 (Wong) |
| `octaves` (exists) | Validator warns when a jump rule is set with under 3 octaves | C4–B6 (Wong); 4 octaves (Solfeto) |
| `out_of_set_rate` (restored) | For subsets: this share of trials plays a note outside the subset, at the edges of the range, answered with an **Other** button | 0.15–0.20 (Wong) |
| `class_weights` | Per-class probability weights; unlisted classes share the rest | Brady: C 0.40, others ≈ 0.0545 |
| `pacing` | `respond` (next trial after the answer) or `stream { ioi_ms, response_window_ms }` (fixed schedule; a miss is a timeout) | Brady: 5000 → 1400 ms |
| `deadline_ms` (exists) | Response ceiling; late answers are stored with `in_time: false` | 2000 (Wong) |
| `labels` | Label and colour set (§3.3) | — |
| `requeue_misses` | Missed classes return later in the block, in another octave | Taneda, Eguchi |
| `register` | `spread` (default) or `fixed` (one octave, for contrast drills such as Burge's E♭3/F♯3) | — |

**Generator.**

- When a draw would break `min_jump_semitones`, the generator redraws the octave, then the class. After a bounded number of tries it records `constraint_relaxed: true`.
- Class weights use weighted sampling without replacement per cycle, so streaks stay short.
- Out-of-set notes are drawn from the classes outside the subset, preferring octaves at the range's extremes.

**"Other" in feedback.** After answering, clicking **Other** plays the out-of-set note that was heard, if that trial had one. Scoring: Other is correct for an out-of-set note and wrong otherwise. Out-of-set trials don't count toward any class's mastery.

### 3.2 Memory scrubber: the Shepard–Risset glissando

- **New mask** `ShepardGlissando { seconds: 20, direction: down, s_per_octave: 8 }`. It is a gliding Shepard tone: octave-spaced partials under a fixed Gaussian log-frequency envelope, gliding endlessly downward. Static Shepard tones already exist in `render.rs`.
- **Block `pre_mask`.** Verification, weekly, UCSF-style and maintenance blocks start with the glissando.
- **Scrub rule.** When a labelled sound was heard less than N minutes (default 5) before a block without feedback, the block starts with the glissando automatically. The daily set (§6.2) places tests first anyway.

### 3.3 Labels and colours

- **A label set maps each pitch class to `{name?, color?, word?, speech?}`.** `speech` is a file in the speech folder (§3.7).
- **Shipped sets:**
  - `note_names` (default);
  - `taneda_colors`: seven white keys, red, blue, yellow, orange, brown, grey, green for C–B; black keys added as the course extends them;
  - `gervain_names`: Sarah, David, Francine, Jimmy, Karen, Leo for the six whole-tone classes; six more names for the other whole-tone set;
  - `my_colors`: empty for you to fill (Burge-style pitch colours, your own words).
- **The coach and you can edit sets** in Settings and over MCP (`set_label_set`).
- **Display:**
  - answer buttons show the label, colour and word;
  - with `color_flash`, the screen tints in the played note's colour after you answer, or during passive pairing;
  - `hide_note_names` hides letter names entirely.
- **Data.** Trials always store the pitch class, so every label set feeds the same mastery numbers. The set's id is stored too.
- **Chord colours** (§4.2) use the same mechanism, with per-chord labels.

### 3.4 Errorless feedback

- **`FeedbackMode::Errorless`:**
  - a wrong answer gets no error mark, red or "wrong" sound;
  - the right answer lights up in its colour, you hear it, and with `requeue_misses` it comes back later in the block.
- **Grading and storage are unchanged**, so progress stays honest.
- **Where it applies.** It can be chosen on any identification block. It is the default for the first levels of the colour, chord-colour and contrast courses. Later levels switch to normal feedback, so you learn exactly which notes you confuse. Tests show no results in either mode.
- **Override.** A Settings switch, "Errorless early levels", turns those defaults off if you prefer normal feedback everywhere.

### 3.5 Time-limited mastery

Identification and detection trials record `in_time`. Mastery counts correct, in-time, unassisted answers. Slow correct answers are kept and shown separately.

### 3.6 Anchors and anchor clips

- **Anchors** (`anchor_song`, which exists) gain an optional `clip`:

  ```json
  {"file": "...", "start_s": 0.0, "dur_ms": 300, "midi": 69}
  ```

  The clip is a short slice of the original recording around the anchor note, played with fades.
- **Uses:**
  - a **reference** ("hear your A anchor"), logged as labelled exposure;
  - **sing the anchor note** (exists, mode A), now also with the lock-on tuner after a miss;
  - **the hot/cold scan**: `pitch_detect` with the anchor as target (§4.1);
  - the **anchor route to a note**: hear a note, recall the anchor whose note matches, then answer. This is `pitch_id` with the anchor list shown as a hint panel.
- **A jukebox** is a set of twelve anchors, one per class. The anchors page shows coverage.
- **Clips from songs.** Once song packages exist, "make anchor from song" picks a vocal note and writes the clip. Until then you choose the file and time.

### 3.7 Speech samples folder

- **Location and format.** The app's `data\speech\` folder, with files named by what they say:

  ```
  speech/digits/0.wav … 9.wav
  speech/letters/A.wav … Z.wav
  speech/notes/C.wav, C#.wav, Db.wav, …   (note names, both spellings optional)
  speech/labels/<set-id>/<label>.wav      (e.g. labels/gervain_names/Sarah.wav)
  speech/prompts/<id>.wav                 (optional phrases such as "sing", "now")
  ```

  WAV, FLAC or MP3 at any sample rate. The app resamples, trims leading and trailing silence, and normalises loudness on load.
- **Generation.** Breeze TTS (or any recorder) produces them. A script in `tools/` writes the file list to generate, so nothing is missed.
- **Uses:**
  - spoken digit span and letter n-back (§4.4);
  - spoken labels in `pitch_pairing` and incidental pairing;
  - the target name in `pitch_detect` prompts;
  - Gervain-name sets;
  - spoken prompts for future hands-free mode.
- **Missing files** disable only the features that need them, and the Settings page lists what's missing.

---

## 4. New exercise kinds

### 4.1 `pitch_detect`: fast detection (Simple Speed, Cuddy, hot/cold)

- **One trial is one stream.**
  1. The target is shown (label, colour) for 1500 ms while the target note is sung (GM choir "aahs"), optionally with its spoken name.
  2. Then 16 tones follow, at least `min_jump_semitones` apart, across the block's octaves and timbres. 25% are the target class, in varying octaves; the rest are drawn from all other classes.
  3. Press **Space** (or tap the pad) on the target. Each tone's response window is 1750 ms; the next tone follows.
- **Targets:**
  - a pitch class, the default, rotating through the active classes;
  - a fixed note (Cuddy: A4);
  - an anchor, where the prompt is the anchor's clip or just its name (hot/cold scan).
- **Results per stream:** hits, misses, false alarms, correct rejections, **d′** (log-linear corrected) and median hit latency. Targets more than an octave from the prompt are reported separately.
- **Feedback options:** none, a mark after each response, or a summary after the stream, with missed targets replayable.

### 4.2 `chord_color_id`: chord colours (Eguchi)

- **Exact voicings with colour labels.** The 14-chord sequence:
  - White phase: Red C E G, Yellow C F A, Blue B D G, Black A C F, Green D G B, Orange E G C, Purple F A C, Pink G B D, Brown G C E;
  - Black phase: Gray A C♯ E, Tan D F♯ A, Light green E G♯ B, Light purple B♭ D F, Sky blue E♭ G B♭.

  Inversions get different colours on purpose. Voicings stay in a fixed register by default; `register_shift` allows octave moves later.
- **Answers** are colour buttons; in the black phase, colour or chord names.
- **Single-note extraction** (`single_note_after`): after a chord answer, one of its notes, or a white-chord note, plays alone and is named by colour or note. It is stored as a `pitch_id` trial (`context: chord_extraction`), so it counts toward class mastery.
- **Short sessions:** 20–25 identifications (2–3 min), meant to recur through the day. The daily set includes one, and the course page offers more.
- **Pass rule:** 100% on all active chords, and ≥ 14 days since the last chord was added. Shown as "next chord due <date>".

### 4.3 `sing_chord_tones`: inside the chord (Beato)

- A chord or cluster of 3–6 notes plays (triads, sevenths, jazz voicings, clusters), then you sing its notes **bottom to top**.
- **Variants:**
  - `missing_tone`: sing the left-out third or fifth;
  - `name_after`: name the classes on the pad with several selections.
- **Grading:** sung notes are aligned in order, untimed, each within ±25 cents of its target (any octave by default); naming is graded per class.

### 4.4 Screening and working memory

These are assessments without feedback. They are repeated occasionally (the screening course schedules one a month) and form the aptitude profile.

- **`pitch_memory_match` (implicit note memory, Van Hedger):**
  - 27 sine tones from 471.58 to 547.99 Hz (A♯4 + 20 cents to C♯5 − 20 cents), about 10 cents per step;
  - a target from the odd steps 5–23 plays, then a 2 s mask, then you recreate it from step 1, 3, 25 or 27;
  - arrows move 1 step, Shift+arrows 2 steps, and each move plays the tone;
  - error in steps and cents. The research's benchmark is ≤ 2.75 steps for later successful learners.
- **`n_back`:**
  - tones or spoken letters (speech folder), 2-back and 3-back;
  - press on a match; scored by d′;
  - research figures for successful learners: 4.05 and 3.52.
- **`span`:**
  - spoken digits (speech folder) or tone sequences;
  - type the digits, or replay the tones on the pad;
  - the length grows until two errors at one length;
  - research figure for successful learners: 8.83 digits.

### 4.5 Production and the feel of a note

`sing_pitch` and the lock-on tuner exist. These are additions built on them.

- **Anchor auralisation check (Brady).**
  - At the start of an AP session, you sing your anchor note (C by default, or any chosen class) from memory before hearing anything.
  - It is graded by cents from the nearest octave of the target.
  - Tracked over time as **"internal anchor accuracy"**: how far your mental C is from a real C, day to day.
- **Sing by colour or name:** `sing_pitch` prompted by a label (a colour swatch, "Sarah", a spoken name) instead of the note name.
- **Sing then check:**
  - after singing, the target plays and the tuner shows your sung note against it (exists with `reveal_target_after` and the lock-on tuner);
  - add **"hold and compare"**: sing, the target fades in under your voice, and the tuner shows the gap.
- **Sing your anchor:** sing an anchor's note (exists), now part of the anchors course.
- **Production in the mastery picture.** Production trials count toward each class's mastery under a production column, so a class is "known" when it is both named and sung reliably.

### 4.6 Colour discovery (Taneda)

This is a preset course, not a new kind. It uses `pitch_id` with the `taneda_colors` set, starting from two or three white keys in a fixed register. It runs in errorless mode with `requeue_misses` and `color_flash`, and adds a key at each level. Later levels spread octaves and switch to normal feedback.

---

## 5. Tracking

### 5.1 The combined AP dashboard

- **Per pitch class:**
  - identification mastery (correct in time, unassisted, last N trials, across all methods);
  - production mastery (sung within ±50 cents of the class, any octave);
  - detection d′;
  - last seen.

  A class shows as mastered at ≥ 90% identification within the time limit.
- **Per course:** current level, pass status, next due date or gate, and time spent this week.
- **Headline numbers:**
  - classes mastered;
  - weekly test score (UCSF-style points and MAD);
  - internal anchor accuracy;
  - total AP practice time.
- **Trend charts:** weekly test, classes mastered, internal anchor accuracy.
- **MCP:** the coach reads the same picture through `get_ap_dashboard`. Per-method breakdowns stay available through `get_stats`.

### 5.2 Pass rules and gates

A course level passes on a rule evaluated over stored blocks:

| Rule | Meaning |
|---|---|
| `accuracy` | ≥ X% (in time) over the last N blocks or sessions |
| `verification_4x` (Wong) | 4 verification blocks (no feedback, scrubbed, ≥ 3 octaves, jumps ≥ 13, deadline ≤ 2 s) at ≥ 90%, with ≥ 12 h between the 3rd and 4th |
| `spacing` (Eguchi) | 100% on all active items and ≥ N days since the last was added |
| `detection` | d′ ≥ X over the last N streams |
| `maintenance` | a maintenance block in the last N days |

`get_gate_status` gives each rule's state, the qualifying passes with times, and what is still due, e.g. "pass 4 due after 07:40 tomorrow". It is displayed, not enforced.

### 5.3 Scoring additions

Every identification block reports:

- exact and ±1 accuracy (these exist);
- **UCSF-style points:** 1.0 for exact, 0.75 within ±1, as a total and a percentage;
- **MAD:** semitones folded to the nearest octave; 0 is perfect, 3 is chance.

### 5.4 Weekly and retention trends

`ap_weekly` runs are tagged, and `get_ap_dashboard` shows the series per timbre. A gap of more than 4 weeks between runs is marked as a retention measurement.

### 5.5 Aptitude profile

`get_aptitude_profile` gives:

- implicit note memory error;
- n-back d′;
- span;
- the date of each run.

The research benchmarks are shown as reference lines. The coach uses it to weight the daily set, e.g. more chord colours and Brady when working memory is lower.

### 5.6 Drift detector

- **What it flags.** A **uniform shift**: most classes' mean signed error moving the same way by about a semitone, across timbres and recent sessions, compared with your own baseline. Production is checked the same way.
- **Checks when a shift is flagged:**
  - the A4 setting and any change to it;
  - the output device's sample rate against the render rate (44.1 vs 48 kHz is ≈ 1.5 semitones);
  - SoundFont or tuning changes.
- **Reporting.** The shift is reported neutrally on the dashboard and to the coach. Internal anchor accuracy (§4.5) is a second, independent signal for the same thing.

---

## 6. Courses and the daily set

### 6.1 Courses

Courses are shipped as manifests: levels pointing at block specs, each with a pass rule. Defaults are tunable, and the coach can edit or add courses.

| Course | Levels | Pass rule |
|---|---|---|
| `ap_naming` (replaces `ap_progressive`) | subsets of 3 → 12 classes; early levels with out-of-set Other; ≥ 3 octaves and 13-semitone jumps from the start; deadlines 4 s → 2 s; training timbres widen | `accuracy` per level; `verification_4x` for the 12-class level |
| `ap_speed` (Simple Speed) | `pitch_detect` targets rotating through the classes; window 2500 → 1750 ms | `detection` |
| `ap_brady` | all 12 classes over 3 octaves; C weight 0.40 → 0.30 → 0.20 → 0.12 → 1/12; stream IOI 5000 → 1400 ms; anchor auralisation check first | `accuracy` per step |
| `ap_chord_colors` (Eguchi) | 9 white chords, then 5 black, then single-note extraction | `spacing` (14 days, 100%) |
| `ap_note_colors` (Taneda) | §4.6 | `accuracy` |
| `ap_contrast` (Burge/Gervain) | E♭3 vs F♯3 (fixed register), + A3, + the black keys, then spread; your colour/word set or Gervain names; "no humming" | `accuracy` |
| `ap_inner_voices` | `sing_chord_tones`: triads → sevenths → clusters; missing tone; naming | `accuracy` |
| `ap_production` | `sing_pitch`: by name → by colour/label → with distractor music first; hold-and-compare | `accuracy` (±50 cents) |
| `ap_anchors` | sing each anchor; anchor reference; hot/cold scan per anchor | `accuracy` / `detection` |
| `ap_screening` | implicit note memory, n-back (tones, letters), span (digits, tones) | — (assessment, monthly) |
| `ap_weekly` | 120 trials: 24 each of piano, cello, clarinet, harpsichord, square; no feedback; scrubbed | — (weekly) |
| `ap_ucsf_style` | 36 piano + 36 sine, no feedback, scrubbed | — (occasional) |
| `ap_maintenance` | 5 min mixed 12-class naming across octaves and timbres | `maintenance` (twice weekly once `ap_naming` is complete) |

### 6.2 The daily set

- **A daily-set config** (Settings, and MCP `set_daily_plan`) lists the active courses with minutes per day, plus the weekly test day. Example: naming 8, speed 4, chord colours 3, Brady 5, production 4, anchors 2; weekly test on Sunday.
- **Composing "Start today's AP set":**
  1. a 20 s scrubber, and the anchor auralisation check if `ap_brady` or `ap_production` is active;
  2. due tests first: the weekly test on its day, any `verification_4x` pass that has come due, and maintenance if due;
  3. each active course's **current level** (its first level not yet passed), trimmed to its minutes. Order rotates daily so no course is always last;
  4. extra short chord-colour sessions offered later in the day as reminders on the Practice page (no notifications).
- **Progress view.** One dashboard (§5.1) covers the whole set. The set's session is tagged `ap_daily_set`, so the coach can see adherence.
- **Coach.** It can read the planned set before it runs (`preview_daily_set`), change minutes, pause a course, or queue its own session instead.

---

## 7. Coach (MCP)

- **Read:**
  - `get_ap_dashboard`, `get_gate_status`, `get_aptitude_profile`, `get_drift_report`;
  - `preview_daily_set`;
  - `get_anchors` (with clips and coverage);
  - per-method detail via `get_stats`.
- **Write:**
  - `set_daily_plan`, `set_label_set`;
  - `save_anchor` with a clip;
  - `set_course_level`, a manual override;
  - course manifest edits;
  - session specs with the new kinds and fields.
- **Server instructions** carry the research's guidance:
  - screen first;
  - weight toward chord colours, Brady and errorless levels when working memory is lower;
  - scrub before tests;
  - verification passes on separate days;
  - maintenance once classes are mastered.

---

## 8. Phases

AP comes first. Sizes: S a few days, M one to two weeks.

### AP-1: foundations, naming and the daily set (M) — built

Built 24 October 2026 as planned, with these details:

- Courses are built in, in `trainer-core/src/courses.rs`; course manifests the coach can edit come later.
- A block's `course` field tags its trials `course.<id>.<level>`, and check blocks also get `course.check`. Pass rules count those blocks. Progress is stored in `course_state` and `course_passes`.
- "In time" is computed by the dashboard: correct within 2 s.
- No separate `register` field: a fixed register is just a single octave in `octaves`.
- In weighted blocks, a repeated note moves octave instead of being redrawn, so an anchor's share stays exact.
- New Tauri commands: `ap_dashboard`, `preview_daily_set`, `save_daily_plan` and `set_course_level`. The matching MCP tools are `get_ap_dashboard`, `list_courses`, `get_course_level`, `preview_daily_set`, `set_daily_plan` and `set_course_level`.


- Course manifests with levels, pass rules, gates and the due display. This is the minimal part of the corpus plan's courses work, pulled forward.
- Sequencing fields: jumps, restored out-of-set Other, class weights, stream pacing, fixed register, `in_time` (§3.1, §3.5).
- Glissando scrubber and the scrub rule (§3.2).
- UCSF-style points and MAD (§5.3).
- The combined dashboard (§5.1).
- The daily set (§6.2).
- Courses: `ap_naming`, `ap_brady`, `ap_weekly`, `ap_ucsf_style`, `ap_maintenance`.

**Exit:**

- "Start today's AP set" runs naming and Brady levels after a scrubber;
- the dashboard shows classes mastered and course levels;
- `verification_4x` due times show correctly;
- no two consecutive tones in a verification block are within 12 semitones.

### AP-2: colours, labels, detection (M) — built

Built 24 October 2026 as planned, with these details:

- Label sets live in the database over the built-in ones (`note_names`, `taneda_colors`, `gervain_names`, `my_colors`, `eguchi_chords`). Settings and `set_label_set` edit them. Buttons show the word and colour; `hide_note_names` drops the letter; `color_flash` tints the screen after the answer, or while a pairing label is up.
- `feedback: errorless` stores the real result and shows no error mark. The right answer lights up and is played, and `requeue_misses` asks that class again later, in another octave when the block has one. A setting, "Errorless early levels", turns the course defaults off. Blocks tagged `ap.errorless_default` are the ones it affects.
- Speech files live in the app's `data\speech` folder. WAV, FLAC and MP3 are trimmed, normalised and resampled on load. Missing files are listed in Settings and only the speaking features stay off. `tools/speech_file_list.py` writes the list to generate.
- `pitch_detect` is one stream per trial: choir prompt, then 16 tones, Space to catch the target. The summary is d′ (log-linear) and median hit time. Targets more than an octave from the prompt are counted apart.
- `chord_color_id` uses the 14 fixed Eguchi voicings. `single_note_after` stores the following note as `pitch_id` with `context: chord_extraction`.
- Courses `ap_speed`, `ap_note_colors`, `ap_chord_colors` and `ap_contrast` are in the daily plan, switched off until turned on. Chord levels pass on 100% and 14 days, shown as "Next chord due …". Contrast uses Gervain names unless Settings points it at another set.
- The old naming course is unchanged. Past progressive results still count on the note tiles only.

- Label and colour sets with `color_flash` (§3.3).
- Errorless mode and requeue (§3.4).
- The speech folder (§3.7).
- `pitch_detect` (§4.1).
- `chord_color_id` with single-note extraction (§4.2).
- Courses: `ap_speed`, `ap_chord_colors`, `ap_note_colors`, `ap_contrast`.

**Exit:**

- a Simple Speed stream reports d′ and latency;
- the chord-colour course shows its next-chord date;
- errorless levels store true results with no error marks.

### Tests rework (after AP-2) — built

Built 25 October 2026 from `docs/2026-10-24_benchmark_assessment_spec.md` and its review
(`docs/ap_benchmark_spec_review_and_revisions.md`). `docs/AP_TESTING.md` is the reference for the
coach.

- The `ap_assessment` and `ap_daily` presets are gone. The tests are courses: `ap_benchmark` (new,
  every 14 days), `ap_weekly`, `ap_transfer` (new, every 28 days) and `ap_ucsf_style`. Their
  presets are built from the course definitions.
- Every test: no feedback, 5 s per note, no replay (replay is now off on any assessment block),
  the clearing glissando first, and no mask between notes except on Shepard tones.
- `min_octave_step` on `pitch_id` makes each tone move two octaves by octave number, chosen
  without looking at the note. With the 13-semitone rule alone, the octave gave away about 0.3
  bits about the note. The naming register is now C3–B6 for training and the benchmark.
- Holdout octaves are dropped. The extra SoundFonts are split into training and holdout, kept to
  their in-tune range, and skipped when not installed.
- Class draws stay even under the jump rule: a replaced class goes back into the bag.
- `get_ap_profile` reports by course tag:
  - accuracy with its 95% range, within 2 s from onset, timeouts, UCSF points and MAD;
  - signed lean, correct-only median time, per timbre, Shepard and sung accuracy;
  - a chaining check on sung notes;
  - minutes since a named note, and cautions;
  - change against noise and a 30-day trend.

### AP-3: production, inner voices, screening (M) — screening and production built

- Anchor auralisation check, sing by label, hold-and-compare (§4.5). **Built:** the daily set
  starts with singing C from memory on days no test starts it (a course's own anchor check moves
  to the front instead); `sing_pitch` `prompt: label | color | spoken`; `hold_and_compare`.
- `sing_chord_tones` (§4.3). *Not built.*
- `pitch_memory_match`, `n_back`, `span` with spoken digits and letters (§4.4). **Built:**
  `n_back` gained `stimuli: tones | letters` rather than a second engine; letters and digits fall
  back to tones when the speech files are missing, and the stored stimulus says so.
- Aptitude profile (§5.5): **built** (`get_aptitude_profile`, the Absolute pitch page). Drift
  detector (§5.6): *not built.*
- Courses: `ap_production` and `ap_screening` **built** (handoff
  `docs/2026-10-25_ap3_handoff.md`); `ap_inner_voices` *not built.*
- Also from the handoff: the daily set keeps the plan's order (no daily rotation; the course
  cards reorder it), the per-note table labels where naming trials came from and adds a
  production column ("known" = named and sung), and contrast level 1 has three notes.

### AP-4: anchors (S)

- Anchor clips, jukebox coverage, the hot/cold scan and the anchor hint panel (§3.6). This is the `ap_anchors` course.
- Clips from songs come with the deferred songs work.

After AP: the remaining courses and warm-ups work, then notation, then the rest of the courses (`song_and_corpus_app_plan.md`). Songs are deferred.

---

## 9. Decisions

**Resolved 24 October 2026:**

- AP first.
- Gates shown, not enforced.
- Spoken digits and letters from locally generated files.
- Choir prompt for detection.
- Out-of-set Other restored for subset training.
- Combined tracking.
- Errorless available everywhere, default on early colour, chord and contrast levels.

**Open (defaults assumed until changed):**

1. Daily set length: about 30 minutes by default, with the per-course minutes in §6.2.
2. Weekly test day: Sunday.
3. Brady anchor class: C.

---

## 10. Tests to automate

- **Sequencing:** 10,000 generated trials at three octaves with a 13-semitone minimum never violate it, or record `constraint_relaxed`. Class weights land within 2% of their targets. The out-of-set rate lands within 2% of its target, and those notes are outside the subset.
- **Glissando:** continuous across buffer joins and descending for its whole length.
- **Detection:** d′ matches hand-computed values, including the all-hits and no-false-alarm cases.
- **Screening:** implicit note memory step and cent errors match fixtures, with starts always outside the target range; span stops after two errors at a length.
- **Gates:** fixture histories pass and fail each rule exactly at the boundaries (11 h 59 m vs 12 h; 13 vs 14 days).
- **Daily set:** given fixture progress, the composed set contains the right levels, puts due tests first, respects minutes, and rotates order.
- **Scoring:** UCSF-style points and folded MAD match fixtures; random answers give MAD ≈ 3.
- **Drift:** a synthetic uniform +1 shift is flagged; random errors are not; an A4 change is reported.
- **Errorless:** stored results are true while the feedback view has no error state.
- **Speech folder:** missing files are listed, and the features that need them are disabled, not broken.
