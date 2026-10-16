//! Laying out a part: which pattern starts at which tick and for how many repeats. Shared by building and previewing a part.
//!
//! The part starts at bar `start_bar` of the tempo map (bars differ in length across signature changes). A pattern's
//! length is its own 4/4 bars (`BAR_TICKS`): the table's beats are written as 16 steps per bar and are not reshaped for
//! another signature, so in 3/4 a four-beat pattern simply lasts longer than a bar of the song.

use daw_contracts::codes::patterns;
use daw_contracts::{BAR_TICKS, Clip, ClipId, ContractError, DrumPartRequest, DrumPattern, PatternLibrary, TempoMap, TrackId};

/// How many steps and repeats make sense in one part (guards against a typo like `repeat: 100000`).
const MAX_SLOTS: usize = 64;
pub(crate) const MAX_REPEAT: u32 = 64;

/// A pattern placed at a specific position.
pub(crate) struct Placement {
    pub(crate) pattern: DrumPattern,
    pub(crate) repeat: u32,
    pub(crate) start_tick: u32,
}

impl Placement {
    pub(crate) fn length_ticks(&self) -> u32 {
        self.pattern.bars * self.repeat * BAR_TICKS
    }
}

/// The track's clips that overlap `[start, end)`: a part placed there either refuses or replaces them.
pub(crate) fn overlapping(clips: &[Clip], track: TrackId, start: u32, end: u32) -> Vec<ClipId> {
    clips.iter().filter(|c| c.track == track && c.overlaps(u64::from(start), u64::from(end))).map(|c| c.id).collect()
}

/// The start tick of bar `bar` (from zero) on the map; a bar too far away is a clear refusal.
pub(crate) fn bar_start_tick(map: &TempoMap, bar: u32) -> Result<u32, ContractError> {
    u32::try_from(map.bar_tick(bar)).map_err(|_| patterns::BAR_TOO_FAR.with("bar", bar))
}

/// Lays out the request: the first pattern at bar `start_bar` of the map, each next one right after the previous.
pub(crate) fn plan(library: &dyn PatternLibrary, request: &DrumPartRequest, map: &TempoMap) -> Result<Vec<Placement>, ContractError> {
    if request.slots.is_empty() {
        return Err(patterns::NO_SLOTS.err());
    }
    if request.slots.len() > MAX_SLOTS {
        return Err(patterns::TOO_MANY_SLOTS.with("max", MAX_SLOTS).with("count", request.slots.len()));
    }
    let mut tick = bar_start_tick(map, request.start_bar)?;
    let mut placements = Vec::with_capacity(request.slots.len());
    for slot in &request.slots {
        if !(1..=MAX_REPEAT).contains(&slot.repeat) {
            return Err(patterns::REPEAT_OUT_OF_RANGE.with("pattern", &slot.pattern).with("max", MAX_REPEAT).with("repeat", slot.repeat));
        }
        let placement = Placement { pattern: library.pattern(&slot.pattern)?, repeat: slot.repeat, start_tick: tick };
        tick = tick.checked_add(placement.length_ticks()).ok_or_else(|| patterns::PART_TOO_LONG.err())?;
        placements.push(placement);
    }
    Ok(placements)
}

/// The part's drums in order of first appearance.
pub(crate) fn lanes_of(placements: &[Placement]) -> Vec<String> {
    let mut lanes: Vec<String> = Vec::new();
    for lane in placements.iter().flat_map(|p| p.pattern.lanes.iter().map(|l| &l.lane)) {
        if !lanes.contains(lane) {
            lanes.push(lane.clone());
        }
    }
    lanes
}

#[cfg(test)]
mod tests {
    use daw_contracts::{Bpm, PatternSlot, SignatureChange, TimeSignature};

    use super::*;
    use crate::PatternBank;

    fn request(start_bar: u32, slots: &[(&str, u32)]) -> DrumPartRequest {
        DrumPartRequest {
            start_bar,
            slots: slots.iter().map(|(pattern, repeat)| PatternSlot { pattern: (*pattern).into(), repeat: *repeat }).collect(),
            replace: false,
            lanes: Vec::new(),
        }
    }

    fn four_four() -> TempoMap {
        TempoMap::constant(Bpm::DEFAULT, TimeSignature::FOUR_FOUR)
    }

    #[test]
    fn the_start_bar_is_found_through_signature_changes() {
        // Two bars of 4/4, then 3/4: bar 3 (from zero) starts at 2 × 3840 + 2880.
        let map = four_four()
            .with_signatures(vec![
                SignatureChange { tick: 0, signature: TimeSignature::FOUR_FOUR },
                SignatureChange { tick: 2 * BAR_TICKS, signature: TimeSignature::new(3, 4).unwrap() },
            ])
            .unwrap();
        let bank = PatternBank::builtin(&crate::testing::NoLog);
        let placed = plan(&bank, &request(3, &[("rock_basic", 1)]), &map).unwrap();
        assert_eq!(placed[0].start_tick, 2 * BAR_TICKS + 2880);
        assert_eq!(placed[0].length_ticks(), BAR_TICKS, "the pattern keeps its own 4/4 bars");
    }

    #[test]
    fn patterns_follow_each_other_and_repeats_multiply_the_length() {
        let bank = PatternBank::builtin(&crate::testing::NoLog);
        let placed =
            plan(&bank, &request(2, &[("rock_basic", 3), ("intro_hat_kick_build", 1), ("fill_snare_roll", 1)]), &four_four()).unwrap();
        assert_eq!(placed.iter().map(|p| p.start_tick / BAR_TICKS).collect::<Vec<_>>(), [2, 5, 7]);
        assert_eq!(placed.iter().map(|p| p.length_ticks() / BAR_TICKS).collect::<Vec<_>>(), [3, 2, 1]);
    }

    #[test]
    fn bad_requests_are_rejected_with_readable_errors() {
        let bank = PatternBank::builtin(&crate::testing::NoLog);
        let error = |slots: &[(&str, u32)]| plan(&bank, &request(0, slots), &four_four()).err().expect("the request must fail");
        assert!(error(&[]).is(patterns::NO_SLOTS));
        assert!(error(&[("rock_basic", 0)]).is(patterns::REPEAT_OUT_OF_RANGE));
        assert!(error(&[("rock_basic", 1000)]).to_string().contains("rock_basic"));
        assert!(error(&[("nope", 1)]).to_string().contains("nope"));
    }

    #[test]
    fn lanes_are_listed_in_order_of_first_appearance_without_repeats() {
        let bank = PatternBank::builtin(&crate::testing::NoLog);
        let placed = plan(&bank, &request(0, &[("rock_basic", 1), ("fill_tom_cascade", 1)]), &four_four()).unwrap();
        assert_eq!(lanes_of(&placed), ["kick", "snare", "hat", "tom_hi", "tom_mid", "tom_lo"]);
    }

    #[test]
    fn a_neighbouring_clip_does_not_count_as_overlap() {
        let clip = |id, start, length| Clip {
            id,
            track: 1,
            start_tick: start,
            length_ticks: length,
            content: daw_contracts::ClipContent::audio(1),
            gain: 1.0,
            fade_in_ticks: 0,
            fade_out_ticks: 0,
            fade_in_shape: Default::default(),
            fade_out_shape: Default::default(),
            offset_ticks: 0,
            offset_frames: 0,
            warp: None,
            pitch_semitones: 0.0,
        };
        let clips = [clip(10, 0, BAR_TICKS), clip(11, BAR_TICKS, BAR_TICKS), clip(12, 3 * BAR_TICKS, BAR_TICKS)];
        assert_eq!(overlapping(&clips, 1, BAR_TICKS, 2 * BAR_TICKS), [11]);
        assert_eq!(overlapping(&clips, 1, 0, 4 * BAR_TICKS), [10, 11, 12]);
        assert!(overlapping(&clips, 2, 0, 4 * BAR_TICKS).is_empty(), "another track");
    }
}
