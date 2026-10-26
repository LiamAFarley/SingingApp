# Singing diagnostics: finding where pitch goes wrong

**For:** the coach agent. Read this before advising on sung accuracy.
**What it is:** a map from the app's measurements to the abilities behind singing in tune, and
how to tell them apart. Background reading on voice pedagogy and auditory imagery lives in the
reference library outside this repository. Look things up there when a question needs it. This
file only covers what the app measures and how to read it.

## The principle

Singing a pitch accurately depends on several abilities that can fail separately. Measure each
one on its own, never as one combined score, and change one thing at a time.

| Ability | Question | Where the app measures it |
| --- | --- | --- |
| Perception | Can the user tell the pitches apart? | `rp_compare`, `rp_intervals`, `interval_id`; the aptitude profile |
| Pitch memory | Is the pitch still there after silence or interference? | `rp_match` "Hold the note" and "Through other notes"; `n_back`; `pattern_memory` |
| Active imagery | Can the user move a pitch in their head, with no voice? | `rp_walk` (silent pitch walk); `melodic_updating`; silent bars in `rp_inner` |
| Vocal onset | Does the first attempt land? | `attack_cents` on every single sung note; `rp_match` "First attempt" grades it |
| Correction | Does the note move to the target, and with what help? | `correction_cents`, `settling_ms`, compared across conditions (below) |
| Steady production | Does a held note stay put? | `sustain`: `drift_cents_per_s`, `stability_cents`, `vibrato` |
| Self-monitoring | Does the user hear their own error? | `self_judge` blocks: `judgement_correct`, `judged_actual` |
| Tonal sense | Can the user find and keep a key? | `resting_tone`, `scale_degree_id`, singback |

Do not read a poorly sung note as poor audiation, or a well-named note as good singing. Find the
link that breaks.

## What every single sung note stores

`pitch_match`, `sustain` and `sing_pitch` trials keep the note's own metrics in `metrics`:

- `attack_cents`: the first reliable pitch, the median 40–140 ms after the voice starts. It sits
  past the onset transient and before most slides. The first 50 ms is not used, because the
  detector is unreliable there.
- `centre_cents`: where the note settled (vibrato excluded).
- `correction_cents`: `|attack| − |centre|`, how far the note moved toward the target after the
  attack. Large and positive means a scoop or slide into place; negative means it moved away.
- `settling_ms`: time from the onset until the note stayed within the stable band (25 cents for
  400 ms). Missing when it never settled.
- `drift_cents_per_s`, `stability_cents` (wavering left after removing the drift), `vibrato`.
- `cents_error`: whichever error the block grades (see `grade_on`). `error_semitones` is that
  error rounded: ±12 is an octave, 7 or −5 a fifth, 5 or −7 a fourth.
- Multi-note sung trials keep `attack_cents` per note in `metrics.notes`.

Note segmentation used to split off a long scoop as a separate note. A single-note trial now
joins any slide that runs straight into the note, so the attack is the true first attempt.

Session reports give per block: `median_abs_attack_cents`, `median_correction_cents`,
`median_settling_ms` and `judgement_accuracy`.

Grading options (block level):

- `"grading": {"grade_on": "attack"}` marks the note on its attack. The default is the settled
  centre, where a scoop that ends in tune counts as right.
- `"self_judge": true`: after each single sung note, before any result, the user answers flat,
  in tune or sharp. The answer is scored against `cents_error` with the in-tune tolerance
  (20 cents by default). Chance is about a third, but base rates vary, so compare
  `judgement_correct` with how often the user's notes actually fall in each band.
  It is skipped on `sing_pitch` trials that use `hold_and_compare`, because the target has
  already sounded by then.

`pitch_match` options:

- `delay_ms`: silence between the reference and singing, up to 30 s. The lane hides the target.
- `distractor_tones`: random notes within a fifth, never the target, played during the delay to
  interfere with the held pitch (Deutsch's paradigm). This slows the exercise down on purpose;
  the user agreed to it for this exercise only. Do not add sounds between trials anywhere else.
- `mode: "unison"` sings along with the reference, so the reference sounds during production.

## The workshops

**`rp_match` Pitch matching.** Levels in order:

1. First attempt, graded on the attack: piano; strings and flute; bright timbres (saw, square,
   oboe); any timbre.
2. Hold the note: 2, 5, 10 and 20 s of silence.
3. Through other notes: 2 notes in 4 s, 4 in 5 s, 6 in 6 s, 8 in 10 s.
4. Judge it first: piano; on the attack; after 5 s; bright timbres.
5. Long notes (`sustain`): 5 s over a drone, 5 s alone, 10 s over a drone, 10 s alone.

**`rp_walk` Silent pitch walk**, after the Pitch Imagery Arrow Task (Gelding, Thompson and
Johnson 2015).

- The key and a starting note sound. Arrows move the note up or down the scale, the first ones
  with their notes and the rest in silence. A probe note then plays: is it where the walk ended?
- Nothing is sung. Group `get_stats` by `stimulus.silent_steps` to get accuracy against walk
  length. The longest length held reliably at 80% or more is the user's imagery span.
- Levels: 2, 3, 4, 5–6 and 6–8 silent steps, then thirds, nothing heard, semitone probes, minor,
  and a new key every walk.

## Reading the patterns

| Pattern in the data | Likely bottleneck | Next step |
| --- | --- | --- |
| Good interval and comparison scores, high `attack_cents` | Turning the heard pitch into the vocal onset | `rp_match` First attempt; delayed feedback; try preparation strategies (below) |
| High `attack_cents`, large positive `correction_cents`, short `settling_ms` | The first attempt is approximate and the singer steers in by ear | Grade on the attack; check whether the correction survives without a sounding reference (next table) |
| Accurate with no delay, worse as `delay_ms` grows | Pitch memory | Hold the note, then through other notes; `n_back`, `pattern_memory` |
| Accurate singing, short `rp_walk` span | Imagery apart from the voice | `rp_walk`, `melodic_updating`, silent bars; no extra vocal work needed |
| Bright timbres give `error_semitones` of ±12, 7 or −5 that piano does not | Pitch heard through the timbre's overtones (octave or fifth confusion) | Bright-timbre first attempts with results after each trial; harmonic P5 against P8 in `rp_intervals` |
| `stability_cents` high with and without a drone | Production, not memory | Not an ear-training problem. Shorter holds, comfortable range, and voice technique (see `physical_vocal_practice_reference.md`). Do not diagnose the voice. |
| `drift_cents_per_s` large alone, small over a drone | The held pitch fades from memory | Sustain alone, then hold the note in silence |
| Good with the live trace, poor without it | Dependence on the display | Fade feedback: continuous, then after the trial, then none |
| `judgement_accuracy` low, or confidently wrong | Self-monitoring | Judge it first, and play back recordings |

Contour right but the whole line shifted is a starting-pitch or tonic problem, not an interval
problem: compare singback starting error with interval error.

## The user's own account (2026-10-26): hypotheses to test

These are the user's own descriptions, recorded so you can test them. Treat them as starting
hypotheses, not findings.

1. **"I scoop and slide into place aggressively when there's a reference note."** The user reads
   this as an inexact internal pitch that gets corrected mainly against the external reference
   once the two voices sound together, rather than an exact inner pitch that the voice fails to
   produce.
   - Test: compare `attack_cents` and `correction_cents` across three conditions on the same
     timbre and range: `pitch_match` mode `unison` (the reference sounds during singing),
     `after` (it stops first) and `after` with `delay_ms` 5000.
   - The hypothesis predicts a similar attack error in all three, a large correction toward the
     target in unison, and a small correction once the reference has stopped (the note settles
     near where it started).
   - If the correction stays large with no reference sounding, the correction is internal and
     the onset is the weak link.
   - Also look at `sing_pitch` with `hold_and_compare`: the graded window comes before the
     target fades in.
2. **Unusual timbres and overtone-rich notes; mixing up 1, 5 and 8 (unison, fifth, octave).**
   - Test: `rp_match` First attempt, piano against bright timbres, then group by
     `stimulus.timbre` and `metrics.error_semitones`.
   - Octave and fifth errors only on bright timbres point to perception through the overtones.
   - Cross-check with harmonic fifths against octaves in `rp_intervals`.
3. **Long notes waver even over a drone, and drift.** The user thinks it is part memory, part
   uneven production.
   - Test: Long notes, drone against alone.
   - `drift_cents_per_s` that grows without the drone is memory.
   - `stability_cents` that stays high over the drone is production.
   - Check `vibrato` before calling regular wavering a fault.
4. **Turning complex song melodies into sung notes.**
   - Measure with singback (`rp_singback`) per-note errors, `melodic_updating`, and `rp_walk`
     span.
   - Separate contour errors from interval-size errors, and starting pitch from the rest.
   - For song material, see `song_practice_ingestion_playback_pipeline.md`.

## Preparation strategies: compare them, don't prescribe them

Offer these as cues to try, not as mechanisms. People differ, and imagery doesn't have to feel
vivid to be accurate.

- **Attack:** imagine the note's start (a piano hammer, a consonant such as "da") as well as its
  sustain.
- **Inner singing:** silently set up the vowel and the note as if singing it, with no sound.
- **Own voice:** hear the target in your own voice on one vowel before singing.
- **Scale degree:** place the note as a degree of a key you imagine.
- **Gesture or space:** a hand movement for the interval, or the note placed in the room.
- **None:** no deliberate preparation, as the baseline.

To compare two strategies:

1. Queue matched blocks of the same lesson, for example First attempt on piano, 12 trials each.
2. Put the strategy in the block `instructions` and a tag such as `strategy.inner_singing`.
3. Alternate the order across days.
4. Compare `median_abs_attack_cents` by tag (`get_stats` with `filter.tag`).
5. Keep the strategy with the better attack and the user's sense of comfort. Save the result
   with `save_coach_note`.

## Claims to leave out

Some suggestions this plan drew on stated physiology as fact. Do not repeat these to the user as
established:

- that an imagined pitch lets the motor cortex "read" exact laryngeal tension;
- that straw or [ŋ] phonation switches off the extrinsic muscles and leaves only the
  pitch-setting ones;
- that the imagined sound must be louder than the room's silence before singing;
- that the first 50 ms of a note is a reliable measure.

The self-voice advantage (Pfordresher and Mantell) is about imitating recordings of one's own
voice. It does not show that imagining your own timbre sets the larynx. The app has no
self-voice bank yet.

## Rules that still apply

- Keep sung work inside the comfortable range. With fatigue, switch to work with no microphone
  (`rp_walk`, `pattern_memory`, `n_back`).
- Test transfer before calling anything learned: new timbres, keys, delays and unfamiliar
  melodies.
- Require repeat success on another day.
- Change one variable at a time when stepping difficulty up.
