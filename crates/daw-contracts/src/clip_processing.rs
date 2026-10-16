//! One-click processing of audio clips (`ClipProcessing`): normalize and reverse. The clip decides what changes
//! (gain, which sample it plays); measuring and writing the audio is the sample editor's job (`SampleEditor::peak`,
//! `SampleEditor::reverse_copy`).

use crate::{Clip, ClipId, ContractError};

/// Default normalize target: just under full scale, so the peak does not clip after resampling.
pub const DEFAULT_NORMALIZE_PEAK_DB: f32 = -0.3;
/// Lowest normalize target.
pub const MIN_NORMALIZE_PEAK_DB: f32 = -60.0;

/// Processing of audio clips. Job: change how an audio clip sounds by looking at (or rewriting) the audio it plays.
///
/// A note clip is `clips::NEEDS_AUDIO_CLIP`, an unknown clip `clips::NOT_FOUND`, a sample not in memory
/// `samples::NOT_FOUND`. Each method is one undo step; on error the project is unchanged.
pub trait ClipProcessing: Send + Sync {
    /// Non-destructive normalize: for each clip, the peak of the source audio it plays (all its takes, from
    /// `offset_frames` for as many frames as its length plays) is measured and the clip gain is set so that the peak
    /// reaches `peak_db` dBFS, at most [`crate::MAX_CLIP_GAIN`]. A silent clip keeps its gain. `peak_db` is from
    /// [`MIN_NORMALIZE_PEAK_DB`] to 0 (outside `clips::NORMALIZE_LEVEL_OUT_OF_RANGE`, not a number
    /// `clips::NORMALIZE_LEVEL_NOT_A_NUMBER`). Returns the clips in `ids` order; an empty list does nothing.
    fn normalize(&self, ids: Vec<ClipId>, peak_db: f32) -> Result<Vec<Clip>, ContractError>;
    /// Reverse: a reversed copy of the clip's sample is written into the project folder and added as a new sample;
    /// the clip plays it with the mirrored offset, so the same piece of audio plays backwards in the same place
    /// (if the clip runs past the sound end, the reversed sound starts at the clip start). Position, length, gain,
    /// fades, warp and pitch stay. A project without a file is `samples::REVERSE_NEEDS_SAVED_PROJECT`; a clip with
    /// several takes is `clips::REVERSE_NEEDS_ONE_TAKE`. Returns the changed clip.
    fn reverse(&self, id: ClipId) -> Result<Clip, ContractError>;
}
