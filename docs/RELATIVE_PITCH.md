# Relative pitch: the core and the workshops

**For:** the coach agent and the user.
**Basis:** the research report `documents/Relative Pitch App Exercises.pdf`
(Gordon's Music Learning Theory, Kodály's inner hearing, Karpinski's dictation phases, the
working-memory work of Klonoski, Gates and Chenette), plus EarMaster's workshop pattern and
Functional Ear Trainer's levels. Only the patterns are theirs; the material is this app's own.

**Extracted rationale:** `corpus/knowledge/RPA-PED/` contains the page-addressable
report text, `COACH_BRIEF.md`, `ACTIVITY_MAP.md`, figure transcriptions, and
`SYMBOL_RECOVERY.md`. Use it to explain *why* an activity exists; this file is
the map of what is actually built.

## The daily routine

1. **Core relative pitch** (`rp_core`), every day, about 12 minutes. It is on in the plan by
   default and comes first in Today's set.
2. **Scale degrees** (`rp_degrees`), about 8 minutes, also on by default: the functional
   backbone, now with more questions on the degrees you have
   been missing.
3. Add a workshop to the set (tick **Daily** inside it) when something keeps failing, for a few
   weeks, then take it out again.

The order follows the report: context before content, tonality before intervals, hearing before
labels, memory and inner hearing before notation.

## The core

Twelve sessions in four tiers. Each session mixes three or four short parts and passes when every
checked part reaches **80%** in one session. The N-back streams are practice inside the core and
do not count.

| Tier | Sessions | Parts |
| --- | --- | --- |
| Tonal context | Home and degrees in C → Any key → Changing keys | Find home (sing, then name, then the modes); scale degrees in C, one key, new keys; same/different, probe, transposed same/different |
| Patterns and working memory | Tonic and dominant → Solfège and the first note → Dialogue and canon | Tonal pattern echo (bum, then translated to solfège, then minor); 1-back and 2-back streams; the first note of a pattern; answering with the other function; a canon one note behind; 6ths via a 5th, and wide descending intervals via their inversion |
| Inner hearing and lines | Silent bars → Flash scores and guide tones → From the tonic alone | Sight singing with bars 2–3 silent, a flash score, beat 1 silent; melodic updating with one, two, then three changes; roots of I IV V I without the bass, ii–V–I guide tones, roots from the tonic alone |
| Structure and error detection | Errors and cadences → Singback and four bars → Everything, in minor too | Single-pass error detection (2 bars, 4 bars, semitone errors); mislabeled syllables; cadences (3 kinds, all 5, both modes); melody singback; guide tones from the tonic |

## What the report asked for, and what is here

| Report | Here |
| --- | --- |
| Scale degrees: post-answer resolution, a 1-beat audiation pause | The resolution was already there. The pause is available (`think_ms`) but off: the answers open as the note starts, so you can answer as soon as you know. Adaptive: a degree missed 20% of the time comes up twice as often, at 40% three times (`adaptive`). |
| Resting tone drop-needle ("find home") | `resting_tone`: a phrase outlining the tonality without a cadence stops on a tendency tone; breathe and sing the resting tone, or name it. Eight tonalities with Gordon's resting syllables (do major, la minor and harmonic minor, re Dorian, mi Phrygian, fa Lydian, sol Mixolydian, ti Locrian). |
| Tonal pattern echo with the breath gate and teaching/evaluation passes | `tonal_pattern`: tonic, dominant, subdominant and tendency-tone patterns of 2–5 notes on an even pulse; a silent "Breathe and audiate" beat; each pattern first with the tonic sounding, then alone; on "bum", in solfège, or heard on "bum" and echoed in solfège. |
| I–V dialogue | `pattern_dialogue`: sing only the first note of a quick pattern; answer with a different pattern of the same function; answer with the other function. Free answers are graded on whether every note belongs to the asked function. |
| Musical N-back and canon | `n_back`: recognize (tap Match, scored as d′) at 1, 2 and 3 back, over a quiet tonic; sing (a canon 1 or 2 behind). |
| Melodic word-updating | `melodic_updating`: replace a degree everywhere, change one note, shift a step, sing backwards, leave a note out; chained up to three times, with a silent rehearsal window. The instruction appears only after the melody. |
| Same/different and Sternberg probe | `pattern_memory`. |
| Additive and complementary interval scaffolding | `sing_interval` `scaffold`: a M6 is sung as a 5th then a step; a descending m7 as a 2nd up then the octave down. |
| Kodály silent singing: bar drop-out, strong/weak beats, flash scores | `sight_sing` `silent` (bars, strong_beats, weak_beats; silent notes are greyed and not graded, the re-entry is reported) and `mode: flash` (the score shows while only the tonic sounds, then vanishes). |
| Melody singback | `sight_sing` `mode: singback`: the melody plays after the key, the score stays hidden, sing it back. |
| Bass-line and guide-tone singing | `bass_line`: roots with the bass, without it, from the tonic alone; the 3rd–7th line through ii–V–I and the turnaround. |
| Contextual cadences | `cadence_id`: perfect and imperfect authentic, plagal, half, deceptive, in four-part writing with a lead-in. |
| Single-pass error detection | `error_detection`: the score and a silent countdown, then one hearing (no replay); tap the wrong note. |
| Solfège Stroop | `syllable_check`. |
| Scale identification through modal melodies | Covered by naming the resting tone in the modes. |
| Interval comparison as a warm-up | Not changed (as you asked). |

Not built, and why:

- **Protonotation canvas** (tapping pulse lines, sketching contour): a new drawing input, worth
  doing with the rhythm work.
- **Non-chord-tone explorer** (skeleton toggle, the ten NCT types): needs harmonised melodies with
  marked decorations; the cadence generator is a start.
- **Modulation tracking** and **extractive listening** (memorise one voice of two): need
  multi-voice material; the harmony exercise and the library could supply it later.
- **Drag-and-drop pattern cards** and the **mislabeled-syllable sung track**: the Stroop here is
  the visual version.
- **Voice-leading resolution after chord answers** and **singing the bass before naming an
  inversion**: the bass-line exercise covers the singing; chord answers do not resolve yet.
- **The DSP pipeline**: the app already tracks vibrato and stability, and the AP tests check for a
  constant offset. Stricter cents bands per level are not set.

## How it is organised

The **Relative pitch** page is a grid of activities in sections (Core, Tonal context, Memory and
inner hearing, Intervals, Chords and harmony, Scales, Melodies). Each activity is one course.
Opening it shows:

- **Lessons:** modules, each one pool of material, with their lessons in order. A tick means
  passed, an arrow marks the current lesson, and each lesson shows its best score. A module shows
  how much of it is complete. Select a lesson to see what it asks, then **Start**. Double-click
  starts it too.
- **Custom levels** (activities of one exercise): your own levels for that activity, built in a settings form (tonic, scale,
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
The core and Scale degrees are ticked at first. There is no clearing glissando and
there are no tests here; those are for absolute pitch.

**Settings** (on every activity page) sets tempos, the cadence tempo, note names and the like for
each exercise. They apply to every lesson and to today's set; an empty field keeps the lesson's own
value, and custom levels keep theirs. The core's page lists every exercise it uses.

**Skip part** in a session leaves the current part and goes on to the next. A today's set ended
before it finished shows **Resume** on its page for the rest of the day, carrying on in the part and
at the question where it stopped; **Start over** composes a new one.

The Practice page no longer lists ear-training presets; it points here.

## The workshops

| Course | Exercise | Modules |
| --- | --- | --- |
| `rp_home` Find home | `resting_tone` | Major and minor (sing, then name) → the modes (Dorian, Mixolydian, Lydian, Phrygian, all eight) |
| `rp_patterns` Tonal patterns | `tonal_pattern`, `pattern_dialogue` | Tonic and dominant in major → in minor → with the subdominant → longer patterns → tendency tones → dialogue |
| `rp_memory` Working memory | `pattern_memory`, `n_back`, `melodic_updating` | Same or different → was it in the melody → N-back match (1, 2, 3 back; d′ 2 passes) → N-back canon → melodic updating |
| `rp_inner` Inner hearing | `sight_sing` | Silent bars → strong and weak beats → flash scores |
| `rp_singback` Melody singback | `sight_sing` `singback` | The melody ladder (do and re first, a degree more each lesson, then rhythm, keys, minor) |
| `rp_harmony` Cadences and bass lines | `cadence_id`, `bass_line` | Cadences → bass lines → guide tones |
| `rp_errors` Error detection | `error_detection`, `syllable_check` | Score against sound → mislabeled syllables |
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

- **Rhythm** activities (sight-reading, clapback, dictation, error detection); tap input is planned
  in `song_and_corpus_app_plan.md`.
- Chord progressions beyond cadences and the bass-line progressions.

Two of EarMaster's melody modules are also not copied:

- "Only do given": there is no tonic-only key reference yet.
- Harmonic minor: the generator already raises the leading tone into the tonic.

## For the coach

- Daily work is the core. Point the user at a workshop when one part of the core keeps failing:
  find home or patterns for the first tier, memory for the N-back and updating, inner hearing for
  the silent bars, harmony for the bass lines, errors for error detection.
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
- **Audiation exercises** (all take `keys`, `modes`, `key_change` and `labels` where it makes sense)
  - `resting_tone`: `modes` (scale ids), `answer` (`sing` or `name`), `notes`.
  - `tonal_pattern`: `functions` (`tonic`, `dominant`, `subdominant`, `tendency`), `lengths`
    (2–5), `passes` (`teaching_then_evaluation`, `teaching`, `evaluation`), `syllables`
    (`neutral`, `solfege`, `translate`), `breath_beats`.
  - `pattern_dialogue`: `stage` (`first_pitch`, `same_function`, `opposite_function`).
  - `n_back`: `n` (1–3), `mode` (`recognize`, `sing`), `degrees`, `length`, `ioi_ms`,
    `target_rate`, `drone`.
  - `melodic_updating`: `notes`, `degrees`, `changes` (1–3), `operations` (`replace_degree`,
    `replace_position`, `shift`, `retrograde`, `omit`), `think_beats`.
  - `pattern_memory`: `task` (`same_different`, `probe`), `notes`, `change` (`step`,
    `chromatic`, `any`), `transpose`, `gap_ms`.
  - `error_detection`: `melody`, `pre_audiation_s`, `change`.
  - `syllable_check`: `notes`, `degrees`.
  - `cadence_id`: `cadences`, `lead_in`.
  - `bass_line`: `line` (`roots`, `guide_tones`), `backing` (`with_bass`, `without_bass`,
    `tonic_only`), `progressions`, `beats_per_chord`.
- **`sight_sing`**: `mode` (`read`, `singback`, `flash` with `flash_s`) and `silent` (`bars`
  with `silent_bars`, `strong_beats`, `weak_beats`).
- **`sing_interval`** `scaffold`: `additive` or `complementary`.
- **`scale_degree_id`**: `think_ms` (an optional pause before the answers open; off by default) and
  `adaptive`.
