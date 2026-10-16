//! Putting a recorded take into the project: cut the latency, give each track its channels, write a WAV per track,
//! import the samples and place clips (or new takes of covered clips). A take that played around the loop region is
//! cut into passes (`passes.rs`), and every pass becomes a take of one clip on the loop region. All of it is one undo
//! step (`HistoryControl::begin_group`).

use std::path::Path;

use daw_contracts::codes::audio;
use daw_contracts::{Clip, ContractError, InputChannels, SampleData, SampleId, StreamConfig, TempoMap, TrackId, tr};

use crate::Inner;
use crate::passes::{Pass, passes};
use crate::split::{align, covered_clip, track_sound};

/// Buffer size when none is chosen.
const DEFAULT_BUFFER_FRAMES: u32 = 512;

/// Seconds to input frames; a meaningless value (negative, NaN) means no shift.
fn to_frames(seconds: f64, input_rate: u32) -> usize {
    if seconds.is_finite() && seconds > 0.0 { (seconds * f64::from(input_rate)).round() as usize } else { 0 }
}

/// How many frames the recorded audio lags behind what the app played.
/// A manual value wins. Otherwise an estimate: one buffer for input and one for output, i.e. two buffers at the output rate.
pub(crate) fn latency_frames(configured_ms: Option<f32>, buffer_frames: Option<u32>, output_rate: u32, input_rate: u32) -> usize {
    let ms = configured_ms.unwrap_or_else(|| {
        let rate = if output_rate == 0 { input_rate } else { output_rate };
        2.0 * buffer_frames.unwrap_or(DEFAULT_BUFFER_FRAMES) as f32 / rate.max(1) as f32 * 1000.0
    });
    to_frames(f64::from(ms) / 1000.0, input_rate)
}

/// Audio of one track, ready to go into the project.
struct TrackTake {
    track: TrackId,
    name: String,
    sound: Vec<f32>,
    channels: u16,
}

impl Inner {
    /// An empty take or one shorter than the latency is an error with no project changes. A failure on one track does
    /// not stop the others: what was created stays, and the error lists the failures.
    pub(crate) fn save_takes(
        &self,
        tracks: &[(TrackId, InputChannels)],
        start_tick: u32,
        loop_region: Option<(u32, u32)>,
        format: StreamConfig,
        mut data: Vec<f32>,
    ) -> Result<Vec<Clip>, ContractError> {
        let channels = usize::from(format.channels.max(1));
        data.truncate(data.len() / channels * channels);
        if data.is_empty() {
            return Err(audio::EMPTY_TAKE.err());
        }
        let settings = self.deps.devices.settings();
        let output_rate = self.deps.devices.status().sample_rate;
        // Plus plugin delay compensation (PDC): the master sounds that much later than the transport position, and the
        // musician playing along is late by the same amount.
        let skip = latency_frames(settings.record_latency_ms, settings.buffer_frames, output_rate, format.sample_rate)
            + to_frames(self.deps.stats.latency().seconds(), format.sample_rate);
        // Cut the start: input audio arrives later than where it belongs on the track by the latency.
        if skip * channels >= data.len() {
            return Err(audio::TAKE_SHORTER_THAN_LATENCY.err());
        }
        data.drain(..skip * channels);
        let map = self.deps.tempo.tempo_map();
        let passes = passes(data.len() / channels, start_tick, loop_region, &map, format.sample_rate);

        let project_file = self.deps.project.path().ok_or_else(|| audio::PROJECT_NOT_SAVED.err())?;
        let names = self.deps.tracks.tracks();
        let takes: Vec<TrackTake> = tracks
            .iter()
            .map(|&(track, input)| {
                let (sound, channels) = track_sound(&data, format.channels, input);
                let name = names.iter().find(|t| t.id == track).map_or_else(|| tr!("Track {track}", "Дорожка {track}"), |t| t.name.clone());
                TrackTake { track, name, sound, channels }
            })
            .collect();
        drop(data);

        self.deps.history.begin_group(&tr!("Audio recording", "Запись звука"));
        let mut clips = Vec::new();
        let mut failures = Vec::new();
        for take in takes {
            let name = take.name.clone();
            // More than one pass means playback jumped back at the loop end during the take.
            let placed = match (passes.as_slice(), loop_region) {
                ([_, _, ..], Some(region)) => self.place_passes(&take, &passes, region, &map, format.sample_rate, &project_file),
                _ => self.place(take, start_tick, &map, format.sample_rate, &project_file),
            };
            match placed {
                Ok(clip) => clips.push(clip),
                Err(error) => failures.push(format!("\"{name}\": {error}")),
            }
        }
        self.deps.history.end_group();
        if failures.is_empty() { Ok(clips) } else { Err(audio::TAKES_FAILED.with("details", failures.join("; "))) }
    }

    /// The track take: over a covered audio clip it becomes that clip's new take (audio aligned to the clip start),
    /// otherwise a new clip at the start position. Placement rules (overlap, length by tempo) are checked by
    /// `ClipControl` itself; if the clip does not fit, the audio is already in the project, only the clip is lost, and
    /// the error says so.
    fn place(&self, mut take: TrackTake, start_tick: u32, map: &TempoMap, rate: u32, project_file: &Path) -> Result<Clip, ContractError> {
        let frames = take.sound.len() / usize::from(take.channels.max(1));
        let end = map.tick_after_frames(f64::from(start_tick), frames as f64, rate);
        let clips = self.deps.clips.clips();
        let covered = covered_clip(&clips, take.track, u64::from(start_tick), end.ceil() as u64).cloned();
        if let Some(clip) = &covered {
            // The audio frame at the clip start must become frame `offset_frames` (shared by all takes).
            let lead = map.frames_between(f64::from(start_tick), f64::from(clip.start_tick), rate).round() as i64;
            let shift = i64::try_from(clip.offset_frames).unwrap_or(i64::MAX) - lead;
            if !align(&mut take.sound, take.channels, shift) {
                return Err(audio::TAKE_BEFORE_CLIP.err());
            }
        }
        let name = tr!("Recording {}", "Запись {}", take.name);
        let sample = self.save_sound(&name, take.sound, take.channels, rate, project_file)?;
        let placed = match covered {
            Some(clip) => self.deps.takes.add_take(clip.id, sample),
            None => self.deps.clips.add_audio_clip(take.track, start_tick, sample),
        };
        placed.map_err(|error| audio::TAKE_CLIP_NOT_CREATED.with("details", error))
    }

    /// Loop recording: every pass becomes a take of one clip on the loop region — the clip the loop covers (the same
    /// rule as `covered_clip`) or a new one at the loop start made from the first pass. Each pass is aligned to that
    /// clip start like a single take. All passes are saved as samples before any clip changes, so a clip that does
    /// not fit loses no audio.
    fn place_passes(
        &self,
        take: &TrackTake,
        passes: &[Pass],
        (loop_start, loop_end): (u32, u32),
        map: &TempoMap,
        rate: u32,
        project_file: &Path,
    ) -> Result<Clip, ContractError> {
        let width = usize::from(take.channels.max(1));
        let clips = self.deps.clips.clips();
        let target = covered_clip(&clips, take.track, u64::from(loop_start), u64::from(loop_end)).cloned();
        let (anchor, offset) = target.as_ref().map_or((loop_start, 0), |clip| (clip.start_tick, clip.offset_frames));
        let mut samples = Vec::new();
        for (number, pass) in passes.iter().enumerate() {
            let mut sound = take.sound[pass.frames.start * width..pass.frames.end * width].to_vec();
            let lead = map.frames_between(f64::from(pass.start_tick), f64::from(anchor), rate).round() as i64;
            // A pass that ends before the clip starts has nothing to play there: the contract drops it, as a single take
            // loses its part before the clip.
            if !align(&mut sound, take.channels, i64::try_from(offset).unwrap_or(i64::MAX) - lead) {
                continue;
            }
            let name = tr!("Recording {} (pass {})", "Запись {} (круг {})", take.name, number + 1);
            samples.push((self.save_sound(&name, sound, take.channels, rate, project_file)?, pass.reached_end));
        }
        self.add_passes(take.track, target, loop_start, &samples).map_err(|error| audio::TAKE_CLIP_NOT_CREATED.with("details", error))
    }

    /// The pass samples in order become the clip's takes; the last pass that played to the loop end is the active one,
    /// a pass cut short by stop stays as a take that does not play.
    fn add_passes(
        &self,
        track: TrackId,
        target: Option<Clip>,
        loop_start: u32,
        samples: &[(SampleId, bool)],
    ) -> Result<Clip, ContractError> {
        let mut clip = target;
        for &(sample, _) in samples {
            clip = Some(match &clip {
                Some(clip) => self.deps.takes.add_take(clip.id, sample)?,
                None => self.deps.clips.add_audio_clip(track, loop_start, sample)?,
            });
        }
        let clip = clip.ok_or_else(|| audio::TAKE_BEFORE_CLIP.err())?;
        match samples.iter().rev().find(|(_, reached_end)| *reached_end) {
            Some(&(sample, _)) if clip.content.sample() != Some(sample) => self.deps.takes.set_active_take(clip.id, sample),
            _ => Ok(clip),
        }
    }

    /// Writes the sound as a WAV in the project's `samples/` and imports it.
    fn save_sound(&self, name: &str, frames: Vec<f32>, channels: u16, rate: u32, project_file: &Path) -> Result<SampleId, ContractError> {
        let path = self.deps.assets.new_asset_path(project_file, name);
        self.deps.writer.write(&path, &SampleData { sample_rate: rate, channels, frames })?;
        Ok(self.deps.samples.import(&path)?.id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn latency_is_manual_or_two_buffers_at_the_output_rate() {
        assert_eq!(latency_frames(Some(10.0), Some(64), 44_100, 48_000), 480, "a manual value is in input frames");
        assert_eq!(latency_frames(None, Some(256), 48_000, 48_000), 512);
        assert_eq!(latency_frames(None, None, 48_000, 48_000), 1024, "the default buffer is 512");
        assert_eq!(latency_frames(None, Some(480), 0, 48_000), 960, "output rate unknown: the input rate is used");
        assert_eq!(latency_frames(Some(-5.0), None, 48_000, 48_000), 0);
        assert_eq!(latency_frames(Some(f32::NAN), None, 48_000, 48_000), 0);
    }
}
