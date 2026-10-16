# `daw-audio-recorder`

`AudioRecorder` (`AudioRecordControl`): "record an audio take from the input onto armed audio tracks" — track arming and inputs (`arming.rs`, passed to the engine via `InputMonitor`), background collection of frames from `AudioCapture`, latency trimming, track channels and take alignment (`split.rs`), loop passes (`passes.rs`, see "Loop recording" ([docs/features/recording.md](../../docs/features/recording.md))), a WAV + sample + clip (or a new take of a covered clip) per track in a single undo step (`take.rs`; notes are recorded by `daw-recorder`).

Depends on: contracts.

Map of the whole code: [ARCHITECTURE.md](../../ARCHITECTURE.md). Cross-layer features it takes part in: [cue](../../docs/features/cue.md), [pdc](../../docs/features/pdc.md), [song-loop](../../docs/features/song-loop.md), [tempo-map](../../docs/features/tempo-map.md).
