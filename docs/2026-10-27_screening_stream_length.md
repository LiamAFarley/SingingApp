# Screening streams are too short to compare with the published d′ figures

**Date:** 2026-10-27
**For:** the coding agent. Small change, one preset.

## The problem

The AP screening runs one stream each for 2-back and 3-back. A 30-trial stream at a 25% target rate gives 9 matches and 21 non-matches, and a d′ estimated from 9 signal trials is both biased down and very noisy.

Consequences:

- A flawless 30-trial stream cannot report above about 3.65, so a score of 4.05 — the Van Hedger figure for adults who later reached AP thresholds — is unreachable at that length no matter how good the listener is.
- The estimate is biased low. Simulating an observer with a true d′ of 4.0 gives a mean estimate of 3.36 on 9 targets.

Simulated true d′ = 4.0 (criterion at half d′, log-linear correction, 4000 runs per row):

| Matches | Non-matches | Mean estimate | SD | Runs scoring ≥ 4.05 |
|---|---|---|---|---|
| 9 | 27 | 3.36 | ±0.39 | 0.0% |
| 20 | 60 | 3.73 | ±0.42 | 15.8% |
| 40 | 120 | 3.90 | ±0.40 | 33.0% |
| 80 | 240 | 3.97 | ±0.34 | 37.8% |
| 160 | 480 | 3.99 | ±0.26 | 36.5% |
| 320 | 960 | 3.99 | ±0.18 | 35.8% |

The bias is about −0.6 at the current length and closes by about 40–80 matches. The spread only tightens noticeably past 80.

## What to change

- Raise each screening stream to **at least 40 matches** (160 trials at a 25% target rate), and prefer 80 matches (320 trials) for the annual or first run.
- Keep 2-back and 3-back as separate streams, each at that length.
- Pooled counts are fine: if a session runs three short streams of the same n, compute d′ from the summed hits, misses, false alarms and correct rejections rather than averaging three d′ values.
- Record in the aptitude profile how many trials the d′ was computed from, since the number is not comparable across lengths without it.

At 2 s between tones, 160 trials is about 5 minutes per stream; 320 trials is about 11.

## Why it matters

`capacity_band` is derived from these numbers, and the band routes the learner between the multi-octave naming curriculum and the colours/chords/anchor branch. A downward-biased estimate pushes the band toward "lower" regardless of ability. The learner's 2-back came out at 3.13 on 9 matches, which at that length corresponds to a true d′ of roughly 3.7 — above the undergraduate figure of 3.37 — but the app has no way to say that today.

## Do not change

The INM task and the span ladder are not affected: both are already multi-trial averages (20 targets, and a ladder that grows until two errors at a length). This note is only about the n-back streams.
