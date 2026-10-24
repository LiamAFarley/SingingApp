# Song Practice Ingestion, Separation, Transcription, and Synchronized Playback
## Focused implementation specification

**Scope:** This subsystem takes a normal song file such as MP3/M4A/FLAC/WAV and turns it into a structured practice asset containing:

- lead vocal audio
- backing/accompaniment audio
- optional backing-vocal audio
- a complete-song multitrack transcription where possible
- a dedicated lead-vocal transcription
- a canonical beat/downbeat/tempo map
- optional MusicXML
- synthesized lead-melody reference audio
- simplified piano accompaniment
- synchronized slow-down / looping / transposition
- a reference timeline against which live singing can be pitch-tracked

The output is intended to plug into the larger singing/ear-training application.

---

# 1. Desired user experience

The user imports:

```text
song.mp3
```

The application performs offline analysis and produces a practice package.

The practice view can then offer combinations such as:

```text
[Backing track] + [user singing]
[Backing track] + [quiet original lead vocal] + [user singing]
[Backing track] + [piano playing lead melody] + [user singing]
[Simplified piano accompaniment] + [piano lead cue]
[Simplified piano accompaniment] + [user singing]
[Full original song]
[Lead vocal only]
[Backing vocals only]
```

At any point the user can:

- slow the song down without changing pitch
- optionally transpose the song
- loop bars or phrases
- enable a count-in
- hear a metronome locked to the actual song
- show the expected vocal notes
- show the original singer's pitch trajectory
- show the user's live pitch trajectory
- receive pitch/timing scoring
- mute/unmute reference vocal or piano melody
- move between arbitrary sections without losing synchronization

The audio, MIDI, score, metronome, live pitch target, and visual playhead must all use **one transport clock**.

---

# 2. High-level pipeline

```text
MP3 / FLAC / M4A / WAV
          │
          ▼
┌─────────────────────────┐
│ Canonical audio decode  │
│ lossless PCM timeline   │
└────────────┬────────────┘
             │
             ├─────────────────────────────────────────┐
             ▼                                         ▼
┌─────────────────────────┐                 ┌─────────────────────────┐
│ Source separation       │                 │ Beat / structure        │
│                         │                 │ analysis                │
│ all vocals              │                 │                         │
│ instrumental            │                 │ beats                   │
│ lead vocal              │                 │ downbeats               │
│ backing vocals          │                 │ tempo map               │
└────────────┬────────────┘                 │ meter / sections        │
             │                              └────────────┬────────────┘
             │                                           │
             ├─────────────────────┐                     │
             ▼                     ▼                     │
┌──────────────────────┐  ┌──────────────────────┐       │
│ Whole-song AMT       │  │ Lead-vocal analysis  │       │
│ MuScriptor-class     │  │ F0 + note events     │       │
└───────────┬──────────┘  └───────────┬──────────┘       │
            │                         │                  │
            └─────────────┬───────────┴──────────────────┘
                          ▼
               ┌───────────────────────┐
               │ Canonical song model  │
               │                       │
               │ source-time map       │
               │ beat/tempo map        │
               │ multitrack MIDI       │
               │ vocal melody          │
               │ confidence/provenance │
               └───────────┬───────────┘
                           │
          ┌────────────────┼──────────────────┐
          ▼                ▼                  ▼
   MusicXML/MIDI     piano reduction     practice player
                                           │
                                           ▼
                                  live vocal tracking
```

---

# 3. Stage 0 — Decode once into a canonical audio master

Do not let every model independently decode the original MP3.

MP3 decoders can produce small timing-offset differences. Even a few tens of milliseconds are undesirable for beat-aligned practice.

On import:

```text
song.mp3
   │
   ▼
ffmpeg
   │
   ▼
source_master.wav
```

Recommended canonical representation:

```text
PCM WAV
stereo
float32 or 24-bit PCM
48 kHz for practice assets
exact duration stored in samples
```

Every downstream result must refer to this same canonical source timeline.

Store:

```json
{
  "source_sample_rate": 48000,
  "source_frames": 11482368,
  "duration_seconds": 239.216,
  "source_hash": "...",
  "original_file": "song.mp3",
  "canonical_file": "source_master.wav"
}
```

Never derive alignment from a newly decoded copy of the MP3 later.

---

# 4. Stage 1 — Vocal / accompaniment separation

The first required separation is:

```text
full mix
├── vocals_all
└── instrumental_no_vocals
```

Use a pluggable separator interface rather than hard-coding one network.

Current useful model families include:

- BS-RoFormer
- MelBand-RoFormer
- MDX/MDXC
- Demucs as a mature fallback

A high-quality two-stem RoFormer model should be the preferred offline path when GPU resources permit.

The Python `audio-separator` ecosystem is useful as the initial backend because it can drive UVR-style MDX, Demucs, and RoFormer checkpoints behind a common API.

---

# 5. Stage 1B — Lead vocal versus backing vocals

A normal "vocals" stem contains:

- lead singer
- harmonies
- doubles
- backing vocals
- ad-libs

For singing practice it is useful to split that result again.

```text
vocals_all
├── lead_vocal
└── backing_vocals
```

A practical current route is a karaoke-specific separator applied **only to the already separated vocal stem**.

Useful current model classes include:

- MelBand-RoFormer Karaoke models
- UVR MDX-Net Karaoke models

The important architecture is two-stage:

```text
full mix
   │
   ├── high-quality vocals/instrumental separator
   │
   ▼
all vocals
   │
   ├── karaoke lead/backing separator
   │
   ▼
lead vocal + backing vocals
```

Do not apply the karaoke separator directly to the complete mixture unless the chosen model specifically expects that input.

---

# 6. Separation outputs

Persist at least:

```text
audio/
├── source_master.wav
├── vocals_all.wav
├── instrumental_no_vocals.wav
├── lead_vocal.wav
├── backing_vocals.wav
└── accompaniment_with_backing.wav
```

`accompaniment_with_backing.wav` is:

```text
instrumental_no_vocals + backing_vocals
```

This is likely the most useful normal practice backing track because the lead singer is absent while harmonies and chorus support remain.

Also keep the **pure instrumental** because backing vocals can sometimes make pitch practice harder.

The practice mixer should therefore expose:

```text
PURE INSTRUMENTAL
INSTRUMENTAL + BACKING VOCALS
FULL ORIGINAL
```

---

# 7. Separation alignment requirements

Every separated stem must:

- have the same sample rate
- begin at source sample zero
- have the same nominal number of samples
- preserve source timing
- record any model-introduced offset if discovered

After separation, run a validation step.

Checks:

```text
length difference
cross-correlation against source where useful
leading silence discrepancy
trailing padding
sample-rate mismatch
NaN/clipping checks
```

Pad/crop stems to the canonical source length rather than allowing them to develop independent timelines.

---

# 8. Stage 2 — Beat, downbeat, tempo, and structure analysis

Do **not** reduce a song to one BPM number.

The canonical timing representation should be a **beat map**:

```text
beat index → source timestamp
```

Example:

```json
[
  {"beat": 0, "time": 0.482, "bar": 1, "beat_in_bar": 1},
  {"beat": 1, "time": 1.001, "bar": 1, "beat_in_bar": 2},
  {"beat": 2, "time": 1.518, "bar": 1, "beat_in_bar": 3},
  {"beat": 3, "time": 2.037, "bar": 1, "beat_in_bar": 4},
  {"beat": 4, "time": 2.553, "bar": 2, "beat_in_bar": 1}
]
```

This handles:

- songs played to a click
- small tempo drift
- live performances
- ritardando/accelerando
- rubato
- tempo changes

---

# 9. Beat-analysis backends

## Primary beat/downbeat tracker

**Beat This!** is a strong open-source default.

It predicts:

- beats
- downbeats

and was explicitly designed to work without rigid DBN tempo/meter constraints, which is useful when songs contain tempo changes or unusual meter.

## Optional structure analyzer

An **All-In-One**-style structure analyzer can additionally provide:

- BPM estimate
- beats
- downbeats
- beat positions
- intro/verse/chorus/bridge/outro-like section boundaries

These structure labels are useful for automatic practice-region creation.

## MuScriptor tempo map

MuScriptor itself can detect:

- beats
- time signature
- bar lines
- variable tempo

and can write a variable MIDI tempo map instead of forcing one constant BPM.

Therefore retain its detected tempo grid as another evidence source.

---

# 10. Canonical BeatGrid

Normalize all timing estimates into one object:

```json
{
  "meter": {
    "numerator": 4,
    "denominator": 4,
    "confidence": 0.96
  },
  "global_bpm": 116.8,
  "beats": [
    {
      "index": 0,
      "source_sec": 0.482,
      "bar": 1,
      "beat_in_bar": 1,
      "downbeat": true,
      "confidence": 0.98
    }
  ],
  "tempo_segments": [
    {
      "start_beat": 0,
      "end_beat": 32,
      "bpm": 116.6
    }
  ],
  "sources": [
    "beat_this",
    "muscriptor"
  ]
}
```

The UI must permit manual correction of:

- first downbeat
- half/double tempo
- beats per bar
- individual misaligned beats
- tempo-section boundaries

One corrected beat map should become authoritative for every downstream practice feature.

---

# 11. Stage 3 — Whole-song automatic transcription

Run **MuScriptor** or an equivalent modern multi-instrument AMT model on the canonical full mix.

The goal is a multitrack symbolic representation such as:

```text
lead/voice if detected
bass
guitar
keyboard
drums
other pitched instruments
```

Required outputs:

```text
full_unquantized.mid
full_quantized.mid
full.musicxml
full_transcription.json
```

The **unquantized MIDI** preserves model-estimated performance timing.

The **quantized MIDI/MusicXML** is for notation and musical-grid operations.

Do not use the quantized copy as the only source of timing truth.

---

# 12. Whole-song transcription should be instrument aware

Store every note as:

```json
{
  "pitch": 64,
  "onset_source_sec": 52.184,
  "offset_source_sec": 52.603,
  "beat": 104.25,
  "duration_beats": 0.75,
  "velocity": 81,
  "instrument": "electric_guitar",
  "confidence": 0.78,
  "source": "muscriptor"
}
```

The critical field is both:

```text
source-time coordinates
AND
beat coordinates
```

Do not choose one or the other.

---

# 13. Stage 4 — Dedicated lead-vocal transcription

Whole-song transcription is not enough for vocal practice.

The lead vocal needs its own higher-resolution analysis.

Use:

```text
lead_vocal.wav
   │
   ├── dense F0 estimation
   │
   ├── voiced/unvoiced estimation
   │
   ├── note segmentation
   │
   └── beat-grid alignment
```

The lead-vocal representation should preserve two fundamentally different things.

---

# 14. Representation A — expressive vocal pitch contour

Store the original singer's dense F0 trajectory.

Example:

```text
time       frequency     midi_float     confidence
52.180     329.0 Hz      64.00          0.98
52.190     331.8 Hz      64.15          0.98
52.200     334.4 Hz      64.29          0.97
...
```

This contains:

- scoops
- falls
- slides
- vibrato
- pitch drift
- ornaments

This is useful in an **imitate recording** mode.

It should *not* automatically be treated as the canonical note target.

---

# 15. Representation B — canonical vocal melody notes

Create discrete musical notes:

```json
{
  "note": "E4",
  "midi": 64,
  "onset_source_sec": 52.184,
  "offset_source_sec": 52.603,
  "onset_beat": 104.25,
  "offset_beat": 105.0,
  "pitch_center_cents": 3.7,
  "confidence": 0.95
}
```

This representation is used for:

- normal singing grading
- piano melody reference
- notation
- looping by phrase
- exercise extraction

For ordinary intonation grading, compare the learner primarily against the **musical note center**, not every instantaneous wiggle made by the original singer.

---

# 16. Vocal F0 backend

A dedicated vocal pitch estimator should be used even if MuScriptor also predicts voice notes.

Useful candidate:

**RMVPE**

It was designed specifically for vocal pitch estimation in polyphonic music and can also operate on mixtures.

Recommended strategy:

```text
lead_vocal.wav → primary RMVPE F0
full_mix.wav   → optional RMVPE cross-check
MuScriptor     → note-event cross-check
```

The separated lead stem gives the cleanest input in many cases.

The full-mix RMVPE pass provides useful protection against separation artifacts that accidentally delete or distort a vocal note.

---

# 17. Vocal-note segmentation

Convert dense F0 into note events using:

- voiced/unvoiced regions
- pitch-center stability
- local pitch derivatives
- expected beat subdivisions
- MuScriptor vocal note hypotheses
- minimum note duration
- confidence thresholds

Do not naïvely split every pitch fluctuation.

A sustained note with vibrato must remain one note.

A slide between two actual notes may require two symbolic notes plus a connecting expressive contour.

Store uncertain boundaries rather than inventing certainty.

---

# 18. Vocal-transcription confidence

Every vocal note should carry independent confidence values where possible:

```json
{
  "f0_confidence": 0.98,
  "segmentation_confidence": 0.83,
  "beat_alignment_confidence": 0.95,
  "separation_quality": 0.88,
  "consensus_confidence": 0.92
}
```

Low-confidence segments should be visually marked for correction before being used for strict grading.

---

# 19. Transcription review UI

The vocal editor should display together:

```text
lead-vocal waveform
spectrogram
dense F0 trace
detected notes
beat grid
staff/piano roll
confidence
```

Editing:

- change pitch
- split note
- merge notes
- move onset
- move offset
- mark breath/rest
- mark ornament
- mark uncertain
- mark verified

Human-corrected vocal notes become authoritative.

---

# 20. Canonical timing model

This is the most important engineering requirement in the subsystem.

There are three coordinate systems:

```text
SOURCE TIME
seconds / source samples in the original canonical audio

MUSICAL TIME
bar / beat / fractional beat / MIDI tick

PRACTICE OUTPUT TIME
seconds heard by the user after time-stretching
```

All objects should be convertible between these systems.

---

# 21. Source-time mapping

Source-time is immutable.

Examples:

```text
lead note starts at 52.184 source seconds
chorus starts at 61.730 source seconds
beat 105 occurs at 52.603 source seconds
```

Source separation, transcription, looping, lyrics, and section markers all refer back to this timeline.

Never rewrite these values because the user changed practice speed.

---

# 22. Musical-time mapping

Represent musical positions using either:

```text
fractional beat
```

or:

```text
bar + beat + subdivision
```

Internally, also support MIDI ticks.

Recommended:

```text
PPQ = 960
```

Functions:

```text
sourceSecondsToBeat(t)
beatToSourceSeconds(b)

beatToMidiTick(b)
midiTickToBeat(tick)
```

Use interpolation between neighboring detected beats.

---

# 23. Variable-tempo map

For consecutive canonical beats:

```text
beat i     at time t_i
beat i+1   at time t_(i+1)
```

the local performed tempo is approximately:

```text
BPM_i = 60 / (t_(i+1) - t_i)
```

The MIDI tempo map should reproduce this beat timing.

This means MIDI follows the actual recording rather than drifting away because the song was assumed to be exactly 120 BPM.

---

# 24. Practice-speed model

Define:

```text
speed = 1.00   normal
speed = 0.75   75% speed
speed = 1.20   120% speed
```

For a constant practice speed:

```text
practice_time = source_time / speed
```

and:

```text
practice_BPM(local) = source_BPM(local) * speed
```

Example:

```text
original local tempo = 120 BPM
practice speed       = 0.75

playback tempo        = 90 BPM
```

The MIDI events themselves remain attached to beats.

The transport changes how fast beats progress.

This avoids destructively rewriting the MIDI whenever the speed slider changes.

---

# 25. Target-BPM mode

Users may prefer to type:

```text
Practice at 90 BPM
```

rather than:

```text
Practice at 75%
```

Choose a reference BPM, normally the median/nominal song BPM.

Then:

```text
speed = requested_BPM / nominal_BPM
```

Apply that same speed multiplier to the complete variable tempo map.

If the original song has local fluctuations, those proportional fluctuations remain.

Example:

```text
original local tempo:
118 → 120 → 121 → 119

target based on 90/120 = 0.75:
88.5 → 90 → 90.75 → 89.25
```

This sounds much more natural than flattening every beat to exactly 90 BPM.

Provide a separate **force fixed tempo** mode only if desired.

---

# 26. Time stretching

Time stretching must preserve musical pitch.

Recommended C++ integration:

## Signalsmith Stretch

Advantages:

- native C++
- MIT license
- real-time capable
- independent pitch and time control
- explicit latency/query support
- good fit for the existing native audio engine

Its own documentation suggests strongest quality in relatively moderate ranges such as roughly 0.75x–1.5x, which covers most practice use.

## Rubber Band

Alternative high-quality time-stretch/pitch-shift engine.

Important licensing note:

- open-source under GPL
- proprietary distribution requires its commercial licensing route

Make the stretcher an interface so the implementation can change later.

---

# 27. Synchronized stem stretching

All audible stems must advance through exactly the same source timeline.

Two implementation strategies are valid.

## Strategy A — shared pre-stretch mix

```text
selected stems
→ gains/mutes
→ summed stereo mix
→ one time stretcher
→ output
```

Advantages:

- guaranteed alignment
- lowest compute cost

Disadvantage:

- switching stems instantly can create artifacts because the stretcher's internal state already contains the previous mix

## Strategy B — synchronized per-stem stretchers

```text
instrumental ─► stretcher A ─┐
backing vocal ─► stretcher B ├► mixer
lead vocal ───► stretcher C ─┤
other refs ───► stretcher D ─┘
```

Every stretcher receives:

- identical speed
- identical source position
- identical seek/reset events
- identical output-frame request count

Advantages:

- reference tracks can be faded/muted freely
- individual stem levels stay editable

Use this for the full practice mixer if CPU permits.

---

# 28. Seeking and looping

A seek operation must atomically reposition:

```text
all audio stems
time-stretch state
MIDI scheduler
metronome
score cursor
reference pitch target
live grading window
```

For a loop:

```text
bar 17 beat 1 → bar 20 beat 1
```

resolve both endpoints into source-time positions using the beat map.

At loop rollover:

1. reset/reseek stretchers
2. reposition MIDI transport
3. start from a small pre-roll if required by the stretcher
4. maintain count-in settings
5. avoid accumulating timing error over repeated loops

Do not implement looping using independent wall-clock timers.

---

# 29. Transposition

Practice may need the song in a different key.

Apply the same semitone offset to:

```text
audio playback pitch
lead-vocal MIDI
full-song MIDI
piano melody
piano accompaniment
displayed note names
grading target
```

Example:

```text
transpose = -2 semitones
```

The lead note:

```text
F#4 → E4
```

and the backing audio must also shift down two semitones.

The user microphone is **not** pitch-shifted for grading.

Grade the user against the transposed target.

---

# 30. Practice mixer

Suggested channels:

```text
1. instrumental
2. backing vocals
3. original lead vocal
4. lead-melody piano
5. simplified piano accompaniment
6. metronome
7. user microphone monitor
```

Each channel gets:

- mute
- solo
- gain
- optional pan
- source label

Useful presets:

### Original

```text
full mix
```

### Karaoke

```text
instrumental + backing vocals
```

### Pure backing

```text
instrumental only
```

### Learn melody

```text
instrumental
+ lead vocal at reduced level
+ optional piano melody
```

### Pitch drill

```text
simplified piano accompaniment
+ piano melody
```

### Test

```text
backing only
no melody reference
```

---

# 31. Lead-melody piano reference

This is straightforward once the vocal note track is verified.

```text
lead vocal notes
      │
      ▼
MIDI melody track
      │
      ▼
piano / synth renderer
```

The piano renderer follows:

- canonical beat positions
- practice tempo
- transposition
- loops
- dynamics if desired

Reference options:

```text
FULL NOTE
  piano plays each complete vocal note

ONSET CUE
  short piano attack only at note onset

PHRASE PREVIEW
  piano plays the phrase once, then goes silent during singing

WEAK-NOTES ONLY
  reference plays only notes the learner repeatedly misses
```

The last mode connects particularly well to the adaptive training system.

---

# 32. Original-singer reference

The lead vocal stem can be mixed in as a reference.

Useful controls:

```text
lead reference level
left/right balance
reference on/off
preview phrase
duck while user sings
```

A useful practice behavior:

```text
first repetition:
  lead vocal + backing

second repetition:
  quiet lead vocal + backing

third repetition:
  piano melody + backing

fourth repetition:
  backing only
```

This can be automated by the adaptive session planner.

---

# 33. Simplified piano accompaniment

There are two different useful products.

## A. Literal piano reduction

Take the full-song transcription and:

1. remove the lead-vocal track
2. optionally remove drums
3. merge pitched instrumental notes
4. map them to one piano program
5. deduplicate simultaneous near-identical pitches
6. compress extreme registers
7. optionally simplify dense voicings

This preserves much of the real arrangement.

## B. Harmonic practice reduction

Infer a simpler accompaniment:

```text
bass/root
+
chord tones
+
basic rhythm
```

Possible rules:

- preserve chord changes
- preserve important bass motion
- reduce each harmony to 3–4 notes
- remove decorative fills
- quantize accompaniment rhythm more strongly
- use simple block chords or arpeggios

This is often better for vocal practice than a literal transcription.

Store the two as different assets rather than pretending one is objectively correct.

---

# 34. Chord/harmony analysis

Whole-song note tracks can support chord inference.

The initial implementation can derive harmonic candidates from:

- MuScriptor pitched notes
- bass track
- chroma features
- beat/downbeat positions

Store chord annotations per beat or half-bar.

Example:

```json
{
  "start_beat": 64,
  "end_beat": 68,
  "root": "A",
  "quality": "minor",
  "bass": "A",
  "confidence": 0.88
}
```

These are primarily for:

- simplified piano accompaniment
- harmonic display
- future harmony-singing exercises

Do not require chord analysis for basic vocal practice.

---

# 35. Live pitch tracking during song practice

The main app's low-latency microphone system supplies:

```text
timestamp
F0
pitch confidence
voiced/unvoiced
```

The song-practice engine knows the current **source-time position** from the master transport.

Therefore:

```text
current source position
→ current expected vocal note
→ target MIDI pitch
→ user cents error
```

Display simultaneously:

```text
expected symbolic note
original singer F0 trace
user F0 trace
```

These should be visually distinguishable.

---

# 36. Two pitch-comparison modes

## Musical accuracy mode

Compare the learner to:

```text
canonical vocal note centers + timing
```

Use this for normal practice.

Do not require copying the original singer's:

- vibrato extent
- scoop
- slide
- pitch drift

## Imitation mode

Compare against the original singer's dense F0 contour.

This can evaluate:

- scoop timing
- portamento
- vibrato onset
- expressive pitch contours

Do not conflate imitation with basic intonation accuracy.

---

# 37. Latency compensation

Live pitch tracking is only useful if recording latency is known.

The shared app audio engine should provide calibrated:

```text
input latency
output latency
analysis delay
manual correction
```

When scoring:

```text
observed microphone time
→ latency corrected source/practice time
→ expected vocal note
```

The visual pitch trace may be delayed slightly for stability, but the recorded timestamps should retain the corrected timing.

---

# 38. Phrase and section extraction

Use beat/downbeat and structural analysis to automatically identify:

```text
intro
verse
pre-chorus
chorus
bridge
outro
```

These labels are suggestions.

The user can create arbitrary practice regions:

```text
bars 17–20
chorus 1
"high phrase"
last two lines
```

A region object should store musical coordinates:

```json
{
  "start_beat": 64,
  "end_beat": 80,
  "label": "chorus line 1"
}
```

Source seconds are resolved through the canonical beat map.

---

# 39. Automatic phrase boundaries

For vocal practice, derive phrase suggestions from:

- vocal rests
- breath-sized gaps
- lyrical boundaries if available later
- bar lines
- structural boundaries

Example:

```text
phrase starts after >= 400 ms vocal silence
unless the silence occurs inside an obvious sustained musical phrase
```

This does not need to be perfect because the user can edit phrase boundaries.

---

# 40. Optional lyrics integration

Not required for V1, but the data model should allow later alignment of:

```text
word
syllable
phoneme
```

to source time and beats.

Prefer imported/synchronized lyric data when available.

Automatic speech recognition on singing should be treated as lower confidence than ordinary speech ASR.

Do not make lyric recognition a dependency for note transcription.

---

# 41. Practice asset directory

Example:

```text
songs/
└── <song-id>/
    ├── metadata.json
    ├── audio/
    │   ├── source_master.wav
    │   ├── vocals_all.wav
    │   ├── lead_vocal.wav
    │   ├── backing_vocals.wav
    │   ├── instrumental.wav
    │   └── accompaniment_with_backing.wav
    │
    ├── timing/
    │   ├── beat_grid.json
    │   ├── sections.json
    │   └── tempo_map.mid
    │
    ├── transcription/
    │   ├── full_raw.mid
    │   ├── full_quantized.mid
    │   ├── full.musicxml
    │   ├── full_notes.json
    │   ├── vocal_notes.json
    │   └── vocal_f0.bin
    │
    ├── generated/
    │   ├── vocal_melody.mid
    │   ├── piano_reduction.mid
    │   └── harmonic_reduction.mid
    │
    ├── edits/
    │   └── user_overrides.json
    │
    └── cache/
```

---

# 42. Canonical data model

## SongPracticeAsset

```json
{
  "song_id": "uuid",
  "title": "...",
  "duration_source_sec": 239.216,
  "sample_rate": 48000,
  "nominal_bpm": 116.8,
  "meter": "4/4",
  "key_estimate": "E major",
  "beat_grid_id": "...",
  "lead_vocal_track_id": "...",
  "full_transcription_id": "...",
  "verification_state": "partially_verified"
}
```

## VocalNote

```json
{
  "id": 184,
  "midi": 66,
  "source_onset_sec": 64.882,
  "source_offset_sec": 65.394,
  "beat_onset": 128.5,
  "beat_offset": 129.5,
  "pitch_center_cents": -4.2,
  "confidence": 0.94,
  "verified": false
}
```

## BeatPoint

```json
{
  "index": 128,
  "source_sec": 64.624,
  "bar": 33,
  "beat_in_bar": 1,
  "downbeat": true,
  "confidence": 0.99
}
```

---

# 43. Analysis job graph

Make ingestion resumable and cacheable.

```text
decode
  │
  ├── separate_vocals
  │       │
  │       └── separate_lead_backing
  │
  ├── beat_track
  │
  ├── structure_analysis
  │
  └── full_transcription
          │
          └── full_note_alignment

lead_vocal
  │
  └── vocal_f0
          │
          └── vocal_note_segmentation

beat_grid + full transcription
  │
  └── piano_reduction

beat_grid + vocal notes
  │
  └── vocal_melody_midi
```

Each job has:

```text
input hashes
model/version
parameters
output hashes
status
progress
error log
```

If the user changes only the beat grid, do not rerun source separation.

---

# 44. Quality presets

## Fast import

Use:

- fast vocal separator
- Beat This!
- MuScriptor small/medium
- lead-vocal F0
- no lead/backing split unless requested

Goal: quickly make a usable practice asset.

## High quality

Use:

- high-quality RoFormer vocals/instrumental
- dedicated karaoke lead/backing split
- MuScriptor large
- RMVPE/F0 cross-check
- consensus beat grid
- full structure analysis
- optional piano reduction

## Reanalyze vocal only

Allows replacement of the lead-vocal transcription without repeating the expensive full-song jobs.

---

# 45. Recommended current component stack

As of October 2026, a practical implementation stack is:

| Task | Preferred initial component |
|---|---|
| decode | FFmpeg |
| 2-stem vocals/instrumental | high-quality BS/MelBand-RoFormer through `audio-separator` |
| lead/backing split | karaoke-specific MelBand-RoFormer or UVR Karaoke model |
| beat/downbeat | Beat This! |
| structure labels | All-In-One-Infer |
| whole-song AMT | MuScriptor |
| lead-vocal F0 | RMVPE |
| simple stem-level AMT fallback | Basic Pitch |
| notation | MusicXML + existing score renderer |
| realtime stretching | Signalsmith Stretch |
| alternative stretching | Rubber Band |
| playback / live input | existing JUCE ASIO/WASAPI engine |

All of these should sit behind interfaces so they can be replaced.

---

# 46. Licensing notes

This matters if the application is distributed.

## MuScriptor

- code: MIT
- current published model weights: CC BY-NC 4.0

Therefore the current weights are appropriate for non-commercial use but require replacement/licensing review for a commercial product.

## UVR/community separator weights

The software wrappers may be permissively licensed while individual community model weights may have unclear or separate licensing.

Do not assume wrapper license == model license.

## Signalsmith Stretch

MIT.

This is particularly convenient for an application that may eventually be distributed.

## Rubber Band

GPL for open-source use, with a commercial licensing option for proprietary distribution.

Keep licensing metadata in the model/plugin manifest.

---

# 47. Practice transport API

The song-practice engine should expose something resembling:

```text
loadSong(song_id)

play()
pause()
seekSourceSeconds(t)
seekBeat(b)

setLoop(startBeat, endBeat)
clearLoop()

setSpeed(factor)
setTargetBpm(bpm)

setTranspose(semitones)

setChannelGain(channel, gain)
setChannelMute(channel, bool)

enableMetronome(bool)
enableCountIn(bars)

getCurrentSourceTime()
getCurrentBeat()
getCurrentBar()

getCurrentVocalTarget()
```

Everything else in the UI uses this transport.

---

# 48. Reference renderer API

```text
setMelodyReference(
    mode = "none" |
           "original_vocal" |
           "piano_full_notes" |
           "piano_onsets" |
           "weak_notes_only"
)

setAccompaniment(
    mode = "instrumental" |
           "instrumental_plus_backing_vocals" |
           "full_mix" |
           "literal_piano_reduction" |
           "harmonic_piano_reduction"
)
```

This abstraction will make adaptive practice much easier.

---

# 49. Pitch-tracking integration API

At each analysis update:

```json
{
  "practice_time": 83.004,
  "source_time": 62.253,
  "beat": 119.38,
  "expected_note": 69,
  "user_midi_float": 68.92,
  "error_cents": -8.0,
  "confidence": 0.97
}
```

The live pitch subsystem should not need to know how source separation or time stretching works.

It only asks the transport:

```text
Where in canonical source time are we now?
What vocal target exists here?
```

---

# 50. Visual practice screen

Recommended layout:

```text
┌─────────────────────────────────────────────────────────────┐
│ waveform / sections / loop                                 │
├─────────────────────────────────────────────────────────────┤
│ score or piano roll                                         │
│ current expected vocal note                                 │
├─────────────────────────────────────────────────────────────┤
│ original singer F0  ─────────                               │
│ target note center   ─────────────────                       │
│ user F0             ────────                                │
├─────────────────────────────────────────────────────────────┤
│ backing  [■■■■■■]   lead ref [■■□□□]   piano cue [■■■□□]   │
│ speed 80%   transpose -1   metronome ON   loop 17–20       │
└─────────────────────────────────────────────────────────────┘
```

Allow switching between:

- score
- piano roll
- simplified pitch lane
- lyrics later

---

# 51. Verification workflow

Automatic transcription will sometimes be wrong.

Use states:

```text
RAW
MODEL_CONSENSUS
REVIEW_NEEDED
USER_VERIFIED
```

Strict scoring should prefer:

```text
USER_VERIFIED
>
high-confidence consensus
>
single-model hypothesis
```

A one-click action should let the user mark an entire phrase as verified after listening.

---

# 52. Failure modes to anticipate

## Separation

- backing vocals remain in lead stem
- lead doubles leak into backing stem
- reverb remains in both
- instruments leak into vocal stem

Mitigation:

- confidence flags
- optional alternate separator
- optional de-reverb before lead/backing separation
- manual stem choice

## Vocal F0

- octave errors
- breathy/unvoiced notes
- growls/distortion
- harmonized lead lines
- separation artifacts

Mitigation:

- RMVPE/full-mix cross-check
- note continuity prior
- range prior
- manual correction

## Beat map

- half/double tempo
- pickup measures
- meter changes
- rubato
- weak/no drums

Mitigation:

- Beat This! + MuScriptor comparison
- editable first downbeat/meter
- local tempo map

## Full-song transcription

- dense mixes
- unusual instruments
- overlapping same-register parts
- expressive timing
- production effects

Mitigation:

- preserve confidence
- retain raw timing
- use stems for targeted re-transcription
- never treat full AMT output as unquestionable ground truth

---

# 53. V1 scope

A useful first implementation needs only:

1. import MP3
2. canonical WAV decode
3. vocals/instrumental separation
4. optional lead/backing separation
5. beat/downbeat detection
6. canonical beat grid
7. whole-song MuScriptor transcription
8. lead-vocal F0 + note segmentation
9. lead melody MIDI
10. piano lead reference
11. synchronized backing-track playback
12. time stretch
13. looping by beat/bar
14. live pitch target lookup
15. manual vocal-note correction

This already enables the core use case.

---

# 54. V2 scope

Add:

- full multitrack transcription editor
- MusicXML cleanup
- automatic section naming
- literal piano reduction
- harmonic piano reduction
- chord analysis
- lyrics alignment
- automatic phrase extraction
- adaptive weak-note reference playback
- "reference first repetition, no reference second repetition"
- automatically generated practice loops from singing errors
- export of corrected vocal MIDI/MusicXML

---

# 55. Implementation order

Recommended order:

```text
1. canonical decoder
2. song asset + source timeline
3. separator interface
4. vocals/instrumental
5. beat/downbeat map
6. source-time ↔ beat conversion
7. practice transport
8. synchronized time stretch
9. lead-vocal F0
10. vocal note segmentation
11. piano melody reference
12. live pitch target integration
13. whole-song MuScriptor
14. lead/backing vocal split
15. transcription correction UI
16. piano reduction
17. structure/chord/lyrics extras
```

The transport and timeline should be implemented **before** fancy automatic transcription UI.

If the timing model is wrong, every later feature becomes fragile.

---

# 56. Acceptance tests

The subsystem is successful when all of these are true.

## Import

A common MP3 produces a cached canonical song asset.

## Separation

The user can hear:

```text
lead vocal
instrumental
instrumental + backing vocals
```

as independently controllable tracks.

## Timing

The metronome remains perceptually locked for the full song.

## MIDI

The full-song MIDI and lead-vocal melody remain synchronized with the recording.

## Slow practice

At:

```text
50%
75%
90%
100%
125%
```

the audio, MIDI, score cursor, and target vocal notes remain synchronized.

## Looping

A four-bar loop can repeat 100 times without accumulating drift.

## Transposition

Audio, MIDI, displayed notes, and pitch target all transpose together.

## Vocal reference

The user can switch between:

```text
original lead singer
piano melody
no melody reference
```

without changing the canonical position.

## Live grading

The application can answer at every moment:

```text
what note should the user be singing?
how many cents away are they?
how early/late are they?
```

using the same timeline that drives the backing track.

---

# 57. Central engineering rule

The entire subsystem should be organized around this invariant:

> **The canonical song is one immutable source-time timeline plus a beat/tempo map. Audio stems, MIDI notes, vocal F0, score positions, section boundaries, loops, synthesized references, and live singing are all views onto that same timeline.**

Time stretching changes the rate at which the transport moves through the timeline.

It does **not** create a second independent song timeline.

That single decision prevents most audio/MIDI synchronization problems.

---

# 58. Current reference implementations and documentation

## Whole-song transcription

MuScriptor  
https://github.com/muscriptor/muscriptor

Paper:  
https://arxiv.org/abs/2607.08168

MuScriptor is especially useful here because it supports real-world multi-instrument transcription, MIDI/MusicXML generation, and variable-tempo beat maps.

## Beat/downbeat tracking

Beat This!  
https://github.com/CPJKU/beat_this

Paper:  
https://arxiv.org/abs/2407.21658

## Structural analysis

All-In-One-Infer  
https://github.com/openmirlab/all-in-one-infer

Original All-In-One:  
https://github.com/mir-aidj/all-in-one

## Source separation

python-audio-separator  
https://github.com/nomadkaraoke/python-audio-separator

This provides a convenient common interface over multiple UVR/MDX/Demucs/RoFormer model families.

## Lead/backing-vocal pipeline examples

Two-stage RoFormer/karaoke workflow:  
https://github.com/mananpa4/SpleeterGui-Installer

Lead/backing split architecture example:  
https://github.com/NullAngst/SplitScope

These are useful architectural references rather than dependencies the app must copy.

## Vocal F0

RMVPE  
https://github.com/Dream-High/RMVPE

Paper:  
https://arxiv.org/abs/2306.15412

## Lightweight stem-level AMT fallback

Spotify Basic Pitch  
https://github.com/spotify/basic-pitch

## Time stretching

Signalsmith Stretch  
https://github.com/Signalsmith-Audio/signalsmith-stretch

Rubber Band  
https://breakfastquay.com/rubberband/
