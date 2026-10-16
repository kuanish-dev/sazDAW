//! Writing export audio to a file of the chosen format (`Encoder`): WAV (`wav.rs`), FLAC (`flac.rs`), MP3 (`mp3.rs`);
//! rounding to integer samples with dither is in `pcm.rs`. Audio arrives in chunks as it renders, so it never piles up
//! in memory. The file is created together with its folder; an unfinished one is removed on failure, because a broken
//! file would look like a finished export.

mod flac;
mod mp3;
mod pcm;
mod wav;

use std::fs::File;
use std::io::BufWriter;
use std::path::Path;

use daw_contracts::codes::export;
use daw_contracts::{BitDepth, ContractError, ExportFormat, ExportRequest};

type Output = BufWriter<File>;

/// Encoder of one file: stereo audio in chunks, then `finish` (tail and headers; without it the file is not complete).
pub(crate) trait Encoder {
    /// Interleaved stereo frames in -1..1; louder samples are clipped.
    fn write(&mut self, interleaved: &[f32]) -> Result<(), ContractError>;
    fn finish(self: Box<Self>) -> Result<(), ContractError>;
}

pub(crate) fn io_error(error: impl std::fmt::Display) -> ContractError {
    export::WRITE_FAILED.with("details", error)
}

/// Checks the format before rendering: otherwise the error would only surface after a long render (when normalizing).
pub(crate) fn check(request: &ExportRequest) -> Result<(), ContractError> {
    match request.format {
        ExportFormat::Flac if request.bit_depth == BitDepth::Float32 => Err(export::FLAC_FLOAT_UNSUPPORTED.err()),
        ExportFormat::Mp3 => mp3::check(request.sample_rate, mp3_bitrate(request)),
        ExportFormat::Wav | ExportFormat::Flac => Ok(()),
    }
}

fn mp3_bitrate(request: &ExportRequest) -> u32 {
    request.mp3_bitrate.unwrap_or(ExportRequest::DEFAULT_MP3_BITRATE)
}

/// Creates the file `path` in the requested format, fills it with `fill` and closes it. On failure the unfinished file
/// is removed; if that fails too, the error says so (`PARTIAL_FILE_LEFT`).
pub(crate) fn write_file<T>(
    path: &Path,
    request: &ExportRequest,
    fill: impl FnOnce(&mut dyn Encoder) -> Result<T, ContractError>,
) -> Result<T, ContractError> {
    if let Some(dir) = path.parent().filter(|d| !d.as_os_str().is_empty()) {
        std::fs::create_dir_all(dir).map_err(io_error)?;
    }
    let file = BufWriter::new(File::create(path).map_err(io_error)?);
    let written = open(file, request).and_then(|mut encoder| {
        let value = fill(encoder.as_mut())?;
        encoder.finish()?;
        Ok(value)
    });
    written.map_err(|error| discard_partial(path, error))
}

fn open(file: Output, request: &ExportRequest) -> Result<Box<dyn Encoder>, ContractError> {
    check(request)?;
    let rate = request.sample_rate;
    let dither = request.dithers();
    Ok(match (request.format, request.bit_depth) {
        (ExportFormat::Wav, depth) => Box::new(wav::WavEncoder::new(file, rate, depth, dither)?),
        (ExportFormat::Flac, BitDepth::Int16) => Box::new(flac::FlacEncoder::new(file, rate, 16, dither)?),
        (ExportFormat::Flac, _) => Box::new(flac::FlacEncoder::new(file, rate, 24, dither)?),
        (ExportFormat::Mp3, _) => Box::new(mp3::Mp3Encoder::new(file, rate, mp3_bitrate(request))?),
    })
}

fn discard_partial(path: &Path, error: ContractError) -> ContractError {
    match std::fs::remove_file(path) {
        Ok(()) => error,
        Err(removal) => export::PARTIAL_FILE_LEFT.with("cause", error).with("path", path.display()).with("details", removal),
    }
}

#[cfg(test)]
mod tests;
