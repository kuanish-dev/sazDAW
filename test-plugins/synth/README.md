# Synth (VST3 test fixture)

A polyphonic subtractive synth (8 voices) with a visual window. **Dev-only:** a fixture for the DAW's VST3 host tests (`crates/daw-vst3/tests`, `crates/daw-bootstrap/tests`, `crates/daw-rpc/tests`); the app neither ships nor loads it. The DAW's own synth is built in (`crates/daw-synth`). Built with nih-plug (the VST3 wrapper is GPLv3). Build: `scripts/build-test-plugins.sh` (from the repository root).

## Build and install (as a regular VST3 for other DAWs)
```
cargo xtask bundle synth --release   # -> target/bundled/Synth.vst3
cargo test
```
macOS: copy `target/bundled/Synth.vst3` to `~/Library/Audio/Plug-Ins/VST3/` and sign it ad-hoc: `codesign -s - --force --deep ~/Library/Audio/Plug-Ins/VST3/Synth.vst3`. Then in the DAW: "Project" > "Scan plugins"; the instrument appears as `vst3:synth`.

## Parameters
Waveform (Sine/Triangle/Saw/Square), Attack, Hold, Decay, Sustain, Release, Cutoff (40…18000 Hz), Gain.

## Structure
- `src/engine.rs` — sound: voices, AHDSR envelope, lowpass. Pure math, knows nothing about nih-plug or the window.
- `src/params.rs` — parameters for the host (names, ranges, defaults).
- `src/visuals.rs` — what the audio thread shows the window (oscilloscope, held notes, "is it sounding"): atomics only, no locks. The DAW host opens the window on a SEPARATE plugin instance, so sounding instances are registered in a process-wide list, and the window shows the one that sounds (holding on to it while it sounds). Limitation: if several synths sound at once, the window shows the one that started first.
- `src/editor.rs` — window layout (egui): OSCILLATOR / FILTER / OUTPUT / AMP ENVELOPE sections, LCD display, keyboard.
- `src/widgets.rs` — the parts of the "hardware" look: 3D knobs with a scale (drag up/down, Shift for fine, double-click for the default), buttons with LEDs, LCD, level meter, keyboard with highlighted notes.
- `src/curves.rs` — envelope and filter curves and the oscilloscope (geometry is pure functions with tests).
- `src/lib.rs` — glue with the host: note events, the per-frame loop.

Window size 720×440. The DAW host checks itself on this plugin with the tests in `crates/daw-vst3/tests/all/synth.rs` (scan, sound, fade-out, window size).

## Playing from the computer keyboard
In the synth window the keys Z X C V… / Q W E R… play notes (as in the DAW main window), `[` and `]` change the octave. The host (`daw-vst3`) does this; the plugin itself does not handle keys: the sound goes through the main instance on the track.
