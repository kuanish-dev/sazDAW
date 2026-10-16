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
