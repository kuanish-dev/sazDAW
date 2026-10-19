# Architecture

Principle: **SOLID, depend only on contracts**.
The contract (interface) comes first, implementations come second: an implementation must follow the interface and honor the contract;
everything else is its own internal business. Everything goes through interfaces, and the code stays simple. The upper layer uses traits from `daw-contracts`
and does not know how they are implemented. The full list of rules is in `CLAUDE.md`.

This file is the **map**: where each piece lives, one line per crate, area and feature. How a piece works is in its own doc:

| Doc | Holds | Read it |
|---|---|---|
| `crates/<crate>/CLAUDE.md`, `app/CLAUDE.md`, `app/src/<area>/CLAUDE.md` | How one crate or frontend area works: its role, code layout, decisions | When you work in that folder (Claude Code loads it by itself on the first file opened there) |
| `docs/features/<feature>.md` | A feature that crosses layers (contract → core → engine → RPC → UI): its whole path in one place | When the task touches that feature |
| `.claude/skills/<recipe>/SKILL.md` | Step-by-step recipes for the usual kinds of work, with the exact files to touch | When the task is one of them (Claude Code offers the skill by itself) |

**Where new text goes.** A change inside one crate or folder is described in that folder's `CLAUDE.md`; a feature that crosses
layers gets `docs/features/<feature>.md`. This map changes only when a crate, area, feature doc or recipe appears or changes its
role: one line in the tables below. Docs say where things are, why they are so and how to add a similar one; they don't retell the code.

## Crates

| Crate | Role | Depends on |
|---|---|---|
| [`daw-contracts`](crates/daw-contracts/CLAUDE.md) | Project model (data, serde), all traits, the shared pure rules, the error-code registry (`codes/`), backend label languages | — |
| [`daw-transport`](crates/daw-transport/CLAUDE.md) | Clock and transport control (atomics): loop region, song loop, cue | contracts |
| [`daw-metronome`](crates/daw-metronome/CLAUDE.md) | Metronome click as an `AudioProcessor` | contracts |
| [`daw-engine`](crates/daw-engine/CLAUDE.md) | Real-time audio engine: plays clips, processes the graph from the snapshot, meters, scopes and taps | contracts |
| [`daw-drums`](crates/daw-drums/CLAUDE.md) | 11 synthesized drums (`InstrumentFactory`) | contracts |
| [`daw-sampler`](crates/daw-sampler/CLAUDE.md) | The sampler instrument | contracts |
| [`daw-synth`](crates/daw-synth/CLAUDE.md) | The built-in subtractive synth | contracts |
| [`daw-strings`](crates/daw-strings/CLAUDE.md) | Plucked strings: acoustic and electric guitar, ukulele, bass guitar | contracts |
| [`daw-effects`](crates/daw-effects/CLAUDE.md) | Built-in effects and the shared DSP blocks they are built from | contracts |
| [`daw-note-effects`](crates/daw-note-effects/CLAUDE.md) | MIDI effects (arpeggiator, chord) and their catalog | contracts |
| [`daw-vst3`](crates/daw-vst3/CLAUDE.md) | VST3 effect and instrument host, plugin windows, crash protection; the only crate with `unsafe` | contracts |
| [`daw-patterns`](crates/daw-patterns/CLAUDE.md) | Drum parts from ready-made patterns, chord parts from strums | contracts |
| [`daw-catalog`](crates/daw-catalog/CLAUDE.md) | Instrument and effect registries: find a factory by kind | contracts |
| [`daw-stretch`](crates/daw-stretch/CLAUDE.md) | Time stretching and pitch of audio clips | contracts |
| [`daw-samples`](crates/daw-samples/CLAUDE.md) | Audio files: decoding, WAV writing, samples in memory, preview, the project sound browser | contracts |
| [`daw-project`](crates/daw-project/CLAUDE.md) | The core (`ProjectService`: the model, the edit pipeline, undo) and one entity per job on the project | contracts |
| [`daw-persistence`](crates/daw-persistence/CLAUDE.md) | Storage on disk: project folder, autosave, recent projects, preferences, presets, templates, small state files | contracts |
| [`daw-render`](crates/daw-render/CLAUDE.md) | Offline rendering: audio export, a track's sound for bouncing, level analysis, the loudness meters | contracts, encoders |
| [`daw-mixcheck`](crates/daw-mixcheck/CLAUDE.md) | Mix check: find what makes the project sound different from what was intended | contracts |
| [`daw-summary`](crates/daw-summary/CLAUDE.md) | A short project summary, mainly for AI | contracts |
| [`daw-midi`](crates/daw-midi/CLAUDE.md) | MIDI files and live MIDI input | contracts |
| [`daw-audio-cpal`](crates/daw-audio-cpal/CLAUDE.md) | Audio devices through cpal | contracts |
| [`daw-audio-recorder`](crates/daw-audio-recorder/CLAUDE.md) | Record an audio take from the input onto armed audio tracks | contracts |
| [`daw-recorder`](crates/daw-recorder/CLAUDE.md) | Record notes played live into a track clip | contracts |
| [`daw-automation-writer`](crates/daw-automation-writer/CLAUDE.md) | Write automation live while the song plays (Touch/Latch) | contracts |
| [`daw-demo`](crates/daw-demo/CLAUDE.md) | Build the starter beat | contracts |
| [`daw-bootstrap`](crates/daw-bootstrap/CLAUDE.md) | Session assembly: the only place where implementations are chosen | everything |
| [`daw-rpc`](crates/daw-rpc/CLAUDE.md) | The RPC method table "name → adapter over a contract", its self-description for AI | contracts |
| [`daw-mcp`](crates/daw-mcp/CLAUDE.md) | The MCP server for AI on top of the RPC table | contracts, rpc |
| [`daw-app`](crates/daw-app/CLAUDE.md) | Console root and the windowless `daw-mcp` binary | contracts, bootstrap |

Implementation crates do not depend on each other. Replacing cpal, a file format or an effect does not affect the rest.

## Frontend areas

The frontend is split by role, like the backend's entities: each role folder in `app/src/` holds its data, its work, its components and its math (how a role is built, and the table of all roles: [app/CLAUDE.md](app/CLAUDE.md)).

| Folder | Role |
|---|---|
| [`app/`](app/CLAUDE.md) | The frontend as a whole: the roles and how one is built, lint rules, the Tauri root `src-tauri/`, running |
| [`src/api/`](app/src/api/CLAUDE.md) | The contract with the backend (`RpcClient`), dialogs, shell; the TS mirror of the Rust JSON; typed wrappers |
| [`src/core/`](app/src/core/CLAUDE.md) | What every role shares: the backend, the project copy, how a call runs and tells the user, the playback tick |
| [`src/common/`](app/src/common/CLAUDE.md) | What several roles share: the UI kit, the look and motion, shared pure helpers |
| [`src/shell/`](app/src/shell/CLAUDE.md) | The window frame, the open tab, window keys |
| [`src/tracks/`](app/src/tracks/CLAUDE.md) | The timeline: tracks, clips, folders, markers, waveforms, row heights |
| [`src/effects/`](app/src/effects/CLAUDE.md) | Effect editors, their live graphs and the nodes' live sound |
| [`src/graph/`](app/src/graph/CLAUDE.md) | The Graph page and the signal path view |
| [`src/mixer/`](app/src/mixer/CLAUDE.md) | The Mixer page, meters, the master |
| [`src/settings/`](app/src/settings/CLAUDE.md) | The Settings page and the preferences |
| `src/pianoroll/`, `automation/`, `instruments/`, `keyboard/`, `recording/`, `midi/`, `transport/`, `history/`, `files/`, `selection/`, `stage/`, `samples/`, `sounds/`, `drums/`, `chords/`, `plugins/`, `presets/` | One role each (see [app/CLAUDE.md](app/CLAUDE.md)) |
| [`src/i18n/`](app/src/i18n/CLAUDE.md) | Interface languages (English default, Russian) |
| [`src/mock/`](app/src/mock/CLAUDE.md) | The browser stand-in for the backend |
| `src/session.tsx` | Creates the core and every role once and gives the UI the `Session` |
| `src/composition.ts` | Composition root: the only place that picks the implementations |

## Features

Cross-layer features (`docs/features/`):

| Feature | Doc |
|---|---|
| Processing graph: nodes, connections, `Graph::check_edge` | [processing-graph.md](docs/features/processing-graph.md) |
| Tempo map: tempo and time signature changes | [tempo-map.md](docs/features/tempo-map.md) |
| Cue (start position) and the ruler strip | [cue.md](docs/features/cue.md) |
| Song loop | [song-loop.md](docs/features/song-loop.md) |
| Song markers, track colors, folders | [markers-colors-folders.md](docs/features/markers-colors-folders.md) |
| Warping audio clips to tempo, pitch, crossfades | [warping.md](docs/features/warping.md) |
| Audio clip editing: fades, gain, trim, normalize, reverse | [audio-clip-editing.md](docs/features/audio-clip-editing.md) |
| Musical note edits, the controller lane | [note-edits.md](docs/features/note-edits.md) |
| MIDI controllers: pitch bend, CC, aftertouch, MIDI learn | [midi-controllers.md](docs/features/midi-controllers.md) |
| Automation: editing and live writing | [automation.md](docs/features/automation.md) |
| Sends: pre/post-fader, connection level | [sends.md](docs/features/sends.md) |
| Sidechain, gain reduction, effect graphs | [sidechain.md](docs/features/sidechain.md) |
| Plugin delay compensation (PDC) | [pdc.md](docs/features/pdc.md) |
| Channel utility: polarity, width, mono, time alignment | [channel-utility.md](docs/features/channel-utility.md) |
| Virtual stage | [virtual-stage.md](docs/features/virtual-stage.md) |
| Input monitoring, recording, takes, loop recording | [recording.md](docs/features/recording.md) |
| Live master meter: loudness, true peak, phase, mono check | [master-meter.md](docs/features/master-meter.md) |
| Master visualizer: spectrum and oscilloscope | [master-visualizer.md](docs/features/master-visualizer.md) |
| Audio export | [audio-export.md](docs/features/audio-export.md) |
| Rendering a track to audio (bounce, freeze) | [bounce.md](docs/features/bounce.md) |
| Checking the result by ear: levels and mix check | [mix-check.md](docs/features/mix-check.md) |
| Project templates and presets | [templates-and-presets.md](docs/features/templates-and-presets.md) |
| User and app preferences | [preferences.md](docs/features/preferences.md) |
| Sounds: the project folder, the sound browser, dragging a sound | [sounds.md](docs/features/sounds.md) |
| Errors: the error log and errors as codes | [errors.md](docs/features/errors.md) |

Topics that live in one crate or area are in its doc: built-in effects, synth, strings, drum patterns and chord parts, VST3
(plugin windows, crash protection), the project core, the AI interface (RPC self-description, MCP), the project summary, MIDI
effects, backend label languages (`daw-contracts`), interface languages (`src/i18n/`), effect visuals (`src/effects/`).
Licenses: [docs/licenses.md](docs/licenses.md).

## Recipes

| Task | Recipe |
|---|---|
| A new operation end to end (contract → implementation → RPC → API → mock → action → UI) | [add-operation](.claude/skills/add-operation/SKILL.md) |
| A new built-in effect | [add-effect](.claude/skills/add-effect/SKILL.md) |
| A new built-in instrument | [add-instrument](.claude/skills/add-instrument/SKILL.md) |
| A new contract failure (error code) | [add-error-code](.claude/skills/add-error-code/SKILL.md) |

## Data flow

```
UI/CLI ──(contracts *Control)──▶ ProjectService ──snapshot Arc<EngineSnapshot>──▶ EngineLink ──ArcSwap──▶ Engine (audio thread)
                                      │                                                ▲
                                      └──DSP objects (Box<dyn Instrument/Effect>)──rtrb────┘
```

* All edits go through `ProjectService::edit`: the state is cloned for history, the edit is applied,
  on error everything is rolled back, then missing DSP objects are created and an immutable snapshot is published.
* The audio thread picks up the fresh snapshot without locks (`ArcSwap`); old snapshots are kept by a "graveyard" on the control side,
  so the last reference is never released in the callback. DSP objects being freed are sent back through a queue.
* Live notes, meters and load are atomics and ring buffers.

## Rules

1. A new entity = its own crate (or module) + a trait in `daw-contracts`.
2. A new instrument or effect is a factory with a parameter table; the engine and the service have no branching on the kind name.
   An instrument factory declares the parameters that require recreating the instrument (`rebuild_on`); an effect is
   never recreated on a parameter change: every effect parameter goes through `Effect::set_param`.
3. Entities with two sides (UI and audio thread) are split into `*Control` (any thread, non-blocking) and a realtime side.
4. The audio callback has no allocations, locks or I/O. Engine tables are preallocated; project limits
   (64 tracks, 64 channels, 12 effects per track) match the capacities.
5. `AudioProcessor` and `Instrument` **add** sound into the buffer.
6. Time is in ticks everywhere (`PPQ = 960`); conversion to seconds, frames and bars is a single piece of math in the `TempoMap` tempo map (`daw-contracts/src/tempo_map.rs`, frontend — `common/tempoMap.ts`), see [tempo-map.md](docs/features/tempo-map.md).
7. The processing graph has no cycles; which connections are allowed is written in one place, `Graph::check_edge` (`daw-contracts/src/graph.rs`), see [processing-graph.md](docs/features/processing-graph.md).

## Running

```bash
export PATH="/opt/homebrew/opt/rustup/bin:$PATH"
cargo run -p daw-app          # console with the starter beat; `--empty` — empty project; `help` — commands
cargo test --workspace
```

**CI** (`.github/workflows/ci.yml`, GitHub Actions; on push to `main` and on pull requests) runs the same checks that CLAUDE.md requires before a commit. The `rust` job (macOS): `cargo fmt --all --check`, `scripts/build-test-plugins.sh` (tests, clippy and bundles of the dev-only VST3 fixtures in `test-plugins/` — the VST3 host tests load them), `cargo clippy … -D warnings`, `cargo test --workspace`. The `frontend` job (Linux) — `tsc`, `npm run lint`, `vite build` and `scripts/third_party_notices.py --check`. Third-party plugins (Surge XT) are not available in CI: their tests print a skip message and pass. Linux is not used for Rust: the app is macOS-only. A new mandatory check = a line in CLAUDE.md and a step in `ci.yml`.

The frontend: `app/CLAUDE.md`.
