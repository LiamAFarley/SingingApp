//! Renders a chromatic run through one preset and reports tuning, level and missing notes.
//! cargo run -p trainer-audio --example sf2_scan -- <bank.sf2> <bank stem> <program> <lo midi> <hi midi>
use std::path::PathBuf;

use trainer_audio::render::{Renderer, SoundFonts};
use trainer_core::pitch::{TrackOptions, track};
use trainer_core::schema::SoundEvent;

fn main() -> anyhow::Result<()> {
    let mut args = std::env::args().skip(1);
    let path = PathBuf::from(args.next().expect("bank.sf2"));
    let stem = args.next().expect("bank stem");
    let program: u8 = args.next().expect("program").parse()?;
    let lo: i32 = args.next().expect("lo midi").parse()?;
    let hi: i32 = args.next().expect("hi midi").parse()?;

    let sf = SoundFonts::load_file(&path)?;
    let mut fonts = SoundFonts::default();
    fonts.banks.insert(stem.clone(), sf);
    let r = Renderer::new(48_000, fonts);
    println!("{stem}:{program}  midi {lo}..{hi}");
    let mut worst = 0.0f32;
    let mut silent = Vec::new();
    let mut sharp = Vec::new();
    let mut flat = Vec::new();
    for midi in lo..=hi {
        let e = SoundEvent {
            midi,
            start_ms: 0.0,
            dur_ms: 700.0,
            timbre: format!("sf:{stem}:{program}"),
            gain_db: -6.0,
            detune_cents: 0.0,
        };
        let b = r.render_events(&[e]);
        let mono: Vec<f32> = b.data.chunks_exact(2).map(|c| 0.5 * (c[0] + c[1])).collect();
        let tr = track(&mono, &TrackOptions::new(48_000.0));
        let mut v: Vec<f32> = tr
            .frames
            .iter()
            .filter(|f| f.t >= 0.15 && f.t <= 0.65)
            .filter_map(|f| f.midi)
            .collect();
        v.sort_by(|a, b| a.total_cmp(b));
        let med = v.get(v.len() / 2).copied();
        let voiced = v.len();
        let total = tr.frames.iter().filter(|f| f.t >= 0.15 && f.t <= 0.65).count();
        match med {
            Some(m) => {
                let cents = (m - midi as f32) * 100.0;
                if cents.abs() > worst.abs() {
                    worst = cents;
                }
                if cents > 15.0 {
                    sharp.push(midi);
                }
                if cents < -15.0 {
                    flat.push(midi);
                }
                if voiced < total / 3 || b.peak() < 0.02 {
                    silent.push(midi);
                }
                println!(
                    "  {midi:>3}: {cents:+6.1} c  voiced {voiced:>2}/{total:<2}  peak {:.2}",
                    b.peak()
                );
            }
            None => {
                silent.push(midi);
                println!("  {midi:>3}: silent (peak {:.2})", b.peak());
            }
        }
    }
    println!(
        "worst {:+.1} cents | flat {} | sharp {} | weak/silent {}",
        worst,
        flat.len(),
        sharp.len(),
        silent.len()
    );
    if !flat.is_empty() {
        println!("  flat: {flat:?}");
    }
    if !sharp.is_empty() {
        println!("  sharp: {sharp:?}");
    }
    if !silent.is_empty() {
        println!("  weak/silent: {silent:?}");
    }
    Ok(())
}
