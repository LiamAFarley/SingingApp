# Absolute pitch: integrating the acquisition research

**Date:** 24 October 2026
**Input:** the deep-research report "Advanced Absolute Pitch Training: Empirical Protocols, Experimental Biohacks, and Architectural Specifications". It is quoted by its section numbers (R§1–R§7).
**Builds on:** `adaptive_music_training_app_plan.md` §7 (stimuli, masks, exposure log) and §9 (absolute-pitch suite), and the courses work in `song_and_corpus_app_plan.md`.
**Scope:** exercises and progress tracking only. The pharmacological, neurostimulation and sleep-cueing material is left out (§1).

---

## 0. Bottom line

1. **Most of the research fits the existing design.** It mostly needs new generator parameters, one new mask, new feedback and label options, and better reports, rather than a new architecture.
   - The app still generates stimuli, grades them deterministically and reports.
   - The coach still decides progression.
   - Gates such as "4 passes with an overnight gap" or "14 days before the next chord" are **computed and shown by the app and enforced by the coach** (or by the user following the course page). There is still no adaptive engine inside the app.
2. **Anti-cheating safeguards become generator and block options:**
   - at least three octaves per trial;
   - consecutive trials at least 13 semitones apart;
   - a 2-second response window;
   - a 20-second descending Shepard–Risset glissando to clear echoic memory before unassisted blocks;
   - verification passes separated by sleep.
3. **Four new exercise kinds:**
   - `pitch_detect`: go/no-go target detection in a fast stream (Van Hedger's Simple Speed, Cuddy's single anchor, the hot/cold anchor scan);
   - `chord_color_id`: the Eguchi chord identification method, 14 voiced chords with colour labels, then single notes;
   - `sing_chord_tones`: Beato-style inner-voice singing;
   - a **screening battery**: implicit note memory matching, a tonal n-back and a span task.
4. **New options on existing kinds:**
   - Brady's probability-weighted anchor class and paced streams;
   - custom per-class labels (names, colours, words) for Burge- and Gervain-style tagging;
   - an errorless feedback mode (Taneda);
   - audio-clip anchors for the mental jukebox.
5. **New tracking:**
   - per-class mastery within the time limit;
   - gate status;
   - UCSF-style scoring (1 point exact, 0.75 within a semitone) and mean absolute deviation;
   - d′ for detection;
   - weekly generalisation trends;
   - an aptitude profile from screening;
   - a **drift detector** that tells a uniform shift across the confusion matrix (settings, device or perception) from random forgetting.
6. **One research recommendation is rejected:** out-of-set "other" distractors during subset training (R§1, safeguard 4). You ruled "Other" out. The loophole it targets, anchoring on the highest and lowest notes of a small subset, is closed instead by the octave spread, the 13-semitone jump rule, and detection streams that always draw from all twelve classes (§3.1).

---

## 1. What is in, adapted or out

| Research item | Decision | Where |
|---|---|---|
| Working-memory screening: implicit note memory (INM), auditory n-back, digit span (R§1) | **In**, as a screening battery and aptitude profile | §4.4, §5.6 |
| Simple Speed go/no-go with a sung prompt (R§1) | **In** | §4.1 |
| Weekly 120-note five-timbre generalisation test (R§1) | **In**, as a preset | §6 |
| UCSF-style 36 piano + 36 sine test and its scoring (R§1) | **In, as "UCSF-style".** The real UCSF test has its own stimuli and is not reproduced; its scoring rule is | §5.2, §6 |
| Wong 2025 safeguards: octave span, scrubber, ≥ 13-semitone jumps, ≤ 2 s responses, 4 passes with an overnight gap, maintenance (R§1) | **In** | §3.1, §3.2, §5.1 |
| Wong safeguard: out-of-set distractors | **Adapted**: no "Other"; closed by octave spread, jumps and detection streams | §3.1 |
| Cuddy single reference; Brady probability fading and paced streams (R§2) | **In** | §3.1, §4.1, §6 |
| Eguchi/CIM 14 chords with colours, then single notes (R§3) | **In** | §4.2, §6 |
| Taneda errorless discovery (R§3) | **In**, as a feedback mode | §3.4 |
| Beato inner-voice singing (R§3) | **In** | §4.3 |
| Age-related drift monitoring (R§3) | **In**, as the drift detector | §5.5 |
| Burge pitch-colour contrast; Gervain proper names; ideasthetic tags (R§4) | **In**, as custom labels and contrast presets | §3.3, §6 |
| Shepard–Risset glissando synthesis (R§6) | **In**, as a mask | §3.2 |
| Mental jukebox flash anchors; hot/cold memorised-piece scan (R§6) | **In.** Clips come from audio files now and from song packages once songs land | §3.6, §4.1 |
| Valproate/butyrate, psychedelics, ketamine, MDMA, ibogaine, carbamazepine dosing, tDCS (R§5) | **Out** | — |
| Sleep-stage cueing (TMR), pink noise, the mobile client (R§5) | **Out** | — |
| Prenatal and infant exposure (Nuryl) (R§3) | **Out** | — |
| Clinical causes of a uniform shift (R§5) | **Out of the app.** The drift detector reports the shift and checks the app's own settings; interpreting it is up to the user and coach | §5.5 |

---

## 2. Principles kept from the current design

- **No adaptive engine in the app.** The app reports gate status and mastery; the coach (or the user, from the course page) advances. Presets and courses still work without the coach.
- **No "Other" answer.** Subsets play and offer only their own notes.
- **Training and holdout pools** stay. Verification and assessment blocks use holdout timbres and octaves unless a block overrides it.
- **The reference-exposure log stays, and is extended.**
  - Every labelled tone counts as exposure:
    - a reference;
    - hearing an answer after a trial;
    - the sung prompt in `pitch_detect`;
    - a chord's colour reveal.
  - A scrubber resets the "echoic" clock (§3.2). The log then records both `secs_since_labeled_reference` and `scrubbed_since_reference`.
- **The app never reports a single "you have perfect pitch" verdict.** It reports dimensions, including the research's ≥ 90% per-class criterion as one of them.

---

## 3. Changes to existing pieces

### 3.1 Sequencing safeguards in `pitch_id` (and the other AP kinds)

New optional fields in `PitchIdParams`, reused by `sing_pitch`, `pitch_detect` and the screening tasks where they apply:

| Field | Meaning | Research default |
|---|---|---|
| `min_jump_semitones` | Each stimulus is at least this far from the previous one (MIDI distance), across the block and across the mask | 13 (Wong) |
| `octaves` (exists) | Validator warns when `min_jump_semitones > 0` and the span is under 3 octaves, which makes the rule impossible or too predictable | C4–B6 (Wong); 4 octaves (Solfeto) |
| `class_weights` | Per-class probability weights; unlisted classes share the rest equally | Brady: `{"C": 0.40}`, others ≈ 0.0545 |
| `pacing` | `respond` (default: next trial after the answer) or `stream { ioi_ms, response_window_ms }`, where tones arrive on a fixed schedule whether or not you answered; a miss is a timeout | Brady: IOI 5000 → 1400 ms |
| `deadline_ms` (exists) | Response ceiling; a late answer is recorded with `in_time: false` | 2000 (Wong) |
| `labels` | Custom label set (§3.3) | — |
| `requeue_misses` | Missed classes come back later in the block, in another octave | Taneda, Eguchi |

**Generator change.** The octave draw and the class bag stay. When the next draw would break `min_jump_semitones`, the generator redraws the octave first, then the class, within a bounded number of tries, and records `constraint_relaxed: true` if it had to give up. Class weights replace the uniform bag with weighted sampling without replacement per cycle, so streaks stay bounded.

**Why this replaces "Other".**

- The subset-anchoring loophole works when a small set of notes sits in a narrow range: the highest note heard is "the G".
- With three or more octaves and 13-semitone jumps, a class's height is uninformative: the same class appears near both ends of the range.
- `pitch_detect` streams (§4.1) always draw distractors from all twelve classes, so subset learners still hear and reject the other notes, but are never asked to name them.

### 3.2 Memory scrubber: the Shepard–Risset glissando

- **New mask:** `Mask::ShepardGlissando { seconds: 20, direction: down, rate_s_per_octave: 8 }`. It is a continuously gliding Shepard tone: octave-spaced partials under a fixed Gaussian log-frequency envelope, with the phase offset gliding so it descends endlessly. The renderer already has static Shepard tones (`render.rs::shepard_note`); this adds a gliding version.
- **Block-level `pre_mask`.** Any block can start with a mask. Verification, weekly and UCSF-style presets start with the 20-second glissando.
- **Automatic scrubbing rules (block option `scrub_after_reference`):**
  - When a labelled reference was heard less than N minutes before an unassisted block, the block starts with the glissando, whether or not the coach asked for it.
  - The runner tracks this from the exposure log.
- Masks between trials stay as they are (`random_tones` by default when feedback reveals answers). The glissando is too long for every trial and is meant for block boundaries.

### 3.3 Custom labels: names, colours, words

- **A label set** is a map from pitch class to `{name?, color?, word?}`, for example:
  - Gervain's names (Sarah, David, Francine, ...);
  - Eguchi colours;
  - the user's own words for each note ("bold", "mellow").
- **Storage.** Label sets are stored in settings, so the coach can set them over MCP (`set_label_set`). A block refers to one by id; `default` means note names.
- **Display.** Answer buttons show the label and colour, and feedback shows the label (plus the note name unless `hide_note_names`).
- **Where they apply:** `pitch_id`, `pitch_detect` (the target prompt), `pitch_pairing`, `incidental_pairing` (which already has a class-to-colour map; it moves onto label sets), and `chord_color_id` (per-chord labels).
- **Tracking.** Labels change only the display: trials still record the pitch class, so stats are comparable across label sets. The trial also records `label_set` so the coach can compare.

### 3.4 Errorless feedback mode (Taneda)

- New `FeedbackMode::Errorless`:
  - a wrong answer shows no error mark, red colour or sound;
  - the correct answer is shown neutrally (its label and colour light up), and you can hear it, as now;
  - with `requeue_misses`, the class returns later in the block.
- **Grading and storage are unchanged.** The trial is still recorded as wrong, so progress stays honest. Only the screen avoids the error signal.
- Block summaries in this mode show "found" counts and per-class progress rather than "wrong".

### 3.5 Time-limited mastery

- Identification trials gain `in_time` (answered within `deadline_ms` or the stream's response window).
- **Mastery and gate reports count only correct, in-time, unassisted answers.** A slow correct answer is recorded as such, because the research's criterion is ≥ 90% within about 2 s.

### 3.6 Anchors with audio clips (mental jukebox)

- Anchors (`anchor_song`) gain an optional `clip`:

  ```json
  {"file": "...", "start_s": 0.0, "dur_ms": 300, "pitch_class": "A", "midi": 69}
  ```

  It is a 200–400 ms slice of the original recording around the anchor note. The file sits in the anchors folder; the app plays the slice with a short fade.
- **Uses:**
  - **reference playback** ("hear your anchor for A"), logged as labelled exposure;
  - **"Sing the anchor note"**, which already exists as mode A;
  - the **hot/cold scan**: `pitch_detect` with the anchor as target (§4.1).
- A twelve-anchor "jukebox" is just twelve anchors, one per class. `get_anchors` reports which classes have one.
- Once song packages exist, "make anchor from song" picks a vocal note and writes the clip and pitch automatically. Until then you choose the file and time.

---

## 4. New exercise kinds

### 4.1 `pitch_detect`: go/no-go target detection

**Covers:** Van Hedger's Simple Speed, Cuddy's single-reference training, and the hot/cold memorised-piece scan.

- **Trial = one stream.**
  1. The target is shown (label, colour) for `prompt_ms` (default 1500), with a sung rendition of the target note. The default prompt timbre is GM choir "aahs"; once own-voice samples exist, the user's own voice.
  2. Then `stream_len` tones (default 16), each at least `min_jump_semitones` from the last, across the block's octaves and timbres. A share `target_rate` (default 0.25) are the target class, in varying octaves; the rest are drawn from all other classes.
  3. You press **Space** (or click the pad) when you hear the target. The response window for each tone is `response_window_ms` (default 1750), and the next tone follows at the end of the window.
- **Target sources:**
  - a pitch class, the default;
  - a fixed note (Cuddy: A4 only);
  - an anchor, where the prompt is the anchor's clip or its name only and the target is its pitch class (hot/cold scan).
- **Grading:**
  - for each tone, hit, miss, false alarm or correct rejection, with latency for hits;
  - per stream: hit rate, false-alarm rate, **d′** (log-linear corrected) and median hit latency;
  - a stream "passes" at the block's `pass_mark` on d′ or on accuracy.
- **Feedback options:** none, a mark after each response, or a summary after the stream. The answer for missed targets can be revealed after the stream (a labelled exposure).
- **Stats:** d′ and latency by target class, by timbre and by octave distance from the prompt. The prompt's octave is recorded, so "targets more than an octave from the prompt" can be reported separately, as the research does.

### 4.2 `chord_color_id`: chord identification method (Eguchi)

- **Chords are exact voicings,** not chord qualities, e.g. Red = C4 E4 G4, Orange = E4 G4 C5 (the inversion has a different colour on purpose). A preset holds the 14-chord sequence from R§3:
  - White phase: Red CEG, Yellow CFA, Blue BDG, Black ACF, Green DGB, Orange EGC, Purple FAC, Pink GBD, Brown GCE;
  - Black phase: Gray AC♯E, Tan DF♯A, Light green EG♯B, Light purple B♭DF, Sky blue E♭GB♭.
- **Params:**
  - `chords`, the active set with labels, colours and voicings;
  - `timbre`, default piano;
  - `register_shift`, default none: CIM keeps fixed voicings, and an option allows octave moves later;
  - `answer`: colour buttons, or note-name chords in the black phase.
- **Single-note extraction.** Option `single_note_after`: after each chord answer, one note from the chord (or, in the research's "white chords only on black levels" option, a note from a white chord) plays alone and is named by colour or note. It is recorded as a separate `pitch_id` trial with `context: chord_extraction`. In the final phase the kind hands over to ordinary `pitch_id`.
- **Sessions are short.** The preset is 20–25 identifications (2–3 minutes), meant to be repeated several times a day; the course page shows sessions done today.
- **Gates** (computed, §5.1):
  - 100% on all active chords;
  - at least 14 days since the last chord was added.

  The course page shows "next chord available on <date>".

### 4.3 `sing_chord_tones`: hearing inside the chord (Beato)

- A chord or cluster plays: 3–6 notes, from a voicing list or generated (triads, sevenths, jazz voicings, clusters). Then you sing the notes **bottom to top**, unaccompanied.
- **Variants:**
  - `all_tones`: sing every note;
  - `missing_tone`: a triad with the third or fifth left out plays, and you sing the missing note;
  - `name_after`: after singing, name the pitch classes, using the `pitch_id` answer pad with several selections.
- **Grading:**
  - sung notes are segmented and aligned in order (untimed: the existing alignment with onset cost off);
  - each note is graded within ±25 cents of its target in `any_octave` mode, or `absolute` if the block asks;
  - naming is graded per class.
- No reference tone is given before singing; the chord is the only stimulus. When the chord's pitches are known by name (a labelled exposure), the next unassisted block's scrubber rule (§3.2) applies.

### 4.4 Screening battery

These are short tasks whose results form an **aptitude profile** (§5.6), which the coach uses to choose a route (§7). They are assessments: no feedback, holdout settings and a scrubber first.

- **`pitch_memory_match` (implicit note memory, Van Hedger):**
  - Grid: 27 sine tones from 471.58 Hz to 547.99 Hz (A♯4 + 20 cents to C♯5 − 20 cents), about 10 cents per step.
  - A target from the 10 odd steps 5–23 plays (1 s), then a mask (default 2 s noise), then you recreate it, starting from step 1, 3, 25 or 27.
  - Inner arrows (or ←/→) move one step, outer arrows (Shift+←/→) two steps, and each move plays the current tone. Enter submits.
  - Graded by absolute error in steps and cents. The research's benchmark is a mean of 2.75 steps (≈ 27.5 cents) or better for later successful adult learners, versus 2.44 for lifelong possessors.
- **`tone_n_back`:**
  - a stream of tones (or spoken letters if speech samples are added) where you press when the current item matches the one n back (2-back, 3-back);
  - graded by d′.
  - Research figures for successful learners: 4.05 (2-back) and 3.52 (3-back), against 3.37 and 2.27 for students.
- **`span`:**
  - hear a sequence, then repeat it, adaptively lengthened (two errors at a length ends the task);
  - spoken digits need recorded digit samples (decision §9);
  - a **tone-sequence span** (sing or play back on the pad) works without them.
  - Research figures: 8.83 digits for successful learners, 8.1 for non-AP musicians, 10.0 for lifelong possessors.

---

## 5. Tracking and reports

### 5.1 Per-class mastery and gates

- **`get_class_mastery`** (MCP and the AP page): for each pitch class, recent correct-in-time accuracy (§3.5) on unassisted trials, split by:
  - training vs holdout;
  - octave band;
  - timbre group.

  The ≥ 90% criterion is marked per class, together with the number of classes at or above it (the research's main outcome, e.g. "7.08 classes mastered").
- **Gates.** A gate is a named rule evaluated over stored blocks:

  | Gate | Rule |
  |---|---|
  | `verification_4x` (Wong) | 4 verification blocks (unassisted, no feedback, scrubbed, ≥ 3 octaves, jumps ≥ 13, deadline ≤ 2 s) at ≥ 90%, with ≥ 12 h between the 3rd and 4th |
  | `spacing` (Eguchi) | ≥ N days since the item was introduced, and 100% on all active items |
  | `consecutive_sessions` (current progressive stages) | ≥ 90% over two sessions |
  | `maintenance` | a maintenance block in the last N days (twice weekly → 4 days) |

  Gates attach to course items, extending the corpus plan's pass criteria with `gate`. `get_gate_status(course, item)` returns: met or not, the qualifying passes with times, and what remains (e.g. "pass 4 available after 07:40 tomorrow").
- **Display and enforcement.** The course page shows the status and the next eligible time. Nothing is locked by the app; the coach and course page use the status. A setting can turn "lock until eligible" on for self-study without the coach.

### 5.2 UCSF-style scoring and mean absolute deviation

- Every identification block reports, alongside exact and ±1 accuracy (which exist):
  - **UCSF-style points:** 1.0 for exact, 0.75 within ±1 semitone, as a total and a percentage;
  - **MAD** in semitones, folded to the nearest octave, so 0 is perfect and 3 is chance.
- Per-test thresholds (e.g. the UCSF "AP-1" cut-offs) are shown only as reference lines in reports, with a note that the app's test is not the UCSF test.

### 5.3 Weekly generalisation trend

- The `ap_weekly` preset (§6) is tagged `weekly_gate`. `get_ap_profile` adds a time series over those runs: accuracy, MAD and UCSF-style points, each per timbre (piano, cello, clarinet, harpsichord, square) and overall.
- Retention: when the gap between runs is long (more than 4 weeks), the report says so.

### 5.4 Detection reports

`pitch_detect` stats are d′, hit rate, false-alarm rate and median hit latency, each by target class, timbre, and octave distance from the prompt.

### 5.5 Drift detector

- **What it computes.**
  - From recent unassisted identification trials, the signed error distribution per class (the profile already has `signed_error_by_pitch_class`).
  - A **uniform shift** is flagged when most classes' mean signed error moves in the same direction by about a semitone or more, consistently across timbres and over the last few sessions, compared with the user's own earlier baseline. Random forgetting spreads errors in both directions instead.
  - Production (`sing_pitch`) is checked the same way: a uniform sharp or flat bias in sung notes across classes.
- **What it reports:** direction, size, how many classes, since when, and whether production shows the same shift.
- **Checks the app runs itself when a shift is flagged:**
  - the A4 reference setting (`a4_hz`) and any change to it in the period;
  - the output device's sample rate against the engine's render rate (a mismatch shifts everything; 44.1 vs 48 kHz is about 1.5 semitones);
  - the SoundFont or tuning changes in the period.
- Beyond that, the report states the shift neutrally. The skill history is not rewritten, and the coach and user decide what it means.

### 5.6 Aptitude profile

- **`get_aptitude_profile`:**
  - INM mean error in steps and cents, n-back d′ (2 and 3) and span;
  - each against the research benchmarks quoted in §4.4;
  - with the date of each run, so it can be repeated later.
- The coach uses it for routing (§7). The app does not route.

---

## 6. Presets and courses

These ship as course manifests (corpus plan, Phase A) with gates. Each is a ladder of items that point at presets.

| Course or preset | Content | Gate |
|---|---|---|
| `ap_screening` | INM (40 trials), tone 2-back and 3-back, span | — (assessment) |
| `ap_progressive` (revise existing) | Current stages, now with ≥ 3 octaves, jumps ≥ 13 and a 2 s deadline at the later stages; verification block per stage | `verification_4x` for the 12-class stage; `consecutive_sessions` earlier |
| `ap_multioctave` (Wong-style ladder) | Class subsets growing toward 12, C4–B6, piano/guitar/synth, training levels with feedback and verification levels without, scrubbed | `verification_4x` per stage. The research does not detail Wong's 288 levels; this ladder follows its rules, not its exact levels |
| `ap_speed` (Simple Speed) | `pitch_detect`, 14 streams per session, a sung prompt, all 12 classes as targets in rotation | d′ trend; no gate |
| `ap_brady` | `pitch_id` with all 12 classes over 3 octaves, `class_weights` C fading 0.40 → 0.30 → 0.20 → 0.12 → 1/12, stream pacing 5000 → 1400 ms; "keep C in mind" instruction | `consecutive_sessions` per step |
| `ap_cim` | `chord_color_id`, 14 chords added one at a time, then single-note extraction, then `pitch_id` | `spacing` (14 days, 100%) |
| `ap_contrast` (Burge/Gervain) | `pitch_id` contrasts: E♭3 vs F♯3, + A3, + the black keys; custom label set; "no humming" instruction | `consecutive_sessions` |
| `ap_errorless` | Any of the above with `feedback: errorless` and `requeue_misses` | as the source course |
| `ap_inner_voices` | `sing_chord_tones`: triads → sevenths → clusters; missing-tone variant | `consecutive_sessions` |
| `ap_weekly` | 120 trials, 24 each of piano, cello, clarinet, harpsichord, square; no feedback; scrubbed first | — (assessment, tagged `weekly_gate`) |
| `ap_ucsf_style` | 36 piano + 36 sine, no feedback, scrubbed, UCSF-style scoring | — (assessment) |
| `ap_maintenance` | 5 minutes, mixed 12-class identification across octaves and timbres | `maintenance` |
| `ap_assessment` (existing) | Unchanged, plus a glissando pre-mask, UCSF-style points and MAD | — |

Existing presets keep working. The `ap_progressive` stages gain the safeguards as new fields with the old behaviour as default, so older sessions still parse.

---

## 7. Coach (MCP)

- **New read tools:**
  - `get_class_mastery`, `get_gate_status`, `get_aptitude_profile`;
  - `get_detection_stats` (or `get_stats` grouping on `pitch_detect` metrics);
  - `get_drift_report`;
  - `get_anchors` (exists) reports clips and coverage.
- **New write tools:**
  - `set_label_set`;
  - `save_anchor` with a clip;
  - session specs with the new params and kinds.
- **Server instructions gain the research's routing** as guidance, not app logic:
  - Screen first.
  - Near-benchmark working memory (INM ≤ 2.75 steps, 3-back d′ ≥ 3.5) → the multi-octave ladder plus Simple Speed.
  - Otherwise → CIM or Brady first, with custom labels and the errorless mode, before the multi-octave ladder.
  - Always scrub before unassisted checks.
  - Use verification passes on separate days, and keep maintenance going once classes are mastered.
  - Breathing, sleep and the other excluded topics are not the app's business.

---

## 8. Phases

Sizes as in the corpus plan: S a few days, M one to two weeks.

### AP-1: safeguards and tracking (M)

- `min_jump_semitones`, `class_weights`, `pacing: stream`, `in_time` (§3.1, §3.5).
- Shepard–Risset glissando mask, block `pre_mask` and the scrub rule (§3.2).
- UCSF-style points and MAD, the weekly trend, class mastery and gate evaluation (§5.1–§5.3).
- Drift detector (§5.5).
- Presets: revised `ap_progressive`, `ap_multioctave`, `ap_brady`, `ap_weekly`, `ap_ucsf_style`, `ap_maintenance`.

**Exit:**

- a verification block never has two consecutive tones within 12 semitones;
- the glissando plays before it;
- `get_gate_status` reports passes and the overnight rule correctly on fixture data;
- a synthetic uniform +1 shift is flagged and a random error pattern is not.

### AP-2: labels, errorless mode, detection (M)

- Custom label sets and `ap_contrast` (§3.3).
- Errorless feedback and `requeue_misses` (§3.4).
- `pitch_detect` with class, note and anchor targets; `ap_speed` (§4.1).

**Exit:** a Simple Speed session runs with sung prompts and reports d′ and latency; an errorless block shows no error marks while storing correct results.

### AP-3: chords, inner voices, screening (M)

- `chord_color_id` with the 14-chord course and single-note extraction (§4.2).
- `sing_chord_tones` (§4.3).
- `pitch_memory_match`, `tone_n_back`, tone span and the aptitude profile (§4.4, §5.6).

**Exit:**

- the CIM course runs with colour buttons and shows its 14-day gate;
- an inner-voice trial grades each sung tone;
- the screening battery produces a profile with benchmark comparisons.

### AP-4: anchor clips (S)

- Anchor clips from audio files, the jukebox coverage report, and the hot/cold scan (§3.6).
- Automatic clips from song packages come with the deferred songs work.

### Where these fit with the other plan

- AP-1 needs the course manifests and gates from the corpus plan's Phase A, so they belong together: Phase A adds course gates and AP-1 uses them.
- AP-2 and AP-3 can follow Phase A and run alongside the notation work (Phase B); they share nothing with it.

---

## 9. Decisions needed

1. **Order:** AP-1 alongside courses Phase A, and AP-2/AP-3 before or after the notation work (Phase B)?
2. **Gate enforcement:** show only (the coach decides), or offer "lock until eligible" for self-study?
3. **Span task:** tone-sequence span only, or also spoken digits, which need recorded digit and letter samples bundled with the app?
4. **Prompt voice for `pitch_detect`:** GM choir "aahs" for now; record your own voice for the twelve classes later (the `own_voice` timbre is in the plan but not built)?
5. **Errorless mode scope:** only for the early stages and CIM, or available on every identification block?

---

## 10. Tests to automate

- **Sequencing:** 10,000 generated trials at three octaves with a 13-semitone minimum never violate it, or record `constraint_relaxed`; class weights match their targets within 2% over 10,000 draws.
- **Glissando:** the rendered scrubber is continuous (no clicks at buffer joins) and descends for its whole length; no partial goes above or below the envelope's audible bounds.
- **Detection:** d′ matches hand-computed values on fixture responses, including the all-hits and no-false-alarm cases (log-linear correction).
- **INM:** step and cent errors match fixtures; start positions are always outside the target range.
- **Gates:** fixture histories pass and fail `verification_4x`, `spacing` and `maintenance` exactly at the boundaries (11 h 59 m vs 12 h).
- **Scoring:** UCSF-style points and folded MAD match fixtures; MAD of uniformly random answers is ≈ 3.
- **Drift:** a synthetic uniform +1 semitone shift across all classes is flagged; the same number of errors in random directions is not; an `a4_hz` change in the period is reported.
- **Errorless:** trials are stored with the true result while the feedback view has no error state.
