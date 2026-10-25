# Review & Revisions to `2026-10-24_benchmark_assessment_spec.md`

**Date:** 2026-10-24  
**Context:** Companion review and revision spec to be read alongside `2026-10-24_benchmark_assessment_spec.md` (authored by `singing-coach`).  
**Purpose:** Document four architectural and statistical bugs identified in the initial proposal, record the owner's design decisions on octaves and response deadlines, provide the updated Tier A and Tier B JSON presets, and specify the exact Rust changes for final review by a third agent.

---

## 0. Executive Summary of Decisions

We endorse the core structural improvements in `2026-10-24_benchmark_assessment_spec.md`:
1. Splitting the bloated 156-trial assessment into **Tier A (Core Benchmark)** and **Tier B (Transfer Panel)**.
2. Eliminating the between-trial `random_tones` mask on no-feedback assessment blocks and enforcing `pre_mask: "shepard_glissando"` on every block.
3. Fixing the Block 0 holdout bug (`gm:1` $\rightarrow$ `gm:0`) so the baseline actually measures the trained piano timbre.
4. Eliminating the separate speeded block in favor of single-pass latency logging and post-hoc threshold scoring.

However, four revisions are required before implementation:
1. **Drop Octave `[2, 6]` Holdouts Globally:** Octaves 2 and 6 are no longer reserved as holdout registers. Tier A and standard training now span 4 octaves (`[2, 3, 4, 5]`), which resolves a severe Markov transition bug caused by pairing `octaves: [3, 4, 5]` with `min_jump_semitones: 13` and eliminates the need for a separate Octave Transfer block in Tier B.
2. **Swap Block Order in Tier A (`sing_pitch` First):** Cold-start vocal production (`sing_pitch`, 12 trials) must run as **Block 0** *before* the 60-trial `pitch_id` block so 4.5 minutes of concert-tuned `gm:0` piano tones do not acoustically prime the learner's ear prior to unassisted singing.
3. **Use a `5000` ms Anti-Calculation Ceiling (`deadline_ms: 5000`):** Instead of `deadline_ms: null`, every naming block uses a 5-second ceiling. This disables tone replay in `runner.rs`, blocks slow multi-step relative interval calculation, captures slow perceptual retrieval ($2000\text{ ms} < \text{RT} \le 5000\text{ ms}$), and still allows automaticity ($\le 2000\text{ ms}$) to be scored post hoc in `courses.rs`.
4. **Re-Power Tier B `shepard` from 12 to 24 Trials:** Dropping the 24-trial Octave Transfer block allows us to double the `shepard` (pure chroma without height) block from 12 to 24 trials, clearing the binomial detection threshold while reducing Tier B from 76 trials to 64 trials (~5.8 min).

---

## 1. Four Architectural Flaws Identified in the Initial Spec

### 1.1 Block Ordering Contamination in Tier A (`sing_pitch` Must Run First)
* **Issue in Original Spec (§5.2, §6.1):** `ap_benchmark_a.json` placed the 60-trial `pitch_id` block (`gm:0`) as Block 0 and the 12-trial `sing_pitch` ("Production - sing from memory") block as Block 1.
* **Why It Fails:** Even with a 20-second `shepard_glissando` before Block 1, exposing the user to 60 concert-tuned ($A_4 = 440\text{ Hz}$) piano tones over 4.5 minutes saturates auditory working memory and calibrates internal tuning templates right before testing "no-reference" vocal production.
* **Revision:** Swap Block 0 and Block 1 in `ap_benchmark_a.json`. Running `sing_pitch` as Block 0 guarantees a true cold-start production measurement before a single reference piano tone is played in the session.

### 1.2 The 3-Octave + 13-Semitone Markov Trap (`octaves: [3, 4, 5]` + `min_jump_semitones: 13`)
* **Issue in Original Spec (§5.2, §6.1):** The baseline naming block combined a 3-octave pool (`[3, 4, 5]`, MIDI $48\dots83$, spanning 36 semitones) with `min_jump_semitones: 13`.
* **Why It Fails:** Consider what happens when trial $t-1$ lands anywhere in the middle octave (**Octave 4**, MIDI $60 + c$ for pitch class $c \in \{0\dots11\}$):
  * To jump **down** into Octave 3 (MIDI $48 + c'$), the distance is $(60 + c) - (48 + c') = 12 + c - c'$. Satisfying $12 + c - c' \ge 13$ requires $c' < c$.
  * To jump **up** into Octave 5 (MIDI $72 + c'$), the distance is $(72 + c') - (60 + c) = 12 + c' - c$. Satisfying $12 + c' - c \ge 13$ requires $c' > c$.
  * **Consequences:**
    1. From **C4 (MIDI 60)**, the lowest note in `[3, 4, 5]` is C3 (MIDI 48, distance 12). The generator *cannot jump down at all*; 100% of transitions from C4 are forced into Octave 5 ($\text{C}\sharp5\text{--B}5$). Conversely, from **B4 (MIDI 71)**, the highest note is B5 (MIDI 83, distance 12), forcing 100% of transitions down into Octave 3.
    2. From *any* note in Octave 4, **zero transitions can land in Octave 4**, lower pitch classes ($c' < c$) are systematically forced into Octave 3, and higher pitch classes ($c' > c$) are systematically forced into Octave 5. Over a 60-trial Markov chain, Octave 4 is starved and pitch class becomes statistically correlated with pitch height.
* **Revision:** Per owner decision, we do not care about holding out Octaves 2 and 6. Expanding the standard register to 4 octaves (`octaves: [2, 3, 4, 5]`, 48 semitones) completely eliminates the boundary trap and allows us to drop the redundant 24-trial Octave Transfer block from Tier B.

### 1.3 Why `deadline_ms: null` + `mask: "none"` Fails (`can_replay` & Relative Chaining)
* **Issue in Original Spec (§4, §6.1):** The proposal set `deadline_ms: null` alongside `mask: "none"`, arguing that post-hoc filtering at $\le 2000\text{ ms}$ makes a runtime deadline unnecessary.
* **Why It Fails:**
  1. **The `can_replay` Bug:** As noted in §4 of the original spec, `runner.rs:564` sets `can_replay = false` *only when a deadline is set*. Setting `deadline_ms: null` inadvertently re-enables tone replay during an unassisted assessment, violating fixed stimulus exposure (`duration_ms: 1000`).
  2. **The Relative-Chaining Loophole:** In Wong et al. (2025), the response ceiling is not just a post-hoc filter; it is an active cognitive constraint preventing a learner from taking 6–10 seconds to octave-transpose the prior tone and calculate relative intervals from a single known anchor pitch.
* **Revision:** Set `"deadline_ms": 5000` on all assessment naming blocks (and hardcode `can_replay = false` when `assessment == true` in `runner.rs`). A 5-second ceiling blocks slow multi-step interval calculation and disables replay, while still capturing genuine 2.0–4.5 s perceptual retrievals so `courses.rs` can report both **Raw Accuracy ($\le 5000\text{ ms}$)** and **Automaticity Accuracy ($\le 2000\text{ ms}$)** post hoc.

### 1.4 Under-Powering of Tier B `shepard` (12 Trials) and Pitch-Class Stratification
* **Issue in Original Spec (§3 vs. §5.3):** In §3, binomial power analysis ($\alpha = .05$, power $.80$ vs. $8.3\%$ chance) showed that **25 trials** are required to detect $25\%$ accuracy and **46 trials** to detect $20\%$ accuracy. Yet §5.3 assigned only **12 trials** to the Tier B `shepard` block (1 trial per pitch class, where a single lucky guess moves the block score by $8.3\%$).
* **Revision:** Because the 24-trial Octave Transfer block is removed from Tier B, we reallocate 12 of those trials to bump `shepard` from 12 to **24 trials** (2 per pitch class). This clears the $\sim 25\%$ detection threshold while cutting total Tier B length from 76 trials to 64 trials (~5.8 min). Furthermore, `generate.rs` must enforce **stratified pitch-class sampling** (exact $N/12$ occurrences per pitch class when $N$ is a multiple of 12) rather than independent uniform random draws.

---

## 2. Decisions on the 8 Open Questions (§8 of Original Spec)

| # | Question in Original Spec (§8) | Decision & Implementation Rule |
|---|---|---|
| **1** | **Reference block pool warning** (`validate.rs:130–142`) | Update `validate.rs:130–142` so the holdout pool warning is suppressed when an assessment block carries the `"ap.baseline"` tag or `"pool": "any"`. A baseline block *must* test the trained timbre (`gm:0`) so transfer deltas ($\Delta_{\text{transfer}} = \text{Acc}_{\text{Tier B}} - \text{Acc}_{\text{Tier A}}$) are valid. |
| **2** | **Deadline policy** (`null` vs. `2000 ms`) | Use `"deadline_ms": 5000` at runtime to block analytic interval chaining and disable replay, and compute both **Raw Accuracy ($\le 5000\text{ ms}$)** and **In-Time Automaticity ($\le 2000\text{ ms}$)** post hoc. |
| **3** | **Where does in-time % get reported?** | Report per block and per session in `get_ap_profile` and session summaries: `accuracy_raw`, `accuracy_in_time_2000ms`, `median_correct_rt_ms` (median RT on *correct* trials only, so fast guessing does not inflate speed metrics), and `ucsf_score` ($1.0$ exact, $0.75$ for $\pm 1$ semitone). |
| **4** | **Is key-without-reference AP?** (`tonal_center_id`) | Keep out of Tier A and Tier B. `tonal_center_id` mixes relative scale-degree extraction with absolute anchoring; move it to an optional `ap_ecological.json` preset. |
| **5** | **Are Shepard trials useful for beginners?** | **Yes—as a pitch-height cheat detector.** Comparing Tier A (`gm:0`) against Tier B (`shepard`, 24 trials) yields the **Chroma-to-Height Ratio**. If `gm:0` accuracy rises while `shepard` stays near the $8.3\%$ floor, the learner is memorizing register/timbral brightness boundaries rather than pitch chroma. |
| **6** | **Block benchmark until WMC screening exists?** | **No.** Run `ap_benchmark_a` immediately with `wmc_profile: null`. When INM micro-cents, auditory $n$-back, and digit span are built, expose them as a one-time `ap_onboarding_screen.json` battery. |
| **7** | **Make `ap_weekly` a standalone preset?** | **Yes.** Create `presets/ap_weekly.json` (120 trials: $24 \times [\text{gm:0, gm:6, gm:42, gm:71, synth:square}]$) so the MCP agent can schedule Friday generalization gates directly. |
| **8** | **Extend `get_ap_profile` with protocol/tier?** | **Mandatory.** Tag session records and profile outputs with `protocol` (`"ap_benchmark_a"`, `"ap_benchmark_b"`, `"ap_weekly"`) and `protocol_version: 2` so historical `gm:1` masked runs are never conflated with new `gm:0` unmasked runs. |

---

## 3. Updated Preset JSON Specifications

### 3.1 `presets/ap_benchmark_a.json` (Tier A — 72 trials, ~6.7 min)

```json
{
  "title": "AP Benchmark (Tier A - Core)",
  "protocol": "ap_benchmark_a",
  "notes": "Cold-start production first, then 60-trial trained-timbre naming across 4 octaves. Shepard scrub, 13-semitone jumps, no between-trial mask, 5000ms ceiling (2000ms automaticity scored post hoc).",
  "blocks": [
    {
      "type": "sing_pitch",
      "title": "Production - cold start from memory",
      "count": 12,
      "assessment": true,
      "feedback": "none",
      "pre_mask": "shepard_glissando",
      "tags": ["ap.assessment", "ap.production"],
      "params": {
        "classes": "all",
        "octave": null,
        "response_ms": 4000,
        "delay_ms": 0,
        "pre_mask": "none",
        "reveal_target_after": false,
        "reveal_timbre": "gm:0"
      }
    },
    {
      "type": "pitch_id",
      "title": "Naming - trained timbre baseline",
      "count": 60,
      "assessment": true,
      "feedback": "none",
      "pre_mask": "shepard_glissando",
      "tags": ["ap.assessment", "ap.baseline"],
      "params": {
        "classes": "all",
        "octaves": [2, 3, 4, 5],
        "timbres": ["gm:0"],
        "pool": "any",
        "duration_ms": 1000,
        "mask": "none",
        "answer": "pitch_class",
        "min_jump_semitones": 13,
        "deadline_ms": 5000,
        "out_of_set_rate": 0.0,
        "detune_cents": 0.0
      }
    }
  ]
}
```

### 3.2 `presets/ap_benchmark_b.json` (Tier B — 64 trials, ~5.8 min)

*(Note for Third Agent: See §5.1 regarding whether `count` in Block 0 should be `40` vs. `48` or `60` for simultaneous timbre and pitch-class balancing.)*

```json
{
  "title": "AP Benchmark (Tier B - Transfer Panel)",
  "protocol": "ap_benchmark_b",
  "notes": "Run pre/post training block. Isolates 5-timbre generalization and pure chroma without height cues (Shepard).",
  "blocks": [
    {
      "type": "pitch_id",
      "title": "Timbre transfer",
      "count": 40,
      "assessment": true,
      "feedback": "none",
      "pre_mask": "shepard_glissando",
      "tags": ["ap.assessment", "ap.transfer.timbre"],
      "params": {
        "classes": "all",
        "octaves": [2, 3, 4, 5],
        "timbres": ["gm:0", "gm:6", "gm:42", "gm:71", "synth:square"],
        "pool": "any",
        "duration_ms": 1000,
        "mask": "none",
        "answer": "pitch_class",
        "min_jump_semitones": 13,
        "deadline_ms": 5000,
        "out_of_set_rate": 0.0,
        "detune_cents": 0.0
      }
    },
    {
      "type": "pitch_id",
      "title": "Chroma without height",
      "count": 24,
      "assessment": true,
      "feedback": "none",
      "pre_mask": "shepard_glissando",
      "tags": ["ap.assessment", "ap.transfer.chroma"],
      "params": {
        "classes": "all",
        "octaves": [3, 4, 5],
        "timbres": ["shepard"],
        "pool": "holdout",
        "duration_ms": 1000,
        "mask": "none",
        "answer": "pitch_class",
        "min_jump_semitones": 13,
        "deadline_ms": 5000,
        "out_of_set_rate": 0.0,
        "detune_cents": 0.0
      }
    }
  ]
}
```

---

## 4. Required Code Changes in `trainer-core` and `trainer-app`

1. **`crates/trainer-core/src/timbre.rs` (Lines 75–116):**
   * Remove `[2, 6]` from the holdout octave list so Octaves `2..=6` are available to `pool: "training"` and `pool: "any"` without triggering empty-pool errors in `generate.rs:143–190`.
2. **`crates/trainer-core/src/validate.rs` (Lines 130–142):**
   * Suppress the *"assessment blocks should use the holdout pool"* warning when a block carries the `"ap.baseline"` tag (or explicitly tests trained-timbre retention).
3. **`crates/trainer-app/src/runner.rs` (Line 564):**
   * Ensure `can_replay` is forced to `false` whenever `block.assessment == true`, even if a future block uses `deadline_ms: null`.
   * Verify that when a trial hits the `5000` ms deadline without a user response, it records the trial as a timeout (`correct: false`, `response: null`, `rt_ms: 5000.0`) rather than silently dropping the trial row.
4. **`crates/trainer-core/src/generate.rs` (Lines 143–190):**
   * Ensure assessment blocks with `classes: "all"` use **stratified pitch-class balancing** (every pitch class appears $\lfloor N/12 \rfloor$ or $\lceil N/12 \rceil$ times per block) ordered via backtracking/rejection sampling to satisfy `min_jump_semitones: 13`, rather than independent uniform random draws per trial.
5. **`crates/trainer-core/src/courses.rs` & `get_ap_profile`:**
   * Compute and expose both `accuracy_raw` ($\le 5000\text{ ms}$) and `accuracy_in_time_2000ms` ($\le 2000\text{ ms}$), plus `median_correct_rt_ms` and `ucsf_score`, partitioned by `protocol`.
6. **Legacy Preset Cleanup (`crates/trainer-core/presets/`):**
   * Replace `ap_assessment.json` (or alias it to `ap_benchmark_a.json`), add `ap_benchmark_b.json` and `ap_weekly.json`, and either delete or regenerate `ap_daily.json` so no preset uses `mask: "random_tones"` without `min_jump_semitones: 13`.

---

## 5. Specific Review Prompts for the Third Agent

When reviewing both `2026-10-24_benchmark_assessment_spec.md` and this revision document against the codebase, please verify:

1. **40 vs. 48/60 Trials in Tier B Timbre Transfer:** In `ap_benchmark_b.json`, Block 0 tests 5 timbres (`["gm:0", "gm:6", "gm:42", "gm:71", "synth:square"]`) across 12 pitch classes. At `count: 40`, each timbre gets exactly 8 trials, but $40 \pmod{12} = 4$ (so 4 pitch classes appear $4\times$ and 8 pitch classes appear $3\times$). At `count: 48`, each pitch class appears $4\times$, while timbres appear $9\text{--}10\times$. At `count: 60` ($\text{LCM}(5, 12) = 60$), every `(timbre, pitch_class)` pair appears **exactly once**. Should we leave Tier B Block 0 at `40`, bump it to `48`, or set it to `60` for an exact Latin-square $5 \times 12$ cross-design?
2. **Generator Backtracking for `min_jump_semitones: 13`:** How does `generate.rs` currently pick the next trial when `min_jump_semitones: 13` is active? If it uses greedy step-by-step sampling from a finite balanced bag without lookahead, can the last 1–2 trials of a 60-trial block get stranded without a valid $\ge 13$-semitone jump, and does it need a simple swap/retry fallback?
3. **Shepard Octave Parameter:** In `generate.rs` and the audio synth, `shepard` is pitch-height invariant, yet `pitch_id` requires an `octaves` array (`[3, 4, 5]`). Does `min_jump_semitones: 13` evaluate against the dummy MIDI octave assigned to the Shepard token, and does that restrict adjacent pitch-class transitions on Shepard trials? (Note: For true Shepard tones, any adjacent pitch class $c' \neq c$ is already height-ambiguous, whereas requiring $|M_t - M_{t-1}| \ge 13$ on a 3-octave dummy MIDI tag `[3, 4, 5]` re-introduces the 3-octave Markov bias from §1.2! Setting `"octaves": [2, 3, 4, 5]` on the `shepard` block as well avoids this completely.)