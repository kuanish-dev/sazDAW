//! Rules of an audio clip's takes and their comp (comping) — pure functions over the clip content. Used by
//! `ProjectTakes`, clip edits and splits (`clip_rules.rs`, `clips.rs`: the comp follows the length) and sample removal
//! (`catalog.rs`): a sample that was one of the takes leaves the list and the comp, and the clip stays.

use daw_contracts::codes::clips;
use daw_contracts::{ClipContent, CompSegment, ContractError, MAX_COMP_SEGMENTS, SampleId};

/// The audio content of a clip by parts: the active take, the take list, the comp.
struct Audio<'a> {
    sample: &'a mut SampleId,
    takes: &'a mut Vec<SampleId>,
    comp: &'a mut Vec<CompSegment>,
}

impl Audio<'_> {
    fn has(&self, take: SampleId) -> bool {
        *self.sample == take || self.takes.contains(&take)
    }

    /// The comp in canonical form: neighbouring segments of one take are merged; when one take is left there is no
    /// comp, and that take becomes active.
    fn set_comp(&mut self, segments: Vec<CompSegment>) {
        let mut merged: Vec<CompSegment> = Vec::with_capacity(segments.len());
        for segment in segments {
            if merged.last().is_none_or(|last| last.take != segment.take) {
                merged.push(segment);
            }
        }
        if merged.len() == 1 {
            *self.sample = merged[0].take;
        }
        if merged.len() <= 1 {
            merged.clear();
        }
        *self.comp = merged;
    }

    /// The comp as a list of segments (without a comp — one segment of the active take).
    fn segments(&self) -> Vec<CompSegment> {
        if self.comp.is_empty() { vec![CompSegment { start_ticks: 0, take: *self.sample }] } else { self.comp.clone() }
    }
}

fn audio(content: &mut ClipContent) -> Result<Audio<'_>, ContractError> {
    match content {
        ClipContent::Audio { sample, takes, comp } => Ok(Audio { sample, takes, comp }),
        ClipContent::Notes(_) => Err(clips::NOTE_CLIP_HAS_NO_TAKES.err()),
    }
}

/// A new active take for the whole clip (the comp is dropped); the clip's previous sound becomes the first in the list
/// if there was no list yet.
pub fn add(content: &mut ClipContent, take: SampleId) -> Result<(), ContractError> {
    let audio = audio(content)?;
    if audio.has(take) {
        return Err(clips::TAKE_ALREADY_ADDED.with("take", take));
    }
    if audio.takes.is_empty() {
        audio.takes.push(*audio.sample);
    }
    audio.takes.push(take);
    *audio.sample = take;
    audio.comp.clear();
    Ok(())
}

/// Makes the take active for the whole clip and drops the comp (an already active take without a comp changes nothing).
pub fn activate(content: &mut ClipContent, take: SampleId) -> Result<(), ContractError> {
    let audio = audio(content)?;
    if !audio.has(take) {
        return Err(clips::NOT_A_TAKE.with("take", take));
    }
    *audio.sample = take;
    audio.comp.clear();
    Ok(())
}

/// Whether `take` is the clip's only sound (removing it means removing the clip).
pub fn is_only_sound(content: &ClipContent, take: SampleId) -> bool {
    match content {
        ClipContent::Audio { sample, takes, .. } => *sample == take && takes.iter().all(|t| *t == take),
        ClipContent::Notes(_) => false,
    }
}

/// Removes a take. If it was active, the clip switches to the last remaining one; its comp segments go to the
/// neighbours (the previous one, or the next one for the first segment); with one take left the list and comp empty.
pub fn remove(content: &mut ClipContent, take: SampleId) -> Result<(), ContractError> {
    if is_only_sound(content, take) {
        return Err(clips::ONLY_SOUND.with("take", take));
    }
    let mut audio = audio(content)?;
    if !audio.takes.contains(&take) {
        return Err(clips::NOT_A_TAKE.with("take", take));
    }
    audio.takes.retain(|t| *t != take);
    if *audio.sample == take
        && let Some(last) = audio.takes.last()
    {
        *audio.sample = *last;
    }
    let mut left: Vec<CompSegment> = audio.comp.iter().filter(|s| s.take != take).copied().collect();
    if let Some(first) = left.first_mut() {
        first.start_ticks = 0;
    }
    audio.set_comp(left);
    if audio.takes.len() == 1 {
        audio.takes.clear();
        audio.comp.clear();
    }
    Ok(())
}

/// The whole comp of a clip `length` ticks long (rules: `TakeControl::set_comp`).
pub fn set_comp(content: &mut ClipContent, length: u32, segments: Vec<CompSegment>) -> Result<(), ContractError> {
    let mut audio = audio(content)?;
    if segments.len() > MAX_COMP_SEGMENTS {
        return Err(clips::TOO_MANY_COMP_SEGMENTS.with("max", MAX_COMP_SEGMENTS));
    }
    if let Some(first) = segments.first().filter(|s| s.start_ticks != 0) {
        return Err(clips::COMP_NOT_FROM_START.with("tick", first.start_ticks));
    }
    if let Some(pair) = segments.windows(2).find(|pair| pair[1].start_ticks <= pair[0].start_ticks) {
        return Err(clips::COMP_UNORDERED.with("before", pair[0].start_ticks).with("tick", pair[1].start_ticks));
    }
    if let Some(late) = segments.iter().find(|s| s.start_ticks >= length) {
        return Err(clips::COMP_PAST_END.with("tick", late.start_ticks).with("length", length));
    }
    if let Some(foreign) = segments.iter().find(|s| !audio.has(s.take)) {
        return Err(clips::NOT_A_TAKE.with("take", foreign.take));
    }
    audio.set_comp(segments);
    Ok(())
}

/// The range `[from, to)` of a clip `length` ticks long plays take `take`; the rest stays as it was.
pub fn choose_range(content: &mut ClipContent, length: u32, from: u32, to: u32, take: SampleId) -> Result<(), ContractError> {
    let (from, to) = (from.min(length), to.min(length));
    if from >= to {
        return Err(clips::TAKE_RANGE_EMPTY.with("from", from).with("to", to));
    }
    let current = audio(content)?.segments();
    let take_at = |tick: u32| current.iter().rev().find(|s| s.start_ticks <= tick).map_or(take, |s| s.take);
    let mut segments: Vec<CompSegment> = current.iter().filter(|s| s.start_ticks < from).copied().collect();
    segments.push(CompSegment { start_ticks: from, take });
    if to < length {
        segments.push(CompSegment { start_ticks: to, take: take_at(to) });
    }
    segments.extend(current.iter().filter(|s| s.start_ticks > to).copied());
    set_comp(content, length, segments)
}

/// The comp after the clip length changes: segments beyond the new end go away.
pub fn fit_comp(content: &mut ClipContent, length: u32) {
    if let Ok(mut audio) = audio(content)
        && !audio.comp.is_empty()
    {
        let kept = audio.comp.iter().filter(|s| s.start_ticks < length).copied().collect();
        audio.set_comp(kept);
    }
}

/// Splits the clip at tick `at` (from its start): the left part `content` keeps the segments before the cut, and
/// `right` — the content of the right part, a copy of `content` — gets the rest, shifted to its start.
pub fn split_comp(content: &mut ClipContent, right: &mut ClipContent, at: u32) {
    shift_comp(right, i64::from(at));
    fit_comp(content, at);
}

/// The clip start moved by `shift` ticks while the sound stays in place on the timeline (`ClipControl::trim_start`):
/// a positive shift drops the beginning of the comp (the new first segment plays the take that was sounding there), a
/// negative one extends the first segment. The caller fits the comp to the new length (`fit_comp`).
pub fn shift_comp(content: &mut ClipContent, shift: i64) {
    let Ok(mut audio) = audio(content) else { return };
    if audio.comp.is_empty() {
        return;
    }
    let current = audio.comp.clone();
    let shifted = if shift >= 0 {
        let at = u32::try_from(shift).unwrap_or(u32::MAX);
        let first = current.iter().rev().find(|s| s.start_ticks <= at).map_or(*audio.sample, |s| s.take);
        let mut shifted = vec![CompSegment { start_ticks: 0, take: first }];
        shifted
            .extend(current.iter().filter(|s| s.start_ticks > at).map(|s| CompSegment { start_ticks: s.start_ticks - at, take: s.take }));
        shifted
    } else {
        let by = u32::try_from(shift.unsigned_abs()).unwrap_or(u32::MAX);
        current
            .iter()
            .enumerate()
            .map(|(i, s)| CompSegment { start_ticks: if i == 0 { 0 } else { s.start_ticks.saturating_add(by) }, take: s.take })
            .collect()
    };
    audio.set_comp(shifted);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn takes(content: &ClipContent) -> (SampleId, Vec<SampleId>) {
        match content {
            ClipContent::Audio { sample, takes, .. } => (*sample, takes.clone()),
            ClipContent::Notes(_) => unreachable!(),
        }
    }

    #[test]
    fn adding_keeps_the_old_sound_and_plays_the_new_one() {
        let mut content = ClipContent::audio(1);
        add(&mut content, 2).unwrap();
        add(&mut content, 3).unwrap();
        assert_eq!(takes(&content), (3, vec![1, 2, 3]));
        assert!(add(&mut content, 2).is_err(), "a take is not added twice");
    }

    #[test]
    fn activating_and_removing() {
        let mut content = ClipContent::audio(1);
        add(&mut content, 2).unwrap();
        activate(&mut content, 1).unwrap();
        assert_eq!(takes(&content), (1, vec![1, 2]));
        assert!(activate(&mut content, 9).is_err());
        remove(&mut content, 1).unwrap();
        assert_eq!(takes(&content), (2, vec![]), "the active take is removed: the remaining one plays, the list is empty");
        assert!(remove(&mut content, 2).is_err(), "the only sound cannot be removed");
        assert!(is_only_sound(&content, 2));
    }

    fn comp(content: &ClipContent) -> Vec<(u32, SampleId)> {
        match content {
            ClipContent::Audio { comp, .. } => comp.iter().map(|s| (s.start_ticks, s.take)).collect(),
            ClipContent::Notes(_) => unreachable!(),
        }
    }

    fn three_takes() -> ClipContent {
        let mut content = ClipContent::audio(1);
        add(&mut content, 2).unwrap();
        add(&mut content, 3).unwrap();
        content
    }

    fn seg(start_ticks: u32, take: SampleId) -> CompSegment {
        CompSegment { start_ticks, take }
    }

    #[test]
    fn choosing_ranges_builds_a_merged_comp() {
        let mut content = three_takes();
        choose_range(&mut content, 4000, 1000, 2000, 1).unwrap();
        assert_eq!(comp(&content), [(0, 3), (1000, 1), (2000, 3)], "without a comp the active take plays around the range");
        choose_range(&mut content, 4000, 1500, 9999, 2).unwrap();
        assert_eq!(comp(&content), [(0, 3), (1000, 1), (1500, 2)], "the end is clamped to the clip");
        choose_range(&mut content, 4000, 1000, 1500, 2).unwrap();
        assert_eq!(comp(&content), [(0, 3), (1000, 2)], "neighbours of one take merged");
        choose_range(&mut content, 4000, 0, 4000, 1).unwrap();
        assert_eq!((comp(&content), takes(&content).0), (vec![], 1), "one take for the whole clip: no comp, and it is active");
        assert!(choose_range(&mut content, 4000, 4000, 5000, 2).is_err(), "a range beyond the clip is empty");
        assert!(choose_range(&mut content, 4000, 0, 10, 9).is_err(), "a sound that is not a take");
    }

    #[test]
    fn comp_is_validated_whole() {
        let mut content = three_takes();
        assert!(set_comp(&mut content, 4000, vec![seg(10, 1)]).is_err(), "the first starts at zero");
        assert!(set_comp(&mut content, 4000, vec![seg(0, 1), seg(0, 2)]).is_err(), "increasing");
        assert!(set_comp(&mut content, 4000, vec![seg(0, 1), seg(4000, 2)]).is_err(), "inside the clip");
        assert!(set_comp(&mut content, 4000, vec![seg(0, 1), seg(100, 7)]).is_err(), "takes of the clip");
        set_comp(&mut content, 4000, vec![seg(0, 1), seg(100, 1), seg(200, 2)]).unwrap();
        assert_eq!(comp(&content), [(0, 1), (200, 2)]);
        set_comp(&mut content, 4000, vec![]).unwrap();
        assert_eq!(comp(&content), []);
    }

    #[test]
    fn comp_follows_removal_length_and_split() {
        let mut content = three_takes();
        set_comp(&mut content, 4000, vec![seg(0, 2), seg(1000, 1), seg(2000, 3)]).unwrap();
        let mut right = content.clone();
        split_comp(&mut content, &mut right, 1500);
        assert_eq!(comp(&content), [(0, 2), (1000, 1)]);
        assert_eq!(comp(&right), [(0, 1), (500, 3)], "the right part starts with the take at the cut");
        fit_comp(&mut content, 800);
        assert_eq!((comp(&content), takes(&content).0), (vec![], 2), "shorter: one take is left");

        let mut content = three_takes();
        set_comp(&mut content, 4000, vec![seg(0, 2), seg(1000, 1), seg(2000, 2)]).unwrap();
        remove(&mut content, 1).unwrap();
        assert_eq!(
            (comp(&content), takes(&content)),
            (vec![], (2, vec![2, 3])),
            "segments of the removed take went to the neighbours: the whole clip is take 2"
        );
        let mut content = three_takes();
        set_comp(&mut content, 4000, vec![seg(0, 1), seg(1000, 2)]).unwrap();
        remove(&mut content, 1).unwrap();
        assert_eq!(comp(&content), [], "the first segment went to the next take");
        activate(&mut content, 2).unwrap();
        set_comp(&mut content, 4000, vec![seg(0, 2), seg(1000, 3)]).unwrap();
        activate(&mut content, 3).unwrap();
        assert_eq!(comp(&content), [], "choosing a whole take drops the comp");
    }

    #[test]
    fn shifting_the_start_keeps_the_comp_in_place() {
        let comp = vec![CompSegment { start_ticks: 0, take: 1 }, CompSegment { start_ticks: 960, take: 2 }];
        let mut content = ClipContent::Audio { sample: 1, takes: vec![1, 2], comp: comp.clone() };
        shift_comp(&mut content, 480);
        assert_eq!(
            content.clone(),
            ClipContent::Audio {
                sample: 1,
                takes: vec![1, 2],
                comp: vec![CompSegment { start_ticks: 0, take: 1 }, CompSegment { start_ticks: 480, take: 2 },]
            }
        );
        let mut content = ClipContent::Audio { sample: 1, takes: vec![1, 2], comp: comp.clone() };
        shift_comp(&mut content, -240);
        assert_eq!(
            content,
            ClipContent::Audio {
                sample: 1,
                takes: vec![1, 2],
                comp: vec![CompSegment { start_ticks: 0, take: 1 }, CompSegment { start_ticks: 1200, take: 2 },]
            }
        );
        let mut content = ClipContent::Audio { sample: 1, takes: vec![1, 2], comp };
        shift_comp(&mut content, 1000);
        assert_eq!(content, ClipContent::Audio { sample: 2, takes: vec![1, 2], comp: vec![] }, "one take left: no comp");
    }
}
