//! Renders a few General MIDI programs and checks the detected pitch.
//! cargo run -p trainer-audio --example sf2_check -- path/to/bank.sf2
use std::path::PathBuf;

use trainer_audio::render::{Renderer, SoundFonts};
use trainer_core::pitch::{TrackOptions, track};
use trainer_core::schema::SoundEvent;

fn main() -> anyhow::Result<()> {
    let path: PathBuf = std::env::args()
        .nth(1)
        .expect("usage: sf2_check <file.sf2>")
        .into();
    let t = std::time::Instant::now();
    let fonts = SoundFonts {
        gm: Some(SoundFonts::load_file(&path)?),
        ..Default::default()
    };
    println!("loaded in {:?}", t.elapsed());
    let r = Renderer::new(48_000, fonts);
    for program in [0u8, 1, 11, 19, 24, 42, 48, 52, 56, 71, 73] {
        for midi in [48, 60, 69] {
            let e = SoundEvent {
                midi,
                start_ms: 0.0,
                dur_ms: 900.0,
                timbre: format!("gm:{program}"),
                gain_db: -6.0,
                detune_cents: 0.0,
            };
            let t = std::time::Instant::now();
            let b = r.render_events(&[e]);
            let took = t.elapsed();
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
                "gm:{program:<3} midi {midi}: median {med:.2} ({} voiced) peak {:.2} render {:?}",
                v.len(),
                b.peak(),
                took
            );
        }
    }
    Ok(())
}
