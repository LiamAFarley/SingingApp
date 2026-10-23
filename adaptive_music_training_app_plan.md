# Adaptive Singing, Ear-Training, Transcription, and Absolute-Pitch Platform
## Consolidated implementation plan

**Purpose:** Build a local-first music-training platform that can measure musical performance, ingest real music, generate exercises, track a learner model, and expose the whole system to an agent through MCP for adaptive practice planning.

**Design principle:** The application should behave like an experimental learning platform rather than a static exercise library. Every exercise should have a clear target, deterministic grading where possible, logged conditions, planned progression, transfer tests, and retention tests.

---

## 1. Product scope

The system combines:

- Singing practice and objective vocal feedback
- Absolute-pitch training
- Relative-pitch and functional ear training
- Sight singing
- Melodic, harmonic, and rhythmic dictation
- Repertoire-derived exercises
- Automatic source separation
- Automatic music transcription
- Drum-kit separation and drum-to-MIDI transcription
- Low-latency ASIO audio on Windows
- Shared desktop/mobile learner history
- Hands-free headphone practice
- Agent-driven adaptive session planning through MCP

The desktop application is the authoritative environment for low-latency audio, detailed analysis, repertoire ingestion, transcription correction, and analytics.

The phone client is intentionally smaller and optimized for hands-free practice, short active drills, microphone pitch exercises, and offline execution of precompiled sessions.

---

# 2. Foundational separation of responsibilities

The system should have four major layers:

```text
                         ┌─────────────────────┐
                         │     MCP Agent       │
                         │ planner / coach     │
                         └──────────┬──────────┘
                                    │
                                    ▼
┌─────────────────────────────────────────────────────────────┐
│                     TRAINING CORE                           │
│ exercises · grading · learner state · sessions · library   │
└──────────────┬───────────────────────────────┬──────────────┘
               │                               │
               ▼                               ▼
┌───────────────────────────┐       ┌─────────────────────────┐
│ Native realtime engine    │       │ Offline ML worker       │
│ C++ / JUCE                │       │ Python / PyTorch        │
│                           │       │                         │
│ ASIO / WASAPI             │       │ source separation       │
│ recording/playback        │       │ transcription           │
│ realtime F0               │       │ high-quality F0         │
│ MIDI / metronome          │       │ beat/key analysis       │
└───────────┬───────────────┘       └───────────┬─────────────┘
            │                                   │
            └─────────────────┬─────────────────┘
                              ▼
                    ┌───────────────────┐
                    │ Desktop UI        │
                    │ React / Tauri     │
                    └───────────────────┘
```

### Hard rule for realtime audio

Nothing potentially blocking belongs in the audio callback.

Do **not** perform in the callback:

- Python calls
- neural inference
- file I/O
- database transactions
- logging
- UI work
- heap allocation where avoidable
- network requests
- mutex acquisition
- transcription
- source separation

Use lock-free or bounded queues to move audio and state into analysis workers.

---

# 3. Desktop audio engine

Use **JUCE C++** for the native audio engine.

Preferred Windows backend order:

1. Native hardware **ASIO**
2. WASAPI Exclusive
3. Low-latency WASAPI Shared
4. Ordinary shared audio for noninteractive playback

The device page should expose:

- driver type
- physical interface
- input channels
- output channels
- sample rate
- buffer size
- measured round-trip latency
- monitoring mode
- microphone level
- calibration offset
- input/output test
- underrun/xrun count

Use **48 kHz** internally for interactive audio when supported.

Typical interactive buffers may be 64–256 samples, but the app must query supported sizes from the driver.

---

# 4. Latency calibration and timing correctness

Rhythm grading requires device-latency compensation.

Provide a calibration routine that plays a short transient and detects its return using loopback or microphone capture.

Persist calibration per device configuration:

```text
device_id
driver
sample_rate
buffer_size
input_latency_samples
output_latency_samples
measured_roundtrip_samples
manual_offset_samples
calibration_timestamp
```

All performed-note and rhythm timestamps should be normalized onto the application's master timeline before scoring.

A switch from ASIO to WASAPI must not silently alter measured performance.

---

# 5. Realtime versus authoritative offline analysis

Use two different analysis paths.

## Realtime path

Goals:

- immediate tuner
- live cents display
- voiced/unvoiced state
- metronome synchronization
- interactive pitch targeting

Candidate native methods:

- YIN
- pYIN-inspired online estimator
- McLeod Pitch Method
- another low-cost native F0 algorithm

Suggested operating target:

```text
audio callback      small hardware-sized blocks
F0 analysis window  ~20–50 ms
pitch updates       ~10 ms
UI updates          30–60 Hz
```

## Offline authoritative path

After an attempt, rerun the recording with a more accurate F0 model such as torchcrepe/CREPE-class analysis.

Retain the **continuous F0 trajectory**, not only note labels.

For accompanied or noisy material, support an RMVPE-like specialized F0 backend.

---

# 6. Objective vocal metrics

Store multiple independent dimensions rather than one "singing score."

For each sung note or phrase calculate where applicable:

| Metric | Description |
|---|---|
| initial pitch error | cents error at first reliable vocal onset |
| settling time | time to enter and remain near target |
| settled pitch error | median pitch error after onset transient |
| note-center error | target versus estimated central pitch |
| pitch drift | change in central pitch across sustained note |
| pitch spread | variability around central pitch |
| onset timing | performed versus expected onset |
| release timing | performed versus expected release |
| duration error | note-length discrepancy |
| voiced continuity | interruptions/unvoiced gaps |
| interval accuracy | produced versus expected interval |
| dynamic trajectory | relative loudness contour |
| vibrato rate | cycles per second |
| vibrato extent | pitch excursion |
| vibrato regularity | cycle-to-cycle consistency |
| vibrato onset delay | note onset to stable modulation |
| vibrato center drift | slow movement underneath modulation |

### Critical scoring rule

Do not penalize a singer simply because instantaneous F0 leaves a narrow cents window during vibrato.

Estimate a slowly varying note center for intonation scoring and characterize vibrato separately.

This prevents a good sustained note from being marked wrong by a naive tuner.

---

# 7. Feedback scheduling

Research on feedback-guided vocal training supports realtime visual feedback as useful, but continuous feedback can create dependency.

Therefore every exercise template should support a feedback schedule:

```text
continuous
intermittent
after-note
after-phrase
end-of-exercise
none
```

The progression hierarchy should be:

1. **Guided acquisition** — frequent feedback
2. **Independent performance** — delayed feedback
3. **Generalization** — new pitch/timbre/key/range/context
4. **Retention** — delayed revisit without recent cues

The learner should not be considered to have mastered a skill merely because it works while watching a moving pitch line.

---

# 8. Master musical representation

Every imported or generated musical item should normalize into one common representation.

```text
Piece
  ├─ metadata
  ├─ original audio
  ├─ stems
  ├─ tempo map
  ├─ meter map
  ├─ tonal/key annotations
  ├─ symbolic tracks
  │    └─ note events
  │         pitch
  │         onset
  │         offset
  │         velocity
  │         instrument
  │         confidence
  │         provenance
  └─ score
       MusicXML
```

Trust order:

1. user-corrected score/transcription
2. trusted MusicXML/MIDI reference
3. consensus automatic transcription
4. individual model output

Never silently overwrite user-corrected symbolic data with a later model rerun.

---

# 9. Automatic repertoire ingestion

```text
audio
  │
  ├──► decode / normalize
  │
  ├──► full-mix transcription
  │
  ├──► source separation
  │      vocals
  │      drums
  │      bass
  │      other
  │
  ├──► stem-specific transcription
  │
  ├──► beat / tempo / meter
  │
  ├──► tonal analysis
  │
  ├──► confidence reconciliation
  │
  └──► editable canonical score
              │
              ▼
       exercise extraction
```

The ingestion system should treat automatic transcription as uncertain evidence, not ground truth.

Every automated analysis run should log:

```text
model_name
model_version
checkpoint_hash
configuration
input_asset_hash
timestamp
confidence outputs
```

---

# 10. Source separation

Make the separator pluggable.

Initial backend family support should include modern architectures such as:

- SCNet / Huge-SCNet
- BS-RoFormer
- MelBand-RoFormer
- MDX23C
- Demucs-family models as a mature fallback

Suggested quality presets:

```text
FAST
  lightweight separator

QUALITY
  strongest general 4-stem model available locally

SPECIALIZED
  target-specific separator
```

The application should not hard-code assumptions about one model family because separation models are changing rapidly.

---

# 11. General automatic music transcription

Use a modern multi-instrument AMT backend such as **MuScriptor** as a primary full-mix transcription route when its licensing and model availability fit the deployment.

Normalize every transcription backend into:

```text
pitch
onset
offset
instrument
velocity_if_available
confidence
provenance
```

Support lighter stem-level systems such as **Basic Pitch** for isolated or nearly isolated melodic stems.

Use multiple evidence streams when possible:

```text
full-mix transcription
stem-level transcription
continuous F0 support
beat-grid support
music-theory plausibility
```

Consensus raises confidence; disagreements are flagged for review.

---

# 12. Vocal-stem transcription

A vocal stem should support two simultaneous representations:

1. dense continuous F0 for analysis
2. segmented symbolic notes for score/exercise generation

The segmentation stage must be conservative around:

- vibrato
- slides
- scoops
- melisma
- ornaments
- breathy/unvoiced transitions
- backing vocals

When uncertain, keep the dense F0 representation and lower note-boundary confidence instead of forcing clean discrete notes.

---

# 13. Drum transcription and kit-component extraction

Provide a dedicated drum pipeline.

```text
full mix
   │
   ▼
drum stem
   │
   ├──► direct drum transcription
   │
   └──► kit-component separation
           kick
           snare
           toms
           hi-hat
           ride
           crash/cymbal
                 │
                 ▼
             onsets
                 │
                 ▼
             drum MIDI
```

Useful backend classes:

- ADTOF-style direct drum transcription
- DrumSep/MDX-family kit-piece separation
- newer separate-and-detect research pipelines

The normalized output should support at minimum:

```text
kick
snare
toms
closed/open hi-hat where reliable
ride
crash/cymbal
velocity
onset confidence
```

Do not require perfect audio isolation for MIDI creation. Direct symbolic transcription and separated-stem evidence can be combined.

---

# 14. Transcription review UI

The editor should synchronize:

- waveform
- spectrogram
- source stem
- piano roll
- notation
- playhead
- confidence overlays

Editing operations:

- insert note
- delete note
- change pitch
- change onset
- change offset
- change instrument
- change drum class
- mark uncertain
- mark verified

A verified segment becomes eligible for high-confidence exercise generation.

---

# 15. Score support and sight singing

Use MusicXML as the rich score interchange format.

Display notation interactively and color-code grading.

Example:

```text
green   correct
yellow  marginal
red     incorrect
blue    current
gray    omitted
```

The score renderer should never own grading logic.

---

# 16. Sight-singing alignment

Use three separate timelines:

```text
REFERENCE SCORE
expected pitch + timing

PERFORMANCE
dense F0 + voiced/unvoiced trajectory

ALIGNMENT
mapping between expected and performed events
```

Do not segment solely by expected note boundaries.

Instead detect performed note candidates and align them with dynamic programming or DTW-like sequence alignment using costs for:

- pitch mismatch
- onset error
- duration error
- insertion
- deletion

This prevents one missed note from shifting every later score.

---

# 17. Exercise schema

All exercises should be declarative, reproducible, and agent-generatable.

Example:

```json
{
  "type": "sing_note",
  "target": {
    "midi": 66,
    "label": "F#4"
  },
  "prompt": {
    "mode": "spoken_label"
  },
  "response": {
    "mode": "voice",
    "timeout_ms": 4000
  },
  "feedback_schedule": "after-note",
  "grading": {
    "attack_tolerance_cents": 35,
    "stable_tolerance_cents": 20,
    "required_hold_ms": 600
  },
  "skills": [
    "absolute_pitch.production.F#",
    "voice.pitch_targeting"
  ]
}
```

The engine, not the LLM, executes and grades this object.

---

# 18. Absolute-pitch skill model

Do not represent absolute pitch as a single boolean.

Track separately:

```text
absolute pitch memory
pitch-class identification
pitch labeling
absolute pitch production
cross-octave transfer
cross-timbre transfer
pitch recognition in musical context
response latency / automaticity
retention
```

Relative pitch remains a separate skill family.

---

# 19. Absolute-pitch training protocols

## A. Progressive pitch-category training

This should be the central explicit AP mode.

Key protocol features to support:

- begin with a restricted pitch-class set
- progressively add categories
- use several octaves
- include out-of-set distractors
- vary feedback availability
- tighten response deadlines
- require repeated successful sessions
- remediate weak pitch classes
- test transfer on unseen timbres and octaves

Provide both:

```text
research-faithful protocol
adaptive experimental protocol
```

The software must log which protocol produced which results.

## B. Cross-octave and cross-timbre generalization

A pitch category is not mastered because one specific piano note is memorized.

Training and testing should vary:

- octave
- instrument
- synthesis timbre
- attack shape
- loudness
- speaker/headphone context where practical

Reserve holdout stimuli never used in training.

## C. Incidental associative learning

Support as an experimental mode, especially for hands-free practice.

Examples:

- recurring tones paired with visual/symbolic categories
- pitch cues embedded in another classification task
- passive category-name pairing
- Shepard-tone experiments

Do **not** count performance within these tasks as evidence of transferable AP.

Evaluate transfer separately using ordinary unseen tones.

## D. Absolute pitch production

Prompt:

> Sing E-flat.

No reference tone is played.

Measure:

- initial produced pitch
- settled pitch
- response latency
- stability
- effect of distractors/delay

## E. Familiar-song pitch anchoring

Optionally use familiar original-key recordings to test and exploit absolute musical memory.

Treat this as a bridge into pitch-category learning, not proof of generalized AP.

---

# 20. Absolute-pitch assessment

Avoid declaring "you have perfect pitch."

Report a profile:

| Test | Measures |
|---|---|
| blind 12-class identification | no-reference note naming |
| cross-octave identification | chroma invariance |
| cross-timbre identification | timbral independence |
| response latency | automaticity |
| semitone confusion matrix | systematic category errors |
| delayed retest | retention |
| no-reference singing | absolute production |
| unfamiliar-music pitch naming | practical transfer |

Maintain a reserved evaluation stimulus bank that never appears during training.

Record recent reference exposure because it can contaminate "absolute" tests.

---

# 21. Relative pitch, harmony, audiation, and solfège

Provide full tracks for:

- melodic intervals
- harmonic intervals
- chord qualities
- chord inversions
- scale degrees
- functional harmony
- cadence recognition
- key/tonic recognition
- melodic dictation
- harmonic dictation
- rhythmic dictation
- audiation
- sight singing

Support:

- movable-do
- fixed-do
- numeric scale degrees
- note names

Do not assume one solfège system is a universal solution.

---

# 22. Vocal imitation and personalized references

The exercise engine should support targets rendered as:

- piano
- synthesized tone
- professional voice
- user's own previously successful voice sample
- pitch-shifted user voice

The user's own successful productions may be useful reference material for later imitation and should be a first-class target type.

This allows experiments comparing which reference timbre produces the best pitch matching.

---

# 23. Singing difficulty matrix

Do not reduce singing difficulty to note range.

Tag each exercise along dimensions such as:

| Dimension | Easier | Harder |
|---|---|---|
| target | sustained note | unfamiliar phrase |
| reference | sing-along | delayed memory |
| tempo | slow | fast |
| articulation | legato | staccato/mixed |
| range | comfortable middle | extreme tessitura |
| intervals | steps | large leaps |
| feedback | continuous | none |
| direction | ascending | descending |
| tonality | diatonic | chromatic/modulating |

This is critical for useful error clustering.

---

# 24. Range and tessitura tracking

Distinguish:

```text
available range
usable range
comfortable tessitura
reliable exercise range
```

Never expand difficulty merely because an extreme note was produced once.

Range progression should consider:

- repeatability
- pitch accuracy
- continuity
- user comfort
- absence of strain flags

The app can measure acoustics but should not claim to diagnose registration physiology.

---

# 25. Vibrato analysis

Vibrato needs a dedicated second-stage analyzer.

Workflow:

1. estimate dense F0
2. fit/estimate slowly varying pitch center
3. subtract center
4. analyze residual modulation
5. classify periodicity only when confidence is sufficient

Useful analysis band: roughly the region in which normal vocal vibrato often occurs, but avoid enforcing one style-specific target.

Track:

- rate
- extent
- regularity
- onset delay
- duration
- center drift
- loudness modulation

Do not provide one universal "vibrato quality score."

Allow style/genre profiles.

---

# 26. Hands-free mode

A deterministic session player should support:

```text
PROMPT
→ RESPONSE WINDOW
→ optional MICROPHONE CAPTURE
→ ANSWER / FEEDBACK
→ NEXT
```

Examples:

### Passive

```text
[tone]
3 second pause
"E-flat."
```

### Active singing

```text
"Sing F-sharp."
[microphone opens]
"Correct. Twelve cents sharp."
```

### Active spoken identification

Optional speech recognition can record spoken note names, but manual/tap responses should remain available for noisy environments.

Hands-free listening is supplemental exposure and should not be credited the same as attention-demanding active trials.

---

# 27. Mobile client

The mobile app should execute precompiled sessions offline.

Heavy separation/transcription remains desktop/server-side.

Mobile capabilities:

- background headphone playback
- active ear training
- AP drills
- note production
- simple microphone pitch feedback
- hands-free responses
- sync attempts/results
- download prepared repertoire snippets

Native audio paths:

- Android: Oboe / AAudio
- iOS: CoreAudio / AVAudioSession

Do not depend on a browser tab for background audio.

---

# 28. Learner-state model

Each skill should track more than accuracy.

```text
skill_id
attempt_count
recent_attempt_count
accuracy_posterior
median_response_time
response_time_distribution
mean_error
error_distribution
last_practiced
retention_estimate
learning_rate
difficulty_range
confidence_in_estimate
```

Skills are hierarchical.

```text
pitch
 ├─ absolute
 │   ├─ recognition
 │   ├─ production
 │   ├─ cross_octave
 │   └─ cross_timbre
 └─ relative
     ├─ interval
     ├─ scale_degree
     └─ function

voice
 ├─ pitch_targeting
 ├─ stabilization
 ├─ sustained_intonation
 ├─ rhythm
 ├─ range
 ├─ agility
 └─ sight_singing
```

One attempt may update several skills.

---

# 29. Error clustering

Aggregate raw attempts into interpretable patterns.

Example:

```text
descending m6 accuracy = 58%
ascending m6 accuracy = 91%

descending m6 above C5 = 49%
descending m6 after stepwise passage = 72%

incorrect descending m6 responses:
  → P5 63%
  → M6 24%
  → other 13%
```

The agent should receive this structured analysis rather than thousands of raw attempts.

---

# 30. Adaptive agent

The LLM/agent decides:

- which weaknesses to target
- which exercise templates to instantiate
- how to vary context
- when to remediate
- when to retest
- how much time to allocate
- when to return to repertoire

The agent does **not** decide:

- whether the note was in tune
- whether the rhythm was correct
- whether AP identification was correct
- whether vibrato was "healthy"
- whether the singer has a medical voice problem

Those are deterministic measurement or external-professional domains.

---

# 31. MCP surface

Suggested tools:

```text
get_skill_profile
get_skill_history
get_recent_attempts
get_error_clusters
get_training_load
get_repertoire
search_music_library
get_piece_analysis
find_passages

generate_exercise
generate_variations
compile_session
schedule_review

import_music
separate_asset
transcribe_asset
create_excerpt

grade_attempt
compare_attempts
analyze_session
```

Useful MCP resources:

```text
skills://
sessions://
exercises://
pieces://
transcriptions://
protocols://
```

The MCP layer should expose domain objects, not raw SQL or unrestricted filesystem access.

---

# 32. Practice-session architecture

A default focused session may contain:

| Block | Typical role |
|---|---|
| vocal calibration | pitch centering/control |
| AP | recognition + production |
| relative/functional ear | relationships and tonality |
| sight singing | notation → audiation → voice |
| transcription | auditory decoding |
| adaptive weakness block | targeted remediation |
| repertoire transfer | apply isolated work to real music |

Exact durations should be adaptive and should not be treated as scientifically fixed.

The app should support **vocal-load-aware sessions** where microphone-heavy work can be reduced while ear training continues.

---

# 33. Repertoire-derived exercise mining

Imported music should become searchable by musical features.

Example query:

```text
find vocal phrases
duration 2–8 bars
contains descending minor sixth
moderate tempo
high transcription confidence
within comfortable tessitura
difficulty near current ability
```

Drum example:

```text
find drum excerpts
4/4
90–120 BPM
16th-note hi-hat
simple kick/snare
high onset confidence
```

Generated exercise types:

- isolated note targeting
- difficult interval extraction
- phrase sing-back
- sight singing from generated notation
- transcription from stem
- rhythm-only dictation
- drum-groove reproduction
- accompaniment practice
- missing-note completion
- harmony singing

---

# 34. Database

SQLite is sufficient initially.

Core tables/entities:

```text
Piece
AudioAsset
Stem
SymbolicTrack
NoteEvent
Score
AnalysisRun
ExerciseTemplate
ExerciseInstance
Attempt
PitchTrace
Skill
SkillEvidence
SkillState
Session
SessionPlan
Protocol
Stimulus
Calibration
DeviceProfile
```

Important provenance fields:

```text
source_type
human_verified
model_name
model_version
checkpoint_hash
confidence
created_at
supersedes
```

---

# 35. Research/experiment support

The app should be able to compare protocols within one learner.

Useful built-in experiments:

- fixed AP category order vs adaptive category order
- explicit vs incidental AP practice
- immediate vs intermittent vs delayed vocal feedback
- piano target vs professional voice vs user's own voice
- assisted vs unassisted sight singing
- realtime visual feedback vs post-trial review

Results should emphasize within-user performance and transfer.

Do not make causal scientific claims from uncontrolled personal usage.

---

# 36. Safety and comfort state

The app can collect a short pre/post subjective status:

```text
comfortable
mild fatigue
discomfort
pain
new hoarseness / voice change
```

Behavior:

- comfortable → normal progression
- mild fatigue → reduce vocal load
- discomfort → avoid demanding range/intensity work
- pain/new significant voice change → suspend demanding vocal exercises and recommend appropriate professional evaluation if persistent or concerning

The application must never treat a correct pitch as proof an uncomfortable technique was successful.

It should not diagnose vocal pathology.

---

# 37. Acoustic limitations

Important constraints:

- microphone level is not absolute SPL unless the chain is calibrated
- spectral features depend strongly on microphone, distance, room, vowel, and style
- formants/resonance can be displayed but should not be reduced to universal good/bad scores
- pitch correction or processed reference audio can corrupt training targets
- source-separation artifacts should lower transcription confidence
- accompaniment can produce octave and F0-tracking errors
- vibrato analysis requires high-resolution F0 without excessive smoothing

---

# 38. Repository layout

```text
music-trainer/
│
├─ apps/
│  ├─ desktop-ui/
│  └─ mobile/
│
├─ native/
│  └─ audio-engine/
│
├─ services/
│  ├─ training-core/
│  ├─ ml-worker/
│  └─ mcp-server/
│
├─ packages/
│  ├─ music-model/
│  ├─ exercise-schema/
│  ├─ learner-model/
│  ├─ score-analysis/
│  ├─ protocol-library/
│  └─ protocol/
│
├─ models/
│  └─ manifests/
│
├─ data/
│  ├─ database/
│  ├─ audio/
│  ├─ stems/
│  ├─ transcription/
│  ├─ stimuli/
│  └─ cache/
│
└─ tests/
   ├─ audio/
   ├─ grading/
   ├─ transcription/
   ├─ protocols/
   └─ golden/
```

---

# 39. Desktop IPC

Do not stream audio through the web UI.

```text
UI
 │
 │ control/status IPC
 ▼
native audio engine

native engine
 │
 │ ring buffers / recording files
 ▼
analysis workers
```

UI state such as meters and pitch can be sent as compact timestamped packets at 30–60 Hz.

---

# 40. Testing strategy

Create synthetic golden signals where pitch/timing are known exactly.

Example:

```text
target: A4
signal: 444.01 Hz
known error: +15.7 cents
attack delay: 70 ms
drift: -4 cents/sec
```

Validate:

- realtime F0
- offline F0
- onset detection
- note-center estimation
- vibrato estimation
- timing compensation
- DTW/alignment
- MIDI output

Also maintain a small manually annotated set of real singing and music excerpts as regression tests.

Stress-test:

- ASIO device changes
- buffer underruns
- sample-rate switches
- interface disconnect/reconnect
- long recordings
- background GPU analysis
- simultaneous recording and UI rendering

---

# 41. Implementation sequence

| Phase | Deliverable |
|---|---|
| 0 | canonical schemas + SQLite |
| 1 | JUCE ASIO/WASAPI engine |
| 2 | realtime + offline F0 |
| 3 | deterministic exercise engine |
| 4 | AP recognition/production protocols |
| 5 | relative-pitch / rhythm engine |
| 6 | MusicXML/MIDI sight singing |
| 7 | learner model + error clustering |
| 8 | hands-free desktop mode |
| 9 | source separation |
| 10 | general transcription |
| 11 | drum-specific pipeline |
| 12 | transcription correction UI |
| 13 | repertoire exercise mining |
| 14 | MCP server |
| 15 | adaptive planner |
| 16 | mobile client |
| 17 | advanced vibrato / resonance analytics |

Dependency priority remains:

```text
audio
→ grading
→ exercises
→ learner model
→ agent
→ repertoire automation
```

Do not start with the agent.

---

# 42. V1 definition

V1 is useful when it can do all of the following reliably:

- ASIO/WASAPI recording/playback
- latency calibration
- live tuner
- offline pitch grading
- pitch-target singing
- interval recognition and singing
- absolute-pitch recognition and production
- cross-octave/cross-timbre AP testing
- MusicXML/MIDI import
- score rendering
- sight-singing alignment
- rhythm grading
- detailed attempt history
- learner-state updates
- error clustering
- hands-free sessions
- MCP session planning

---

# 43. V2 definition

V2 adds the repertoire-intelligence loop:

```text
real audio
→ source separation
→ full-mix + stem transcription
→ confidence reconciliation
→ manual correction where needed
→ musical feature tagging
→ exercise mining
→ adaptive scheduling
→ measured transfer back to repertoire
```

---

# 44. End-state workflow

A user asks:

> Give me 35 minutes. Prioritize singing. Keep absolute pitch daily, but otherwise decide what I need.

The planner sees that recent sight-singing errors cluster around descending chromatic leaps.

It generates prerequisite interval drills, then short melodies in several keys.

The user improves.

The system retests without realtime feedback.

Later an imported song is separated and transcribed. The repertoire engine detects the same difficult interval in a vocal phrase and schedules the phrase as transfer practice.

During work, the phone executes a long hands-free AP/ear-training session.

Everything returns to the same learner model.

The core product loop is:

```text
real music
→ analysis
→ measured learner state
→ adaptive practice
→ objective grading
→ transfer/retention tests
→ revised learner state
```

---

# 45. Research-informed implementation notes incorporated into this plan

The research review motivating this design emphasizes several distinctions that should remain explicit in the software:

- absolute-pitch memory, pitch-class identification, labeling, production, and relative pitch are not interchangeable
- adult AP training appears possible for at least some learners, but transfer and long-term generalization are critical
- cross-octave and cross-timbre tests are necessary to avoid overestimating learning
- response latency matters in addition to accuracy
- feedback should be faded rather than left permanently continuous
- sight singing and vocal accuracy should be analyzed by task dimensions rather than one average score
- user comfort and repeatability matter when expanding range
- vibrato should be analyzed separately from note-center intonation
- automatic transcription must remain uncertainty-aware
- hands-free exposure should not be counted as equivalent to concentrated active testing
- app-based acoustic metrics should not be treated as diagnosis or proof of healthy technique

The separate **Physical Vocal Practice Reference** should be used by the planning agent for off-app warm-ups, SOVT work, mobility, breath coordination, range exploration, and other physical practice that the software cannot directly supervise.
