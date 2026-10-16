//! Project sounds: decoded data, store, decoder and writer, sample catalog, editor and preview.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::{ContractError, SampleId};

/// Decoded sound: interleaved frames.
#[derive(Debug, Clone, PartialEq)]
pub struct SampleData {
    pub sample_rate: u32,
    pub channels: u16,
    pub frames: Vec<f32>,
}

/// Waveform envelope for drawing: a (min, max) pair per bucket.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Waveform {
    pub frame_count: usize,
    pub sample_rate: u32,
    pub channels: u16,
    /// Frame range the buckets were computed for.
    pub from: usize,
    pub to: usize,
    pub peaks: Vec<(f32, f32)>,
}

impl SampleData {
    /// Min and max over all channels for `buckets` equal parts of the range `[from, to)`.
    /// Bounds are clamped to the file length; an empty range gives silence.
    pub fn peaks(&self, from: usize, to: usize, buckets: usize) -> Vec<(f32, f32)> {
        let frames = self.frame_count();
        let to = to.min(frames);
        let from = from.min(to);
        let buckets = buckets.max(1);
        let channels = usize::from(self.channels.max(1));
        let mut peaks = Vec::with_capacity(buckets);
        for bucket in 0..buckets {
            let start = from + (to - from) * bucket / buckets;
            let end = (from + (to - from) * (bucket + 1) / buckets).max(start + usize::from(from < to));
            let (mut low, mut high) = (0.0f32, 0.0f32);
            for frame in start..end.min(frames) {
                for channel in 0..channels {
                    let value = self.frames[frame * channels + channel];
                    low = low.min(value);
                    high = high.max(value);
                }
            }
            peaks.push((low, high));
        }
        peaks
    }

    /// A copy of the frame range `[from, to)` as a separate sound.
    pub fn slice(&self, from: usize, to: usize) -> SampleData {
        let frames = self.frame_count();
        let to = to.min(frames);
        let from = from.min(to);
        let channels = usize::from(self.channels.max(1));
        SampleData { sample_rate: self.sample_rate, channels: self.channels, frames: self.frames[from * channels..to * channels].to_vec() }
    }

    pub fn frame_count(&self) -> usize {
        self.frames.len() / usize::from(self.channels.max(1))
    }

    /// Left/right channel of a frame (mono is duplicated).
    pub fn stereo_at(&self, frame: usize) -> (f32, f32) {
        let channels = usize::from(self.channels.max(1));
        let base = frame * channels;
        let left = self.frames.get(base).copied().unwrap_or(0.0);
        let right = if channels > 1 { self.frames.get(base + 1).copied().unwrap_or(0.0) } else { left };
        (left, right)
    }

    /// Left/right channel at a fractional position: linear interpolation between neighbouring frames.
    pub fn stereo_lerp(&self, position: f64) -> (f32, f32) {
        let index = position as usize;
        let fraction = (position - index as f64) as f32;
        let (l0, r0) = self.stereo_at(index);
        let (l1, r1) = self.stereo_at(index + 1);
        (l0 + (l1 - l0) * fraction, r0 + (r1 - r0) * fraction)
    }
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SampleInfo {
    pub id: SampleId,
    pub name: String,
    pub path: PathBuf,
    pub frame_count: usize,
    pub sample_rate: u32,
    /// Slice markers in frames, ascending.
    pub markers: Vec<u64>,
}

/// Store of decoded samples (for instruments and the engine).
pub trait SampleStore: Send + Sync {
    fn get(&self, id: SampleId) -> Option<Arc<SampleData>>;
    fn insert(&self, id: SampleId, data: Arc<SampleData>);
    fn remove(&self, id: SampleId);
}

/// Writes sound to a file (WAV).
pub trait SampleWriter: Send + Sync {
    fn write(&self, path: &Path, data: &SampleData) -> Result<(), ContractError>;
}

/// Audio file decoder.
pub trait SampleDecoder: Send + Sync {
    fn decode(&self, path: &Path) -> Result<SampleData, ContractError>;
}

/// The project's sample catalog. Job: know which sounds the project has, including taking in new ones.
pub trait SampleCatalog: Send + Sync {
    /// Adds an audio file to the project (copy into the project folder, decode, add to the model); importing the
    /// same file again returns the existing sample.
    fn import(&self, path: &Path) -> Result<SampleInfo, ContractError>;
    fn samples(&self) -> Vec<SampleInfo>;
    /// Removes the sample from the project together with the audio clips that play it; sampler tracks lose their
    /// sound choice (the track stays but is silent). The file on disk is left alone.
    /// One undo step; an unknown id is `samples::NOT_FOUND`.
    fn remove(&self, id: SampleId) -> Result<(), ContractError>;
}

/// Sample editor. Job: everything done with one project sound while it is open: view the waveform, place slice
/// markers, save or extract a piece.
pub trait SampleEditor: Send + Sync {
    /// Waveform envelope of the frame range `[from, to)` in `buckets` buckets.
    fn waveform(&self, id: SampleId, from: usize, to: usize, buckets: usize) -> Result<Waveform, ContractError>;
    /// The sample's slice markers (frames); saved in the project. Order and duplicates are normalised automatically.
    fn set_markers(&self, id: SampleId, markers: Vec<u64>) -> Result<(), ContractError>;
    /// Saves the range `[from, to)` to a WAV file at `path`; the project is unchanged (the file is not added to it).
    /// `to` is clamped to the end of the sound; an empty range is `ErrorKind::Invalid`, an unknown sample
    /// `samples::NOT_FOUND`.
    fn export_region(&self, id: SampleId, from: usize, to: usize, path: &Path) -> Result<(), ContractError>;
    /// Extracts each region into its own WAV in the project folder and adds it as a new sample (the source file is
    /// unchanged). Returns one sample per region, in the same order; everything is one undo step.
    /// Errors: unknown sample `samples::NOT_FOUND`; a project without a file (nowhere to put them) or an empty list
    /// `ErrorKind::Invalid`; more than `MAX_REGIONS` regions `ErrorKind::Limit`; an empty region `ErrorKind::Invalid`.
    /// A failure midway restores the project, but WAVs already written stay in `samples/` (the project does not
    /// reference them).
    fn extract_regions(&self, id: SampleId, regions: &[Region]) -> Result<Vec<SampleInfo>, ContractError>;
    /// The largest absolute sample value (all channels, linear, 0 for silence) in the frame range `[from, to)`; `to`
    /// is clamped to the end of the sound, an empty range gives 0. An unknown sample is `samples::NOT_FOUND`.
    fn peak(&self, id: SampleId, from: usize, to: usize) -> Result<f32, ContractError>;
    /// Writes the whole sound reversed into its own WAV in the project folder and adds it as a new sample named after
    /// the original (the source is unchanged); one undo step. An unknown sample is `samples::NOT_FOUND`, a project
    /// without a file `samples::REVERSE_NEEDS_SAVED_PROJECT`.
    fn reverse_copy(&self, id: SampleId) -> Result<SampleInfo, ContractError>;
}

/// How many regions one `SampleEditor::extract_regions` call may extract.
pub const MAX_REGIONS: usize = 1024;

/// A sample region `[from, to)` in frames, with the name of the file to create.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, JsonSchema)]
pub struct Region {
    pub from: usize,
    pub to: usize,
    pub name: String,
}

/// Sample preview: a separate engine voice, independent of channels and the transport.
pub trait SamplePreview: Send + Sync {
    /// Plays the frame range `[from, to)`, looped when `looping`; a new playback replaces the previous one.
    /// `to` is clamped to the end of the sound. An unknown sample is `samples::NOT_FOUND`; an empty range
    /// (`from >= to`, `from` past the end, sound shorter than two frames) is `ErrorKind::Invalid`.
    fn play(&self, id: SampleId, from: usize, to: usize, looping: bool) -> Result<(), ContractError>;
    /// Plays a whole file from disk without adding it to the project (library browsing).
    fn play_file(&self, path: &Path) -> Result<(), ContractError>;
    fn stop(&self);
    /// Playback position in file frames; `None` if nothing is playing.
    fn position(&self) -> Option<usize>;
}

/// A sound browser row: a folder or an audio file.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LibraryEntry {
    pub name: String,
    pub path: PathBuf,
    pub is_dir: bool,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn stereo(frames: &[(f32, f32)]) -> SampleData {
        SampleData { sample_rate: 48_000, channels: 2, frames: frames.iter().flat_map(|(l, r)| [*l, *r]).collect() }
    }

    #[test]
    fn peaks_report_min_and_max_over_all_channels_per_bucket() {
        let data = stereo(&[(0.1, -0.5), (0.3, 0.2), (-0.4, 0.0), (0.0, 0.9)]);
        let peaks = data.peaks(0, 4, 2);
        assert_eq!(peaks, [(-0.5, 0.3), (-0.4, 0.9)]);
    }

    #[test]
    fn peaks_handle_more_buckets_than_frames_and_bad_ranges() {
        let data = stereo(&[(0.5, 0.5), (-0.5, -0.5)]);
        let peaks = data.peaks(0, 2, 8);
        assert_eq!(peaks.len(), 8);
        assert!(peaks.iter().any(|p| p.1 == 0.5) && peaks.iter().any(|p| p.0 == -0.5));
        assert_eq!(data.peaks(5, 1, 3), vec![(0.0, 0.0); 3], "an empty range is silence");
        assert_eq!(data.peaks(0, 100, 1), vec![(-0.5, 0.5)], "the end is clamped to the file");
        assert_eq!(data.peaks(0, 2, 0).len(), 1, "zero buckets count as one");
    }

    #[test]
    fn slice_copies_the_requested_frames() {
        let data = stereo(&[(0.1, 0.1), (0.2, 0.2), (0.3, 0.3), (0.4, 0.4)]);
        let part = data.slice(1, 3);
        assert_eq!(part.frame_count(), 2);
        assert_eq!(part.frames, [0.2, 0.2, 0.3, 0.3]);
        assert_eq!(data.slice(3, 99).frame_count(), 1);
        assert_eq!(data.slice(9, 99).frame_count(), 0);
    }
}
