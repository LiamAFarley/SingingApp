# Note colours: split into a sine course and a curated-timbre course

**Date:** 2026-10-28
**Decision by:** Liam. Implement as two courses.

## What and why

The colour course currently draws from the training pool as it stands — **134 timbres**, essentially the whole General MIDI set plus the installed SoundFonts. At three notes with errorless feedback that pool produces trials the learner cannot work with: pads and choirs with slow attacks and broad spectra ("a wall of air with a pitch"), organs, percussion with short decay, and sampled instruments stretched up to B6 where they no longer sound like themselves.

The fix is two courses rather than one pool:

1. **`ap_note_colors_sine`** — sine only. This is the course the learner trains on. One partial, no harmonics, nothing to latch onto but the fundamental. Also removes the overtone-driven fifth/unison confusion the learner reports.
2. **`ap_note_colors`** — curated random timbres. Kept for transfer. Fewer timbres, none of the pad/percussion material, and a narrower register.

Anything not named stays as it is: class pacing from three notes toward twelve, `Other` present, errorless feedback with re-queued misses, and the existing jump rule (≥13 semitones, two octaves apart).

## Pool for `ap_note_colors` — exclude

- **Pads and choirs**, all of: `gm:52` Choir Aahs, `gm:53` Voice Oohs, `gm:54` Synth Voice, and the SoundFont vocal patches (`sf:SoloVoices`, `sf:SoftVoice`, `sf:HeavenChoir`, `sf:MaleOohs`, `sf:OperaFemale`). Slow attack, wide spectrum, and they blur register.
- **Organs**: `gm:16` Drawbar Organ, `gm:17` Percussive Organ, `gm:18` Rock Organ, `gm:20` Reed Organ, `sf:DrawbarOrgan`. Dense harmonics that mask pitch height.
- **Short-decay and unpitched percussion**: `gm:47` Timpani, `gm:12` Marimba, `gm:13` Xylophone, `gm:14` Tubular Bells, `gm:9` Glockenspiel, `gm:10` Music Box, `gm:8` Celesta, `gm:55` Orchestra Hit. Decay truncates the tone before the answer, and Orchestra Hit is a noise burst.
- **Bass-register programs** `gm:32`–`gm:39`: outside the training register and muddy.
- **Distorted and overdriven guitars** `gm:29`, `gm:30`: inharmonic.
- **Holdout timbres** must stay out of training: `gm:1`, `gm:11`, `gm:19`, `gm:42`, `gm:56`, `gm:71`, `synth:pulse`, `shepard`.

## Pool for `ap_note_colors` — use

Pitched, clear attack, stable sustain:

`gm:0` Acoustic Grand, `gm:2` Electric Grand, `gm:4` Electric Piano 1, `gm:6` Harpsichord, `gm:24` Nylon Guitar, `gm:25` Steel Guitar, `gm:40` Violin, `gm:41` Viola, `gm:73` Flute, `synth:triangle`, `synth:soft_piano`.

Eleven timbres is enough for a transfer check and few enough that no single spectrum accumulates enough exposures to be memorised on its own.

## Registers

- `ap_note_colors_sine`: octaves **3–6**. A sine stays clean and identifiable at the top.
- `ap_note_colors`: octaves **3 and 5** (C3–B5; the two-octave step rule has no legal target from octave 4). Sampled instruments above C6 stop sounding like themselves, and the learner reports not being able to tell what is happening up there.

## Transfer checking

The arbiters stay as they are: the SoundFont transfer test and the Shepard block, on their existing schedule. Add nothing. In the session report, keep per-timbre accuracy for the random course so a single timbre scoring far above the rest is visible — that would be stimulus-specific learning and the cue to act.

## One correction to an earlier note

The coach told the learner the draw was "thirteen instruments" on the basis of the distinct timbres that appeared in his trial rows. The pool itself is 134. The trial rows show what was drawn, not what was available, and the two were conflated.

## Implementation note (2026-10-28)

- `ap_note_colors_sine` and `ap_note_colors` are separate ladder courses. The sine course uses only `synth:sine`; the random course uses exactly the eleven listed timbres.
- The sine course keeps octaves 3–6. The random course uses octaves **3 and 5**, not 3, 4, 5: with `min_octave_step: 2`, a middle octave has no legal target and the generator would never use it. This keeps the C3–B5 range and the existing two-octave rule.
- `SessionReport.by_timbre` now carries per-timbre accuracy, and the Results page shows it as a table when a session used more than one timbre.
- Test: `colour_courses_use_their_planned_pools_and_registers` checks both pools, both registers, the jump rule, holdout exclusion, and that the random course draws every listed timbre.
