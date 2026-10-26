//! The aptitude screening (Van Hedger et al. 2015, 2019): implicit note memory and auditory span.
//! The third task, N-back on tones or letters, is `n_back` with `stimuli: tones | letters`.

use super::*;

/// Implicit note memory: each target from the odd steps 5–23 once per pass, in a new order each
/// pass, and the four starting points in turn.
pub(super) fn gen_pitch_memory_match(
    p: &PitchMemoryMatchParams,
    n: usize,
    rng: &mut Rng,
) -> Result<Vec<TrialSpec>, GenError> {
    if !(300..=3000).contains(&p.tone_ms) {
        return err("tone_ms must be 300–3000");
    }
    if !(500..=5000).contains(&p.mask_ms) {
        return err("mask_ms must be 500–5000");
    }
    if !(200..=2000).contains(&p.step_ms) {
        return err("step_ms must be 200–2000");
    }
    check_timbres(std::slice::from_ref(&p.timbre))?;
    let mut targets = BalancedBag::new(INM_TARGETS.to_vec());
    let mut starts = BalancedBag::new(INM_STARTS.to_vec());
    let mut out = Vec::with_capacity(n);
    for _ in 0..n {
        let target = targets.draw(rng).unwrap();
        let start = starts.draw(rng).unwrap();
        let cents = INM_BASE_CENTS + (target - 1) as f32 * INM_STEP_CENTS;
        let hz = 440.0 * 2f64.powf((INM_BASE_MIDI as f64 + cents as f64 / 100.0 - 69.0) / 12.0);
        out.push(TrialSpec::Match(MatchTrial {
            prompt: "Find the tone again".into(),
            target_step: target,
            start_step: start,
            steps: INM_STEPS,
            step_cents: INM_STEP_CENTS,
            base_midi: INM_BASE_MIDI,
            base_cents: INM_BASE_CENTS,
            tone_ms: p.tone_ms,
            mask_ms: p.mask_ms,
            step_ms: p.step_ms,
            timbre: p.timbre.clone(),
            stimulus: json!({
                "target_step": target,
                "start_step": start,
                "target_hz_at_a440": (hz * 100.0).round() / 100.0,
                "step_cents": INM_STEP_CENTS,
            }),
        }));
    }
    Ok(out)
}

/// A digit sequence of `len` from 1–9 with no digit twice in a row and none more than twice.
fn span_sequence(len: usize, rng: &mut Rng) -> Vec<u8> {
    let mut seq: Vec<u8> = Vec::with_capacity(len);
    while seq.len() < len {
        let d = rng.range_i32(1, 9) as u8;
        if seq.last() == Some(&d) || seq.iter().filter(|x| **x == d).count() >= 2 {
            continue;
        }
        seq.push(d);
    }
    seq
}

/// The digits a spoken span needs.
const SPAN_DIGITS: [&str; 9] = [
    "digits/1.wav",
    "digits/2.wav",
    "digits/3.wav",
    "digits/4.wav",
    "digits/5.wav",
    "digits/6.wav",
    "digits/7.wav",
    "digits/8.wav",
    "digits/9.wav",
];

/// One span ladder per trial. Spoken digits need their files; otherwise the tone pad runs, and
/// the stimulus records the fallback.
pub(super) fn gen_span(
    p: &SpanParams,
    n: usize,
    rng: &mut Rng,
    ctx: &GenContext,
) -> Result<Vec<TrialSpec>, GenError> {
    if !(2..=9).contains(&p.start_length) {
        return err("start_length must be 2–9");
    }
    if !(p.start_length..=16).contains(&p.max_length) {
        return err("max_length must be from start_length to 16");
    }
    if !(1..=3).contains(&p.per_length) {
        return err("per_length must be 1–3");
    }
    if !(500..=3000).contains(&p.ioi_ms) {
        return err("ioi_ms must be 500–3000");
    }
    check_timbres(std::slice::from_ref(&p.timbre))?;
    let digits_there = ctx.has_speech(SPAN_DIGITS);
    let (mode, fallback) = match p.stimuli {
        SpanStimuli::Tones => ("tones", false),
        SpanStimuli::Auto => (if digits_there { "digits" } else { "tones" }, false),
        SpanStimuli::Digits if digits_there => ("digits", false),
        SpanStimuli::Digits => ("tones", true),
    };
    let mut out = Vec::with_capacity(n);
    for _ in 0..n {
        let items: Vec<SpanItem> = (p.start_length..=p.max_length)
            .flat_map(|len| {
                (0..p.per_length)
                    .map(|_| SpanItem {
                        length: len,
                        sequence: span_sequence(len as usize, rng),
                    })
                    .collect::<Vec<_>>()
            })
            .collect();
        out.push(TrialSpec::Span(SpanTrial {
            prompt: if mode == "digits" {
                "Listen, then type the digits in order".into()
            } else {
                "Listen, then play the tones back in order".into()
            },
            mode: mode.into(),
            items,
            per_length: p.per_length,
            ioi_ms: p.ioi_ms,
            pad: SPAN_PAD.to_vec(),
            timbre: p.timbre.clone(),
            stimulus: json!({
                "mode": mode,
                "fallback": fallback,
                "start_length": p.start_length,
                "max_length": p.max_length,
                "per_length": p.per_length,
            }),
        }));
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn block(v: serde_json::Value) -> BlockSpec {
        let mut v = v;
        v.as_object_mut()
            .unwrap()
            .entry("params")
            .or_insert(json!({}));
        serde_json::from_value(v).unwrap()
    }

    /// A context whose speech folder holds these files.
    fn with_speech(files: impl IntoIterator<Item = String>) -> GenContext {
        GenContext {
            speech: Some(files.into_iter().collect()),
            ..GenContext::default()
        }
    }

    #[test]
    fn note_memory_covers_each_target_and_starts_outside_them() {
        let b = block(json!({ "type": "pitch_memory_match", "count": 20 }));
        let trials = generate_block(&b, 3, &GenContext::default()).unwrap();
        let mut seen = std::collections::BTreeMap::new();
        for t in &trials {
            let TrialSpec::Match(t) = t else { panic!() };
            *seen.entry(t.target_step).or_insert(0) += 1;
            assert!(INM_STARTS.contains(&t.start_step));
            assert!(t.start_step < 5 || t.start_step > 23);
            assert!(t.target_step % 2 == 1 && (5..=23).contains(&t.target_step));
        }
        // Two passes: every target twice.
        assert_eq!(seen.len(), 10);
        assert!(seen.values().all(|n| *n == 2));
    }

    #[test]
    fn note_memory_steps_are_ten_cents_from_a_sharp_plus_twenty() {
        let b = block(json!({ "type": "pitch_memory_match", "count": 1 }));
        let TrialSpec::Match(t) = &generate_block(&b, 1, &GenContext::default()).unwrap()[0] else {
            panic!()
        };
        let hz = |step: u32| {
            let e = t.tone(step, 500);
            440.0 * 2f64.powf((e.midi as f64 + e.detune_cents as f64 / 100.0 - 69.0) / 12.0)
        };
        assert!((hz(1) - 471.58).abs() < 0.02, "{}", hz(1));
        assert!((hz(5) - 482.60).abs() < 0.02, "{}", hz(5));
        assert!((hz(27) - 547.99).abs() < 0.02, "{}", hz(27));
    }

    #[test]
    fn span_uses_tones_without_digit_files_and_says_so() {
        let b = block(json!({ "type": "span", "count": 1, "params": { "stimuli": "digits" } }));
        let TrialSpec::Span(t) = &generate_block(&b, 1, &GenContext::default()).unwrap()[0] else {
            panic!()
        };
        assert_eq!(t.mode, "tones");
        assert_eq!(t.stimulus["fallback"], true);
        // Lengths 3–12, two at each, shortest first.
        assert_eq!(t.items.len(), 20);
        assert_eq!(t.items[0].length, 3);
        assert_eq!(t.items[19].length, 12);
        for item in &t.items {
            assert_eq!(item.sequence.len(), item.length as usize);
            assert!(item.sequence.windows(2).all(|w| w[0] != w[1]));
            assert!(item.sequence.iter().all(|d| (1..=9).contains(d)));
        }
        let ctx = with_speech(SPAN_DIGITS.iter().map(|s| s.to_string()));
        let TrialSpec::Span(t) = &generate_block(&b, 1, &ctx).unwrap()[0] else {
            panic!()
        };
        assert_eq!(t.mode, "digits");
        assert_eq!(t.stimulus["fallback"], false);
    }

    #[test]
    fn sing_by_label_asks_for_the_labels_note() {
        let ctx = GenContext::default();
        let gervain = crate::labels::builtin_set(crate::labels::GERVAIN_NAMES).unwrap();
        let b =
            block(json!({ "type": "sing_pitch", "count": 24, "params": { "prompt": "label" } }));
        for t in generate_block(&b, 2, &ctx).unwrap() {
            let TrialSpec::SingPitch(t) = t else { panic!() };
            let word = t.prompt.trim_start_matches("Sing ");
            let (pc, _) = gervain
                .entries
                .iter()
                .find(|(_, e)| e.word.as_deref() == Some(word))
                .unwrap_or_else(|| panic!("{word} is not a Gervain name"));
            assert_eq!(pc, t.target_class.name(), "{word}");
            assert_eq!(t.stimulus["prompt"], "label");
        }
        // A colour swatch, with no name in the prompt.
        let taneda = crate::labels::builtin_set(crate::labels::TANEDA_COLORS).unwrap();
        let b =
            block(json!({ "type": "sing_pitch", "count": 12, "params": { "prompt": "color" } }));
        for t in generate_block(&b, 2, &ctx).unwrap() {
            let TrialSpec::SingPitch(t) = t else { panic!() };
            assert_eq!(t.prompt, "Sing this colour");
            assert_eq!(t.prompt_color, taneda.entries[t.target_class.name()].color);
        }
        // Spoken without the files: the word is shown instead, and the stimulus says so.
        let b =
            block(json!({ "type": "sing_pitch", "count": 3, "params": { "prompt": "spoken" } }));
        for t in generate_block(&b, 2, &ctx).unwrap() {
            let TrialSpec::SingPitch(t) = t else { panic!() };
            assert!(t.speech.is_none());
            assert_eq!(t.stimulus["prompt"], "label");
        }
        let with_files = with_speech(gervain.entries.values().filter_map(|e| e.speech.clone()));
        for t in generate_block(&b, 2, &with_files).unwrap() {
            let TrialSpec::SingPitch(t) = t else { panic!() };
            assert!(
                t.speech
                    .as_deref()
                    .unwrap()
                    .starts_with("labels/gervain_names/")
            );
            assert_eq!(t.stimulus["prompt"], "spoken");
        }
    }

    #[test]
    fn hold_and_compare_plays_the_target_after_the_window() {
        let b = block(
            json!({ "type": "sing_pitch", "count": 6, "params": { "classes": ["E"], "hold_and_compare": true } }),
        );
        for t in generate_block(&b, 2, &GenContext::default()).unwrap() {
            let TrialSpec::SingPitch(t) = t else { panic!() };
            let c = t.compare.unwrap();
            assert_eq!(PitchClass::of_midi(c.midi), t.target_class);
            assert_eq!(t.compare_ms, 3000);
        }
    }

    #[test]
    fn screening_n_back_is_keyless_tones_over_three_octaves() {
        let b = block(json!({ "type": "n_back", "count": 3, "params": {
            "n": 3, "stimuli": "tones", "length": 30, "octaves": [3, 4, 5] } }));
        for t in generate_block(&b, 4, &GenContext::default()).unwrap() {
            let TrialSpec::Detect(t) = t else { panic!() };
            assert!(t.prompt_events.is_empty(), "no cadence");
            assert_eq!(t.stimulus["stimuli"], "tones");
            for (i, tone) in t.tones.iter().enumerate() {
                assert_eq!(tone.events.len(), 1, "no drone");
                if tone.target {
                    assert_eq!(tone.midi, t.tones[i - 3].midi, "a match is the same tone");
                } else if i >= 3 {
                    assert_ne!(tone.midi % 12, t.tones[i - 3].midi % 12, "no half matches");
                }
            }
            let octaves: std::collections::BTreeSet<i32> =
                t.tones.iter().map(|x| x.midi / 12 - 1).collect();
            assert!(octaves.len() >= 2);
        }
        // Letters without their files fall back to tones, and say so.
        let b = block(
            json!({ "type": "n_back", "count": 1, "params": { "n": 2, "stimuli": "letters" } }),
        );
        let TrialSpec::Detect(t) = &generate_block(&b, 4, &GenContext::default()).unwrap()[0]
        else {
            panic!()
        };
        assert_eq!(t.stimulus["stimuli"], "tones");
        assert_eq!(t.stimulus["fallback"], true);
        let ctx = with_speech(N_BACK_LETTERS.iter().map(|l| format!("letters/{l}.wav")));
        let TrialSpec::Detect(t) = &generate_block(&b, 4, &ctx).unwrap()[0] else {
            panic!()
        };
        assert_eq!(t.stimulus["stimuli"], "letters");
        assert!(
            t.tones
                .iter()
                .all(|x| x.speech.is_some() && x.events.is_empty())
        );
    }

    #[test]
    fn new_blocks_validate_and_bad_parameters_are_errors() {
        let ctx = GenContext::default();
        for good in [
            json!({ "type": "pitch_memory_match", "count": 20, "assessment": true }),
            json!({ "type": "span", "count": 1, "assessment": true }),
            json!({ "type": "n_back", "count": 1, "assessment": true, "params": { "n": 2, "stimuli": "tones" } }),
            json!({ "type": "sing_pitch", "count": 4, "params": { "prompt": "color", "hold_and_compare": true } }),
        ] {
            let (_, r) = crate::validate::validate_value(
                json!({ "title": "x", "blocks": [good.clone()] }),
                &ctx,
            );
            assert!(r.ok, "{good}: {:?}", r.errors);
        }
        for bad in [
            json!({ "type": "pitch_memory_match", "params": { "mask_ms": 100 } }),
            json!({ "type": "span", "params": { "start_length": 1 } }),
            json!({ "type": "span", "params": { "start_length": 6, "max_length": 4 } }),
            json!({ "type": "n_back", "params": { "stimuli": "tones", "octaves": [4] } }),
            json!({ "type": "n_back", "params": { "stimuli": "tones", "mode": "sing" } }),
            json!({ "type": "sing_pitch", "params": { "prompt": "label", "labels": "no_such_set" } }),
        ] {
            let (_, r) = crate::validate::validate_value(
                json!({ "title": "x", "blocks": [bad.clone()] }),
                &ctx,
            );
            assert!(!r.ok, "{bad} should fail");
        }
    }

    #[test]
    fn span_stops_after_every_sequence_at_one_length_is_wrong() {
        let b = block(json!({ "type": "span", "count": 1 }));
        let TrialSpec::Span(t) = &generate_block(&b, 1, &GenContext::default()).unwrap()[0] else {
            panic!()
        };
        assert_eq!(t.next_item(&[]), Some(0));
        // One of two right at a length: go on.
        assert_eq!(t.next_item(&[true, false]), Some(2));
        assert_eq!(t.next_item(&[false, true]), Some(2));
        // Mid-length: always the second sequence.
        assert_eq!(t.next_item(&[true, false, false]), Some(3));
        // Two errors at one length: stop.
        assert_eq!(t.next_item(&[true, true, false, false]), None);
        assert_eq!(t.next_item(&[false, false]), None);
        // The last length done: stop.
        assert_eq!(t.next_item(&[true; 20]), None);
    }
}
