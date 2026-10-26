# Absolute pitch: tests and routine

**For:** the coach agent (and the user).
**Supersedes:** the `ap_assessment` preset. `docs/2026-10-24_benchmark_assessment_spec.md` and
`docs/ap_benchmark_spec_review_and_revisions.md` record how this design was reached. Where they
disagree with this file, this file is what the app does.

## 1. The short version

Training happens in **courses**, through **Today's set** on the Absolute pitch page. Four
**tests** measure progress. Each test runs on its own schedule and comes up first in Today's set
when it is due. The coach's regular job is:

| When | Tell the user | Tool to check |
| --- | --- | --- |
| Every day | Run **Today's set**, first thing, on headphones. Any due test is already at the front. | `preview_daily_set` |
| Every two weeks | The **Benchmark** (about 7 min). It comes up on its own. | `get_ap_profile` → `benchmark` |
| Every week, on the test day (Sunday unless changed) | The **Weekly test** (about 9 min). | `get_ap_profile` → `weekly` |
| Every four weeks | The **Transfer test** (about 8 min). | `get_ap_profile` → `transfer` |
| Every four weeks, on the test day | The **UCSF-style test** (about 6 min). | `get_ap_profile` → `ucsf_style` |
| Twice a week, once note naming is complete | **Maintenance** (5 min, a training block, not a test). | `get_ap_dashboard` |
| Once to start, then every four weeks | The **Aptitude screening** (about 9 min): note memory, tone 2- and 3-back, span. Not an AP measure; it weights the plan (§3.5). | `get_aptitude_profile` |
| Every day no test is due | **Your C from memory**: one note sung before anything is heard, at the start of Today's set (Settings can turn it off). | `get_ap_dashboard` → `anchor` |

Tests are not graded pass or fail. They are measurements, and they only mean something across
several runs (§5).

## 2. Rules for every test

The app enforces most of these. The coach should still say them, because the user can undo them.

1. **First thing in the day, before any named note.** Named notes are any feedback that says or
   shows a note's name, sight singing, pairing, or a song in a known key. If the user has heard one
   today, the test's due note says so and suggests tomorrow morning. A run that started within
   6 hours of a named note is flagged in its results.
2. **Headphones, normal volume, quiet room.** No instrument, tuner, phone app or song in the head
   beforehand.
3. **Do not hum, sing along, or work a note out from the one before.** Answer from the sound.
4. **5 seconds per note, no replay.** Late answers count as wrong and are logged as timeouts.
   The 5 s limit is long enough for a slow but real recognition, and too short to work a note out
   step by step from another.
5. **No feedback until the end.** That is deliberate.
6. Each test starts with a 20-second endlessly falling Shepard–Risset tone. It clears pitch memory
   and is not a test item. There is no mask between notes, except on the Shepard tones (§3.3).

What the app does to keep a test honest:

- **The octave says nothing about the note.** Consecutive notes are at least 13 semitones apart.
  In the four-octave register, each note is also at least two octaves from the last *by octave
  number*, chosen without looking at the note. With the 13-semitone rule alone, the octave a note
  landed in gave away whether it was above or below the last note's name. The step rule removes
  that leak.
- **The notes come up evenly, but not predictably.** Each block plays each of the twelve notes
  about equally often, and a note can come straight back in another octave, so the next answer
  cannot be guessed from the last.
- **Instruments stay in tune.** Extra SoundFont instruments are only used inside the range they
  were measured in tune (docs/SOUNDFONTS.md). A bank that is not installed is left out.

## 3. The four tests

### 3.1 Benchmark (`ap_benchmark`): every 14 days

The repeatable core measure. If only one number is tracked, it comes from this test.

| Block | What | Why |
| --- | --- | --- |
| 1. Sing from memory | 12 notes, each asked by name. Nothing is played, there is no feedback, any octave counts. | Absolute pitch production, measured before a single reference tone has been heard today. It runs first for that reason. |
| 2. Name the piano note | 60 notes, General MIDI piano (`gm:0`), C3–B6, two-octave steps. | Naming on a trained instrument in the trained register. This is the headline number. |

Piano is one of the training instruments, and C3–B6 is the training register, so this measures
the trained skill. Transfer is measured separately (§3.3).

### 3.2 Weekly test (`ap_weekly`): every 7 days, on the test day

Van Hedger's weekly generalisation test: 120 notes, 24 each on piano, cello, clarinet, harpsichord
and square wave. It stays at C3–B5 because cello and clarinet leave their real range outside it.
With three octaves the two-octave step cannot be used, so only the 13-semitone rule applies. It
reports per-instrument accuracy.

### 3.3 Transfer test (`ap_transfer`): every 28 days

| Block | What | Why |
| --- | --- | --- |
| 1. Untrained instruments | 60 notes on instruments training never uses: French horn, trombone, cello, violin, drawbar organ, female voice (the extra SoundFonts), plus General MIDI trumpet and vibraphone. | Whether naming carries over to sounds never trained. Without the SoundFonts installed, only the two General MIDI instruments play. |
| 2. Shepard tones | 24 tones that have a note name but no height. | The **height check**. If piano naming climbs while Shepard stays near 8%, the user is learning how high notes sound, not what they are. |

Shepard tones have no height, so a jump rule cannot separate them. Two Shepard tones in a row are
heard as a short interval, which could make working one out from the last easy. With no results
until the end of the block, though, no tone is ever labelled, so there is nothing to work from.
Nothing plays between notes.

### 3.4 UCSF-style test (`ap_ucsf_style`): every 28 days, on the test day

36 piano and 36 sine tones over five octaves, C2–B6, two-octave steps, scored as the UCSF test is
(1 point exact, 0.75 a semitone off). It allows comparison with the published cutoffs below. It
is not the UCSF test itself.

### 3.5 Aptitude screening (`ap_screening`): once to start, then every 28 days

Not an absolute pitch measure: it measures the auditory memory that predicts how adult absolute
pitch training goes (Van Hedger et al. 2015, 2019; `AP_Trainer_Literature_search.md` §1, §7). No
feedback, about 9 minutes, in this order:

| Part | What happens | Result |
|---|---|---|
| Note memory (`pitch_memory_match`, 20 searches) | A sine tone from a 27-step ladder (471.58–547.99 Hz, 10 cents a step; targets the odd steps 5–23) plays, then a hiss. The learner finds it again from step 1, 3, 25 or 27 with the arrows (Shift for two steps), hearing each step. The position is never shown. | Mean absolute error in steps and cents; signed error (positive is high). |
| 2-back and 3-back (`n_back`, `stimuli: tones`) | 30 tones each, one every 2 s, from all twelve notes over C3–B5, no key, drone or cadence. Space when a tone is the same as the one 2 (or 3) back. Matches are the same tone; other tones differ in pitch class from it. | d′ per stream. |
| Span (`span`) | Sequences of 3, then 4, … items, two at each length; enter each back in order. Spoken digits when `speech/digits/1–9` exist, otherwise a 9-key tone pad (C4–D5) played back in order. Stops when both sequences at one length are wrong. | Longest length recalled. |

`get_aptitude_profile` gives every run with its date, the mode that ran, and the reference lines:

| Measure | Adults who later reached AP | Comparison |
|---|---|---|
| Note memory error | 2.75 steps, 27.5 cents (SE 0.30) | Lifelong AP 2.44 steps (SE 0.19) |
| 2-back d′ | 4.05 (SE 0.17) | Undergraduates 3.37 |
| 3-back d′ | 3.52 (SE 0.32) | Undergraduates 2.27 |
| Digit span | 8.83 (SE 0.65) | Non-AP musicians 8.1, lifelong AP 10.0 |

`capacity_band` restates §7's thresholds (note memory ≤ 27.5 cents and 3-back d′ ≥ 3.5) for the
latest runs: `near_ceiling`, `mixed` or `lower`. The coach routes from the raw numbers; the band
does not decide anything and the screening gates nothing. Read with care:

- the span reference lines are for spoken digits; a tone-pad span is a different task;
- searches answered without moving say nothing about memory (the profile notes them);
- one run is one sample: wait for the second before reading a change.

### Not tests

- **Course checks** (blocks marked "Check: no feedback" inside each level) are training gates.
  They decide when a level passes. They are not the progress measure.
- **Maintenance** is training.
- The old **`ap_assessment`** runs used holdout piano and random-tone masks. They are not
  comparable and `get_ap_profile` leaves them out; it only counts them (`legacy_assessments`).
- Key finding without a reference (`ap_key_finding` preset) is a separate, optional ability. It is
  not part of the absolute pitch measure.

## 4. Running a test now

Tests come up by themselves. To run one off-schedule (before and after a training push, or after a
break), the user clicks **Take it now** on its card on the Absolute pitch page. The coach can
instead queue its preset: `get_preset` with the id (`ap_benchmark`, `ap_weekly`, `ap_transfer`,
`ap_ucsf_style`), then `queue_session` with that spec. The trials carry the course tags, so they
count on the dashboard however the test was started. Do not build custom absolute pitch
assessments; they would not be comparable.

## 5. Reading results (`get_ap_profile`)

Each test section has `latest`, `previous`, `change_from_previous`, `trend_per_30_days` and
`history`. Each run has:

| Field | Meaning | How to use it |
| --- | --- | --- |
| `accuracy`, `accuracy_low`–`accuracy_high` | Correct within 5 s, timeouts wrong, with its 95% range. Chance is 8.3%. | The headline. Quote the range, not just the number. |
| `in_time` | Correct within 2 s **of the tone starting** (1 s after it ends). | Automaticity. Wong's adult learners reached ≥90% within about 2 s on mastered notes. |
| `timeouts` | Answers not given in 5 s. | Many timeouts with a decent accuracy means slow, effortful naming. |
| `ucsf_points` | 1 exact, 0.75 a semitone off, 0 otherwise, as a share of the maximum. | Moves sooner than exact accuracy, because near misses count. |
| `mad` | Mean error in semitones over answered notes: 0 perfect, about 3 guessing. | Lower is better. Also moves sooner than accuracy. |
| `mean_signed_error` | Average lean, sharp (+) or flat (−). | See "lean" below. |
| `median_correct_rt_ms` | Median time of *correct* answers only. | Speed of real recognitions, not of guesses. |
| `by_timbre`, `shepard` | Per instrument, and Shepard. | Transfer and the height check. |
| `sung` | Accuracy, median cents off, `chaining_ratio`, `chaining_likely`. | See "sung notes" below. |
| `named_note_minutes_before` | How long after a named note the run started. | Under 6 h: treat the run as possibly inflated. |
| `notes` | Plain-language cautions. | Relay them. |

The benchmark section also has the confusion matrix, signed error per note and per octave.

### Noise: what counts as a real change

60 trials measure accuracy to about ±11 points. The difference between two runs is therefore
uncertain by about ±16 points. Wong's adult learners gained 18 points on average over the whole
eight-week course (13.9% to 31.7%). So:

- **Never call one run better or worse than the last** unless `change_from_previous.beyond_noise`
  is true. Otherwise say it is within run-to-run noise.
- Judge progress from **`trend_per_30_days`** (needs three runs) and from three or more runs in a
  row, giving more weight to `ucsf_points` and `mad` than to exact accuracy.
- A run whose `accuracy_low` is at or below 8.3% is **not clearly above chance**. That is normal
  at the start.

### Cautions and what to do

| Sign | Likely meaning | Action |
| --- | --- | --- |
| "Answers lean … semitones high/low on almost every note" | Audio setup, not the ear. A 44.1 vs 48 kHz sample-rate mismatch shifts everything by about 1.5 semitones. | Ask the user to check the Scarlett and Windows sample rates match the app, then retest. Do not change training on it. |
| `chaining_likely` on sung notes | The user sang each note relative to the previous one. The steps between notes are right while the notes themselves are off. | Remind them to reset between notes (breathe, wait a moment) and to sing each note from memory alone. Discount that run's sung accuracy. |
| Piano accuracy up, `shepard` near 8% | Learning height, not note names. | More Shepard and chord-colour work (`ap_chord_colors`), and use the full four octaves. |
| Piano good, untrained instruments much lower | Instrument-specific learning. | More instrument variety in training (the training pool already mixes General MIDI and SoundFont instruments). |
| Named note heard before the run | Possibly inflated. | Note it and do not compare it with clean runs. Remind the user that tests go first in the day. |
| Many timeouts | Slow recognition. | Speed work (`ap_speed`), and shorter deadlines in the naming course as levels progress. |

## 6. Reference points

- Chance: 8.3% exact, MAD about 3 semitones.
- UCSF test cutoffs, from the original test and not exact for this version: AP-1 ≥ 24.5 points on
  sine, AP-4 ≥ 27.5 on piano, out of 36 per part. In this app's share terms, about 68% and 76%.
  Van Hedger's adult trainees started around 9–10 points.
- Van Hedger's weekly test: 46% on piano after week 1, 55% by week 7.
- Wong 2025 (adults, ~21 h): 13.9% → 31.7% on the trained timbre, mean error 2.62 → 1.50
  semitones, about 7 notes at ≥90% within ~2 s on average.
- The dashboard's per-note mastery (≥90% within 2 s over the last 40 trials, at least 20) comes
  from training as well as tests. Note mastery belongs to training. A 60-trial test is far too
  small to rate single notes: about 5 trials per note.

## 7. Register and instruments

- The training register for naming, colours and contrast is **C3–B6**, with two-octave steps. The
  benchmark uses the same register.
- **Training instruments:** General MIDI piano, strings, flute, nylon guitar, choir, a triangle
  wave, and the installed SoundFonts upright piano, tenor sax, classical guitar, viola, concert harp,
  male voice (Duncan) and soft female voice.
- **Never trained** (tests only): the General MIDI holdouts, the other SoundFonts (French horn,
  trombone, cello, violin, flute, drawbar organ, male oohs, operatic and "Dawn" female voices,
  "na" and bass male voices, choir), the pulse wave and Shepard tones.
- SoundFont instruments are kept to the range they were measured in tune, mostly C3–B5 (cello and
  the bass voice to C5). In the four-octave register they skip octave 6 and the generator plays
  other instruments there.
