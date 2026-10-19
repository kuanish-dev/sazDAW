# Input monitoring, multi-track recording, and takes

**Contracts.**
- `MonitorMode { Off, On, Auto }` — a project field, `NodeKind::Audio::monitor` (undoable), edited via `TrackControl::set_monitor`. Off by default: with speakers next to the microphone, monitoring causes feedback.
- `AudioRecordControl` (`daw-contracts/src/record.rs`; `AudioRecorder` in `daw-audio-recorder`) — arming and `InputChannels` per track are session state, not written to the project. Its doc comments are the rules of a take (start errors, covered clip, loop passes).
- `InputMonitor` (the engine): the recorder hands it the arming table (`TrackInput`), it answers `MonitorStatus` (`problem()` is the text the UI shows).
- `LiveInput` — the input read by the output audio thread; `AudioCapture::take_live_input` hands it out once, and `daw-bootstrap/src/wiring/audio.rs` passes it to the engine with `DspCommand::SetLiveInput`.
- `TakeControl` (`daw-contracts/src/takes.rs`; `ProjectTakes` in `daw-project/src/takes.rs`, pure rules in `take_rules.rs`) — clip takes and the comp.

## Monitoring signal path

Besides peaks and the recording queue, the input callback (`daw-audio-cpal/src/capture.rs: InputFeed::feed`) writes channels 1–2 into a second ring queue (`daw-audio-cpal/src/live.rs`) while the engine wants the input. The engine asks for it with `LiveInput::set_wanted`, and the host thread opens or closes the input on its loop (`host/input.rs: Host::follow_monitoring`). The one input stream is shared with recording and the level meter and closes when nobody needs it; without running output the request has no effect.

In the engine, `daw-engine/src/monitor.rs` (`InputListener`) adds the input, by the track's channels (`InputChannels::sides`), into the node buffer of every listening audio track (`MonitorMode::hears_input(armed)`) before the mixer — so the input goes through the track's fader, mute/solo, effects and PDC, on top of its clips. An armed track keeps the input open too (levels and an instant recording start). Input and output clocks drift, so reading goes through a small jitter buffer; shortages count as `EngineLosses::monitor_underruns` (logged by `daw-bootstrap/src/engine_watch.rs`). If the input rate differs from the output rate, the input is not monitored (no real-time resampling): `MonitorStatus::problem` says so, and the UI colors "IN" and shows the text. The input table is `daw-engine/src/link/monitor.rs` (`MonitorTable`, atomics only).

## Multiple tracks and latency

There is one input: the recorder keeps channels 1–2 whole, and on `stop` each armed track gets its own channels (`daw-audio-recorder/src/split.rs: track_sound`). The latency trim is shared (`take.rs: latency_frames`): the "Recording latency, ms" setting (`AudioSettings::record_latency_ms`, tuned by ear), or else an estimate from the buffer size, plus PDC. It is cut from the audio before the WAV is written, because `ClipEdit` cannot change `offset_frames`. Arming and input changes during a take are rejected; deleted tracks are silently dropped.

## Takes

A recording over an audio clip on the same track that it covers at least halfway or lies inside (`split.rs: covered_clip`) becomes a new active take of that clip (`TakeControl::add_take`) instead of a new clip. Model: `ClipContent::Audio { sample, takes, comp }` — `sample` plays, `takes` holds all takes in recording order. All takes share the clip's position, length, `offset_frames`, warp, pitch, volume and fades, so a new take's audio is aligned to the clip start (`split.rs: align`). Removing a sample from the project removes it from takes; a clip goes only if that was its only sound (`daw-project/src/catalog.rs`). The snapshot and clip processing (`clip_audio`, `ProcessJob`) use the sounding takes (`Clip::sounding_takes`), so changing the take of a warped clip recomputes its audio.

## Take comping (by segments)

`ClipContent::Audio::comp: Vec<CompSegment>` — from each segment's `start_ticks` until the next, its take plays; empty means `sample` plays the whole clip. The rules are in the `TakeControl` doc comments: `set_comp` (the whole comp) and `choose_take_range` (one range — what the UI calls); `set_active_take` and a new take from recording clear the comp. The comp follows clip edits: split (`take_rules::split_comp`), shortening (`fit_comp`).

How a comp sounds is one formula, `Clip::take_span` (`daw-contracts/src/takes.rs`, allocation-free): each piece with half of the equal-power transitions (`COMP_CROSSFADE_SECONDS`) to its neighbours. The engine (`daw-engine/src/audio_voices.rs`) plays one voice per piece (`AudioVoice::part`), crossfaded like overlapping clips; when the comp is edited during playback, voices follow their pieces. Export uses the same engine (`daw-render: clip_audio`, per take), so export = live.

- RPC: `clips.setComp`, `clips.chooseTakeRange`, `clips.setActiveTake`, `clips.removeTake`.
- UI: `pianoroll/TakeLanes.tsx` in the audio clip panel — one waveform row per take (`tracks/clipWaves.ts: takeColumns`), sounding pieces highlighted (`tracks/takes.ts: takeRanges`); dragging along a row calls `actions.clips.chooseTakeRange` (snapped to the timeline grid, `tracks/takes.ts: draggedRange`; the grid comes from `TracksView` via `ClipPanel`, Alt — no snapping), clicking a name calls `setActiveTake`, ✕ calls `removeTake`. On the timeline a comped clip's waveform is assembled from its takes' pieces; the label is `tracks/clipRules.ts: clipLabel`.
- Mock: `mock/takeRules.ts` (the same rules).
- Tests: rules — `daw-project/src/take_rules.rs`; contract — `daw-project/src/tests/takes.rs`; formula — `daw-contracts/src/takes.rs`; engine (seam without a click, the right take frames, editing during playback) — `daw-engine/src/tests/comp.rs`; export = live — `daw-bootstrap/tests/all/render_comp.rs`; wire — `daw-rpc/tests/all/warp.rs`.

## Loop recording

With a loop region, playback jumps back at its end during an audio take, while the input arrives as one straight run. `AudioRecorder::start` remembers the loop region; on `stop`, after the latency cut, `daw-audio-recorder/src/passes.rs: passes` (pure) cuts the run at the jumps, counting boundaries in fractional frames by the tempo map so many passes do not drift. One pass is an ordinary take. With several, `take.rs: place_passes` gives each track one clip on the loop region (the clip the loop covers, or a new one at the loop start) and every pass becomes its take; the contract's `stop` comment says which pass is active. All passes are written and imported before any clip changes, so a clip that does not fit loses no audio. Everything stays one undo step.

Tests: `passes.rs` (cuts, pre-roll, mid-loop start, tempo change, no drift), `src/tests/take.rs`. Mock: `mock/loopPasses.ts` (the same cut in seconds) and `placePasses` in `mock/handlers/audioRecord.ts`. Note recording keeps its own loop recording inside the take clip (`daw-recorder`).

## Frontend

- RPC (`daw-rpc/src/audio_record.rs`): `audioRecord.setArmed`, `setInput`, `start`, `stop` (→ clips), `status`; `tracks.setMonitor`. `audio.devices` carries each device's channel count; `audio.settings`/`audio.apply` carry `recordLatencyMs`.
- Actions: `recording/audioRecord.ts` — `setArmed`, `setInput`, `stop` (selects the created clips, warns when frames were dropped) and `pressRecord`, the shared "● Record" transport button, which asks `recording/noteRecord.ts` for note recording.
- State: `recording/recordingStatus.ts` — one poll (`core/poller.ts`), started by actions (`recording.followRecording()`) and at startup, stopping itself when no take runs and nothing is armed; read via `session.recording.recordingStatus()`, `session.recording.trackInput(track)`, `session.recording.monitorProblem()`.
- UI: the audio track header, `tracks/TrackInput.tsx` — the "●" arm button, the input menu `InputMenu` (choices by the input device's channel count — `recording/recordInput.ts`), the input level, and `MonitorButton` ("IN"). The recording badge with "Stop" is `AudioRecordBadge` in `transport/TransportBar.tsx`. Takes — `pianoroll/TakeLanes.tsx` (above). The "Recording latency, ms" field (empty = auto) is in the Audio settings section.
- Mock: `mock/handlers/audioRecord.ts` (a take, levels, a take over a covered clip), take rules — `mock/takeRules.ts`, `tracks.setMonitor` — `mock/handlers/tracks.ts`, `clips.setActiveTake/removeTake` — `mock/handlers/clips.ts`.

## Limitations

- A new take from recording clears the comp (undo brings it back).
- The loop region is read when the take starts: moving it during the take is not followed. Every loop pass is a take with no upper limit (a long take over a short loop makes many; one undo removes them all).
- Comp seams are exactly 10 ms, not configurable.
- A new take is recorded at the project tempo: in a warped clip with a different source tempo it is stretched like the earlier ones (warp is shared). Takes with different sample rates align precisely only if `offset_frames = 0` (the offset is shared, in frames).
- A recording that covers a clip by less than half and does not lie inside it becomes a new clip (if there is room).
- Monitoring works only when the input and output rates match; its latency = device buffers + margin (from ~3 ms) + the track's PDC; with different devices the clocks drift — occasional clicks (visible in the log). The clips of a monitoring track keep playing along with the input.
- Arming and input selection are not saved in the project. Input channels are only 1 and 2.

**How to add something similar** (e.g. inputs 3–4): a variant in `InputChannels` + `sides`/`file_channels`, recording and monitoring queues for more channels (`daw-audio-cpal/src/capture.rs`, `live.rs`), labels — `recording/recordInput.ts`.
