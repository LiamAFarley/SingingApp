//! Renders a short audition WAV per preset and measures the detected pitch.
//! cargo run -p trainer-audio --example sf2_audition -- <bank.sf2> <bank stem> <out dir> <program> [...]
use std::path::PathBuf;

use trainer_audio::render::{Renderer, SoundFonts};
use trainer_core::pitch::{TrackOptions, track};
use trainer_core::schema::SoundEvent;

fn wav(path: &PathBuf, rate: u32, data: &[f32]) -> anyhow::Result<()> {
    let spec = hound::WavSpec {
        channels: 2,
        sample_rate: rate,
        bits_per_sample: 16,
        sample_format: hound::SampleFormat::Int,
    };
    let mut w = hound::WavWriter::create(path, spec)?;
    for s in data {
        w.write_sample((s.clamp(-1.0, 1.0) * 32767.0) as i16)?;
    }
    w.finalize()?;
    Ok(())
}

fn main() -> anyhow::Result<()> {
    let mut args = std::env::args().skip(1);
    let path = PathBuf::from(args.next().expect("bank.sf2"));
    let stem = args.next().expect("bank stem");
    let out = PathBuf::from(args.next().expect("output dir"));
    std::fs::create_dir_all(&out)?;
    let programs: Vec<u8> = args.map(|p| p.parse().expect("program number")).collect();
    assert!(!programs.is_empty(), "give at least one program");

    let sf = SoundFonts::load_file(&path)?;
    let mut fonts = SoundFonts::default();
    fonts.banks.insert(stem.clone(), sf);
    let r = Renderer::new(48_000, fonts);
    let notes = [48, 60, 72, 67];
    for program in programs {
        let mut data = Vec::new();
        println!("\n{stem}:{program}");
        for midi in notes {
            let e = SoundEvent {
                midi,
                start_ms: 0.0,
                dur_ms: 900.0,
                timbre: format!("sf:{stem}:{program}"),
                gain_db: -6.0,
                detune_cents: 0.0,
            };
            let b = r.render_events(&[e]);
            let mono: Vec<f32> = b
                .data
                .chunks_exact(2)
                .map(|c| 0.5 * (c[0] + c[1]))
                .collect();
            let tr = track(&mono, &TrackOptions::new(48_000.0));
            let mut v: Vec<f32> = tr.frames.iter().filter_map(|f| f.midi).collect();
            v.sort_by(|a, b| a.total_cmp(b));
            let med = v.get(v.len() / 2).copied().unwrap_or(f32::NAN);
            println!(
                "  midi {midi}: median {med:.2} ({:+.1} cents), voiced {}/{}, peak {:.2}, {:.2}s",
                (med - midi as f32) * 100.0,
                v.len(),
                tr.frames.len(),
                b.peak(),
                b.duration_s()
            );
            // 120 ms of silence between notes in the audition.
            data.extend_from_slice(&b.data);
            data.extend(std::iter::repeat_n(0.0, 4800 * 2));
        }
        let file = out.join(format!("{stem}_p{program}.wav"));
        wav(&file, 48_000, &data)?;
        println!("  wrote {}", file.display());
    }
    Ok(())
}
