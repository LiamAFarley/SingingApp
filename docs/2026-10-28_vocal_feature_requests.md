# Features the vocal session needs

**Date:** 2026-10-28
**For:** the coding agent. Referenced from `2026-10-28_vocal_daily_session.md`, which cannot be run as written without most of these.

Ordered by what blocks the session, then what improves it.

## 1. Onset detection is measuring artefacts (bug)

**Symptom.** `attack_cents` reports large swings inside a ~20 ms window at the start of a note. It also misfires when the singer stops and restarts the same note, and on plosive consonants — a /d/ burst is read as a pitch excursion. The report looks like sub-perceptual nonsense because at that window length the detector is measuring the voicing onset, not a pitch.

**Cause.** The first reliable frame is taken as soon as clarity clears the threshold. In the first frames of phonation the signal is still a fricative or plosive transient plus a not-yet-periodic glottal source, so the estimated f0 moves fast and the first accepted frame is arbitrary.

**What to change.**

1. **Require a stable run before sampling.** Accept the onset only after N consecutive frames (N sized for 60–80 ms) where clarity is above threshold *and* each frame's midi is within ±50 cents of the run's median. Report `attack_cents` as the median of that run.
2. **Reject transient frames.** If spectral flatness or high-band energy is far above the value of the following steady portion, the frame contains a consonant transient — skip it rather than treating it as pitch.
3. **Re-arm after a voicing gap.** If voicing drops below threshold for more than 30 ms within the first second, discard the earlier window and treat the new voicing start as the onset. This covers stopping and restarting the same note.
4. **Store the provenance.** Add `attack_ms` (how long the window took to stabilise), `voicing_onset_ms` (where voicing actually began relative to the note start) and `attack_confidence`. Outliers can then be diagnosed instead of argued about.
5. **Report the rejects.** Per block, count trials where no stable onset was found and report the median over the stable ones, with the count. A median over five usable trials and a median over twelve are not the same measurement.

**Acceptance.** Singing a steady note with a soft /d/ onset and again with a plain vowel onset gives attack values within 20 cents of each other. Stopping and restarting the same pitch does not produce a second, larger attack error.

## 2. Block-level click

**Needed by:** sections 1.2, 1.3, 2.1–2.3, 4.2, 4.3, 5.4 of the session.

Tempo, accent on beat one, subdivision (quarter or eighth). The metronome already exists inside `pattern` and `sing_interval`; expose it as a block-level option so any exercise can run against a click, with the option to keep it audible during singing or only during the count-in.

## 3. Block-level drone

**Needed by:** sections 1, 3, 5.3.

A sustained pitch, or a chord, at a stated level, available on any block rather than only `sustain` and `n_back`. Needs: pitch selection, level in dB relative to the stimulus, and on/off per block.

## 4. Pattern playback at tempo

**Needed by:** sections 2.3, 4.2, 4.3, 5.4.

Play the target line at the stated tempo so the singer can hear it once, sing it with the guide, then sing it alone. `pitch_match` and `sing_interval` play single notes and intervals; what is missing is a short melodic pattern played as a guide track at a tempo, with the guide muted on the final pass.

## 5. Glide exercise

**Needed by:** section 5.4 in the progression, and by any siren work.

The app has no continuous-pitch exercise. Define one: a target contour over time (octave up and back over 4 beats, or a five-note scale in one glide), graded on maximum deviation in cents, mean absolute deviation, and smoothness (rate of change of the deviation). Discrete-note grading cannot score this. Without it, sirens stay off-app.

## 6. Messa di voce report

**Needed by:** sections 1.4, 4.1, 5.3.

The trace already stores per-frame level (`rms_db`) and pitch. Add a report: level contour over the hold, time to reach peak, symmetry of swell against fade, and whether pitch stayed inside a stated tolerance throughout. The failure this catches is the one that matters — a swell that also drags the pitch, or a fade that is really the support collapsing.

## 7. Staccato pulse drill

**Needed by:** sections 2.1–2.3.

A pulse train at a stated tempo where each pulse is graded separately: onset (after the fix in §1), separation between pulses, and pitch centre per pulse. Currently a sustained exercise cannot express this and `pitch_match` grades one note at a time.

## 8. Vowel as a first-class field

**Needed by:** sections 3.2–3.4, 5.5.

Sung blocks should take a vowel, show it in the prompt, and store it on the trial so results can be filtered by vowel. The session asks for the same pitch on five vowels and expects different results; without the field there is nothing to compare.

## 9. Backing track

**Needed by:** every section that runs against a click or drone, if the singer would rather have one file than two settings.

Allow a block to play an audio file from the library — a drone-plus-click mix, or a short accompaniment loop — at a stated level, and store which file was used on the trial. This is the difference between a session that can be run as written and one that needs the app configured per section.

## 10. Repeat a saved session in one action

**Needed by:** daily use.

A saved preset should be startable from the home screen without editing settings, with the option to run it on a fixed daily schedule. Currently a preset is a session spec; there is no notion of "my daily session".

## 11. Ungraded reference tracks, including for unvoiced work

**Needed by:** section 1 setups, all of section 2, and anything else used as a play-along rather than a test.

Every block type in the app grades the microphone. Unvoiced consonants have no pitch to grade, and straw setups and spoken chains do not need a score either. What those items need is a reference track: click with accent and subdivision, count-in bars, drone or pattern playback at tempo, with nothing recorded as right or wrong.

**What to change.** Add one of the following; the first is the smaller change:

1. A `graded: false` flag on `pattern`, `sustain` and `pitch_match`. When set, the block plays its reference, backing, click and drone exactly as now, shows the same lane, but records no score and writes no accuracy. The session report lists the block as reference only.
2. Rhythmic patterns without pitch: a click track that plays short-long and long-short patterns at the stated tempo with a count-in, so /s/ and /f/ work has something to follow. No microphone input.
3. Count-in bars on every sung block, so a pattern starts on a known beat rather than after an arbitrary pause.

## Notes on what already exists

- `sustain` reads centre error, drift, stability and vibrato, and has `drone`, `hold_ms`, `range`, `timbre` — sections 5.1, 5.2 and 5.3 can run on it today except for the level report.
- `pitch_match` has `delay_ms`, `distractor_tones`, `reference_ms`, `response_ms`, `timbres` and `grading.grade_on: attack` — section 2.4 runs today.
- The metronome and the trace data that §2, §3, §6 and §7 need are already in the engine; the work is exposure and reporting, not new synthesis.
