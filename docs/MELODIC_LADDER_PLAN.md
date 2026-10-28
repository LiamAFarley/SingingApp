# Melodic ladder refactor — plan

**Status:** implemented 2026-10-27. The shared 26-step ladder is in
`crates/trainer-core/src/melody_ladder.rs`; sight singing, dictation and singback use it; the old
`melody_modules` courses are gone.

**Decisions taken:** new `rp_sight_v2` / `rp_dictation_v2` / `rp_singback_v2` ids with a fresh
start; `0.85` (7/8) pass mark; eight questions per lesson, not eight bars; Melodia keeps its source
exercise count and only gets the register policy. The `rp_singback` workshop entry was replaced by
`rp_singback_v2`; old trial history is left in the database.

## 1. Why the current ladder is wrong

`crates/trainer-core/src/rp_courses.rs::melody_modules` (now removed) built the generated courses.

For sight singing (`melody_modules(&[1, 2, 4])`) it produces **242 levels**:

- **34 note-count lessons** first: 3, 4, 5, 6, 7 and 8 notes, each with a fresh
  degree ceiling starting at do–re. This is the “do re, do re mi” repetition.
- **208 rhythm lessons** after that, in 13 modules of 16.
- The first eighth note is at **level 67**.
- Every rhythm module runs **1 bar → 2 bars → 4 bars**, so length drops back to
  1 and 2 after a 4-bar block.

Dictation and melody singback use the same function with `[1, 2]`, so they have
the same non-progression with a shorter length ladder. The singback item inside
the Inner hearing workshop has its own two-bar melody and does not share a
ladder at all.

## 2. Target design

One shared ladder definition, consumed by three generated courses:

- **Melodic sight singing** — read from the staff.
- **Melodic dictation / transcription** — hear, then write.
- **Melody singback** — hear, score hidden, then sing back.

Each course keeps its activity-specific settings, but pitch, rhythm, length,
key and leap progression come from the same `LadderStep` list. The courses
cannot drift apart.

### Fixed rules

1. **Eight trials per generated lesson.** This is the app's `count`, not eight
   bars. Bar lengths remain 2 → 4 → 8.
2. **First lesson is do–sol only.** Every later lesson uses the full scale.
3. **No one-bar generated melodies.**
4. **Length never goes backwards.** 2 bars for lessons 1–2, 4 bars for 3–18,
   8 bars from 19 onward.
5. **Rhythm only grows.** half/quarter → eighth → dotted quarter → dotted
   eighth/sixteenth → sixteenth runs → syncopation → mixed.
6. **Leaps only grow.** steps → thirds → fourths → fifths → sixths → octave.
7. **Keys grow by accidental count.** C → F/G → B♭/D → E♭/A → A♭/E →
   D♭/B → G♭/F♯ → all major.
8. **All major keys are introduced before the course moves to 8 bars.**
9. **Minor comes after the major ladder**, as a separate phase.
10. **Generated material only.** Melodia stays a separate source-ordered
    course.

## 3. The ladder

Every step has `count: 8`.

| # | Module | Bars | Keys | Rhythm | Max leap | New demand |
|---:|---|---:|---|---|---|---|
| 1 | Start | 2 | C | half, quarter | step | do–sol only |
| 2 | Start | 2 | C | half, quarter | step | full scale |
| 3 | C major, 4 bars | 4 | C | half, quarter | step | length |
| 4 | C major, 4 bars | 4 | C | + eighth | step | eighth notes |
| 5 | C major, 4 bars | 4 | C | eighth | third | thirds |
| 6 | One accidental | 4 | F, G | eighth | third | new keys |
| 7 | One accidental | 4 | F, G | + dotted quarter | third | dotted quarter/eighth |
| 8 | Two accidentals | 4 | B♭, D | dotted quarter | third | new keys |
| 9 | Two accidentals | 4 | B♭, D | + dotted eighth, sixteenth | third | dotted eighth + sixteenth |
| 10 | Two accidentals | 4 | B♭, D | dotted eighth | fourth | fourths |
| 11 | Three accidentals | 4 | E♭, A | dotted eighth | fourth | new keys |
| 12 | Three accidentals | 4 | E♭, A | + sixteenth runs | fourth | sixteenth runs |
| 13 | Four accidentals | 4 | A♭, E | sixteenth runs | fourth | new keys |
| 14 | Four accidentals | 4 | A♭, E | sixteenth runs | fifth | fifths |
| 15 | Five accidentals | 4 | D♭, B | sixteenth runs | fifth | new keys |
| 16 | Five accidentals | 4 | D♭, B | + syncopation | fifth | syncopation |
| 17 | Six accidentals | 4 | G♭, F♯ | syncopation | fifth | new keys |
| 18 | Six accidentals | 4 | G♭, F♯ | syncopation | sixth | sixths |
| 19 | All major, 8 bars | 8 | all major | syncopation/eighth | sixth | length 8 |
| 20 | All major, 8 bars | 8 | all major | mixed | octave | octave leap |
| 21 | All major, 8 bars | 8 | all major | mixed | octave | consolidation |
| 22 | All major, 8 bars | 8 | all major | mixed | octave | review |

Minor phase:

| # | Module | Bars | Keys | Rhythm | Max leap | New demand |
|---:|---|---:|---|---|---|---|
| 23 | Minor | 8 | A minor | mixed | sixth | minor mode |
| 24 | Minor | 8 | E minor, D minor | mixed | sixth | new minor keys |
| 25 | All minor | 8 | all minor | mixed | octave | all minor keys |
| 26 | Mixed review | 8 | major + minor | mixed | octave | mode switching |

Lessons 1–22 are the first implementation. Lessons 23–26 are a second phase.

## 4. Grading: sustained and repeated notes

A second, independent false negative was found in the grader.

**Symptom:** two repeated notes at the same pitch, sung as one continuous vowel,
show the second note as missed. A consonant such as the `m` in “me” can also
create a short extra segment or level dip that does not correspond to a new
written onset.

**Cause:** `segment_notes` creates one performed segment when the pitch does not
change and the level does not dip enough. `align` is one-to-one, so one segment
can match only one expected note. The second repeated note is then deleted and
reported as `matched: false`, even though its pitch was correct.

**Rule:** onset detection is feedback, not a gate for whether a note existed.
For a segment that covers a written note at the target pitch, credit the note
even when there is no separate onset. This is deliberately biased against false
negatives. A held **wrong** pitch still cannot satisfy a different target.

Implementation in `grade.rs`:

- after alignment, an unmatched expected note is checked against performed
  segments that overlap it by at least 30% of its length (at least 80 ms);
- the segment must be within one semitone of the target after the octave or
  relative-transposition offset;
- the overlapping slice is measured against that note's target;
- `matched` and `correct` come from the pitch measurement;
- `onset_s`, `onset_error_ms`, and `on_time` are left unset for a continuation,
  because no onset was detected; they are not reported as a failure.

Tests:

- `a_sustained_pitch_covers_repeated_targets_without_a_false_miss`
- `a_sustained_wrong_pitch_does_not_cover_a_later_note`

This applies to every `grade_sung` path, including sight singing, dictation,
singback and patterns.

## 5. Range and register policy

The Melodia complaint is real: the source is written high, often around C5–D5,
and can land in the upper voice.

`melodia_course.rs` already sets `fit_range: true`, and
`Melodies::new` now calls `library::fit_practice_octaves` when `fit_range` is set. The old
`fit_octaves` only chose the octave that kept a melody **inside** the comfortable range. It does not aim for the middle or lower part of that range.
A wide or high comfortable range can therefore leave the melody at C5. The
generated `tonic_octave` has the same limitation: it maximizes usable degrees,
not vocal comfort.

Before changing the algorithm I will reproduce this with the first Melodia
lesson and assert the resulting MIDI range. The fix is a single
**practice-register policy** used everywhere a melody is placed:

```text
floor   = comfortable.low + 2
target  = comfortable.low + 7       // a fifth above the bottom
ceiling = min(comfortable.high - 2, comfortable.low + 12)
```

- The policy prefers an octave where the melody's **median** is near `target`
  and its top is below `ceiling`.
- Notes outside `[floor, ceiling]` are penalized heavily.
- When the melody's own span is wider than the practice window, the ceiling
  expands only as far as needed, up to the comfortable high.
- Shift by **whole octaves only**; key, intervals and spelling are unchanged.
- Apply it to library pieces (Melodia) and generated melodies.
- A melody that already fits low is not pushed up just because the comfortable
  range is wide.

This is the fix for “comically high” and it applies to sight singing,
dictation and singback because all three use the same melody placement.

If the reproduction shows `fit_range` is being bypassed rather than merely
under-constrained, fix that first; the register policy is the fallback and the
long-term rule.

## 6. Code shape

New shared module, for example `crates/trainer-core/src/melody_ladder.rs`:

```rust
pub struct LadderStep {
    pub title: String,
    pub summary: String,
    pub params: serde_json::Value,
}

pub fn steps() -> Vec<LadderStep>;
pub fn lessons(mode: LadderMode) -> Vec<Lesson>;
```

`LadderMode` supplies only the activity-specific pieces:

| Mode | Course id | Exercise | Extra settings |
|---|---|---|---|
| Sight | `rp_sight_v2` | `sight_sing` | `mode: read` |
| Dictation | `rp_dictation_v2` | `melodic_dictation` | `max_plays`, `grade_rhythm`, key/first-note settings |
| Singback | `rp_singback_v2` | `sight_sing` | `mode: singback`, `start: key_only` |

All three call `steps()` and set `count: 8`. The old `melody_modules` function was removed when the
new courses landed.

## 7. Trials, pass rule, and Melodia

### Trials

The generated ladder uses **8 trials per lesson**.

With `PASS = 0.9`, 8 trials means 8/8 (7/8 is 0.875). That is brittle. The
refactor should set the three ladder courses' pass mark to **0.85** (7/8) or
define the pass rule per note rather than per trial. This is a deliberate
decision, not an accident.

### Melodia

Melodia is a source-ordered course with 1–12 exercises per source lesson.
Setting every Melodia lesson to 8 would truncate or pad the source. The plan
therefore:

- keeps Melodia's source lesson count and order;
- applies the new register policy so the same material is singable;
- does not reorder, filter or regenerate it.

If “all trials” was meant to include Melodia, the correct feature is a separate
**8-item drill** that samples a source lesson without altering the source
course. That is not included in this refactor unless requested.

## 8. Generator changes

### 7.1 Dotted eighth

Add `NoteValue::DottedEighth` (3 ticks). `ui/src/notation/layout.ts` already
handles a 3-tick dotted eighth, so no notation change is needed.

### 7.2 Rhythm cells and syncopation

Add cells:

- `[DottedEighth, Sixteenth]`
- `[Eighth, Quarter, Eighth]`
- `[Quarter, Eighth, Eighth]`
- optionally `[Sixteenth, DottedEighth]`

Add a `syncopation` weight to `MelodyParams`, default 0. It is off through
lesson 15 and turned on at lesson 16. This is offbeat-attack syncopation, not
tied notes across beats.

### 7.3 Phrase grammar

The current `walk` is a random walk. Add:

- a phrase shape per melody: arch, ascent, descent or wave;
- tonic-triad weighting on strong beats;
- at most one large leap per phrase;
- step in the opposite direction after a leap of a fourth or more;
- last two notes approach and land on the tonic;
- no more than two identical pitches in a row;
- a narrow range through lesson 13, widening after the fourth/fifth stages.

### 7.4 Key and duplicate control

- For two-key lessons, ensure both keys appear in an 8-trial set.
- Retry a generated trial if its pitch-and-rhythm fingerprint duplicates
  another trial in the same lesson.
- No persistent exercise list; each session generates fresh material.

## 9. Migration (done)

The old courses had 242 (sight), 190 (dictation) and 190 (singback) levels. Existing progress by
level number could not be mapped meaningfully, so the migration used the recommended new course
ids and left trial history in the database for reference.

What was done:

- new course ids: `rp_sight_v2`, `rp_dictation_v2`, `rp_singback_v2`;
- cards in `RpPage.tsx` replaced, and the old singback workshop entry removed from
  `audiation_courses.rs`;
- old trial history left in the database for reference;
- the new courses start at level 1;
- the new ids are suggested at 8 minutes; `estimate_trial_seconds` is trial-based and needed no
  per-course branch.

The alternative is keeping the old ids and resetting progress. That keeps the
ids but leaves misleading level history.

The singback part inside `rp_inner` stays a mixed workshop; it gets the register
policy but not the full ladder.

## 10. Verification

### Metadata

- Lesson 1 has degrees 1–5 only; lesson 2 has 1–8.
- No generated lesson has `bars == 1`.
- Bars are 2, 2, then 4 through lesson 18, then 8.
- Eighth notes appear by lesson 4.
- Dotted eighth, sixteenth and syncopation appear at the intended steps.
- `max_leap` never decreases across the major ladder.
- All 13 major spellings are covered by lesson 18.
- Every generated lesson has `count == 8`.
- Dictation and singback use the same 26 steps as sight singing.

### Generation

- Generate 20 seeds for every step; all succeed.
- Every score fills its bars exactly.
- Lesson 1 contains only do–sol pitch classes.
- Every lesson ends on the tonic.
- Melodia lesson 1 is shifted into the practice register; its median and top
  are asserted, not just “inside the comfortable range”.
- No duplicate fingerprints within an 8-trial lesson across 100 seeds.
- Dotted eighth and syncopation render with no fallback durations.
- Dictation and singback notes are in the same register policy as sight
  singing.

### App

- Render lessons 1, 4, 9, 16, 19 and 22 for sight singing.
- Render the same first/middle/last lessons for dictation and singback.
- Check the Melodia card opens with a comfortable register.
- Check tempo, solfège labels, score display, and pass progression.
- Confirm old and new courses do not both appear.

## 11. Decisions taken

1. **Course ids:** new `*_v2` ids with a fresh start. Old ids are no longer built-in.
2. **Pass rule with 8 trials:** `0.85` (7/8) for the three ladder courses.
3. **Melodia trials:** keep its source exercise count; no separate 8-item drill was added.
4. **“All trials = 8”** means eight questions per lesson, not eight bars.

## 12. Implementation record

- `melody_ladder.rs`: 26 steps, three modes, 8 trials, 0.85 pass.
- `melody.rs`: `DottedEighth` (3 ticks), syncopation weight, phrase shapes, strong-beat tonic-triad
  weighting, one large leap per phrase, step recovery, the practice-register placement, and a
  generated-melody duplicate fingerprint check.
- `Melodies`: keys are scheduled per trial so a two-key lesson hears both; generated trials are
  retried when their pitch-and-rhythm fingerprint repeats within the lesson.
- `library.rs`: `practice_window`, `best_practice_octave`, `fit_practice_octaves`; Melodia uses it.
- `courses.rs`: `rp_*_v2` suggested at 8 minutes; old melody courses removed from the built-in list.
- `RpPage.tsx`: cards point at the three `*_v2` courses.
- Tests: 201 passed, 1 ignored (`cargo test -p trainer-core`); `cargo check -p trainer-app` and
  `npm run typecheck` pass.
