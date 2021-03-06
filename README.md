Explore musical tunings and create synthesizer tuning files for microtonal scales.

# Resources

- [Changelog](https://github.com/Woyten/tune/releases)
- [CLI documentation](https://crates.io/crates/tune-cli)
- [API documentatin](https://docs.rs/tune/)

# Overview

`tune` is planned to be part of a larger ecosystem for microtonal software in Rust.
So far, `tune` offers a CLI and an API with the following features:

- Pitch conversions
  - Convert between linear and logarithmic ratios
  - Determine the frequency for a given note in a custom tuning system
  - Determine the note for a given frequency in a custom tuning system
  - Find fractional approximations for frequency ratios
- Export scales
  - To Scala (scl and kbm) format
  - As Midi Tuning Standard (MTS) Sysex Messages
    - Single Note Tuning
    - Scale/Octave Tuning
- Import scales
  - From Scala (scl) format
- Tuning maps / Live retuning
  - Enhance the capabilities of synthesizers with limited tuning support
- EDO scales
  - Analyze meantone and porcupine temperaments
  - Find keyboard layouts
