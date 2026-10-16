# Distortion (test VST3)

Simple stereo distortion effect used only to test the DAW's VST3 host (dev-only fixture, not shipped; the DAW's own distortion is the built-in `distortion` effect in `crates/daw-effects`). Built with nih-plug (VST3 wrapper is GPLv3) — test plugin, not for distribution. Build: `scripts/build-test-plugins.sh` (from the repository root); the tests find the bundle in `target/bundled`, no installation needed.

## Build
```
cargo xtask bundle distortion --release   # -> target/bundled/Distortion.vst3
cargo test
```
Install (macOS): copy `target/bundled/Distortion.vst3` to `~/Library/Audio/Plug-Ins/VST3/` and ad-hoc sign: `codesign -s - --force --deep ~/Library/Audio/Plug-Ins/VST3/Distortion.vst3`.

## Parameters
- Drive: 0..40 dB (12) — gain before shaping
- Tone: 200..12000 Hz (6000, log) — one-pole low-pass after shaping
- Mix: 0..100 % (100) — dry/wet
- Output: -24..+6 dB (-6)
- Shape: Soft (tanh) / Hard (clip) / Fold (foldback)

Native GUI (nih_plug_egui, 360x240, five sliders), no MIDI, no sidechain, zero latency. The GUI exists to test the DAW's plugin window host (IPlugView).
