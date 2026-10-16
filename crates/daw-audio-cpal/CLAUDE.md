# `daw-audio-cpal`

Audio via cpal: `CpalAudio` (`AudioDevices`) — device list, choice of output/input and buffer size, stream restart, input level, input capture (`AudioCapture`), input for monitoring (`LiveInput`, `live.rs`).

Depends on: contracts.

Map of the whole code: [ARCHITECTURE.md](../../ARCHITECTURE.md). Cross-layer features it takes part in: [recording](../../docs/features/recording.md).

## Audio: devices and buffer

**Contracts** (`daw-contracts/src/audio.rs`): `AudioDevices`, `AudioCapture`, `LiveInput` and `AudioSettingsStore` (where the choice is stored: `AudioSettingsFile`, `audio.json` in `daw-persistence`). The UI sees them only as `Controls.audio` and the `audio.*` RPC (`daw-rpc/src/audio.rs`).

- **The host thread.** `CpalAudio` is the only code that knows about cpal. A cpal stream cannot move between threads, so every output and input stream is created, lives and closes on one host thread (`host/`: `mod.rs` — commands, the loop, the shared state `Shared`; `output.rs`; `input.rs` — input and capture; `faults.rs` — faults to the log). The rest of the code sends it commands and waits for the reply; status, choice and peaks are read from `Shared` without going through the thread. Output is started and stopped with `CpalAudio::start_output/stop_output` (not `AudioCapture::start/stop`). System audio sits behind the private trait `Backend` (`backend.rs`; the real one is `system.rs`), so the host thread is tested with fake hardware (`src/tests/`). Pure rules: `choose.rs` (device by name, valid buffer), `peaks.rs`.
- **Stream restart.** The engine lives in an `Arc<Mutex<..>>` outside the output stream and moves to the new stream on a device or buffer change; the output callback takes it with `try_lock` and plays silence while the switch holds it. The new choice is checked while the old sound still plays; if the new stream fails, sound goes back to the previous device (else the system one) and the choice is not saved.
- **Startup and faults.** `Session::start_audio()` (`daw-bootstrap`) starts the host with the saved choice; a missing device plays on the system one with a warning in `AppLog` (source `audio`). The cpal error callback can run on the audio thread (macOS reports overloads there), so it only counts faults in atomics (`faults.rs`, `StreamFaults`); the host thread moves them to the log and `status.error`. Headless sessions (`build_session`) get `CpalAudio::without_devices`.
- **Input.** One input stream serves the level meter (`watch_input`), recording (capture) and monitoring (`LiveInput`, requested by the engine); it is open while any of them needs it. It opens only on request because otherwise macOS would ask for microphone access at startup. A denied permission comes back as an error with a hint; a built `.app` needs the `NSMicrophoneUsageDescription` key (`app/src-tauri/Info.plist`).
- **Input capture** (`capture.rs`). The input callback computes peaks and, while capturing, copies the first two channels into a preallocated SPSC `rtrb` queue; no allocations or locks there. Its reading side sits in `Shared` and is drained by `AudioRecorder` (`daw-audio-recorder`) from a regular thread, also after `stop`. Overflow is counted in `dropped_frames`. Changing the input device during a capture is rejected (it would change the take's format).
- **Adding a similar setting:** a field in `AudioSettings` → `audio.json` → `CpalAudio::apply` → RPC.
