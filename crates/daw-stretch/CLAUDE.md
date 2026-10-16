# `daw-stretch`

**Time stretching and pitch** for audio clips (see "Fitting audio clips to tempo" ([docs/features/warping.md](../../docs/features/warping.md))): `process` — WSOLA + resampling (pure DSP), `StretchCache` (`ProcessedAudio`: ready processed sounds and a background computation thread).

Depends on: contracts.

Map of the whole code: [ARCHITECTURE.md](../../ARCHITECTURE.md). Cross-layer features it takes part in: [warping](../../docs/features/warping.md).
