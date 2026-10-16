//! Frequent data for the interface: position, meters, effect gain reduction and band gains, load, latency compensation,
//! the mono check.

use daw_contracts::{Controls, TempoMap, TransportState};
use serde_json::{Value, json};

/// What to send to the interface in one poll.
pub struct Poll {
    pub tick: Value,
    /// The new project revision, if it changed since the last poll.
    pub project_revision: Option<u64>,
}

#[derive(Default)]
pub struct Telemetry {
    last_revision: Option<u64>,
}

impl Telemetry {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn poll(&mut self, controls: &Controls) -> Poll {
        let revision = controls.view.revision();
        let changed = self.last_revision != Some(revision);
        self.last_revision = Some(revision);

        let meters: Vec<Value> =
            controls.stats.meters().into_iter().map(|m| json!({ "node": m.node, "left": m.peak_left, "right": m.peak_right })).collect();
        let reductions: Vec<Value> = controls.stats.gain_reductions().into_iter().map(|r| json!({ "node": r.node, "db": r.db })).collect();
        let bands: Vec<Value> = controls.stats.band_gains().into_iter().map(|b| json!({ "node": b.node, "db": b.db })).collect();
        let state = controls.transport.state();
        let map = controls.tempo.tempo_map();
        let latency = controls.stats.latency().seconds();
        let position = heard_position(controls.transport.position_ticks(), state == TransportState::Playing, latency, &map);
        // Tempo and signature at the heard position: the transport panel shows what sounds now.
        let at = position as f64;
        let tick = json!({
            "state": state,
            "positionTicks": position,
            "cue": controls.transport.cue(),
            "tempo": map.tempo_at(at).get(),
            "signature": map.signature_at(at).signature,
            // What playback wraps around now (the loop region or the song): the interface cursor stops there until the
            // next tick brings the jump.
            "wrap": controls.transport.wrap_region(),
            "songLoop": controls.transport.song_loop(),
            "metronome": controls.metronome.is_enabled(),
            // The mono check is a monitoring mode of this run, like the song loop: shown wherever the interface is.
            "mono": controls.mono.is_mono(),
            "automationWrite": controls.automation_writer.mode(),
            "cpu": controls.stats.cpu_load(),
            "latencyMs": latency * 1000.0,
            "preview": controls.preview.position(),
            "record": controls.record.status(),
            "meters": meters,
            "reductions": reductions,
            "bands": bands,
        });
        Poll { tick, project_revision: changed.then_some(revision) }
    }
}

/// The position heard now: with plugin latency compensation the master lags behind the transport by `latency`
/// seconds, and the interface cursor must keep pace with the sound. A stopped transport is shown as is.
/// Seconds are converted to ticks via the tempo map: across a tempo change the lag in ticks changes.
fn heard_position(position: u64, playing: bool, latency: f64, map: &TempoMap) -> u64 {
    if !playing || latency <= 0.0 {
        return position;
    }
    let heard = map.tick_at_seconds(map.seconds_at(position as f64) - latency);
    heard.round().max(0.0) as u64
}

#[cfg(test)]
mod tests {
    use daw_contracts::{Bpm, SignatureChange, TempoChange, TempoMap, TimeSignature};

    use super::heard_position;

    #[test]
    fn the_heard_position_lags_by_the_latency_only_while_playing() {
        // 120 BPM: 1920 ticks per second, 50 ms is 96 ticks.
        let map = TempoMap::constant(Bpm::new(120.0).unwrap(), TimeSignature::FOUR_FOUR);
        assert_eq!(heard_position(1_000, true, 0.05, &map), 904);
        assert_eq!(heard_position(1_000, false, 0.05, &map), 1_000);
        assert_eq!(heard_position(50, true, 0.05, &map), 0, "does not go negative");
        assert_eq!(heard_position(1_000, true, 0.0, &map), 1_000);
    }

    #[test]
    fn the_lag_follows_the_tempo_map() {
        // Up to tick 960 it is 120 BPM, then 60 BPM (960 ticks per second): 50 ms after the change is 48 ticks.
        let changes = vec![
            TempoChange { tick: 0, bpm: Bpm::new(120.0).unwrap(), ramp: false },
            TempoChange { tick: 960, bpm: Bpm::new(60.0).unwrap(), ramp: false },
        ];
        let map = TempoMap::new(changes, vec![SignatureChange { tick: 0, signature: TimeSignature::FOUR_FOUR }]).unwrap();
        assert_eq!(heard_position(2_000, true, 0.05, &map), 1_952);
        // 25 ms at 60 BPM (24 ticks) and 25 ms at 120 BPM (48 ticks).
        assert_eq!(heard_position(984, true, 0.05, &map), 912);
    }
}
