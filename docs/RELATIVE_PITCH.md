# Relative pitch: courses and controls

**For:** the coach agent and the user.
**Pattern:** EarMaster's workshops are ladders: one exercise type, a level at a time, each level
locking its settings and passing on a score. The courses here follow that pattern with this
app's own exercises. They do not use any of EarMaster's content.

## Routine

Use the **Relative pitch** page: it has its own **Today's set**, separate from the absolute-pitch
one. Scale degrees and Intervals in a key are switched on at first. Tick **Daily** on any other
course to add it, and set its minutes. Each level is a practice block with feedback, then a
check without. Two checks in a row at 90% or more move the course on. Levels can be set by hand
from the course card.

There are no clearing glissandos and no tests here; those are for absolute pitch. Relative pitch
can be practised any time of day.

## The courses

| Course | Exercise | Ladder |
| --- | --- | --- |
| `rp_degrees` Scale degrees | `scale_degree_id` | 1 2 3 → 1–5 → all seven (one key for the session) → new key each question → minor → major and minor → chromatic → chromatic in changing keys |
| `rp_intervals_key` Intervals in a key | `interval_id`, `in_key`, from do | P5/P8 → M3/P5 → P4/P5 → M2/M3 → M6/M7 → all up → all down → up and down in changing keys → from any note of the key → the same in changing keys → together |
| `rp_intervals_chromatic` Chromatic intervals in a key | `interval_id`, from do, `chromatic` | m3/M3 → m6/M6 → m7/M7 → m2/M2 → TT/P4/P5 → all twelve up → down → changing keys |
| `rp_intervals` Intervals without a key | `interval_id` on random roots | P4/P5 → m3/M3 → m2/M2 → 3rds–5ths → 6ths → 7ths → TT/P8 → all up → all down → either way → together → beyond the octave |
| `rp_compare` Interval comparison | `interval_comparison` | 3–4 semitones apart from the same note → 2–3 → 0–2 → same last note → nearby starts → unrelated → unrelated, either direction |
| `rp_sing` Singing intervals | `sing_interval` | from do: sol/do → mi/sol → up the scale → down the scale → from any note of the key → without a key |

Intervals start in pairs because one interval alone is not a question. To drill a single
interval, use `other_rate`: intervals from outside the set are mixed in and answered Other.

## Controls for custom drills

Any of these can go into a session (`queue_session`) or a saved preset.

- **`interval_id`**
  - `start`: where the first note heard comes from.
    - `random`: anywhere in `root_range`, or any note of the scale in a key.
    - `tonic`: "from do"; without a key, from `start_note`.
    - `fixed`: always `start_note`.
  - `start_octaves`: the octaves that first note moves through, chosen per question. For example `[-1, 0, 1]` gives one interval across random keys and octaves.
  - `in_key`, `keys`, `modes`, `key_change`:
    - `keys: "all"` with `key_change: per_block` gives one random key for the session.
    - `per_trial` gives a new key every question.
  - `chromatic`: in a key, the second note may leave the scale (non-diatonic intervals). Without it, intervals that do not fit the scale are an error.
  - `cadence`: `start`, `key_change` or `every`. `cadence_bpm` sets its tempo.
  - `other_rate` and `other_intervals`: questions answered Other.
  - `labels`: how the two notes are named after answering in a key (`fixed_do` by default, `solfege` or `numbers`). For example: "It was a major third (sol → si)".
- **`interval_comparison`**
  - `shared`, chosen per question:
    - `first`: the same starting note (easiest).
    - `second`: the same last note.
    - `near`: starts a semitone to a minor third apart.
    - `none`: no shared note (hardest).
  - `differences`: semitone gaps between the two intervals; 0 means the same interval.
- **`sing_interval`**
  - `in_key` plays a cadence and then the first note.
  - `start`: `tonic` or `random` (any scale note).
  - `chromatic`, `keys`, `key_change` and `cadence` work as for `interval_id`.
  - The prompt names the notes in fixed do ("From sol, sing si"). The notes stay in the singer's range, which is stretched by up to a fourth when a key would not fit.
- **`scale_degree_id`**
  - `labels`: `fixed_do` (default), `solfege` or `numbers`.
  - `upper_tonic`: answers run 1–8.
  - `cadence`, `cadence_bpm`, `keys` and `key_change` as above.
  - The Practice card has the same choices.

## Against EarMaster's activities

| Activity | Here |
| --- | --- |
| Chord identification, inversions | `chord_id` (`answer_inversion`) |
| Interval identification | `interval_id` and the three interval courses |
| Interval comparison | `interval_comparison` and `rp_compare` |
| Scale identification | `scale_id` |
| Melodic dictation | `melodic_dictation` |
| Interval singing | `sing_interval` and `rp_sing` |
| Melodic sight-singing | `sight_sing` |
| Melody singback | Partly: `pattern` in echo mode (scales, arpeggios, degree patterns) |
| Chord progressions | Not yet |
| Rhythm clapback, rhythmic sight-reading, error detection, rhythmic dictation | Not yet (tap input is planned in `song_and_corpus_app_plan.md`) |

### Still to build

1. **Courses as JSON files** the coach can write: instruction pages, extra questions after a low score, and "finished but not passed".
2. **Melody singback** with generated melodies, graded on pitch and duration.
3. **Chord progressions** from a library of named progressions.
4. **The rhythm ladder** for all four rhythm activities, and pulse joining.
