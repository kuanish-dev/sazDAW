//! Rendering a track to audio: offline sound of one track (`TrackRender`) and replacing a track with its sound
//! (`TrackBounce`).

use crate::{ContractError, Project, SampleData, TrackId};

/// The sound of one track and the tick it starts at.
#[derive(Debug, Clone, PartialEq)]
pub struct TrackSound {
    pub start_tick: u32,
    pub data: SampleData,
}

/// The sound of one track apart from the rest. Job: offline render of one track.
///
/// Contract:
/// * only the track sounds, the way sound reaches its output: instrument (or audio clips) → the track's volume and
///   pan (with automation; in this graph the source fader sits BEFORE the inserts, so without it inserts such as a
///   compressor would sound different) → inserts, the effects on its path (`Graph::insert_path`, as in the mixer) up to
///   the output. The track's mute/solo, its output connection, sends, buses and the master have no effect; sidechain
///   inputs of the inserts are silent;
/// * the range runs from the start of the track's first clip to the end of its last one plus `tail_seconds`
///   (`check_tail`: `0..=MAX_TAIL_SECONDS`);
/// * a frozen instrument is silent, as in the engine (`Track::sounding_instrument`);
/// * errors: no such track is `tracks::NOT_FOUND`; a track without clips is `export::TRACK_HAS_NO_CLIPS`; a tail out of
///   range is `export::TAIL_OUT_OF_RANGE`; an instrument or effect that failed to create gives the catalog's
///   error.
pub trait TrackRender: Send + Sync {
    fn render_track(&self, project: &Project, track: TrackId, tail_seconds: f32) -> Result<TrackSound, ContractError>;
}

/// Replacing a track with its sound. Job: render a track into an audio clip ("Bounce to audio", "Freeze").
///
/// Contract for `bounce` and `freeze`:
/// * the track's sound (`TrackRender`) is written as a WAV into the project's sounds folder and becomes a sample; right
///   below the track a new audio track appears with one clip at the position of the first clip; the copy's volume is
///   1 and pan 0 (they are already in the sound), its output goes where the track's chain goes, at the same level
///   (otherwise to the master); sends are not copied;
/// * all of this is **one undo step**; on error the project does not change (the written WAV stays in `samples/`);
/// * the result is the id of the new audio track;
/// * errors: a project without a file (nowhere to put the WAV) is `ErrorKind::State`; a frozen track is
///   `ErrorKind::Invalid`; plus the `TrackRender` errors.
///
/// `bounce` mutes the source track (mutes its node); the track with its notes stays. `freeze` works only on an
/// instrument track (an audio track is `ErrorKind::Invalid`): the instrument is marked frozen (`InstrumentSpec::frozen`
/// = id of the copy), the engine does not create it (less load), and the copy sounds. `unfreeze` removes the copy
/// (with its sample if it plays nowhere else) and clears the mark in one undo step; a track that is not frozen is
/// `ErrorKind::Invalid`; if the copy was already deleted by hand, only the mark is cleared.
pub trait TrackBounce: Send + Sync {
    fn bounce(&self, track: TrackId, tail_seconds: f32) -> Result<TrackId, ContractError>;
    fn freeze(&self, track: TrackId, tail_seconds: f32) -> Result<TrackId, ContractError>;
    fn unfreeze(&self, track: TrackId) -> Result<(), ContractError>;
}
