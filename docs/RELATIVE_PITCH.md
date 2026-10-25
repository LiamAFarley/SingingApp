# Relative pitch: workshops

**For:** the coach agent and the user.
**Pattern:** EarMaster's general workshops (course → workshop → module → lesson) and Functional
Ear Trainer's levels. Only the pattern is theirs; the lessons and their material are this app's own.

## How it is organised

The **Relative pitch** page is a grid of activities in sections (Scale degrees, Intervals, Chords,
Scales, Rhythm, Melodies). Each activity is one course, a *workshop*. Opening it shows:

- **Lessons:** modules, each one pool of material, with their lessons in order. A tick means
  passed, an arrow marks the current lesson, and each lesson shows its best score. A module shows
  how much of it is complete. Select a lesson to see what it asks, then **Start**. Double-click
  starts it too.
- **Custom levels:** your own levels for that activity, built in a settings form (tonic, scale,
  range, chromatics, enabled tones, cadence and so on for scale degrees). Scores are kept per level.
  **Customise this lesson** opens the form filled in from the selected lesson. **Try once** plays
  the settings without saving them.

A lesson is one run of questions with feedback after each (20 for most activities, 8 for
singing, 4 melodies for dictation and sight singing). One run at **90%** passes it. When the
current lesson passes, the course moves on to the next lesson. "Test" lessons give no feedback
until the end. Any lesson can be played at any time; **Make current** moves the course to the
selected lesson.

**Today's set** on the Relative pitch page plays the current lesson of each activity ticked
**Daily**, stretched to its minutes. Ticks and minutes are in the bar under the lesson list.
Scale degrees and Intervals in a key are ticked at first. There is no clearing glissando and
there are no tests here; those are for absolute pitch.

The Practice page no longer lists ear-training presets; it points here.

## The workshops

| Course | Exercise | Modules |
| --- | --- | --- |
| `rp_degrees` Scale degrees | `scale_degree_id` | Major scale, Chromatics in major, Minor scale, Chromatics in minor. Each has 8 lessons: C, bottom half (1–4) → C, top half (5–8) → C, one octave → C, many octaves → not C, one octave → not C, many octaves → random key every question (with its cadence), one octave → the same over many octaves. |
| `rp_intervals` Interval identification | `interval_id`, no key | 12 pool modules (2nds; 3rds; 2nds and 3rds; P4/TT; TT/P5; P4/TT/P5; m2–P5; 6ths; 7ths; 6ths and 7ths; P4–M7; m2–M7). Each pool has four lessons: ascending, descending, harmonic, all three. Then three practice modules (perfect; 3rds and 6ths; 2nds, 7ths and the tritone), where each pair is heard harmonic then mixed before the set's four drills. Then all intervals in the octave. |
| `rp_intervals_key` Intervals in a key | `interval_id`, `in_key` | Up from the tonic (pairs, then the scale) → down from the tonic → mixed and new keys → between any two notes of the key → chromatic from the tonic. Notes are named in fixed do after each answer. |
| `rp_compare` Interval comparison | `interval_comparison`, `pairs_from_pool` | Five pools (perfect; 3rds and 6ths; 2nds, 7ths and the tritone; all simple; compound) × four relations (same first note; a shared note; nearby first notes; no shared note) = 20 modules. Each grows its pool, then ends in a test, up and down. |
| `rp_sing` Interval singing | `sing_interval`, `in_key` | Up from do (1 → 2, 1 → 3, … every degree) → down from do → above or below do (complementary intervals: a 2nd up or a 7th down both reach re). |
| `rp_chords` Chord identification | `chord_id` | 22 modules: triads in contrasting pairs, all triads, sevenths by the tone added to each triad, then the families. Each set has four lessons: arpeggio up, down, block, all three. |
| `rp_inversions` Chord inversions | `chord_id`, `answer_inversion` | 18 modules: each triad, triads with altered fifths, then sevenths in pairs and families, four drills each. The augmented triad and the diminished seventh have no inversions to name. Sus2 is left out because its inversions are sus4 chords on another root. |
| `rp_scales` Scale identification | `scale_id` | Modes (by how they begin, then all), the melodic minor family, the harmonic minor family, the pentatonics, then descending. |
| `rp_dictation` Melodic dictation | `melodic_dictation` | Notes only: 3–4, 5–6, then 7–8 notes in C major, on do and re first and one degree more each lesson. Then rhythm: C major in 4/4 and 3/4, with and without 8th notes, then other major keys, then A minor, then all minor keys. Each rhythm module runs 1-bar then 2-bar melodies through the same degree ramp. |
| `rp_sight` Melodic sight singing | `sight_sing` | The same ladder, with 1, 2 and 4-bar melodies. |

### Not built yet

These show greyed out on the page:

- **Chord progressions**, from a library of named progressions.
- **Melody singback**: hear a melody, sing it back, graded on pitch and duration.
- **The four rhythm activities**: sight-reading, clapback, dictation and error detection. Tap input is planned in `song_and_corpus_app_plan.md`.

Two of EarMaster's melody modules are also not copied:

- "Only do given": there is no tonic-only key reference yet.
- Harmonic minor: the generator already raises the leading tone into the tonic.

## For the coach

- `get_workshop` returns a course's lessons by module, each with its runs, best and last score
  and whether it passed, plus the current lesson. Use it to see where the user is and what keeps
  failing.
- `save_custom_level` adds a drill the lessons lack, for example one interval pair the user keeps
  confusing, or a register. Give the course, a title, a question count and that course's exercise
  params. It is checked by generating it, and appears on the activity's Custom levels tab with
  its own scores. `list_custom_levels` and `delete_custom_level` manage them.
- `set_daily_plan` ticks activities into today's set. `set_course_level` moves an activity to a
  lesson. `preview_daily_set` with category `Relative pitch` shows the set.

## Exercise controls

Any of these can go into a custom level, a session (`queue_session`) or a saved preset.

- **`scale_degree_id`**
  - `degrees`: numbered against the major scale, with `b` and `#`. `8` is the tonic above, and
    can be asked without `1`.
  - `upper_tonic: false` leaves 8 out when 1 is in.
  - `keys`, `modes`, `key_change`: a fixed tonic, one random key for the session (`keys: "all"`,
    `per_block`) or a new key every question (`per_trial`).
  - `octaves`: `[0]` one octave, `[-1, 0, 1]` many.
  - Also `cadence` (`start`, `key_change`, `every`), `cadence_bpm`, `labels` (`fixed_do` by
    default) and `resolution`.
- **`interval_id`**
  - `start`: `random`, `tonic` ("from do") or `fixed` (`start_note`).
  - `start_octaves`: the octaves the first note moves through.
  - `in_key` with `chromatic` lets the second note leave the scale.
  - `other_rate` and `other_intervals` mix in intervals answered Other, so one interval can be
    drilled alone.
  - Also `cadence`, `cadence_bpm` and `labels`.
- **`interval_comparison`**
  - `pairs_from_pool`: both intervals from `intervals`, always different, with no Same answer.
    Otherwise the second interval is `differences` semitones from the first.
  - `shared`: `first`, `second`, `near` or `none`, picked per question.
- **`sing_interval`**
  - `in_key`, `start` (`tonic` or `random`), `chromatic` and `labels`.
  - With both `directions`, an interval that would leave the key one way is sung the other way.
- **`chord_id`**
  - `qualities`: 24 types, including `major_b5`, `dominant7_b5`, `dominant7_s5`,
    `dominant7_sus4`, `dominant7_sus2`, `minor_major7_b5`, `major7_b5`, `major7_s5`,
    `major7_sus4` and `major7_sus2`.
  - `styles`: `arpeggio`, `arpeggio_down` or `block`, mixed per question.
  - `answer_inversion` with `inversions`.
- **`scale_id`**: 28 scales, including the modes of melodic and harmonic minor (for example
  `locrian_s2`, `altered`, `phrygian_dominant`, `ultra_locrian`) and both diminished scales.
  Played with `direction`.
- **Generated melodies** (`melody.generate`)
  - `degrees`: the notes used, 1–8 from the tonic. They are placed around the tonic when the
    range will not hold them above it.
  - `notes`: a fixed count, one quarter note each, with no rhythm.
