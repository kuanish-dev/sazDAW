//! Clip rules as pure functions over `Project`: bounds, the place on a track (overlaps and crossfades), applying an
//! edit, copying. Single and batch operations of `ProjectClips` share them: a batch is checked on its final state.

use daw_contracts::codes::clips;
use daw_contracts::{
    Clip, ClipContent, ClipEdit, ClipId, ContractError, MAX_CLIP_GAIN, MAX_CLIP_TICK, MAX_CLIPS, Project, TempoMap, TrackId,
};

use crate::warp_rules::{self, SoundFacts};
use crate::{ops, take_rules};

pub(crate) fn check_range(start: u32, length: u32) -> Result<(), ContractError> {
    if length == 0 || start > MAX_CLIP_TICK || length > MAX_CLIP_TICK {
        return Err(clips::RANGE_OUT_OF_BOUNDS.with("start", start).with("length", length).with("max", MAX_CLIP_TICK));
    }
    Ok(())
}

fn copy_too_far(tick: impl std::fmt::Display) -> ContractError {
    clips::COPY_TOO_FAR.with("tick", tick).with("max", MAX_CLIP_TICK)
}

/// The place on a track, on the final state. Note clips do not overlap; audio clips may overlap (crossfade), but no
/// more than two at one point and none inside another (see `ClipControl`).
fn check_track(project: &Project, track: TrackId) -> Result<(), ContractError> {
    let audio = ops::track(project, track)?.is_audio();
    let mut spans: Vec<(u64, u64)> =
        project.clips.iter().filter(|c| c.track == track).map(|c| (u64::from(c.start_tick), c.end_tick())).collect();
    spans.sort_unstable();
    for (i, &(start, end)) in spans.iter().enumerate() {
        let Some(&(next_start, next_end)) = spans.get(i + 1) else { break };
        if next_start >= end {
            continue;
        }
        if !audio {
            return Err(clips::NOTE_CLIPS_OVERLAP.with("track", track));
        }
        if next_start == start || next_end <= end {
            return Err(clips::AUDIO_CLIP_INSIDE_ANOTHER.with("track", track));
        }
        // Ends grow with starts (nothing is nested), so a third clip at one point is the one after next.
        if spans.get(i + 2).is_some_and(|&(third, _)| third < end) {
            return Err(clips::TOO_MANY_AUDIO_CLIPS_AT_ONCE.with("track", track));
        }
    }
    Ok(())
}

/// Checks the place of clips `ids`, already in the project, on their tracks (on the final state).
pub(crate) fn check_places(project: &Project, ids: &[ClipId]) -> Result<(), ContractError> {
    let mut tracks = Vec::new();
    for &id in ids {
        let track = ops::clip(project, id)?.track;
        if !tracks.contains(&track) {
            tracks.push(track);
        }
    }
    tracks.into_iter().try_for_each(|track| check_track(project, track))
}

/// Applies an edit to the clip in place, without checking its place (the caller does that on the final state).
/// Tempo fitting and pitch are `warp_rules::apply`; an explicit `length_ticks` overrides the length it computed.
pub(crate) fn apply_edit(project: &mut Project, id: ClipId, edit: &ClipEdit, facts: &SoundFacts) -> Result<(), ContractError> {
    let mut current = ops::clip(project, id)?.clone();
    let sound_length = warp_rules::apply(project, &mut current, edit, facts)?;
    let track = edit.track.unwrap_or(current.track);
    let start = edit.start_tick.unwrap_or(current.start_tick);
    let length = edit.length_ticks.unwrap_or(sound_length);
    ops::check_clip_fits_track(ops::track(project, track)?, &current.content)?;
    check_range(start, length)?;
    let gain = ops::finite(edit.gain.unwrap_or(current.gain), clips::GAIN_NOT_A_NUMBER)?;
    if !(0.0..=MAX_CLIP_GAIN).contains(&gain) {
        return Err(clips::GAIN_OUT_OF_RANGE.with("gain", gain).with("max", MAX_CLIP_GAIN));
    }
    let clip = ops::clip_mut(project, id)?;
    clip.track = track;
    clip.start_tick = start;
    clip.length_ticks = length;
    take_rules::fit_comp(&mut clip.content, length);
    clip.gain = gain;
    clip.warp = current.warp;
    clip.pitch_semitones = current.pitch_semitones;
    clip.fade_in_ticks = edit.fade_in_ticks.unwrap_or(current.fade_in_ticks).min(length);
    clip.fade_out_ticks = edit.fade_out_ticks.unwrap_or(current.fade_out_ticks).min(length);
    clip.fade_in_shape = edit.fade_in_shape.unwrap_or(current.fade_in_shape);
    clip.fade_out_shape = edit.fade_out_shape.unwrap_or(current.fade_out_shape);
    Ok(())
}

/// The clip with its left edge at `start` and the same end, the content staying in place (`ClipControl::trim_start`).
/// `sample_rate` is the rate of an audio clip's sound (unused for notes). The place is checked by the caller.
pub(crate) fn trim_start(clip: &Clip, start: u32, map: &TempoMap, sample_rate: u32) -> Result<Clip, ContractError> {
    let end = clip.end_tick();
    if u64::from(start) >= end {
        return Err(clips::TRIM_PAST_END.with("tick", start).with("id", clip.id).with("end", end));
    }
    let mut trimmed = clip.clone();
    let mut start = start;
    match &clip.content {
        ClipContent::Notes(content) => {
            let shift = i64::from(start) - i64::from(clip.start_tick);
            let phase = (i64::from(clip.offset_ticks) + shift).rem_euclid(i64::from(content.loop_ticks.max(1)));
            trimmed.offset_ticks = u32::try_from(phase).unwrap_or(0);
        }
        ClipContent::Audio { .. } => {
            let offset = clip.offset_frames as f64;
            let frames_from =
                |tick: u32| offset + clip.ticks_to_source_frames(f64::from(tick) - f64::from(clip.start_tick), map, sample_rate);
            if frames_from(start) < 0.0 {
                // Nothing before the file start: the edge stops where the offset reaches zero.
                let file_start = f64::from(clip.start_tick) + clip.source_frames_to_ticks(-offset, map, sample_rate);
                start = file_start.ceil().max(0.0) as u32;
            }
            trimmed.offset_frames = frames_from(start).max(0.0).round() as u64;
        }
    }
    let length = u32::try_from(end - u64::from(start)).unwrap_or(u32::MAX);
    check_range(start, length)?;
    trimmed.start_tick = start;
    trimmed.length_ticks = length;
    take_rules::shift_comp(&mut trimmed.content, i64::from(start) - i64::from(clip.start_tick));
    take_rules::fit_comp(&mut trimmed.content, length);
    trimmed.fade_in_ticks = trimmed.fade_in_ticks.min(length);
    trimmed.fade_out_ticks = trimmed.fade_out_ticks.min(length);
    Ok(trimmed)
}

/// A copy of the clip right after its end (`ClipControl::duplicate_clip`; "variation to a new clip" uses it too).
pub(crate) fn copy_after(project: &mut Project, id: ClipId) -> Result<Clip, ContractError> {
    let end = ops::clip(project, id)?.end_tick();
    let at = u32::try_from(end).map_err(|_| copy_too_far(end))?;
    let mut copies = copy_group(project, &[id], at)?;
    copies.pop().ok_or_else(|| clips::COPY_NOT_CREATED.err())
}

/// Copies of clips `ids` on the same tracks: the leftmost goes to `at_tick`, the others keep their distance to it.
pub(crate) fn copy_group(project: &mut Project, ids: &[ClipId], at_tick: u32) -> Result<Vec<Clip>, ContractError> {
    if ids.is_empty() {
        return Err(clips::NOTHING_TO_COPY.err());
    }
    if project.clips.len() + ids.len() > MAX_CLIPS {
        return Err(clips::TOO_MANY.with("max", MAX_CLIPS));
    }
    let sources = ids.iter().map(|&id| ops::clip(project, id).cloned()).collect::<Result<Vec<_>, _>>()?;
    let left = sources.iter().map(|c| c.start_tick).min().unwrap_or(0);
    let mut copies = Vec::with_capacity(sources.len());
    for mut copy in sources {
        let start = u64::from(at_tick) + u64::from(copy.start_tick - left);
        copy.start_tick = u32::try_from(start).map_err(|_| copy_too_far(at_tick))?;
        check_range(copy.start_tick, copy.length_ticks)?;
        copy.id = project.alloc_id();
        copies.push(copy);
    }
    project.clips.extend(copies.iter().cloned());
    let new_ids: Vec<ClipId> = copies.iter().map(|c| c.id).collect();
    check_places(project, &new_ids)?;
    Ok(copies)
}
