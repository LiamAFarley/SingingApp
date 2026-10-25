# Extra SoundFonts

The app renders extra instruments from ordinary `.sf2` files in `<data>\soundfonts`. It loads every
bank at startup and addresses a preset as `sf:<file stem>:<program>`. Only **bank 0** presets are
reachable, because the renderer sends a program change without a bank-select message.

`<data>` is `data\` beside the executable: `SingingApp\data\` when running from the repository, or
`<install dir>\data\` for an installed build. `SINGING_TRAINER_DATA_DIR` overrides it. (Older builds
used `%APPDATA%\SingingTrainer`; that folder is copied into `data\` and removed on first run.)

This document covers the extra banks installed for timbre diversity. All tuning figures below were
measured by rendering a chromatic run through the app's own `Renderer` and tracking it with
`trainer_core::pitch::track`; the tooling is in `crates/trainer-audio/examples/sf2_scan.rs`.

## Installed set (16 banks, 68.8 MB)

Peak is the rendered peak at C4 with the app's default `-6 dB` event gain, useful for comparing
loudness. "Worst" is the largest median pitch error over the usable range, in cents.

| Timbre ID | Preset | Usable range | Worst | Peak | Source / license |
| --- | --- | --- | --- | --- | --- |
| `sf:UprightPiano:0` | Upright piano KW | C3-B5 | +8.6 c | 0.53 | FreePats, CC0 1.0 |
| `sf:TenorSax:0` | Tenor Saxophone | C3-B5 | +0.6 c | 0.53 | FreePats, CC0 1.0 |
| `sf:ClassicalGuitar:0` | Classical guitar | C3-B5 | +17.5 c (C#4 only; rest < 14 c) | 0.53 | FreePats, CC0 1.0 |
| `sf:ConcertHarp:0` | Concert harp | C3-B5 | -8.0 c | 0.53 | FreePats, CC0 1.0 |
| `sf:DrawbarOrgan:0` | Drawbar organ | C3-B5 | -1.9 c | 0.49 | FreePats, CC0 1.0 (re-keyed +12) |
| `sf:Violin:100` | Violin Arco | C3-B5 | -12.7 c | 0.43 | Saint James Orchestra, GPL-3.0 |
| `sf:Viola:41` | Viola | C3-B5 | -11.1 c | 0.53 | Saint James Orchestra, GPL-3.0 |
| `sf:Cello:42` | Cello | C3-C5 | -8.8 c | 0.43 | Saint James Orchestra, GPL-3.0 |
| `sf:Flute:73` | Flute | C3-B5 | +12.8 c | 0.22 (quiet) | Saint James Orchestra, GPL-3.0 |
| `sf:FrenchHorn:60` | French Horn | C3-B5 | +4.5 c | 0.49 | Saint James Orchestra, GPL-3.0 |
| `sf:Trombone:57` | Trombone | C3-B5 | +4.0 c | 0.53 | Saint James Orchestra, GPL-3.0 |
| `sf:SoloVoices:0` | Duncan, straight | C3-B5 | -14.7 c | 0.49 | Zanderjaz archive, license not stated |
| `sf:SoloVoices:1` | Duncan, "na" | C3-B5 | -13.4 c | 0.48 | Zanderjaz archive, license not stated |
| `sf:SoloVoices:4` | Buck | C3-C5 | -18.1 c | 0.52 | Zanderjaz archive, license not stated |
| `sf:SoloVoices:5` | Dawn | C3-B5 | -20.4 c | 0.53 | Zanderjaz archive, license not stated |
| `sf:OperaFemale:0` | opera fem vox | C#3-D#5 | -13.6 c | 0.53 | Zanderjaz archive, license not stated |
| `sf:SoftVoice:0` | Voice | C3-B5 | -12.3 c | 0.36 | Zanderjaz archive, license not stated |
| `sf:MaleOohs:1` | Basic Oohs | D3-B5 | +13.1 c | 0.95 | Zanderjaz archive, license not stated |
| `sf:HeavenChoir:1` | HeavenChoir2 | C3-B5 | -11.5 c | 0.20 (quiet) | Zanderjaz archive, license not stated |

The table lists 19 presets across 16 banks; several banks deliberately expose more than one useful
program. Presets outside these ranges either cut off, drift by more than a quarter tone, or are
chords/octave layers rather than single notes.

### Voices: which preset is male or female

| Voice | Presets | Source samples | Register |
| --- | --- | --- | --- |
| Male solo, straight | `SoloVoices:0` | 9 "Dunc" samples, 25 kHz | full C3-B5 |
| Male solo, "na" syllable | `SoloVoices:1` | "Dunc" samples with consonant | full C3-B5 |
| Male bass | `SoloVoices:4` | 17 "Buck" samples, 15 kHz | C3-C5 only |
| Female solo | `SoloVoices:5` | 21 "Dawn" samples, 17.5 kHz | full C3-B5 |
| Female operatic | `OperaFemale:0` | 6 samples, vibrato | C#3-D#5 |
| Female soft "ah" | `SoftVoice:0` | one looped sample | full C3-B5 |
| Male "ooh" ensemble | `MaleOohs:1` | 18 samples, 11 programs of variants | D3-B5 |
| Ethereal choir | `HeavenChoir:1` | 15 samples, mixed ensemble | full C3-B5 |

`SoloVoices` is the best bank for solo male/female contrast: programs 0 and 1 are the same male singer
(straight vs "na"), 4 is a second male singer with a bass source stretched up to C5, and 5 is the
"Dawn" female source. Programs 2 ("Nicholas") and 3 ("Soprano") are present in the file but rejected:
2 drifts to -34 cents around C5 and cuts off above D#5, and 3 is up to +49 cents sharp above C4.

`HeavenChoir` and `Flute` are much quieter than the rest at the same velocity. Raise the stimulus
gain for those timbres (or normalise loudness in the exercise) before using them in a mixed-timbre
test.

## Rejected candidates

These were downloaded and measured, then left out. Keeping the reasons here stops the same bad
banks being re-added later.

| Candidate | Why it was rejected |
| --- | --- |
| FreePats Clarinet | +18.9 cents sharp in several registers |
| FreePats Church Organ | Octave-ambiguous; the tracker locks to the 16' rank on C3/C4, so a global re-key would make the already-correct notes an octave high |
| FreePats Recorder | Only E4-E7; silent below E4 |
| FreePats YDP Grand Piano | +25.3 cents sharp; also 113 MB |
| Saint James violin (program 40), oboe, trumpet | Presets render silence; the working violin is the separate `Violin_Arco` file at program 100 |
| Saint James bassoon | Octave errors at A2-C3 and A#3-B3; very short samples |
| Saint James choir | -30.9 cents flat in the low register |
| Zanderjaz boy choir | -34.8 cents flat across C3-B3; +16.8 cents sharp around G#4-A4 |
| Zanderjaz Opera Singer Female 2 | About -20 cents across the range |
| Zanderjaz Opera Singer Female 3 | Sounds +1200 cents (one octave high) |
| Zanderjaz Mellotron choirs | Octave mapping is one octave low; tape choir, not a realistic voice |
| Virtual Playing Orchestra / SSO mixed choir | Converted from SFZ with Polyphone to test; +28/-22 cents. Realistic but not reliable enough for pitch training |
| Salamander C5 Lite | Not included: the conversion is marked free for personal use, not freely redistributable |

## Re-keying an octave error

A bank that is uniformly one octave low is not badly recorded; its sample root keys are wrong. The
root key is the MIDI note at which the sample plays at its recorded speed, so lowering it by 12 makes
the same sample play one octave higher at the same requested note. `tools/rekey_sf2.py` changes only
the `byOriginalPitch` byte in every `shdr` record and leaves the audio data, loops, envelopes and
preset mapping untouched:

```powershell
python tools/rekey_sf2.py input.sf2 output.sf2 --root-shift -12
```

The FreePats drawbar organ was exactly this case: every note rendered at -1200 cents. Shifting its 17
sample roots by -12 changed the measured error from **-1200 cents to a worst case of -1.9 cents**
across C3-B5. Shifting the result back by +12 reproduces the original file's SHA-256 exactly, so the
operation touches nothing else. `DrawbarOrgan.sf2` in the installed set is that re-keyed file, and
`fetch-extra-soundfonts.ps1` applies the same shift after downloading the original.

What re-keying does **not** fix:

- A uniform cents offset (e.g. a bank 20 cents sharp). That is a tuning error, not a wrong octave;
  correct it with a fine-tune adjustment, not a root shift.
- Register-dependent drift (clarinet, cello above C5, one sharp guitar note). Each sample would need
  its own correction.
- Silent presets (Saint James violin/oboe/trumpet), missing key ranges, or broken loops.
- An organ with a deliberate 16' rank, where the perceived octave is genuinely ambiguous. Shifting
  the whole bank would move the notes that were already correct.

## Audition files

`F:\Documents\music app\soundfont-auditions\` contains one WAV per recommended preset. Each file
plays **C3, C4, C5, G4**: a 0.9 s note, the renderer's 0.6 s release tail, then a 0.1 s gap, 48 kHz
stereo. Use these to judge character and loudness before committing a timbre to a training pool.

`tuning-report.txt` in the same folder has the full chromatic scan for every recommended preset.
`bank-check.txt` shows that all 16 banks load from the app's data folder and that every used preset
renders at the correct pitch.

## Recommended pool split (not applied)

The defaults in `timbre.rs` still use General MIDI only. A starting point for AP training/holdout,
using disjoint timbres:

- **Training:** `gm:0`, `gm:48`, `sf:UprightPiano:0`, `sf:TenorSax:0`, `sf:ClassicalGuitar:0`,
  `sf:Viola:41`, `sf:SoloVoices:0`, `sf:SoftVoice:0`
- **Holdout:** existing GM holdouts plus `sf:FrenchHorn:60`, `sf:Trombone:57`, `sf:Cello:42`,
  `sf:Violin:100`, `sf:Flute:73`, `sf:DrawbarOrgan:0`, `sf:MaleOohs:1`, `sf:OperaFemale:0`,
  `sf:SoloVoices:4`, `sf:SoloVoices:5`, `sf:HeavenChoir:1`

Keeping `SoloVoices:0` in training and `SoloVoices:4`/`5` in holdout tests cross-voice transfer
inside one bank. Do not put the same timbre in both lists; the pool validator treats the holdout
list as authoritative for assessment.

## Checking a new bank

Four small examples in `crates/trainer-audio/examples` do the same checks used here:

```powershell
# Presets, bank/program numbers and key ranges (add --samples to list sample names)
cargo run -p trainer-audio --example sf2_info -- data/soundfonts/NewBank.sf2

# Chromatic tuning/level scan of one preset over C3-B5
cargo run -p trainer-audio --example sf2_scan -- data/soundfonts/NewBank.sf2 NewBank 0 48 83

# Four-note audition WAV per preset
cargo run -p trainer-audio --example sf2_audition -- data/soundfonts/NewBank.sf2 NewBank auditions 0 1 2

# Load a whole folder the way the app does and render one note per bank-0 preset
cargo run -p trainer-audio --example sf2_banks_check -- data/soundfonts

# Fix a bank that is uniformly an octave low (lowers sample roots by 12)
python tools/rekey_sf2.py input.sf2 output.sf2 --root-shift -12
```

Run these with the newer Rust toolchain required by the current `Cargo.lock` (the repo currently
needs rustc 1.95 or newer).

Other places worth checking when a new colour is needed:

- **FreePats** (https://freepats.zenvoid.org/) — CC0 instrument banks, individual SF2 downloads.
- **Polyphone library** (https://www.polyphone.io/en/soundfonts) — per-file licences and preset
  lists; many downloads require a free account.
- **Musical Artifacts** (https://musical-artifacts.com/) — large SF2 archive; check the licence on
  each artifact, not the site as a whole.
- **open-soundfonts** (https://github.com/open-soundfonts) — SGM, Saint James Orchestra and Virtual
  Playing Orchestra mirrors with explicit repository licences.
- **VSCO 2 CE / VCSL** (https://github.com/sgossner/VSCO-2-CE, https://github.com/sgossner/VCSL) —
  CC0 raw samples and SFZ mappings. These need conversion to SF2; Polyphone's CLI does it with
  `polyphone -1 -i <file.sfz> -d <output dir> -o <name>`.
- **GeneralUser GS** (https://github.com/mrbumpy409/GeneralUser-GS) — the bundled GM bank.

The app cannot load SFZ or Kontakt libraries directly. Converting an SFZ is fine for evaluation, but
measure the result: the SSO choir conversion tested here looked good on paper and was up to 28 cents
out in practice.

## Re-fetching and installing

```powershell
# Development checkout (writes to SingingApp\data\soundfonts)
scripts/fetch-extra-soundfonts.ps1

# Installed app (point at the data folder beside the exe)
scripts/fetch-extra-soundfonts.ps1 -Dest "$env:LOCALAPPDATA\Programs\Singing Trainer\data\soundfonts"
```

The script downloads the exact files, re-keys the drawbar organ (+12) after unpacking, checks each
SHA-256 against the values verified here, and refuses to install a changed upstream file. `-Force`
replaces an existing file whose hash differs. Restart the app afterwards; banks are only read at
startup.

## Licensing

- **FreePats** banks are CC0 1.0: no attribution required.
- **Saint James Orchestra** files are GPL-3.0. They are fine for personal use and for a GPL-compatible
  build; do not bundle them into a proprietary installer without checking the licence.
- **Zanderjaz** is an archive, not the author. Its page says licensing varies by file and some files
  have no creator or licence metadata. Treat `SoloVoices`, `OperaFemale`, `SoftVoice`, `MaleOohs` and
  `HeavenChoir` as personal-use-only until the original licences are found. They are downloaded into
  `data\` (git-ignored) and are not part of the repository or the installer.

A clean-licensed alternative worth trying if these voices ever need to be redistributed:
Polyphone's **ChorMaleFem** (listed as public domain, bank 0 programs 0/1/2 = male, mixed, female),
which requires a free Polyphone account to download; and the Virtual Playing Orchestra 3 male/female
choir SFZ files, which have good samples but need per-note pitch correction before they are suitable
for AP training.
