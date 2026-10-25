//! Loads every SF2 in a folder the way the app does for extra banks and renders one note from
//! every bank-0 preset, so missing or silent banks show up before a session uses them.
//! cargo run -p trainer-audio --example sf2_banks_check -- <soundfonts dir>
use std::path::PathBuf;

use trainer_audio::render::{Renderer, SoundFonts};
use trainer_core::pitch::{TrackOptions, track};
use trainer_core::schema::SoundEvent;

fn main() -> anyhow::Result<()> {
    let dir = PathBuf::from(std::env::args().nth(1).expect("soundfonts dir"));
    let mut fonts = SoundFonts::default();
    let names = fonts.load_dir(&dir);
    println!("loaded {} banks from {}", names.len(), dir.display());
    let mut presets: Vec<(String, u8, String)> = Vec::new();
    for name in &names {
        let sf = fonts.banks.get(name).expect("just loaded");
        for p in sf.get_presets() {
            if p.get_bank_number() == 0 {
                presets.push((
                    name.clone(),
                    p.get_patch_number() as u8,
                    p.get_name().to_string(),
                ));
            }
        }
    }
    let r = Renderer::new(48_000, fonts);
    for name in names {
        println!("{name}:");
        for (bank, program, preset) in presets.iter().filter(|(b, _, _)| b == &name) {
            let e = SoundEvent {
                midi: 60,
                start_ms: 0.0,
                dur_ms: 500.0,
                timbre: format!("sf:{bank}:{program}"),
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
            let mut v: Vec<f32> = tr
                .frames
                .iter()
                .filter(|f| f.t >= 0.15 && f.t <= 0.45)
                .filter_map(|f| f.midi)
                .collect();
            v.sort_by(|a, b| a.total_cmp(b));
            let med = v.get(v.len() / 2).copied();
            println!(
                "  p{program:<3} {preset:<22} peak {:.2}  midi {}",
                b.peak(),
                med.map(|m| format!("{m:.1}")).unwrap_or_else(|| "-".into())
            );
        }
    }
    Ok(())
}
