//! `analysis.*` methods: "listen" to the project — sound levels (`MixAnalysis`) and the mix check (`MixCheck`) on a
//! project snapshot (rendering works on a copy, the project is not locked).

use daw_contracts::LevelRequest;
use schemars::JsonSchema;
use serde::Deserialize;

use crate::RpcTable;

#[derive(Default, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", default)]
struct Range {
    /// Start in ticks; 0 by default.
    from_tick: Option<u32>,
    /// End in ticks; the end of the last clip by default.
    to_tick: Option<u32>,
}

pub(crate) fn register(table: &mut RpcTable) {
    table.add_flushing(
        "analysis.levels",
        "Measures the project as numbers: an offline render (no files) with sound levels. `per`: `master` (the whole mix, default), \
         `track` (each track at its own output — instrument, fader, inserts; no mute/solo or buses) or `bus` (each bus output). \
         For each target: `level` { peakDb, rmsDb } over the whole range (dBFS; peak 0 is the ceiling, a sine's RMS is 3 dB \
         below its peak), `loudnessLufs` (BS.1770 integrated loudness; a finished track is usually −16…−8) and `sections` — levels \
         per section of `resolutionBars` bars (4 by default; section bounds are in the shared `sections` list with bar numbers); \
         `null` means silence. Range `fromTick`…`toTick` (the whole project by default, at most 10 minutes); each target is \
         rendered separately, so `per: track` on a long song takes longer.",
        |c, p: LevelRequest| c.analysis.levels(&c.view.snapshot(), &p),
    );
    table.add_flushing(
        "analysis.balance",
        "How loud each track is IN THE MIX, for setting the volume balance between tracks. A track's sound in the mix is the \
         whole mix minus the mix without its clips (offline renders, no files), measured at the master output after the \
         fader: it keeps what the other tracks do to it — a compressor keyed by the kick ducks it, a gate another track \
         opens lets it through, the master's compressor and limiter react to the whole mix — and it follows the track's \
         fader. Returns `mix` { loudnessLufs (BS.1770 integrated), maxShortTermLufs (the loudest 3 s), truePeakDb } for the \
         whole mix; `floorLufs`, how much two renders of the same mix differ (null: not at all; a plugin that sounds \
         different each time raises it, and a track within about 10 LU of it is not measured reliably); and `tracks`: \
         { track, name, node, volume, muted, keysOnly, silent, loudnessLufs, maxShortTermLufs, truePeakDb, relativeLu } \
         where relativeLu = the track's loudness in the mix minus the mix's (0 = as loud as the whole mix). Equal parts of \
         a mix of N tracks sit near −10·log10(N) LU (8 tracks ≈ −9); the lead part is louder than that, background parts \
         quieter. To make a track X dB louder, multiply its `volume` by 10^(X/20) (+6 dB ≈ ×2, −6 dB ≈ ×0.5) with \
         `graph.setVolume` on its `node` (the fader goes up to 1.5 ≈ +3.5 dB: past that, turn the others down), then \
         measure again: a compressor after the fader moves the result less than the fader. A track with `keysOnly` reaches \
         the master only by keying a sidechain: its numbers are what it opens or ducks, not a level to set with its fader. \
         Solo is ignored; a muted track is measured as if it played (`muted` tells it is muted now); a track that adds \
         nothing (no clips, volume 0, only a muted bus) is `silent` with null levels. Range `fromTick`…`toTick` as in \
         analysis.levels; one render of the whole mix per track with clips, plus two.",
        |c, p: Range| c.analysis.balance(&c.view.snapshot(), p.from_tick, p.to_tick),
    );
    table.add_flushing(
        "analysis.check",
        "Checks the mix before calling it done: returns findings { severity: error|warning|info, code, message, \
         track?, node?, lane?, ranges? (ticks) }, an empty list means all is well. Catches: tracks with clips that are silent or \
         barely audible (volume or instrument cutoff at zero, empty clips), master clipping (peak ≥ −0.1 dBFS) with bars, \
         a quiet or silent master, mute/solo, tracks and buses that don't reach the master, empty buses, volume/cutoff params \
         at minimum, automation out of range, disabled and missing plugins, bypassed effects. Listens bar by bar \
         (renders each track and the master); the range `fromTick`…`toTick` works as in analysis.levels.",
        |c, p: Range| Ok(c.mix_check.check(&c.view.snapshot(), p.from_tick, p.to_tick)),
    );
}
