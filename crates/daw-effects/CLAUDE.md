# `daw-effects`

Built-in effects: eq, compressor, reverb, delay (tempo-synced), distortion, limiter, chorus, filter, gate, saturator, multiband compressor, dynamic EQ, de-esser, transient shaper, linear-phase EQ, tape/console, bitcrusher, phaser (see "Built-in effects"); the shared DSP blocks they are built from (`biquad.rs`, `smooth.rs`, `dynamics.rs`, `crossover.rs`, `oversample.rs`, `fft.rs`).

Depends on: contracts.

Map of the whole code: [ARCHITECTURE.md](../../ARCHITECTURE.md). Cross-layer features it takes part in: [pdc](../../docs/features/pdc.md), [sidechain](../../docs/features/sidechain.md).

## Built-in effects (`daw-effects`)

**The frame** (`lib.rs`). Each effect is one module: a `PARAMS` table (`ParamInfo` in real units, `L10n` labels; newer tables use `number`/`switch`/`choice`) and `EffectKind` + `Effect`. The factory is shared (`Factory<E>`, `build::<E>`); registration is one line in `factories()`, which `daw-bootstrap/src/wiring/catalogs.rs` registers. `clamp_param` fits a value through `ParamInfo::fit` (a choice gets a whole index: its automation arrives fractional). `build` ends with `reset`, so a new effect's smoothed params start at their values instead of gliding in. An effect that needs more than its fields (a thread) starts it in `EffectKind::start`, on the control side after `build`; a failure is the factory's `Err`.

Every module's `//!` header explains how it works and names the frontend module that mirrors its math for the graph ("a change here must be mirrored there"). The engine reaches an effect only through `Effect` (`daw-contracts/src/effect.rs`): `set_timing` before every segment, `gain_reduction_db`/`band_gains_db` after `process` ([sidechain](../../docs/features/sidechain.md)), `latency_samples` for [PDC](../../docs/features/pdc.md).

**Shared DSP blocks** — pure math without allocations, to move as they are into the `daw-dsp` crate (batch 6 of `docs/refactor-audit.md`). An effect that needs one takes it from here rather than copying it:
* `biquad.rs` — RBJ coefficients (shelves, peaking, low/high/band-pass, a crossover's all-pass), their magnitude at a frequency, the biquad state, `DcBlocker`;
* `smooth.rs` — `Smoothed` (`SMOOTH_MS`): the engine sends params once per block, and the step would click;
* `dynamics.rs` — `peak_db`, `GainComputer` (the soft-knee curve), `Follower` (attack/release); the compressor, multiband, de-esser, dynamic EQ and transient shaper run on them;
* `crossover.rs` — the Linkwitz–Riley band split;
* `oversample.rs` — a curve at twice the rate (saturator, tape): aliasing roughly 20 times weaker, not gone;
* `fft.rs` — a radix-2 complex FFT set up in `new`, so a transform never allocates.

**Effects worth a note** (the rest — eq, compressor, limiter, chorus — are told by their headers):
* **`filter`** (`filter.rs`): Simper's SVF rather than a biquad — stable under fast cutoff and resonance automation, and a type change crossfades its three outputs.
* **`gate`** (`gate.rs`): hysteresis (`HYSTERESIS_DB`) plus hold, so it does not chatter; `sidechain` keys it like the compressor.
* **`distortion`** (`distortion.rs`) and **`saturator`** (`saturator.rs`, curve at twice the rate): waveshapers; a NaN/inf input sample becomes silence in the distortion.
* **`delay`** (`delay.rs`): with `sync` on, the time comes from the tempo the engine passes in `Effect::set_timing` (`BlockTiming::tempo`; `Bpm::DEFAULT` before the first segment); "Off" (the default) uses `time` in ms, up to `MAX_SECONDS`. A time change crossfades from the old echo to the new (`FADE_MS`) instead of sliding: a fast slide squeals. `low_cut`/`high_cut` filter the echo as it is read, inside the loop, so every repeat is thinner and darker than the last; at their ends they are off.
* **`reverb`** (`reverb.rs`): Freeverb; the mono input first waits `predelay` (a ring allocated for `MAX_PREDELAY_MS` in `new`/`prepare`) and passes a low cut, so the room starts after the hit and the lows stay out of the tail.
* **`multiband`** (`multiband.rs`): four bands, or three (the top two play as one with band 3's settings), split by `crossover.rs`. Reports each band's reduction as band gains and the deepest as its gain reduction. No latency.
* **`dynamic_eq`** (`dynamic_eq.rs`): four peaking bands that move only while their band (or the `sidechain` key) is loud; gain 0 leaves a band flat, so a new one is transparent. Reports each band's signed gain. No latency.
* **`de_esser`** (`de_esser.rs`): split-band (only the band is turned down; with no reduction the sound is untouched, phase included) or wideband; `listen` plays the detected band alone. Reports its gain reduction. No latency.
* **`transient`** (`transient.rs`): attack and sustain at any level (no threshold), from two follower pairs. No lookahead: the first fraction of a millisecond of a hit is shaped as the peak follower catches it.
* **`linear_eq`** (`linear_eq/`), for mastering: the EQ's table and bands (`eq::PARAMS`, `eq::filters`) as a linear-phase FIR. Three jobs, three files: `design.rs` (settings → kernel), `worker.rs` (the designer thread), `convolver.rs` (FFT overlap-save, crossfading to a new kernel at the next block). Why a thread: params arrive on the audio thread, where designing a kernel has no place, so each instance has its own (`daw-linear-eq`, started in `EffectKind::start`; failing to start is GR-412 `EFFECT_WORKER_FAILED`). The kernel crosses as a seqlock over atomics: no lock, no `unsafe`, the audio side never waits. Allocations only in `new`/`prepare` (control side). `kernel_len` grows with the rate so the low end keeps its shape to about 25 Hz; **latency** `len + len/2` (6,144 frames, 128 ms at 48 kHz) is reported by `latency_samples` for PDC. Limitation: a setting is heard after the designer's poll and the next block; an offline render faster than real time may hear an automated move a block or two late.
* **`tape`** (`tape.rs`): Tape and Console voices on an off-centre soft knee at twice the rate; both voices run and `mode` crossfades between them. No latency.
* **`crusher`** (`crusher.rs`): bit depth (a continuous grid of `2^(bits-1)` steps, so automation sweeps it without jumps of the setting) and downsampling (a held sample on a phase, so a fractional factor is allowed; both channels held at the same moments). Not oversampled on purpose: the aliasing is the sound. No latency.
* **`phaser`** (`phaser.rs`): 4–12 first-order all-pass stages swept in octaves by a sine LFO, feedback around the chain, the right channel's LFO `spread` degrees later. Stages that a `stages` change brings in start from silence. No latency.

**How to add a similar effect:** the recipe [add-effect](../../.claude/skills/add-effect/SKILL.md). Tests live in the module (helpers in `testing.rs`); the shared contract check (`testing.rs: assert_honours_contract`) runs on every kind in `factories()` by itself.
