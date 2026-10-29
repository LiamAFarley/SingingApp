# Vocal review rubric

Date: 2026-10-29. For any reviewer vetting `2026-10-29_vocal_knowledge_exercises.md`. This file is the acceptance bar. A reviewer returns PASS only if every gate below passes. One failed gate means FAIL with every occurrence listed.

## Gate 0: length

Count the words with `wc -w`. The document this rubric was written against holds 10,253 words. Instant FAIL under 82,024 words (8x). State the count in the verdict. Length alone never passes; it is a floor, not a score.

## Gate 1: no parroting

Never write the source as the explanation. Phrases such as the docx orders, the doc states, Ragan states, or Malde states followed by an undigested claim fail on sight. Every claim needs its mechanism spelled out in the text: what happens in the body, why the instruction works, what goes wrong when violated. Citations support the explanation. They never substitute for it.

## Gate 2: no bulleted jargon dumps

Any passage matching this pattern fails completely, wherever it appears: compressed stacks of technical clauses with a bare citation and no unpacking. Quoted examples of the banned pattern:

- "Sequence matters. Raising the larynx and constricting the pharynx pulls reflexive glottal thickening, which traps beginners loud and strained in Overfull. The docx orders weight thinning first, then resonance condensing, then articulation and prosody. Swallow-and-hold is explicitly rejected: it recruits swallowing constrictors, risks tension dysphonia and gagging, and locks the airway. Voiceless panting drills such as Big Dog Small Dog and whisper scream require glottal constriction to be audible and carry false-fold strain into phonation. (Docx lines 93.)"
- "True sustain holds the sternum comfortably high, inhales silently without clavicular lift, suspends briefly, and balances inspiratory muscle activity against expiratory abdominal activity with epilaryngeal inertance so airflow use stays low. (Spectrogram reference notes, sustain section lines 178 to 185.)"
- "Posture as stasis is rejected in favor of balance, buoyant, springy. Skeleton carries weight, soft tissue stays resilient. Head balances at the joint without the jaw: balancing head and jaw as one unit misplaces the head high or low, the jaw hangs as an appendage. Neck release signals balance."
- "Weight is fold mass per cycle and closed quotient versus open quotient. Lightening means raising open quotient toward a smoother softer tone without tipping into breathy incomplete closure. Size is tract resonance. Fullness is the ratio between them. (Docx lines 88 to 93.)"

Each of these states terms without teaching them. Weight, open quotient, inertance, buoyant, appendage: every term needs its meaning and its felt consequence in the surrounding text, or the passage fails.

## Gate 3: exercise organization

Tables with giant sensation and pitfall cells are banned for exercises. Each exercise gets its own ### subsection with bullet points: purpose, setup, execution, sensation, pitfall and correction, dose and move-on, source. Tables are allowed only for small comparison data such as the fullness states or spectrogram cutoffs. Any exercise row whose sensation or pitfall cell runs past two lines fails.

## Gate 4: prior failure catalog

Every item below failed before and fails again on recurrence. Check each against the current text with quotes of offending lines:

1. Invented protocols. The six straw protocols, the weight protocols, and the phoneme mechanics come from the feminization docx. Anything not traceable to a stated passage fails.
2. Vague sensation cues. Deliberately light, chest vibration rising, hollow and dopey, buzzy and metallic, with no mechanism, fail.
3. Misnamed drills. Ping-pong means alternating heavy-heavy-light; an ascending line is not ping-pong.
4. Overlapping duplicated sections. Straw counted twice, sustained notes at different volumes across three sections with no distinct target, fail.
5. App blocks for what the microphone cannot measure: airflow, pressure, vowel, loudness. Those stay off-app or ungraded reference tracks.
6. Random-tone masks in any form. Banned, removed in commit 3ccd96a, never evidence-backed.
7. Unexplained symbols. Every phonetic symbol gets a sound example on first use.
8. Articulation texts without phoneme mechanics and placement method.
9. Summaries pointing at information not in the document: citations to subagent extractions the reader cannot open, repo paths that do not exist in a clone, Session as a citation.
10. Video names without direct URLs mapped to sections.
11. Missing move-on criteria, missing safety and stop rules, missing stretches with reps.

## Verdict format

PASS or FAIL, then the gate-by-gate result with file section and quoted lines for every failure. No rewriting. Review only.
