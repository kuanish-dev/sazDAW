//! Time stretching and pitch shifting of audio clips (tempo fitting).
//!
//! `process` is pure DSP: WSOLA (`wsola.rs`) stretches time, resampling to a frame count (`resample.rs`) changes
//! pitch. A pitch shift = stretch by `2^(semitones/12)` and squeeze back by resampling: same length, higher frequencies.
//! `process_curve` does the same with a stretch that changes along the audio (the tempo inside a fitted clip changes):
//! WSOLA takes input windows along a curve of anchor points instead of one factor.
//! `StretchCache` (`cache.rs`) implements the `ProcessedAudio` contract: finished audio per job, computed in background.

mod cache;
mod resample;
mod wsola;

#[cfg(test)]
mod tests;

pub use cache::StretchCache;

use daw_contracts::codes::clips;
use daw_contracts::{ContractError, MAX_PITCH_SEMITONES, MAX_STRETCH, MIN_STRETCH, ProcessJob, SampleData, StretchAnchor};

/// Audio for a job: along its stretch curve if it has one, otherwise one stretch for the whole sound.
pub fn process_job(data: &SampleData, job: &ProcessJob) -> Result<SampleData, ContractError> {
    match job.curve() {
        Some(anchors) => process_curve(data, anchors, job.semitones()),
        None => process(data, job.stretch(), job.semitones()),
    }
}

/// Audio stretched in time by `stretch` (exactly `round(frames × stretch)` frames) and shifted by `semitones`.
/// A stretch outside [`MIN_STRETCH`]..=[`MAX_STRETCH`] is `clips::STRETCH_OUT_OF_RANGE`; a pitch outside
/// ±[`MAX_PITCH_SEMITONES`] (or not a number) is `clips::PITCH_OUT_OF_RANGE`.
pub fn process(data: &SampleData, stretch: f64, semitones: f32) -> Result<SampleData, ContractError> {
    check_stretch(stretch)?;
    check_pitch(semitones)?;
    let channels = usize::from(data.channels.max(1));
    let output_len = (data.frame_count() as f64 * stretch).round() as usize;
    let pitch = 2f64.powf(f64::from(semitones) / 12.0);
    let time_stretch = stretch * pitch;
    let stretched = if (time_stretch - 1.0).abs() < 1e-9 {
        data.frames.clone()
    } else {
        wsola::stretch_time(&data.frames, channels, data.sample_rate, time_stretch)
    };
    Ok(finish(data, stretched, channels, output_len))
}

/// The audio from the first anchor to the last, stretched along `anchors` (output frame `output` plays input frame
/// `source`, linearly in between; the output starts at the first anchor) and shifted by `semitones`. Fewer than two
/// anchors is `clips::STRETCH_CURVE_TOO_SHORT`, anchors not increasing `clips::STRETCH_CURVE_UNORDERED`; a piece
/// stretched outside [`MIN_STRETCH`]..=[`MAX_STRETCH`] is `clips::STRETCH_OUT_OF_RANGE`, a pitch outside
/// ±[`MAX_PITCH_SEMITONES`] `clips::PITCH_OUT_OF_RANGE`.
pub fn process_curve(data: &SampleData, anchors: &[StretchAnchor], semitones: f32) -> Result<SampleData, ContractError> {
    check_pitch(semitones)?;
    if anchors.len() < 2 {
        return Err(clips::STRETCH_CURVE_TOO_SHORT.err());
    }
    for pair in anchors.windows(2) {
        let (a, b) = (pair[0], pair[1]);
        if b.source <= a.source || b.output <= a.output {
            return Err(clips::STRETCH_CURVE_UNORDERED.err());
        }
        check_stretch((b.output - a.output) as f64 / (b.source - a.source) as f64)?;
    }
    let channels = usize::from(data.channels.max(1));
    let origin = anchors[0].output;
    let output_len = (anchors[anchors.len() - 1].output - origin) as usize;
    let pitch = 2f64.powf(f64::from(semitones) / 12.0);
    let wsola_len = (output_len as f64 * pitch).round() as usize;
    let source_at = |out: f64| source_frame(anchors, origin as f64 + out / pitch);
    let stretched = wsola::stretch_mapped(&data.frames, channels, data.sample_rate, wsola_len, source_at);
    Ok(finish(data, stretched, channels, output_len))
}

/// The input frame for output frame `output` along the curve (past the edges the outer pieces continue).
fn source_frame(anchors: &[StretchAnchor], output: f64) -> f64 {
    let next = anchors.partition_point(|a| (a.output as f64) <= output).clamp(1, anchors.len() - 1);
    let (a, b) = (anchors[next - 1], anchors[next]);
    let share = (output - a.output as f64) / (b.output - a.output) as f64;
    a.source as f64 + share * (b.source - a.source) as f64
}

/// Resampling to the needed length (pitch) and protection against broken input.
fn finish(data: &SampleData, stretched: Vec<f32>, channels: usize, output_len: usize) -> SampleData {
    let mut frames =
        if stretched.len() / channels == output_len { stretched } else { resample::resample(&stretched, channels, output_len) };
    for value in &mut frames {
        if !value.is_finite() {
            *value = 0.0;
        }
    }
    SampleData { sample_rate: data.sample_rate, channels: data.channels, frames }
}

fn check_stretch(stretch: f64) -> Result<(), ContractError> {
    if !(MIN_STRETCH..=MAX_STRETCH).contains(&stretch) {
        return Err(clips::STRETCH_OUT_OF_RANGE.with("stretch", stretch).with("min", MIN_STRETCH).with("max", MAX_STRETCH));
    }
    Ok(())
}

fn check_pitch(semitones: f32) -> Result<(), ContractError> {
    if !semitones.is_finite() || semitones.abs() > MAX_PITCH_SEMITONES {
        return Err(clips::PITCH_OUT_OF_RANGE.with("semitones", semitones).with("max", MAX_PITCH_SEMITONES));
    }
    Ok(())
}
