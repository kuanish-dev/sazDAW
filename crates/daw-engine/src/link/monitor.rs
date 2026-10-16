//! The track input table for monitoring: the control side (the recorder) writes which tracks are armed and which
//! channels they listen to, the audio thread reads it; back, the audio thread reports how many tracks listen and the
//! input rate. Atomics only: one track entry is one `u64`, so half an entry can never be read.

use std::sync::atomic::{AtomicU32, AtomicU64, Ordering};

use daw_contracts::{InputChannels, MAX_TRACKS, TrackId, TrackInput};

/// An empty slot (no track has this id: ids start at 1).
const EMPTY: u64 = 0;

pub struct MonitorTable {
    slots: Vec<AtomicU64>,
    listening: AtomicU32,
    input_rate: AtomicU32,
}

/// A slot: the track id in the high 32 bits, the "armed" flag and the channels in the low ones.
fn pack(input: TrackInput) -> u64 {
    let channels = match input.channels {
        InputChannels::Stereo => 0,
        InputChannels::Left => 1,
        InputChannels::Right => 2,
    };
    u64::from(input.track) << 32 | u64::from(input.armed) << 2 | channels
}

fn unpack(bits: u64) -> TrackInput {
    let channels = match bits & 0b11 {
        1 => InputChannels::Left,
        2 => InputChannels::Right,
        _ => InputChannels::Stereo,
    };
    TrackInput { track: (bits >> 32) as TrackId, armed: bits & 0b100 != 0, channels }
}

impl MonitorTable {
    pub fn new() -> Self {
        Self {
            slots: (0..MAX_TRACKS).map(|_| AtomicU64::new(EMPTY)).collect(),
            listening: AtomicU32::new(0),
            input_rate: AtomicU32::new(0),
        }
    }

    /// Control side: replaces the whole table; anything beyond the capacity is dropped.
    pub fn set(&self, inputs: &[TrackInput]) {
        let mut filled = inputs.iter().filter(|input| input.track != 0);
        for slot in &self.slots {
            slot.store(filled.next().map_or(EMPTY, |input| pack(*input)), Ordering::Relaxed);
        }
    }

    /// Audio thread: the track input; a track missing from the table is not armed and listens to `Stereo`.
    pub fn get(&self, track: TrackId) -> TrackInput {
        self.slots
            .iter()
            .map(|slot| slot.load(Ordering::Relaxed))
            .find(|&bits| bits != EMPTY && (bits >> 32) as TrackId == track)
            .map_or(TrackInput { track, armed: false, channels: InputChannels::Stereo }, unpack)
    }

    pub fn report(&self, listening: u32, input_rate: u32) {
        self.listening.store(listening, Ordering::Relaxed);
        self.input_rate.store(input_rate, Ordering::Relaxed);
    }

    pub fn listening(&self) -> u32 {
        self.listening.load(Ordering::Relaxed)
    }

    pub fn input_rate(&self) -> u32 {
        self.input_rate.load(Ordering::Relaxed)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn inputs_round_trip_and_unknown_tracks_are_disarmed_stereo() {
        let table = MonitorTable::new();
        table.set(&[
            TrackInput { track: 7, armed: true, channels: InputChannels::Right },
            TrackInput { track: 9, armed: false, channels: InputChannels::Left },
        ]);
        assert_eq!(table.get(7), TrackInput { track: 7, armed: true, channels: InputChannels::Right });
        assert_eq!(table.get(9).channels, InputChannels::Left);
        assert_eq!(table.get(3), TrackInput { track: 3, armed: false, channels: InputChannels::Stereo });
        table.set(&[]);
        assert!(!table.get(7).armed, "a new table replaces the old one whole");
    }
}
