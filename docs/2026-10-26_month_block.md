# The month block: fixed routine, what gets measured, and what counts as progress

**For:** the coach agent and the user.
**Decided:** 2026-10-26, after AP-3 landed (`9a6ecef`) and note colours moved to four octaves from level 1 (`7fc1ff5`).
**Purpose:** stop incremental tuning and produce one clean block of daily data that can actually be judged. No parameter changes inside the block.

## 1. Why a block rather than more changes

The last four days produced a dozen design decisions and almost no learning data: four cold checks at 41.7%, 20.8%, 50% and 8.3% across different tasks, none comparable. Every change reset the comparison. The literature's own timelines — Wong 21.4 h mean to move 13.9% → 31.7%, Van Hedger 32 h over 8 weeks, Eguchi 75–150 h — mean that session-to-session tuning cannot be evaluated at all. A month of fixed conditions can.

## 2. The routine

- **Today's set, once a day**, on headphones, in a quiet room, normal volume.
- **First thing in the day** when a test is due, before any named note: the benchmark (every 14 days), the weekly test (every 7), the transfer and UCSF-style tests (every 28).
- **Aptitude screening once**, early in the block, for the working-memory band that §7 of the literature review routes on. This is the only measurement that changes the curriculum rather than describing it.
- Roughly 25 minutes on ordinary days, up to 45 when a test is due.

## 3. The channel

- **Note colours** carry the AP binding: Taneda labels, octaves 3–6 from level 1, `Other` present, errorless feedback with misses re-queued. The jump rule is the course's existing one (≥13 semitones, two octaves apart); no additional semitone exclusion, since none of the literature supports one.
- **Naming** stays as alignment and transfer checking only. Its letters are what the benchmark measures; it is not the training channel.
- **Contrast** stays as a drill. It is a two-way discrimination task whose cold check inverted completely, so its scores are not progress.
- **No changes inside the block.** No new courses, no swapped label sets, no level jumps, no retuned blocks. If something looks broken, note it and keep going.

## 4. What gets read, and when

Nothing is read from a single session. Weekly, at most:

| Measure | Where | Why |
|---|---|---|
| Benchmark accuracy | `get_ap_profile` → benchmark | the arbiter: letters, twelve uniform classes, four octaves |
| In-time % (2 s) | benchmark and cold checks | automaticity, moves before accuracy does |
| Median correct RT | same | retrieval versus deliberation |
| MAD | same | narrows as categories form |
| Confusion matrix | `get_confusion_matrix` | whether errors are narrowing into fewer classes |
| Classes with any signal | `get_ap_dashboard` | how many notes are above chance at all |
| The quality question | your own answer | for C, E, A♭: did the note arrive as a quality today, yes or no |

The quality question is the one thing the app cannot record and the thing Burge's method is actually about. One line a day is enough.

## 5. What counts as progress at roughly 30 days

Set in advance so the result cannot be reinterpreted afterwards.

**Memory-driven performance** means: the answer arrives by retrieval rather than computation. Concretely — median correct RT under about 1 s, no collapse when feedback is removed, and accuracy that holds across octaves and instruments rather than only in one register.

**The primary test** is the benchmark, one or two runs in the block. The baseline is 15.0% (95% range 8.1–26.1%) against an 8.3% floor. Per `docs/AP_TESTING.md` §5, a change is only real if `change_from_previous.beyond_noise` says so — roughly 16 points at these trial counts, so around 31% or better.

**Leading indicators**, which should move first: in-time %, median correct RT, MAD, and the number of classes showing any above-chance signal.

**The honest caveat about dose.** At 25 minutes a day, 30 days is about 12 hours. Wong's completers averaged 21.4 hours and Van Hedger's 32. So the block is roughly half a dose by the literature's own accounting, and a null benchmark result at day 30 would mean *under-dosed*, not *impossible*. That is why the leading indicators decide the next step and the benchmark only confirms it.

## 6. The stop rule

If at the end of the block the colour check is still at chance (≈33% for three colours), the benchmark is unchanged, and RT is flat, then the channel is not working — change the channel, not the minutes. Candidates, in the order the literature supports them: chord colours as the primary binding route, then an over-exposed anchor, then a label vocabulary change.

Conversely, if the leading indicators moved but the benchmark did not, the learning is bound to the colour labels. That is the transfer question the benchmark exists to answer, and it points at adding letter alignment rather than abandoning the colour work.

## 7. State of the build when this was written

`7fc1ff5` and `9a6ecef` are on `origin/main`, but `target/release/singing-trainer.exe` predates both (last built 2026-10-25 20:53). The running app therefore does **not** yet have the aptitude screening, the production course, or the four-octave colour levels — level 1 still reports `octaves: [4]`. Rebuild with `cargo build --release` and restart before starting the block, or the block will measure the old design.
