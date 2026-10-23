//! Loop passes of a take: while a loop region is set, the transport jumps from the loop end back to its start, but the
//! input keeps arriving as one straight run. This pure rule cuts that run where the jumps were, so `take.rs` can lay
//! every pass over the bars it was played to (each pass becomes a take of one clip on the loop region).

use std::ops::Range;

use daw_contracts::TempoMap;

/// A stretch of the take played without a jump.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Pass {
    /// Its frames in the take (after the latency cut).
    pub frames: Range<usize>,
    /// The tick its first frame belongs to: the take start for the first pass, the loop start for the others.
    pub start_tick: u32,
    /// It played to the loop end; the last pass of a take stopped mid-loop did not.
    pub reached_end: bool,
}

/// Cuts a take of `frames` frames at `rate`, started at `start_tick`, into the passes the transport played around
/// `loop_region` `[start, end)`. Playback that starts at or past the loop end never jumps back: the whole take is one
/// pass. Boundaries are counted in exact (fractional) frames from the take start, so many passes do not drift.
pub(crate) fn passes(frames: usize, start_tick: u32, loop_region: Option<(u32, u32)>, map: &TempoMap, rate: u32) -> Vec<Pass> {
    let whole = vec![Pass { frames: 0..frames, start_tick, reached_end: false }];
    let Some((loop_start, loop_end)) = loop_region.filter(|&(start, end)| start < end && start_tick < end) else {
        return whole;
    };
    let loop_frames = map.frames_between(f64::from(loop_start), f64::from(loop_end), rate);
    if loop_frames < 1.0 {
        return whole;
    }
    let mut result = Vec::new();
    let mut boundary = map.frames_between(f64::from(start_tick), f64::from(loop_end), rate);
    let (mut from, mut tick) = (0, start_tick);
    while from < frames {
        let end = boundary.round() as usize;
        let to = end.min(frames);
        if to > from {
            result.push(Pass { frames: from..to, start_tick: tick, reached_end: end <= frames });
        }
        (from, tick) = (to, loop_start);
        boundary += loop_frames;
    }
    result
}

#[cfg(test)]
mod tests {
    use daw_contracts::{Bpm, TempoChange, TimeSignature};

    use super::*;

    const RATE: u32 = 48_000;

    /// 120 BPM at 48 kHz: 25 frames per tick, a beat (960 ticks) is 24,000 frames.
    fn map() -> TempoMap {
        TempoMap::constant(Bpm::new(120.0).unwrap(), TimeSignature::FOUR_FOUR)
    }

    fn pass(frames: Range<usize>, start_tick: u32, reached_end: bool) -> Pass {
        Pass { frames, start_tick, reached_end }
    }

    #[test]
    fn without_a_loop_or_past_its_end_the_take_is_one_pass() {
        assert_eq!(passes(60_000, 960, None, &map(), RATE), [pass(0..60_000, 960, false)]);
        assert_eq!(passes(60_000, 3_840, Some((0, 1_920)), &map(), RATE), [pass(0..60_000, 3_840, false)], "started past the loop end");
        assert_eq!(passes(10_000, 960, Some((960, 1_920)), &map(), RATE), [pass(0..10_000, 960, false)], "stopped before the first jump");
    }

    #[test]
    fn every_jump_starts_a_pass_at_the_loop_start_and_a_stop_mid_loop_cuts_the_last_one_short() {
        let cut = passes(60_000, 960, Some((960, 1_920)), &map(), RATE);
        assert_eq!(cut, [pass(0..24_000, 960, true), pass(24_000..48_000, 960, true), pass(48_000..60_000, 960, false)]);
        let exact = passes(48_000, 960, Some((960, 1_920)), &map(), RATE);
        assert_eq!(exact, [pass(0..24_000, 960, true), pass(24_000..48_000, 960, true)], "a stop right on the jump leaves no empty pass");
    }

    #[test]
    fn the_first_pass_runs_from_the_take_start_to_the_loop_end() {
        let pre_roll = passes(72_000, 0, Some((960, 1_920)), &map(), RATE);
        assert_eq!(pre_roll, [pass(0..48_000, 0, true), pass(48_000..72_000, 960, true)]);
        let mid = passes(36_000, 1_440, Some((960, 1_920)), &map(), RATE);
        assert_eq!(mid, [pass(0..12_000, 1_440, true), pass(12_000..36_000, 960, true)]);
    }

    #[test]
    fn passes_follow_the_tempo_map_and_do_not_drift() {
        let slow = TempoMap::constant(Bpm::new(100.0).unwrap(), TimeSignature::FOUR_FOUR);
        let many = passes(3_000, 0, Some((0, 1)), &slow, RATE);
        assert_eq!(many.len(), 100);
        assert!(many.iter().all(|p| p.frames.len() == 30 && p.reached_end));
        let changing = TempoMap::constant(Bpm::new(120.0).unwrap(), TimeSignature::FOUR_FOUR)
            .with_tempos(vec![
                TempoChange { tick: 0, bpm: Bpm::new(120.0).unwrap(), ramp: false },
                TempoChange { tick: 960, bpm: Bpm::new(60.0).unwrap(), ramp: false },
            ])
            .unwrap();
        let cut = passes(80_000, 0, Some((0, 1_920)), &changing, RATE);
        assert_eq!(cut, [pass(0..72_000, 0, true), pass(72_000..80_000, 0, false)]);
    }
}
