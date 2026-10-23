# Refactor audit against CLAUDE.md (2026-10-15)

Whole codebase: 30 crates, `app/src-tauri`, and the frontend `app/src`. Audited on branch `feat/song-loop` at `32a33a4`.

**How to read this.**
- **Severity.** **high** means a Forbidden item, a broken contract, a lost error, or a real bug. **medium** means mixed roles, duplication, backward-compat code, a missing contract test, or a broken encapsulation. **low** means naming, missing headers or "why" comments, doc drift, or language.
- **R-numbers** are the core rules in CLAUDE.md.
- **Verification.** Every **high** item was re-read against the code. Medium and low items are listed as the area audits reported them. Each batch checks its items again before changing code.
- **File size** was not judged anywhere; splits are proposed only by role.

The batch numbers name the batches. The order they run in is under "Order of work" below.

---

## Decisions (settled by the owner, 2026-10-15)

1. **No locks in the audio callback.** Rework daw-audio-cpal so the open stream owns the renderer and the live producer. When the host drops the stream, a guard's `Drop` sends them back over a channel. The `try_lock` paths and `SharedRenderer` go away. There is no exception to the rule. (Batch 5)
2. **Shared DSP blocks live in a new `daw-dsp` crate.** It is a leaf toolkit: it depends on nothing, or only on `daw-contracts`. Instrument and effect crates may depend on it. CLAUDE.md "How it is checked", ARCHITECTURE and `tests/all/architecture.rs` gain that one allowed edge before the code moves. (Batch 6)
3. **Every contract that does more than one job is split, one contract per step.** Each step runs the full feature workflow: contract, tests, implementation, wiring, RPC/MCP/frontend api, ARCHITECTURE.md. RPC method names stay the same wherever the job doesn't change, so the frontend is untouched. (Batch 7)
4. **Old-file `#[serde(default)]` attributes are removed now (R10).** Projects saved by the current build keep opening, because every field is written. To keep an older project, open and re-save it with today's build before this lands. (Batch 13)
5. **The default accent is blue** (the VS Code-like look). Fix `Appearance::default` and its test in Rust; the enum and the frontend already say blue. (Batch 1)

## Order of work

Each step is one commit or a few made straight on `dev`, with no feature branch and no PR (`dev` is merged into `main` when it is stable; see CONTRIBUTING.md). Verification: `cargo check` for the touched crates, and `npx tsc --noEmit` plus `npm run lint` for the frontend. Tests are written as the rules require, but they are not run (owner's choice); every report says so. Each step re-checks its findings first, ticks them off here, and updates ARCHITECTURE.md for what it moved.

**Rust track** (in this order; most steps touch `daw-contracts`, so they don't run in parallel):
1. **Batch 1: real bugs**, including the blue accent default. The `SAVE_METHODS` flush is the largest item; it goes last in the batch.
2. **Batch 3: contract docs and ARCHITECTURE/CLAUDE.md drift.** It makes the baseline truthful before anything moves.
3. **Batch 13: test groundwork.** A `testing` feature on daw-contracts with `MemoryLog` and the shared fakes, a `Clip` test builder, one `serial()` in daw-vst3, and the serde-default removal (decision 4). It comes before the batches that add tests, so they stop adding copies.
4. **Batch 2: errors that disappear, backend part.** New codes and logging.
5. **Batch 4: shared small things into `daw-contracts`.** `in_group`, `sync::read`/`write`, ticks↔frames, `fit_param`, graph and track helpers, `PollThread`, `ContractError::coded`.
6. **Batch 5: the audio thread and the engine** (decision 1).
7. **Batch 6: the `daw-dsp` crate** (decision 2). The dependency rule changes first, then the blocks move one at a time.
8. **Batch 7: contract splits, one per step**, in this order:
   1. `TransportControl`
   2. the `ClipControl` loop region
   3. the disabled-plugins owner
   4. `Presets`/`ProjectTemplates` and `ProjectStorage`
   5. `FolderControl`
   6. `TrackControl`
   7. `AudioDevices`
   8. the small moves (`vary_to_new_clip`, `Controls.storage`, `Chord`)
   9. `GraphControl`, the largest, last
9. **Batch 8: role splits inside the implementations.** In order: daw-render, daw-audio-recorder, daw-project, daw-vst3, daw-patterns, daw-persistence.
10. **Batch 9: logic out of the front doors and daw-bootstrap.**
11. **Batch 14: small cleanups.**

**Frontend track** (separate files, so it can run alongside the Rust track in its own session):
1. **Batch 2: errors that disappear, frontend part.**
2. **Batch 10: actions and state.** Skip `actions/plugins.ts` (Batch 7, step 3 replaces it).
3. **Batch 11: UI.**
4. **Batch 12: mock, domain, api.** Run it after Batch 7, because the mock mirrors the backend contracts.

---

## Batch 1: Real bugs (fix first; each with a test)

**Status:** done on branch `refactor/batch-1-bugs` (2026-10-15), one commit per fix. Each was checked with `cargo check` (plus `tsc` for the frontend type); the new tests were written but not run.

- **Done.** [high] `crates/daw-render/src/stems.rs:28`. `stem_graph` keeps every edge between kept nodes, and `MASTER` is always kept. A track that sends to a bus and also outputs to the master therefore leaks its dry sound into the bus stem. `analysis.levels per: bus` has the same bug (`analysis/mod.rs:167 bus_only`). → Keep only `bus → MASTER` among the edges into the master. Add tests with a send and with a sidechain key. Effort S.
- **Done.** [high] `crates/daw-recorder/src/lib.rs:249-260, 339`. `set_armed(false)` disarms first and then polls. `handle_events` runs only `if state.active && allowed`, so the notes from the last poll interval are dropped without a log entry. The engine still returns queued events after disarm (`daw-engine/src/link/mod.rs:257`). The test fake drains nothing while disarmed, which hides the bug. → Handle drained events whenever `state.active`, make the fake behave like the engine, and add the test "disarm while playing keeps the last notes". Effort S.
- **Done.** [high] `crates/daw-project/src/midi_mappings.rs:141`. A mapping without min/max uses `0..1`. The contract (`daw-contracts/src/model.rs:433-435`) says `None` means the parameter's own range (pan −1..1, volume 0..MAX_LEVEL). → Make `min`/`max` plain `f32` (R10). Put the range next to `ops::validate_target`, and drop the second copy of the volume/pan ranges. Effort S.
- **Done.** [high] `app/src-tauri/src/plugin_window.rs:27` and `main.rs:49-51`. Before an operation that saves or renders the project, open plugin windows should hand their state to it. That flush is keyed by a hand-kept `SAVE_METHODS` list inside the Tauri `rpc` command, so three paths skip it:
  - the MCP door (an AI `project_save` or `tracks_bounce` never flushes);
  - autosave;
  - `project.export`, `analysis.levels` and `analysis.check`.

  → Put the flush behind a narrow contract (for example `EditorStateFlush`) that the saving and rendering implementations call, wired in bootstrap. Delete `SAVE_METHODS`. Effort M. *Done as:* a `PluginStateFlush` contract, which the RPC table runs before methods registered with `add_flushing`. Autosave is left out on purpose: windows write their state within a second anyway.
- **Done.** [high] `crates/daw-contracts/src/preferences.rs:84-90, 120` and `app/src/domain/preferences.ts:10`. The accent default disagrees in three places. → Blue (decision 5): fix `Appearance::default` and the test at `:189`. Effort S.
- **Done.** [medium] `crates/daw-stretch/src/cache.rs:54-55`. `work` checks `wanted` and removes from `queued` under two separate locks. A `ready()` call in between loses the job, and the clip stays silent until the next edit. → Use one lock scope and add the test "cancelled while queued". Effort S.
- **Done.** [medium] `crates/daw-project/src/service/mod.rs:160-172`. A failed "Save as" leaves the open project renamed after a file it is not bound to, and the rename bumps no revision. → Write a renamed clone, and commit the name, path and dirty flag only after `Ok`. Effort S.
- **Done.** [medium] `crates/daw-render/src/export/range.rs:23`. `tail_seconds.max(0.0)` accepts a negative tail. A huge tail saturates to `u64::MAX`, and `body + tail` overflows. Bounce already validates the same value (`track.rs:75 check_tail`). → Use one shared tail check with a code, for both export and bounce. Effort S.
- **Done.** [medium] `crates/daw-vst3/src/instance/mod.rs:152-154`. If the forward `connect` succeeds and the backward one fails, `connected = false`, so the forward link is never disconnected. → Disconnect each direction that actually succeeded. Effort S.
- **Done.** [medium] `crates/daw-drums/src/voice.rs:151`. `all_notes_off` is empty, so drums ring up to about 5 s after Stop, seek or a loop wrap. `Instrument::all_notes_off` has no documented contract, and the implementations disagree with each other. → Write the contract first, then give drums a short fade. Effort S.
- **Done.** [medium] `crates/daw-sampler/src/lib.rs:376`. An unknown sample id silently becomes a silent sampler. → Return `samples::NOT_FOUND` for an unknown id. Keep `0` meaning "no sample". Effort S.
- **Done.** [medium] `crates/daw-vst3/src/effect.rs:63-69`, `instrument.rs:82` and `host.rs:64-66`. Plugin latency is read only in `prepare`, and `kLatencyChanged` is ignored. ARCHITECTURE:360 says a changed latency is picked up on the next edit, which is false for VST3. → Either fix the docs (S) or handle `kLatencyChanged` (M). *Done as:* the docs were fixed. Handling `kLatencyChanged` moved to Batch 8 (daw-vst3).
- **Done.** [low] `crates/daw-audio-recorder/src/take.rs:149-152`. A loop pass that ends before the target clip starts is skipped with `continue`. Its audio is neither saved nor reported, though the contract says every pass becomes a take. → Save it anyway, or state the exception in the contract. Effort S. *Done as:* the contract states the exception, because a single take already loses its part before the clip.
- **Done.** [low] `crates/daw-project/src/note_transforms.rs:79`. `semitones.abs()` overflows for `i32::MIN`. → Use `unsigned_abs()`. Effort S.
- **Done.** [low] `crates/daw-project/src/tracks.rs:130-134`. `set_instrument` checks `frozen` outside the `edit` closure, so a freeze can slip in between. → Move the check into the edit. Effort S.
- **Done.** [low] `crates/daw-persistence/src/templates.rs:109`. The fallback `file.parent().unwrap_or(&self.dir)` would `remove_dir_all` the whole templates folder if it were ever reached. → Compute the template folder directly. Effort S.
- **Done.** [low] `app/src-tauri/src/main.rs:95-103`. The startup flag is set after `open()` returns. A panic there leaves every `rpc` call waiting forever. → Set the flag from a drop guard. Effort S.

## Batch 2: Errors that disappear (R11)

**Backend**
- [high] `crates/daw-vst3/src/editor.rs:112` (`save_state().ok()`) and `editors.rs:108` (`capture().ok()`). Plugin `getState` failures are swallowed. → Return the error and log it with `journal::error` in `Vst3Editors`. Effort S.
- [high] `crates/daw-patterns/src/render.rs:113` and `crates/daw-samples/src/preview.rs:43`. `let _ = dsp.send(PreviewStop)` hides `ENGINE_QUEUE_FULL`, so the preview keeps looping after Stop. The comment next to it is wrong. → Log the failure, or return a `Result`. Fix the comment. Effort S.
- [medium] daw-vst3 FFI results are ignored at `instance/mod.rs:120, 152-154, 244-251` and `editor.rs:166, 184`. `reset` also runs on the audio thread outside `guarded`. → Return the results, log them on the control side, and run `reset` inside `guarded`. Effort S/M.
- [medium] daw-engine overflow drops on the audio thread are never counted:
  - `notes.rs:75, 90, 134`;
  - `controllers.rs:76`;
  - `note_effects.rs:47, 53, 162, 196`;
  - `audio_voices.rs:114`, where a voice is not started and the clip stays silent.

  → Add `Loss` variants (`ScheduleOverflow`, `VoiceOverflow`) and count them through the existing telemetry. Effort S.
- [medium] `crates/daw-audio-cpal/src/system.rs:98, 104`. A device enumeration failure becomes an empty list, so an existing input is reported as `INPUT_DEVICE_NOT_FOUND`. → Return a `Result` and log the failure. Effort S.
- [medium] `crates/daw-project/src/service/mod.rs:194-204`. Sample load failures on open go only to in-memory warnings, never to `errors.log`. → Log each one. Effort S.
- [medium] `crates/daw-samples/src/decoder.rs:87`. Broken packets are dropped silently, and the sound gets shorter and shifted. → Count them, log one warning, and add a test. Effort S.
- [medium] `crates/daw-persistence/src/sources.rs:49`. `SoundSources::add` saves with `persist`, which only logs, then returns `Ok`. The link is lost on restart. → Use `try_persist` and roll back on failure. Effort S.
- [medium] Failures carried as free text instead of codes:
  - `ScanFailure.reason` (`daw-contracts/src/effect.rs:72`, from `daw-vst3/src/isolated.rs:103, 119-126` and `scanner.rs:130, 164`). The UI shows it raw.
  - "plugin file is gone" folded into PL-460 (`daw-vst3/src/source.rs:76`).
  - `MonitorStatus::problem()` builds localized text with `tr!` (`daw-contracts/src/engine.rs:238`, `record.rs:90`).
  - Nested errors flattened into English `details` (`daw-audio-recorder/src/take.rs:95, 99, 125, 156`).

  → Register codes and carry a `ContractError` instead. Effort M.
- [medium] `crates/daw-mcp/src/bridge.rs:47-52`. Bridge errors reach the client as free text. MCP's own refusals (`lib.rs:118, 120, 131`) and HTTP rejections (`http.rs:130-153`) are never logged. → Register codes and log through `server.log()`. Effort S.
- [medium] `app/src-tauri/src/plugin_window.rs:103, 128, 134, 143, 188, 199`. `let _ =` on window calls such as unminimize, destroy, set_focus and set_size. → Log through `note_failure`, or comment why the failure is harmless. Effort S.
- [medium] Startup failures go only to stderr, never to errors.log: `crates/daw-app/src/main.rs:23-31, 35` and `mcp_main.rs:65-83, 89`. → `Session::start_audio` logs its own failure; also log project-open and stdin errors. Effort S.
- [low] Directory and read errors are swallowed:
  - `filter_map(Result::ok)` in `daw-vst3/src/scanner.rs:76`, `daw-persistence/src/presets.rs:120`, `templates.rs:89` and `daw-samples/src/browser.rs:43`;
  - `daw-persistence/src/assets.rs:12-15` treats two unreadable files as the same;
  - `daw-vst3/src/isolated.rs:89, 167-170`.

  Effort S.
- [low] `crates/daw-persistence/src/log.rs:39-43, 68`. Rotation and serialization failures vanish, and rotation is retried on every record. Effort S.
- [low] Partial files are left after a failed write: `crates/daw-midi/src/file/mod.rs:65` and `crates/daw-samples/src/writer.rs:26-31`. → Write a temp file and rename it, as daw-render and daw-persistence already do. Effort S.
- [low] `crates/daw-render/src/lib.rs:62, 104`. A take with a missing sample is skipped silently, which contradicts the doc right above it. Effort S.
- [low] `crates/daw-patterns/src/kit.rs:136`. `sampler_root` hides a catalog failure behind `.ok()` even though its caller already returns `Result`. Effort S.
- [low] `crates/daw-project/src/service/sync.rs:146-151, 169-174, 234-246`. A failed note-effect or instrument removal is not remembered, so it is re-sent and re-logged on every edit, contrary to the doc. Effort S.
- [low] `crates/daw-contracts/src/history.rs:22`. `undo`/`redo` return `bool`, which merges "nothing to undo" with `GROUP_OPEN`. → Return `Result<bool, _>`. Effort S.
- [low] Quiet failures in daw-mcp and daw-rpc:
  - `crates/daw-mcp/src/http.rs:113`: a missing `Content-Length` gets "202 Accepted" while the request is dropped.
  - `lib.rs:108`: a message with an `id` but no `method` gets no reply.
  - `http.rs:30`: a bad port is ignored.
  - `crates/daw-rpc/src/lib.rs:112, 147`: `to_value(..).unwrap_or(Null)`.

  Effort S.

**Frontend**

**Status (frontend):** done on branch `claude/hopeful-roentgen-558581` (2026-10-15), one commit; checked with `npx tsc --noEmit` and `npm run lint` only (no frontend tests exist; no build or run, by the owner's rule).

- **Done.** [high] `app/src/state/notifications.ts:39`. `showBackendError` only shows the error. Failures that are not backend errors (IPC, a throw in `.then`, a reconcile during re-read) are never logged. → `console.error` anything for which `!isBackendError(error)`. Effort S.
- **Done.** [high] `app/src/actions/soundBrowser.ts:18-25` and `ui/sounds/soundPlaces.ts:33, 72`. The `quiet` path turns any failure into a `console.warn`, which is not captured, and `reload()` uses it after every import. It exists for "old projects without `samples/`" (R10). → Delete the fallback and the `quiet` flag; use `ctx.run`. Effort S.
- **Done.** [medium] `app/src/state/appearance.ts:41, 50`. `console.warn` never reaches errors.log, because the reporter patches only `console.error`. Effort S.
- **Done.** [low] `app/src/actions/project.ts:51-53` and `state/projectSync.ts:74`. Open-project warnings are shown on two paths with different logging. Effort S. *Done as:* one path, `ProjectActions.showOpenWarnings` (after open, after restore, once at startup), which only shows them: the backend logs each warning (the backend item `daw-project/src/service/mod.rs:194-204` above).
- **Done.** [low] `app/src/mock/handlers/midi.ts:14-23`. `try { validateTarget } catch { return false }` uses an exception as a predicate. Effort S.

## Batch 3: Contract docs and ARCHITECTURE.md out of date (R1, R7)

**Contract text that disagrees with the code**
- [high] Five docs name `ErrorKind::Invalid` while the code returns another kind:
  - `daw-contracts/src/presets.rs:39, 57, 60` (PR-071, PT-07x are NotFound);
  - `patterns.rs:131`;
  - `record.rs:98` (AU-343 is State);
  - `analysis.rs:103` (EX-351 is State).

  → Name the exact codes and add `.is(code)` tests. Effort S.
- [high] `daw-contracts/src/effect.rs:102-104`. `PluginScanner::installed` says "built-in first", but it returns only VST3 entries. Effort S.
- [medium] Contract docs that disagree with their implementation:
  - `export.rs:88`: "stems = each track", but the code writes one file per bus.
  - `midi.rs:54-55`: "CC moves are not written to automation", but they are reported to the writer.
  - `sample.rs:125`: `remove` "deletes the clips", but clips that have other takes are kept.
  - `project_io.rs:39-40`: snapshot tempo "is the transport's", but it comes from the tempo map.
  - `clips.rs:128-129`: "CURRENT tempo", but the map at the clip position is used.
  - `engine.rs:169`: meters "bus and master only", but tracks have meters too.

  Effort S.
- [medium] Contracts with no list of their errors:
  - `Exporter::export`, `MidiFileIo`, `MidiInputControl::connect`, `ProjectIo`;
  - `SampleCatalog::import`, `AutosaveControl::restore`, `DspSink::send`, `AudioCapture::start`;
  - `RecordControl::set_armed`/`status` (also missing: the song-loop hold and the synchronous finish);
  - `ProjectStorage::save` (it copies sounds; its failure policy is not stated).

  Effort M.
- [medium] `daw-project/src/catalog.rs:15, 66-68` and `sample_editor.rs:15-16`. `MAX_SAMPLES`, `MAX_MARKERS` and `MAX_BUCKETS` are private limits that are not in the contract. `MAX_MARKERS` also shadows `daw_contracts::MAX_MARKERS`, which means something else. → Make them contract constants. Effort S.
- [low] `daw-contracts/src/lib.rs:1`, `model.rs:1` and ARCHITECTURE:12 say "only types and traits", but the crate also holds the shared pure rules. → Reword. The `TrackChain` doc is also stale.

**ARCHITECTURE.md and CLAUDE.md**
- [medium] CLAUDE.md:40, 45 and ARCHITECTURE:174 cite `daw-bootstrap/tests/architecture.rs`; the file is `tests/all/architecture.rs`. :174 also omits daw-mcp.
- [medium] Crates table:
  - no row for `daw-rpc` or `daw-recorder`;
  - `ProjectNoteEffects` is missing;
  - the bootstrap row says `wiring/drums.rs` (now `parts.rs`) and omits `arrangement.rs`;
  - the daw-app row's dependencies are wrong.
- [medium] Stale how-tos:
  - "How to add a new operation" (:537-543) points to `daw-project/src/mixer.rs` and `mixer.renameTrack`, which don't exist;
  - :70 describes `cargo run -p daw-app` with `--empty`, which no longer applies.
- [low] Test paths miss `all/` at :87, 121, 231, 262, 371 and in the render_*/presets_templates references. Section links at :22, 24 and 26 point to old headings.
- [low] Section by section:
  - :86 quotes old `TAKE_CLIP_NOT_CREATED` text;
  - :121 puts the automation overrides in `compile.rs` (they are in `mixer/mod.rs`), and the daw-recorder layout omits `controllers.rs`;
  - :133 mentions `insertSlot` and `moveIndex` (the first moved, the second doesn't exist);
  - :99 leaves `chords` out of `api/types`;
  - :101 Actions row is incomplete;
  - :113 mock description is incomplete;
  - :141 doesn't say that language is cached;
  - :143 mentions a ☀/☾ button that doesn't exist;
  - :360 PDC claim (see Batch 1);
  - :382 "InputMonitor implements EngineLink" is backwards;
  - "Audio export" says stems add up to the mix, which is false.
- [low] Missing sections: "Built-in sampler" and "Built-in drums". The `gain` unit differs between instruments without a note.
- [low] Stale file headers:
  - daw-vst3: `src/lib.rs:5, 36`, `context.rs:6`, `Cargo.toml:24`, `tests/all/main.rs:2`, `tests/all/editors.rs:2`;
  - daw-audio-cpal: `src/lib.rs:4`;
  - daw-bootstrap: `wiring/mod.rs:1`;
  - daw-project: `lib.rs:10-13`;
  - frontend: `state/poller.ts:2`, `state/session.tsx:2`, the `ui/tracks/timelineLayout.ts` header and :174, `ui/common/trackColors.ts` ("row" vs index), `domain/clips.ts:8` (orphan comment);
  - comments: `daw-project/src/catalog.rs:48-54` (the "tests" comment is really production behaviour), `daw-rpc/src/log.rs:27`.
- [low] Missing "why" comments (obsolete since 2026-10-23: plain comments were removed; a "why" goes into a doc comment or a `CLAUDE.md`, CLAUDE.md rule 7):
  - the voicing numbers in `daw-strings/src/engine.rs:255` and the drum recipes in `daw-drums/src/voice.rs:108-131`;
  - `daw-vst3/src/runner.rs:41, 76`;
  - the key-ownership mechanism in `ui/samples/SampleEditorView.tsx:57-69`;
  - why `BackendError` lives in `i18n/errors`.
- [low] Comments that tell history instead of "why" (the plain ones are gone since 2026-10-23; doc comments still apply):
  - `daw-render/src/encode/pcm.rs:8`, test names "like before" / "old rounding", `stems.rs:136`;
  - `daw-rpc/tests/all/methods.rs:180`, `project.rs:175`;
  - `app/src-tauri/src/main.rs:132`;
  - ARCHITECTURE "the old rounding" and "Without the new fields…".

## Batch 4: Shared small things, written once (duplication)

Everything here lands in `daw-contracts` helpers (or the existing frontend domain module), and the copies are deleted.

- [medium] `in_group(history, label, f)`: begin_group, then end_group on Ok or cancel_group on Err. It is hand-written 9 times:
  - daw-project: `bounce.rs:78-88, 164-175`, `sample_editor.rs:95-102`, `clip_processing.rs:99-113`;
  - daw-persistence presets;
  - daw-patterns: `chord_part.rs:162`, `part.rs:138`;
  - daw-midi file;
  - daw-demo: `lib.rs:31`.
- [medium] Locks:
  - `sync::read`/`sync::write` for `RwLock`: `daw-catalog/src/groups.rs:66, 71` and `daw-vst3/src/directory.rs:19-28`.
  - Hand-rolled `sync::lock`: `app/src-tauri/src/main.rs:44, 99, 101, 138, 139`, `plugin_window.rs:53`, `daw-persistence/src/log.rs:69, 81`, `daw-midi/src/input.rs:93`.
- [medium] Ticks↔frames is hand-rolled in 8 places:
  - `daw-contracts/src/warp.rs:67` and `takes.rs:103`;
  - `daw-note-effects/src/arpeggiator.rs:250`;
  - `daw-patterns/src/render.rs:36-42`, which rounds differently, so a preview lands one frame off playback;
  - `daw-patterns/src/strum.rs:62`;
  - `daw-transport/src/lib.rs:202` (approximate inside a ramp);
  - `daw-engine/src/audio_voices.rs:181` and `engine.rs:302`.

  → Use `Bpm::ticks_per_frame`, `BlockTiming::frame_at` and a new `BlockTiming::tick_at`, or `TempoMap::frames_between`.
- [medium] `fit_param(table, name, value)` exists in 6 copies with 3 behaviours: daw-effects `lib.rs:52`, daw-note-effects `lib.rs:31`, daw-synth `lib.rs:215`, daw-strings `lib.rs:154`, daw-sampler `lib.rs:152`, daw-drums `voice.rs:157`.
- [medium] Instrument defaults are written twice, and the copies can drift:
  - `daw-drums/src/voice.rs:38` doesn't apply its `PARAMS` table at all;
  - synth and strings `Settings::default`, and arpeggiator/chord `new()`, repeat the table values.

  → Defaults live only in the `PARAMS` table.
- [medium] The drum kit is defined 4 times: `daw-drums/src/lib.rs:85`, `daw-patterns/src/kit.rs:17`, the `library.rs:84` test and `daw-demo/src/lib.rs:43` (the demo also has literal effect kinds and parameter names). → One kit table in contracts.
- [medium] `Graph::heard_upstream` (regular edges only) is hand-rolled in `daw-mixcheck/src/rules.rs:260` and `daw-summary/src/lib.rs:176`.
- [medium] `Graph::chain` and `Graph::insert_path` disagree. The console (`daw-app/src/commands/tracks.rs:29`) shows a different effect path from the mixer. → Keep `insert_path` only.
- [medium] `ContractError::coded()` ("CODE: message") is formatted by hand in 7 places in daw-mcp, daw-app, daw-rpc and src-tauri.
- [medium] `PollThread` (named poll loop, logged spawn failure, stop and join) and the lazy group flag are duplicated: `daw-recorder/src/lib.rs:279-308, 322-331` and `daw-automation-writer/src/lib.rs:356-392`.
- [medium] `daw-engine/src/controllers.rs:23-31` repeats `notes.rs:102-112` `Placement` and its pass math. → One `ClipRepeat`.
- [low] Small helpers written more than once:
  - `Track::is_frozen()`: 4 places;
  - `TempoMap::bar_number()`: 2;
  - `song_range` plus the `u32` clamp of `song_end`: 3 (export, analysis, summary);
  - project name = file stem: 4 (daw-rpc `project.rs:76-83`, daw-project `files.rs:47`, `service/mod.rs:163`, daw-persistence `templates.rs:119`);
  - `$HOME`: 3, with different fallbacks.
- [low] Small duplicates inside daw-project:
  - the edge predicate is spelled 5 ways (`ops.rs:96-98, 106`, `graph.rs:53-55, 117, 131`);
  - tick dedup in `automation.rs:83-86` and `note_rules.rs:99-102`;
  - `notes.rs:48-56` and `notes.rs:147-160`;
  - `checked_name` in markers and folders.
- [low] Smaller duplicates in other crates:
  - `daw-midi/src/file/write.rs:41-88` has two unroll loops that differ silently;
  - the `"vst3:"` prefix is written twice;
  - `scanner::PluginKind` duplicates `PluginType`;
  - `daw-vst3/src/journal.rs:9, 13` builds entries by hand instead of using `LogEntry::warn/error`;
  - `daw-engine/src/automation.rs:13` shadows `MAX_LANES`, and `MAX_INSTRUMENTS`/`MAX_CHAINS` are not tied to `MAX_TRACKS`.
- [low] Instruments: synth and strings hand-roll `AudioBlock::add_to_frame`, and the sampler has a mono branch the contract rules out.

## Batch 5: Audio thread and engine (decision 1: rework, no locks)

- [high] `daw-audio-cpal/src/live.rs:64` and `backend.rs:21`: `try_lock` in the input and output callbacks (decision 1).
- [medium] `daw-engine/src/link/mod.rs:55`. `EngineLink` has seven jobs. → Split it into `SnapshotLink`, `DspInbox` and `LiveLink`, and let `Telemetry`, `MonitorTable` and `HeldLanes` implement their own traits. Effort M.
- [medium] `daw-engine/src/engine.rs:210-222`. The conductor maintains `NoteSchedule`'s `active` list through pass-through methods. → `collect` updates the list itself. Effort S.
- [medium] `daw-audio-cpal/src/host/mod.rs:32`. `Shared` has three jobs, and its comment "only the host thread changes it" is false. → Move the capture end into its own entity. Effort M.
- [low] `daw-engine/src/note_effects.rs:149-179`. The rack reads `SegmentNotes` fields directly. → Give `SegmentNotes` its own module. `InstrumentRack::note`/`controller` return a `bool` nobody reads (`instruments.rs:91, 104`).

## Batch 6: Shared DSP blocks (decision 2: new `daw-dsp` crate)

- [medium] Smoothing is written 3 times: daw-effects `smooth.rs:15`, daw-strings `lib.rs:127, 171-173` and daw-synth `Ramp`. `SMOOTH_SECONDS` is copied, and nothing explains why effects use 10 ms and instruments 20 ms.
- [medium] Envelope: the sampler (`lib.rs:304`) has its own attack/release, separate from the synth AHDSR (`engine.rs:216`), and `release` means a different thing in each.
- [medium] Voice allocation is the same code in the sampler (`lib.rs:223`) and the synth (`engine.rs:177`).
- [medium] Biquad: the strings `Resonator` (`engine.rs:176`) re-implements the effects biquad. Only the copy flushes denormals.
- [medium] Delay line: chorus, delay, reverb comb/allpass and the strings voice each hand-roll a ring buffer.
- [medium] Pitch math: `midi_to_hz` appears twice and `2^(s/12)` four times. `BEND_RANGE_SEMITONES` is defined twice.
- [medium] Noise: xorshift32 appears 3 times (drums, strings, arpeggiator); two copies share the seed.
- [medium] Tests: one shared instrument contract check (factory, create, prepare, render adds, unknown or clamped params, `all_notes_off`) for drums, strings, synth and sampler.

## Batch 7: Contract splits by role (decision 3: all, one per step)

- [medium] `GraphControl` (`graph_control.rs:5`, 18 methods): split into routing, `MixerControl` (volume, pan, mute, solo) and `EffectControl` (param, bypass). Effort L.
- [medium] `TransportControl` (`transport.rs:21`): split into playback, `PlaybackBounds` (`set_loop` and `set_song_end`, for the core only) and `SongLoopHold` (for the recorders). Anyone holding `Controls.transport` can overwrite project-owned bounds today. Effort M.
- [medium] `ClipControl` loop region (`clips.rs:104, 115, 169`): move to its own `LoopRegionControl`. Effort M.
- [medium] `TrackControl` (`tracks.rs:43`): move `set_monitor` to the recording side, and colour and height to a `TrackAppearance` contract. Effort M.
- [medium] `AudioDevices` (`audio.rs:113`): split off an `InputMeter` (`watch_input`, `input_level`). Effort M.
- [medium] `FolderControl` (`folders.rs:44`): it says "only a view" but mutes and solos the sources. → Move group mute/solo to the mixer. Effort M.
- [medium] The disabled-plugins list has two writers: `PreferencesStore::set` and `PluginScanner::set_disabled`. → One `PluginBlocklist` owner. This also removes the frontend workaround (`actions/plugins.ts:31-34`). Effort M.
- [medium] `Presets` and `ProjectTemplates`: split each into a file store (in daw-persistence) and an applier (next to the other edit jobs). Today `PresetFolder::apply` is a project edit living in the storage crate. Effort M.
- [medium] `ProjectStorage::save` also copies sounds, with a second collision rule (`daw-persistence/src/storage.rs:50` vs `assets.rs:44`). → It only writes the file; sounds are copied through `AssetStore::adopt`. Effort M.
- [low] Smaller moves:
  - `NoteControl::vary_to_new_clip` → `ClipControl`;
  - `Controls.storage` out of `Controls` (only autosave uses it);
  - `Chord`/`ChordQuality`, `LaneHits::from_grid` and `Project::song_loop_end` out of contracts into their only user.

## Batch 8: Role splits inside implementations

- [medium] **daw-render**: `OfflineExporter` has three jobs: the engine loader, `Exporter` and `TrackRender`.
  - Extract an `OfflineRenderer` core with one `play` loop; the loop is written 3 times today.
  - Build the exporter, `TrackRenderer` and `LevelMeter` on it.
  - Put "cut the project to one target" (`isolate`, `bus_only`, `stem_graph`) in one `reduce.rs`.

  Effort M.
- [medium] **daw-audio-recorder**:
  - `Inner` has two jobs on one 15-field `Deps`: capture and placing the take. → Split into `Capture` and `TakePlacer`.
  - The take-ending sequence is copied at 4 exits (`lib.rs:247-305`). → One `end_take` or guard.
  - `place` and `place_passes` share their align logic.

  Effort M.
- [medium] **daw-project**:
  - The core also loads files and decodes samples (`service/mod.rs:174-204`). → Move that to `ProjectFiles`.
  - `ops.rs` has four jobs. → Split into `graph_rules.rs` and `target_rules.rs`, and move the clip rules into `clip_rules.rs`.
  - `edit` hands every entity `&mut Project`: folders write graph strips, and bounce writes `frozen`. → Ask the owners instead.
  - `squash` republishes to the engine only to bump the revision.
  - `SoundFacts` is a one-field wrapper.

  Effort M.
- [medium] **daw-vst3**:
  - `Vst3Effect` and `Vst3Instrument` share a core that has already drifted. → One composed `PluginVoice`; `BlockFaults::start` moves into it.
  - `Vst3Editors` reaches into `Editor`'s lock and public fields. → Give `Editor` the methods and make its fields private.
  - Unsafe calls are spread through scanner, editor and params logic. → Add safe `Bundle::classes()`/`vendor()` and a view wrapper.
  - 14 SAFETY comments don't justify their block ("a plain call without pointers", "same.").
  - Handle `restartComponent(kLatencyChanged)`: re-read the latency on the main thread and re-prepare the plugin (carried over from Batch 1).
  - `Vst3Scanner` mixes finding bundles with reading one bundle.
  - The test-only `Option` preferences and marker paths, with `PL-563`, are never reached in production.
  - `Vst3Plugins` keeps the plugin list twice.

  Effort M.
- [medium] **daw-patterns**: `chord_part.rs` holds both the plan and the builder. → Split it, like drums (`plan.rs`/`part.rs`), move the shared placement rules to `placement.rs`, and rename the two `plan` functions. Effort S.
- [medium] **daw-persistence**: `FORMAT_VERSION`/`Envelope`/`PJ-570` has been version 1 since the first commit, so it distinguishes nothing (R10). → Drop it. `autosave.rs:137-142` creates a folder that `write_atomic` already creates, and it reads `ProjectAutosave`'s private field. Effort S.
- [low] Smaller items:
  - `EffectKind::new(config)`: the parameter is never used;
  - `DemoBeat` has public fields; give it a `new()`;
  - `daw-mixcheck/src/rules.rs:41`: `finding()` only forwards;
  - `daw-recorder` `poll_once` and daw-automation-writer `poll_at` are public only for tests;
  - `RpcTable::methods()` is test-only and duplicates `describe()`.

## Batch 9: Logic out of the front doors and the wiring crate (R3, R4)

- [high] `daw-rpc/src/telemetry.rs:34, 63-69`. The "heard position" (adjusted for plugin latency) is computed here, and the engine computes it with different math (`engine.rs:298-300`). → One function in contracts. Effort S.
- [medium] `daw-rpc/src/project.rs:41-51`: `without_plugin_states` walks model internals in the front door. → Make it a `Project` method. Effort S.
- [medium] `daw-rpc/src/project.rs:76-83, 143-145`. The recent-project name and `$HOME` (which silently becomes `"/"`) are decided in the front door. → Move both behind contracts, with codes. Effort S.
- [medium] `daw-rpc/src/patterns.rs:21-64`. The handler filters patterns and builds the crate's only hand-made DTOs. → Add a filter to `PatternLibrary`, use `#[serde(default)]` on `Filter`, and drop the `anyOf` branch in daw-mcp. Effort S/M.
- [medium] `app/src-tauri/src/main.rs:46-48` and `plugin_window.rs:57-69`. `pluginEditor.open` bypasses `RpcTable` and re-does param parsing and logging by hand. → Register it on the root's table. Effort S.
- [medium] daw-bootstrap runs behaviour, not just wiring:
  - `clip_audio_watch.rs` → daw-project, which makes `refresh_clip_audio` private;
  - `engine_watch.rs` → daw-engine;
  - `Startup::open` → `ProjectFiles`.

  Effort M.
- [medium] `daw-bootstrap/tests/all/architecture.rs:26, 105`. The guard can be bypassed: `[target.'cfg(..)'.dependencies]` and `[build-dependencies]` are skipped, and the lint check ignores which table `workspace = true` is under. Effort S.
- [low] Defaults decided in the front door: `daw-rpc/src/log.rs:31` (`limit.min(1000)`) and `notes.rs:84-86` (`full_strength`). → Contract constants. The stdio loop in `daw-app/src/mcp_main.rs:88-94` should become `daw_mcp::serve_stdio`.

## Batch 10: Frontend actions and state

**Status:** done on branch `claude/hopeful-roentgen-558581` (2026-10-15), one commit, except `actions/plugins.ts`; checked with `npx tsc --noEmit` and `npm run lint` only (no frontend tests exist; no build or run, by the owner's rule).

- **Done.** [medium] `actions/context.ts:11-54`. `ActionContext` carries about 20 handles across 6 roles, and every factory gets all of them. → Split it into role interfaces assembled in `state/actionContext.ts`. Effort M. *Done as:* seven role interfaces in `actions/context.ts`; each factory names the roles it uses.
- **Done.** [medium] `state/session.tsx:65-98`. `Session = ReturnType<typeof createSession>` is inferred from the implementation, not designed first. → Declare an interface grouped by role, move the preferences copy to `state/preferences.ts`, and install the touch-release listener the way `installGlobalKeys` is installed. Effort M.
- **Done.** [medium] "Refresh, then select" is copied 11 times (`clips.ts:78`, `tracks.ts:44`, `newTracks.ts:18`, `bounce.ts:23`, `chords.ts:27`, `drums.ts:27`, `audioRecord.ts:40`, `soundPlacement.ts:49, 58, 82, 95`). → One `showCreated` intent. Effort S.
- **Done.** [medium] Action interfaces with two jobs:
  - `TransportActions` holds undo/redo → `HistoryActions`;
  - `setRecordQuantize` moves next to note recording; *Done as:* `NoteRecordActions` (arming and the quantize), which `AudioRecordActions.pressRecord` asks.
  - the loop region moves from clips to transport;
  - `ProjectActions` holds export → `ExportActions`; *Done as:* `AudioExportActions`.
  - `MidiActions` → input and learn.

  Effort S.
- **Done.** [medium] `actions/midi.ts:38-45`. Components decide when the `mappings` copy is fresh. → Derive it from `project.midiMappings`. Effort M. *Done as:* `MidiLearnActions.mappings()` reads the project copy (`domain/automation.ts: targetExists`); the learn state is re-read by the action after `learn`/`cancelLearn` and on project changes while waiting.
- **Skipped.** [medium] `actions/plugins.ts:31-34`. It writes the preferences copy itself, bypassing its owner. → Call `preferences.load()` (or fix it in Batch 7). Effort S. *Skipped:* Batch 7, step 3 (one disabled-plugins owner) replaces it; only the factory's context type was narrowed.
- **Done.** [medium] Duplicated flows:
  - the presets and templates save flow (`presets.ts:24-40`, `templates.ts:22-37`);
  - the device-watch poll lifecycle (`ui/settings/AudioSection.tsx:54-75`, `ui/mixer/MasterVisualizer.tsx:77-93`).

  → `createWatchedPoll` in `state/poller.ts`. Effort S.
- **Done.** [medium] The UI does an actions job:
  - `ui/pianoroll/PianoRoll.tsx:93-98` and `ControllerLaneView.tsx:52-55` re-read the whole project after note edits. → `notes.*` use `ctx.patchProject`, and `refresh` is deleted.
  - `ui/automation/PatternEditor.tsx:17` keeps a module-global point clipboard. → `AutomationActions` copy/paste.
  - `ui/graph/GraphView.tsx:57-63` and `ui/mixer/StripRouting.tsx:25-30` both decide "insert after, or before the master". → `graph.insertEffectAt`.

  Effort S.
- **Done.** [low] Smaller items:
  - the startup sequence in `App.tsx:36-42` → an action;
  - `uiPrefs` has two jobs, and its name clashes with app `Preferences`;
  - `ParamTouch` → `actions/paramTouch.ts`;
  - dead `AudioRecordActions.status`; `start` and `add` should be local functions;
  - the `NewTrack` builders → domain, and the `ROOT_NOTE` alias goes;
  - `importOne` duplicates `importQuietly`.

## Batch 11: Frontend UI

**Status:** done on branch `claude/hopeful-roentgen-558581` (2026-10-15), one commit; checked with `npx tsc --noEmit` and `npm run lint` only (no frontend tests exist; no build or run, by the owner's rule). The canvas changes were not seen in the running app.

- **Done.** [medium] Business rules living in UI:
  - clip rules in `ui/tracks/timelineLayout.ts:89-114` (`minClipLength`, `trimStartAt`, `resizePlacement`, `draggedGain`; `repeatTicks` duplicates `autoRepeatTicks`) → `domain/clips.ts`;
  - the free span in `lanePointer.ts:104-116` and the fade edge in `timelinePointer.ts:252` → domain;
  - the format rules in `ExportDialog.tsx:50-125` → `domain/exportAudio.ts`;
  - the folder M/S rule in `FolderHeader.tsx` → `domain/trackRows.ts`.

  Effort M.
- **Done.** [medium] Duplicated drawing and geometry:
  - clip rectangle and frame (`laneRender.ts`, `timelineRender.ts`, `timelineDrop.ts`) → `spanRect` plus one clip-frame helper;
  - the pencil stroke (`patternPointer.ts`, `controllerPointer.ts`) → `domain/controllers.ts`;
  - the loop playhead (`PatternEditor.tsx:49-53`, `clipView.ts:13-17`) → `domain/noteClip.ts`, deleting `clipView.ts`;
  - the bars stepper (`AutomationClipPanel`, `ClipPanel`);
  - the grid loop (`render.ts`, `controllerRender.ts`);
  - the selection box ×3; the in-window test ×4;
  - `tickAtX` ×5; *Done as:* `timelineLayout.ts: tickAtX` for the timeline pointers; the piano roll's (keys offset), the take lanes' (share of the width) and the pattern editor's (snapped) are different conversions and stay.
  - track colour ×3, with disagreeing fallbacks;
  - `InputMenu` re-implements `ContextMenu`, `Divider` copies `ToolbarDivider`, and `TrackHeader` skips `menuPoint`;
  - `clipWaves` is created twice (`TakeLanes.tsx`);
  - the audition toggle in drum and chord previews, `MAX_REPEAT` ×2, the bar/repeat parsing ×3;
  - `kindOptions` ×5, `instrumentTracks` ×3;
  - the `graphLayout` `hasInput`/`hasOutput` aliases.

  Effort S each.
- **Done.** [low] Dead code, leftovers and i18n gaps:
  - dead code: `patternRender`/`patternPointer` null points, the unused `window` and old `Lane*` names; `IconMenu` and `IconBell`;
  - `RULER_HEIGHT` alias and `LaneView` are leftovers of the tempo lane;
  - the `PatternEditor` keys need a table like `NOTE_KEYS`;
  - hard-coded English `aria-label`s: `PianoRoll.tsx:163`, `MuteSolo.tsx:11, 15`, `NodeCard.tsx:78`;
  - a third, undocumented key-ownership mechanism in `SampleEditorView`. *Done as:* named `claimSpace` in `state/globalKeys.ts`, whose header lists all three ways (also in ARCHITECTURE).

## Batch 12: Frontend mock, domain, api

- [medium] `mock/world.ts:21-78`. `MockWorld` holds about 8 jobs as public mutable state, written by 14 modules. → Split it into owners: transport, note recorder, samples, file system, preferences. Effort L.
- [medium] The mock runs its own history pipeline by hand (`take.ts:61-66`, `handlers/automation.ts`, `demo.ts`). drums and chords build several undo steps. → `beginGroup`/`endGroup` on the history owner. Effort M.
- [medium] `mock/MockRpc.ts:102-125` runs the transport clock and builds telemetry. → Move the clock to the transport owner and the tick to `mock/telemetry.ts`. Effort M.
- [medium] The mock disagrees with the backend:
  - `handlers/audioRecord.ts:63-92`: start and stop don't use the transport the way the backend does;
  - `handlers/audio.ts:43-46`: a `recordLatencyMs` limit that the backend doesn't have.
  - the mock's `project.export` doesn't check the tail; the backend refuses it with EX-250 (`check_tail`, added in Batch 1).

  Effort S.
- [medium] Rules written twice, once in domain and once in mock:
  - `trackPlacementProblem` / `checkTrack`;
  - `checkEdge` ×2;
  - preset names ×2, plus a third copy of the messages in i18n.

  → Domain returns a problem kind; the UI maps it to text and the mock to a code. Effort M.
- [medium] `domain/noteTransforms.ts`, `groove.ts` and `variation.ts` are backend copies that only the mock uses, with two dead string throws. → Move them to `mock/noteRules.ts`. Effort M.
- [medium] `api/api.ts:360`. `DawApi` is inferred, and mock handlers take `params: any`. → One typed `RpcMethods` map for both. Effort L.
- [medium] Contract constants defined twice: `MAX_TICK` ×3, `MAX_CLIP_LOOP_TICKS`, `MAX_CC_NUMBER`, max clip gain, the BPM range, max level, and `ScaleChoice` vs `ScaleKey`. Effort S.
- [medium] Duplicated mock code: "register a sample" ×4, and the drum/chord part placement ×2. Effort S.
- [medium] `domain/automation.ts` holds 4 jobs. → Split it. Effort M.
- [low] Smaller items:
  - dead mock handlers (about 10);
  - handlers importing rules from other handlers;
  - mock i18n spread over 3 sections;
  - three different `lanesOf`, and `take.ts` naming;
  - `api.ts` names `pendingAutosave` and `exportAudio`;
  - the `BackendError` location;
  - file filters inside the `dialogs.ts` contract;
  - path helpers ×5, and `format.ts` mixes paths with number labels;
  - `src/log/**` has no Biome layer override.

## Batch 13: Tests: layout, shared fakes, history-only tests (R10)

- [medium] `#[serde(default)]` is kept only for old files. Remove it now (decision 4):
  - `model.rs:74, 178, 300-312, 429, 468, 481, 492, 495`;
  - `graph.rs:84, 196`;
  - plus the tests asserting old JSON opens: `model.rs:610, 616-625`, `graph/tests.rs:176-181`, `export.rs:171`.
- [medium] Missing contract tests:
  - automation-writer errors (`Rig` drops its log);
  - `DspSink` queue full and the snapshot graveyard;
  - `StretchCache` job cancelled while queued;
  - `Previewer` (none at all);
  - MIDI import `cancel_group`, `FILE_HAS_NO_NOTES`, `BAD_FILE`;
  - `set_tempo_map`; a mapping without min/max; `samples::TOO_MANY`; `rename_node` on a source;
  - stems with sends;
  - contracts rule tests (`Bpm::new`, `TimeSignature::new`, `frame_at`, `bar_start`, `NewNote::validate`, `check_color`, `NoteBuffer::push`, `stereo_lerp`);
  - RPC wire tests for markers, folders, samples, sounds, preview, play/pause, metronome, track rename/remove/colour, graph solo/bypass and `automation.removeLane`.
- [medium] Copied test fakes:
  - `MemoryLog`: about 15 copies → a `testing` feature of daw-contracts;
  - `FakeTransport`, `FakeHistory`, `FakeTempo`: ×3 in the recorders;
  - full `Clip` literals: about 10 → `Clip::new` or a test builder.
- [medium] `daw-vst3/tests/all`: four `ONE_AT_A_TIME` mutexes live in one test binary, so Surge tests run in parallel. → One `serial()`.
- [low] Tests outside `src/tests/<area>.rs`:
  - daw-engine: scope, params, preview, monitor, notes, controllers, held, `mixer/tests.rs`;
  - daw-audio-cpal: capture, live, choose, peaks, faults;
  - daw-stretch: one file mixes DSP tests and cache tests;
  - inline tests in daw-rpc, daw-mcp and daw-app;
  - daw-project: `take_rules`, `warp_rules`, `testing.rs`;
  - daw-vst3: `support` → `common`, `test_fakes`, the fakes inside `plugins.rs`;
  - daw-render, daw-mixcheck, daw-summary, daw-persistence;
  - daw-synth's duplicated helpers, and the sampler tests have no header.
- [low] RPC tests that only record history:
  - `daw-rpc/tests/all/methods.rs:159-163`, `project.rs:21-23`, `live.rs:53-57`;
  - in daw-bootstrap tests, `session.rs:15 temp()` duplicates `common::temp_dir`, and a `dead_code` allow is stale.

## Batch 14: Small cleanups

- [low] Rust:
  - `thiserror` is unused in daw-contracts;
  - `pub fn fill`, `ProcessJob::with_curve` and `SampleData::stereo_at` should be private;
  - SM-112 has the wrong kind; PT-071 should merge into PT-070.
- [low] Labels: the undo labels "Mute", "Solo" and "Bypass" are not translated (`daw-project/src/graph.rs:190, 197, 215`).
- [low] Language and leftovers:
  - Russian comments in `Cargo.toml:57` and `daw-contracts/src/chord_part.rs:30`;
  - `test-plugins/distortion/src/lib.rs:1` has no role header;
  - `daw-vst3/src/keys/piano.rs` mirrors `domain/keyboard.ts`, and nothing guards the two.
