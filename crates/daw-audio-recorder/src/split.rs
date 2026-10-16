//! Pure take rules: which input channels a track gets, which clip the recording covered (and becomes a take of), and
//! how to align the take audio to that clip start.

use daw_contracts::{Clip, ClipContent, InputChannels, TrackId};

/// A track audio from the shared capture (`channels` interleaved channels): its own channels, mono or stereo.
pub(crate) fn track_sound(data: &[f32], channels: u16, input: InputChannels) -> (Vec<f32>, u16) {
    let width = usize::from(channels.max(1));
    let out_channels = input.file_channels(channels);
    let frames = data.chunks_exact(width);
    let sound = match (out_channels, input) {
        (2, _) => frames.flat_map(|frame| [frame[0], frame[1]]).collect(),
        (_, InputChannels::Right) if width > 1 => frames.map(|frame| frame[1]).collect(),
        _ => frames.map(|frame| frame[0]).collect(),
    };
    (sound, out_channels)
}

/// The track clip that the recording `[start, end)` (ticks) covers at least by half, or that contains it entirely (a new
/// clip would not fit there anyway); of several, the most covered. Audio clips only.
pub(crate) fn covered_clip(clips: &[Clip], track: TrackId, start: u64, end: u64) -> Option<&Clip> {
    clips
        .iter()
        .filter(|clip| clip.track == track && matches!(clip.content, ClipContent::Audio { .. }))
        .map(|clip| (clip, end.min(clip.end_tick()).saturating_sub(start.max(u64::from(clip.start_tick)))))
        .filter(|(clip, covered)| *covered > 0 && (covered * 2 >= u64::from(clip.length_ticks) || *covered >= end.saturating_sub(start)))
        .max_by_key(|(_, covered)| *covered)
        .map(|(clip, _)| clip)
}

/// Shifts the audio so that its frame `offset_frames` lands on the clip start (all takes of a clip share this offset):
/// `shift > 0` pads silence in front, `shift < 0` cuts the start. Returns whether any audio is left.
pub(crate) fn align(sound: &mut Vec<f32>, channels: u16, shift: i64) -> bool {
    let width = usize::from(channels.max(1));
    let frames = usize::try_from(shift.unsigned_abs()).unwrap_or(usize::MAX);
    if shift > 0 {
        sound.splice(0..0, std::iter::repeat_n(0.0, frames.saturating_mul(width)));
    } else {
        let cut = frames.saturating_mul(width).min(sound.len());
        sound.drain(..cut);
    }
    !sound.is_empty()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn audio(id: u32, start: u32, length: u32) -> Clip {
        Clip {
            id,
            track: 1,
            start_tick: start,
            length_ticks: length,
            content: ClipContent::audio(1),
            gain: 1.0,
            fade_in_ticks: 0,
            fade_out_ticks: 0,
            fade_in_shape: Default::default(),
            fade_out_shape: Default::default(),
            offset_ticks: 0,
            offset_frames: 0,
            warp: None,
            pitch_semitones: 0.0,
        }
    }

    #[test]
    fn tracks_take_their_channels_from_the_shared_capture() {
        let stereo = [1.0, 2.0, 3.0, 4.0];
        assert_eq!(track_sound(&stereo, 2, InputChannels::Stereo), (stereo.to_vec(), 2));
        assert_eq!(track_sound(&stereo, 2, InputChannels::Left), (vec![1.0, 3.0], 1));
        assert_eq!(track_sound(&stereo, 2, InputChannels::Right), (vec![2.0, 4.0], 1));
        assert_eq!(track_sound(&[5.0, 6.0], 1, InputChannels::Right), (vec![5.0, 6.0], 1), "mono input: the only channel");
        assert_eq!(track_sound(&[5.0, 6.0], 1, InputChannels::Stereo), (vec![5.0, 6.0], 1));
    }

    #[test]
    fn the_take_goes_to_the_clip_covered_at_least_half_and_most() {
        let clips = [audio(1, 0, 1000), audio(2, 1000, 1000)];
        assert_eq!(covered_clip(&clips, 1, 0, 1000).map(|c| c.id), Some(1));
        assert_eq!(covered_clip(&clips, 1, 400, 1700).map(|c| c.id), Some(2), "covered more");
        assert_eq!(covered_clip(&clips, 1, 800, 1300), None, "none is covered by half");
        assert_eq!(covered_clip(&clips, 1, 100, 300).map(|c| c.id), Some(1), "the recording is entirely inside the clip");
        assert_eq!(covered_clip(&clips, 2, 0, 1000), None, "another track");
        assert_eq!(covered_clip(&clips, 1, 2000, 3000), None, "adjacent is not covered");
    }

    #[test]
    fn alignment_pads_or_cuts_whole_frames() {
        let mut sound = vec![1.0, 2.0, 3.0, 4.0];
        assert!(align(&mut sound, 2, 1));
        assert_eq!(sound, [0.0, 0.0, 1.0, 2.0, 3.0, 4.0]);
        assert!(align(&mut sound, 2, -2));
        assert_eq!(sound, [3.0, 4.0]);
        assert!(!align(&mut sound, 2, -5), "everything was cut");
    }
}
