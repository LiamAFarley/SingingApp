# Benchmark assessment: what it should measure, and why the current preset fails

**Date:** 2026-10-24
**Author:** singing-coach agent (Hermes profile `singing-coach`)
**Status:** proposal, nothing implemented. Written for review by the agents that build the app.
**Scope:** replaces `crates/trainer-core/presets/ap_assessment.json` as the specification of the absolute-pitch benchmark. Does not propose changes to `courses.rs`, which already carries the updated AP guards.

---

## 0. The proposal in one page

1. **A benchmark is not a battery of near-identical drills.** It is a small set of blocks, each measuring a different construct, short enough to run on demand.
2. **Every block carries the same five guards** from the AP research: a 20 s Shepard–Risset scrub before the block, consecutive tones at least 13 semitones apart, an explicit response ceiling, out-of-set distractors during subset learning, and no feedback whatsoever.
3. **No separate speeded block.** Response time is already stored per trial. One uncapped naming block yields accuracy *and* the in-time proportion at any ceiling, post hoc. A second block with a shorter deadline re-measures the same construct while discarding late trials.
4. **No between-trial random-tone mask in an assessment.** The mask exists to stop a trial being solved by the previous trial's revealed answer. With feedback off, nothing is revealed, so the mask only adds ~1.5 s per trial and a stream of unrelated pitches.
5. **Length is bounded by statistics, not by ritual.** 46 trials detect 20% against the 8.3% chance floor; 60 gives per-trial resolution good enough to place a beginner, and no more. Repetition inside one sitting does not tighten a stable trait estimate; the literature repeats measurement *across days* (weekly gates, verification passes), not within a session.
6. **Two tiers.** Tier A is the repeatable benchmark, 72 trials, 6.7 minutes, validated. Tier B is a transfer panel (timbre, octave, chroma-without-height), 76 trials, 7.0 minutes, run before and after a training block, not every time.
7. **What the app already does well and should keep:** holdout pools, deterministic grading, per-trial `rt_ms`, post-hoc in-time computation, the UCSF-style pitch-class scoring, the confusion matrix, and the signed-error metrics.

---

## 1. What the current preset actually does, and why the run feels random and repetitive

Evidence from the live session `01a170aa-fd97-7032-a528-1c22ea312a2a` (102 trials completed at the time of writing), read through `query_trials`:

| Block | Trials | Octaves | Timbres | Deadline | Feedback |
|---|---|---|---|---|---|
| 0 "12-class identification" | 48 | 3–5 | `gm:1` only | none | none |
| 1 "Cross-octave" | 24 | 2, 6 | `gm:0`, `gm:48` | none | none |
| 2 "Cross-timbre" | 36 | 3–5 | 7 timbres incl. `shepard` | none | none |
| 3 "Speeded" | 24 | 3–5 | `gm:1`, `gm:11` | 1500 | none |
| 4 "No-reference singing" | 12 | — | reveal `gm:0` | 4000 response | none |
| 5 "Key without a reference" | 12 | — | `gm:1` | none | none |

Confirmed per-block timbres from the trial rows: block 0 = `gm:1` ×48, block 1 = `gm:0` ×12 + `gm:48` ×12, block 2 = six holdout timbres + `shepard` ×4.

### 1.1 "Every activity is basically the same thing"

Correct. Blocks 0, 1, 2 and 3 are all `pitch_id` — name a note, twelve choices, no reference. They differ only in an incidental parameter:

- Block 1 changes the octave **and** the timbre at once (octaves 2 and 6 vs 3–5; `gm:0`/`gm:48` vs `gm:1`).
- Block 3 changes the deadline **and** adds a timbre.
- Block 2 mixes seven timbres in one block, one of which (`shepard`) has no height cue at all.

So a drop in block 1 or 3 cannot be attributed to the variable named in the title, and one Shepard trial in block 2 moves that block's accuracy by about 3%.

### 1.2 "The speed thing is no different but shorter"

Correct, and there is no documented reason for it to exist as a separate block. See §2.3 and §2.4.

### 1.3 "The random notes instead of the Shepard tone for clearing"

Correct. `ap_assessment.json` sets `mask: random_tones` on every identification block and never sets `pre_mask`. The runner only inserts the glissando automatically when `pre_mask` is absent **and** the block has no feedback **and** a named tone was heard within the scrub window (`trainer-app/src/runner.rs:305–329`). An assessment with no reveals never satisfies the last condition, so the Shepard never plays. What plays instead is a stream of random pitches between trials.

### 1.4 The reference block is measured on an untrained timbre

`timbre.rs:75–93` puts `gm:1, gm:11, gm:19, gm:42, gm:56, gm:71, synth:pulse, shepard` in the **holdout** set; `timbre.rs:111–116` makes the default training timbres `gm:0, gm:48, gm:73, gm:24, gm:52, synth:triangle`. Block 0 therefore measures naming on `gm:1`, an instrument training never uses. The comparison that block 0 is supposed to anchor — trained timbre versus transferred timbre — is not available from this preset at all.

### 1.5 It is long because it repeats, not because it measures

156 trials, app estimate 17.3 minutes. Decomposed using the app's own estimator (`validate.rs:44–62`: tone span + mask + 2.5 s + 1.2 s) plus the observed ~1.5 s mask, the run is roughly 16 minutes of which about 4 minutes is mask audio that serves no function when feedback is off.

---

## 2. What the AP documentation requires, and what it does not

Two documents govern this: `docs/AP_Trainer_Literature_search.md` (the research synthesis) and `ap_research_integration_plan.md` (the implementation plan). They distinguish **guards** (constraints applied to every unassisted block) from **tests** (measurements taken at intervals) and from **training drills** (stimuli used to build the skill). The current preset treats all three as interchangeable blocks.

### 2.1 Guards — belong on every block

| Guard | Source | Plan ref | What it defeats |
|---|---|---|---|
| 20 s descending Shepard–Risset glissando before the block | Wong 2020/2025; integration plan §3.2 | §3.2, §5.2 | echoic and tonal working memory carrying the previous trial |
| Consecutive tones ≥ 13 semitones apart | Wong 2020/2025 | §3.1 (default 13) | naming by interval from the previous labelled tone |
| Explicit response ceiling ≤ 2000 ms | Wong 2020/2025 | §3.1 (default 2000) | slow analytic reconstruction |
| Out-of-set distractors at 0.15–0.20 during subset learning | Wong 2020/2025 | §3.1 (default 0.15–0.20) | boundary anchoring on the trained subset |
| No feedback on assessment blocks | Van Hedger; integration plan §5.2 | §5.2 | self-priming |

### 2.2 Tests — few, each a different construct

| Test | Source | Content | Frequency |
|---|---|---|---|
| Trained-timbre naming | Van Hedger (Piano Block); Wong pre/post | unassisted naming, 12 AFC | pre, post, periodically |
| Generalisation gate | Van Hedger (Weekly Generalization Gate) | 120 notes, 24 each on piano, cello, clarinet, harpsichord, square wave, no feedback | weekly |
| External benchmark | UCSF AP Test as used by Van Hedger | 36 piano + 36 sine, scored 1.0 exact / 0.75 within ±1 semitone | periodic |
| Verification passes | Wong | 4 consecutive ≥90% passes, ≥12 h between the 3rd and 4th | per level promotion |

### 2.3 What is a training drill, not a test

Van Hedger's **Simple Speed** is a training stimulus: a 1500 ms visual label plus a sung target, then a 16-note stream with 25% targets and a 1750 ms deadline, scored as hit rate and d′. It is listed as a drill in the UChicago regimen (`AP_Trainer_Literature_search.md` §1, "Dual-Phase Speed/Accuracy Regimen"), and in the plan it appears as the `pitch_detect` exercise (`ap_research_integration_plan.md` §4.1), not as part of the assessment. Its purpose is to force fast retrieval during training; it is not a separate property of a learner that needs its own block in a benchmark.

### 2.4 What the documents do not require

- **No separate speeded identification block.** Wong's 2000 ms is a constraint on every block, not a test in its own right.
- **No between-trial random-tone mask.** Neither document lists a mask as an anti-relative-pitch measure. The plan's mask vocabulary exists for blocks whose feedback reveals answers; an assessment has no feedback.
- **No repetition of the same construct inside one sitting.** Repetition in the literature is *across days* (Friday gates, ≥12 h verification lockouts, 4-week UCSF spacing), precisely because within-sitting repetition measures fatigue and short-term priming rather than the underlying representation.
- **No Shepard tokens mixed into a timbre block.** Shepard tones are specified as (a) the scrubber and (b) a chroma-without-height stimulus. Both roles are separate from timbre transfer.

---

## 3. How many trials a benchmark actually needs

Binomial sizing, one-sided α = .05, power .80, against the app's 12-alternative chance floor of 8.3%:

| True accuracy | Trials needed to show above chance |
|---|---|
| 20% | 46 |
| 25% | 25 |
| 30% | 15 |
| 50% | 5 |

Observed-accuracy precision:

| Trials | 95% CI half-width at p = .25 | at p = .50 | Trials per class | Accuracy change from one error |
|---|---|---|---|---|
| 48 | ±12.2 pts | ±14.1 pts | 4.0 | 25 pts |
| 60 | ±11.0 | ±12.7 | 5.0 | 20 pts |
| 96 | ±8.7 | ±10.0 | 8.0 | 12.5 pts |
| 120 | ±7.7 | ±8.9 | 10.0 | 10 pts |

Consequences that shape the design:

1. **60 trials is the right size for the benchmark.** It clears the 46-trial requirement with margin, costs about 4.5 minutes, and gives a point estimate good to roughly ±11 points.
2. **Per-class accuracy is not estimable at any affordable length.** Even 120 trials give 10 per class, where one error moves that class by 10 points, and the dashboard's mastery rule (`courses.rs:1449–1458`, `MASTERY_RT_MS = 2000`, `MASTERY_MIN_TRIALS = 20`, ≥90% in time) cannot be satisfied by a benchmark at all. Per-class mastery belongs to training gates, not to the benchmark.
3. **What the benchmark can and should report:** overall accuracy, accuracy within ±1 semitone (UCSF-style), mean absolute error in semitones, the 12×12 confusion structure, signed error per class, and the RT distribution. Those are stable at 60 trials; per-class rates are not.

---

## 4. Response time without a speed block

The app stores `rt_ms` per trial and computes in-time percentage post hoc against a threshold: `courses.rs:1449` counts a trial in time when `correct && rt_ms <= MASTERY_RT_MS`, with `MASTERY_RT_MS = 2000.0` (`courses.rs:1250`). The runtime deadline only closes the answer window (`runner.rs:536`, `580–610`), and `can_replay` is false whenever a deadline is set (`runner.rs:564`).

Therefore:

- One **uncapped** naming block records every answer with its latency, and allows the in-time proportion at 2 s, 1500 ms or any other ceiling to be computed afterwards, for the whole block and per class.
- A **hard deadline** converts slow trials into non-answers and throws that information away.
- The recommended configuration is therefore `deadline_ms: null` on the benchmark block, with the 2 s figure derived in reporting. If a hard ceiling is wanted for comparability with Wong's guard, it can be applied to *verification* blocks (which are training gates), not to the benchmark.

This is the direct answer to "why not just one response deadline and measure time to response": because the ceiling is derivable from latency, while latency is not derivable from a ceiling.

---

## 5. Proposed design

### 5.1 Principles

1. One block per construct; no block repeats another's construct under a different title.
2. All five guards on every block (§2.1).
3. No mask audio anywhere in the assessment; the Shepard scrub is the only pre-block audio.
4. Transfer variables isolated: octave block changes only octave, timbre block changes only timbre, chroma block changes only the stimulus type.
5. Length set by the sizing table in §3.
6. `assessment: true` on every block, so feedback is forced off and the holdout rules apply.
7. Everything derivable is derived at report time (in-time %, ±1 semitone accuracy, MAD, confusion), not re-measured with extra blocks.

### 5.2 Tier A — the benchmark (72 trials, 6.7 min, validated)

| Block | Trials | Timbres | Octaves | Deadline | Purpose |
|---|---|---|---|---|---|
| Naming, trained timbre | 60 | `gm:0` | 3–5 | none | overall accuracy, ±1 semitone, MAD, confusion, RT distribution |
| Production, no reference | 12 | target revealed on `gm:0` | free | 4 s response | AP production, scored on pitch class |

Omitting: key-without-reference (a contextual/relative-pitch construct, not naming — keep as a separate optional preset), Shepard (tier B), transfer blocks (tier B).

Validated output: `ok: true`, `trial_count: 72`, `estimated_minutes: 6.7`, preview minimum |MIDI| jump 13, all preview timbres `gm:0`.

One validation warning is expected and accepted: *"assessment blocks should use the holdout pool so results are not inflated by training stimuli"* on block 0. The reference block must use the trained timbre — that is the construct Van Hedger's Piano Block measures — so `pool: training` is deliberate here. The warning is appropriate for blocks that claim to test transfer while using training stimuli; it is not appropriate for a reference block, and the app may want a way to mark that intent.

### 5.3 Tier B — transfer panel (76 trials, 7.0 min, validated)

Run before a training block and after it, not on every benchmark run.

| Block | Trials | Timbres | Octaves | Pool | Purpose |
|---|---|---|---|---|---|
| Timbre transfer | 40 | `gm:0`, `gm:6`, `gm:42`, `gm:71`, `synth:square` | 3–5 | any | the doc's five-timbre gate, shortened |
| Octave transfer | 24 | `gm:0` | 2, 6 | any | generalisation outside the training register |
| Chroma without height | 12 | `shepard` | 3–5 | holdout | pitch class with the height cue removed |

Validated output: `ok: true`, `trial_count: 76`, `estimated_minutes: 7.0`, no warnings, preview minimum |MIDI| jump 13.

Note on the five timbres: in the app's pool model `gm:0`, `gm:6` and `synth:square` are training timbres while `gm:42` and `gm:71` are holdout (`timbre.rs:75–116`), so the doc's gate necessarily spans both pools and must be declared `pool: any`. That is a property of the doc's own choice of instruments, not of this proposal.

### 5.4 Tier C — the doc's milestone tests, separate presets

- `ap_weekly`: 120 trials, 24 each on piano, cello, clarinet, harpsichord, square, no feedback, scrubbed. Currently has no preset; exists only as a dashboard course.
- `ap_ucsf_style`: 36 piano + 36 sine, 1.0 / 0.75 / 0 scoring, reporting MAD and the AP-1 / AP-4 cutoffs.
- `verification_4x`: Wong's four consecutive ≥90% passes with ≥12 h between the third and fourth. This is a training gate and already lives in the course machinery.

These are periodic and should not be folded into the benchmark.

### 5.5 Tier 0 — screening, not built

`AP_Trainer_Literature_search.md` §1 and plan §4.4 specify an Implicit Note Memory micro-cents task, an auditory n-back and a digit span, and route learners by their results. No exercise type implements any of them (checked against `list_exercise_types`, 20 types). Until they exist, no routing claim can be made and the benchmark cannot set difficulty ceilings from working-memory capacity. This is a build item, flagged here so the benchmark is not mistaken for the full onboarding protocol.

---

## 6. Ready-to-implement JSON

### 6.1 `presets/ap_benchmark_a.json` (Tier A)

```json
{
  "title": "AP benchmark",
  "protocol": "ap_benchmark_a",
  "notes": "Shepard scrub, 13-semitone jumps, no mask, no deadline (latency is scored post hoc). Run before any named reference tone today.",
  "blocks": [
    {
      "type": "pitch_id",
      "title": "Naming - trained timbre",
      "count": 60,
      "assessment": true,
      "feedback": "none",
      "pre_mask": "shepard_glissando",
      "tags": ["ap.assessment"], 
      "params": {
        "classes": "all", "octaves": [3, 4, 5], "timbres": ["gm:0"], "pool": "training",
        "duration_ms": 1000, "mask": "none", "answer": "pitch_class",
        "min_jump_semitones": 13, "deadline_ms": null, "out_of_set_rate": 0.0, "detune_cents": 0.0
      }
    },
    {
      "type": "sing_pitch",
      "title": "Production - sing from memory",
      "count": 12,
      "assessment": true,
      "feedback": "none",
      "pre_mask": "shepard_glissando",
      "params": {
        "classes": "all", "octave": null, "response_ms": 4000, "delay_ms": 0,
        "pre_mask": "none", "reveal_target_after": false, "reveal_timbre": "gm:0"
      }
    }
  ]
}
```

### 6.2 `presets/ap_benchmark_b.json` (Tier B)

Three `pitch_id` blocks as in the table in §5.3, each with `"assessment": true`, `"feedback": "none"`, `"pre_mask": "shepard_glissando"`, and `params` of `classes: "all"`, `mask: "none"`, `answer: "pitch_class"`, `min_jump_semitones: 13`, `deadline_ms: null`, `out_of_set_rate: 0.0`, differing only in `count`, `timbres`, `octaves` and `pool`.

---

## 7. Changes to `presets/ap_assessment.json` if it is kept rather than replaced

1. Add `pre_mask: "shepard_glissando"` to all four identification blocks and the singing block.
2. Change `mask` from `random_tones` to `none` on all blocks.
3. Add `min_jump_semitones: 13` to all four identification blocks.
4. Remove `deadline_ms: 1500` from the speeded block and delete that block; latency comes from the reference block.
5. Split `shepard` out of the seven-timbre block into a 12-trial block of its own.
6. Change block 0's timbre from `gm:1` (holdout) to `gm:0` (training), or retitle it so it does not claim to be the trained-timbre reference.
7. Hold the octave block to `gm:0` so octave is the only thing that differs.
8. Correct the trial budget: 48 + 24 + 36 + 24 + 12 + 12 = 156 down to 72 (Tier A) or 148 (A + B).
9. Add `assessment: true` to every block (already present in the shipped JSON) and keep the `ap.assessment` tag.

`presets/ap_daily.json` has the same staleness (`timbres: []`, `min_jump_semitones` absent, no scrub, `mask: random_tones`). Since the live daily practice path is composed from the courses (`courses.rs:1214` forces the scrub), this preset looks vestigial and should either be regenerated from the course machinery or deleted.

---

## 8. Open questions for the vetting agents

1. **Reference block pool.** Should the app support an explicit "reference, not transfer" flag so a training-pool assessment block does not raise the holdout warning? Or should the warning be narrowed to blocks whose title or tag claims transfer?
2. **Deadline policy.** Keep `deadline_ms: null` on the benchmark and derive in-time %, or hold to Wong's 2000 ms guard literally? The guard exists to stop slow analytic naming; if it is applied, late answers should still be stored with their latency, not discarded.
3. **Where does in-time % get reported?** Today `get_ap_dashboard` computes it per class with `MASTERY_RT_MS = 2000`. The benchmark needs it per block and per session, at a stated threshold, and the threshold should be visible in the report.
4. **Is key-without-reference AP?** `tonal_center_id` without a reference measures a contextual/relative ability. If it stays in the AP picture it should be its own preset with its own trend line, not a block in the naming benchmark.
5. **Shepard trials.** Are 12 tokens with no height cue informative for a beginner, or do they belong only to later stages? The literature treats them as a memory scrubber first and a chroma probe second.
6. **Screening battery.** Should `ap_benchmark_a` be blocked until INM / n-back / digit span exist, or should the benchmark run and flag that no capacity estimate is available?
7. **Weekly gate.** Should `ap_weekly` become a preset (`ap_weekly.json`) so the Friday gate can be run at all, given the dashboard currently only exposes it through course composition?
8. **Report shape.** Should `get_ap_profile` be extended to name the benchmark protocol and tier, so assessments from different protocols are never compared as if interchangeable?

---

## 9. Verification performed

- `get_preset ap_assessment` and the live session's trial rows, read through the app's own MCP server: per-block timbres, octaves, deadlines, masks.
- `crates/trainer-core/src/generate.rs:143–190` — `pool_combos`, the holdout rules and the empty-pool error.
- `crates/trainer-core/src/timbre.rs:75–116` — holdout timbre list, holdout octaves `[2, 6]`, default training timbres.
- `crates/trainer-core/src/validate.rs:44–62, 130–142` — the trial-time estimator and the pool warnings.
- `crates/trainer-core/src/courses.rs:136–137, 254–267, 307, 1214, 1250, 1449–1458` — the guards that were implemented, the forced scrub on the daily set, `MASTERY_RT_MS`, and the post-hoc in-time computation.
- `crates/trainer-app/src/runner.rs:305–329, 536, 564, 580–610` — the automatic scrub condition, deadline handling, `can_replay`.
- `crates/trainer-core/presets/*.json` — all dated 2026-10-24 03:11, before the AP integration work in `courses.rs`.
- Both proposed tiers dry-run through `queue_session` with `dry_run: true`: Tier A `ok: true`, 72 trials, 6.7 min; Tier B `ok: true`, 76 trials, 7.0 min; previews confirm 13-semitone minimum jumps and the intended timbres and octaves.
- Trial-count and confidence-interval figures computed from the binomial normal approximation, one-sided α = .05, power .80.
