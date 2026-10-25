//! Lists the presets in one or more SoundFont files, including the bank-0 programs the app
//! can address with `sf:<file stem>:<program>`.
//! cargo run -p trainer-audio --example sf2_info -- path/to/bank.sf2 [...]
use std::path::PathBuf;

use trainer_audio::render::SoundFonts;

fn main() -> anyhow::Result<()> {
    let mut args: Vec<String> = std::env::args().skip(1).collect();
    let show_samples = args.iter().any(|a| a == "--samples");
    args.retain(|a| a != "--samples");
    for arg in args {
        let path = PathBuf::from(arg);
        let t = std::time::Instant::now();
        let sf = SoundFonts::load_file(&path)?;
        let load = t.elapsed();
        let samples = sf.get_sample_headers();
        let wave = sf.get_wave_data();
        let mut shortest = usize::MAX;
        let mut longest = 0usize;
        for s in samples {
            let len = (s.get_end() - s.get_start()).max(0) as usize;
            shortest = shortest.min(len);
            longest = longest.max(len);
        }
        println!(
            "\n=== {} | {:.1} MB | loaded {:?} | {} samples | {:.1} MB wave data | shortest {} frames, longest {} frames",
            path.file_name().unwrap().to_string_lossy(),
            std::fs::metadata(&path).map(|m| m.len()).unwrap_or(0) as f64 / 1e6,
            load,
            samples.len(),
            wave.len() as f64 * 2.0 / 1e6,
            if shortest == usize::MAX { 0 } else { shortest },
            longest
        );
        let mut presets: Vec<_> = sf.get_presets().iter().collect();
        presets.sort_by_key(|p| (p.get_bank_number(), p.get_patch_number()));
        for p in presets {
            let mut keys = Vec::new();
            for k in 0..128 {
                if p.get_regions().iter().any(|r| r.contains(k, 100)) {
                    keys.push(k);
                }
            }
            let (lo, hi) = match (keys.first(), keys.last()) {
                (Some(a), Some(b)) => (*a, *b),
                _ => (-1, -1),
            };
            println!(
                "  bank {:>3} program {:>3}  {:<22} regions {:>2}  keys {:>3}..{:<3} {}",
                p.get_bank_number(),
                p.get_patch_number(),
                p.get_name(),
                p.get_regions().len(),
                lo,
                hi,
                if p.get_bank_number() == 0 {
                    ""
                } else {
                    "(not reachable: app sends no bank select)"
                }
            );
        }
        if show_samples {
            for s in samples {
                println!(
                    "  sample {:<28} root {:>3} rate {:>5} frames {:>7}",
                    s.get_name(),
                    s.get_original_pitch(),
                    s.get_sample_rate(),
                    s.get_end() - s.get_start()
                );
            }
        }
    }
    Ok(())
}
