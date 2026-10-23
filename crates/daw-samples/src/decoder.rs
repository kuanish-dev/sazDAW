//! `FileDecoder`: "read an audio file into memory" (symphonia). A file whose reading stopped before the end is returned
//! with what was read, and the cut is logged (`source: "samples"`): the sound is shorter than on disk, and without the
//! log entry nobody could tell why.

use std::fs::File;
use std::path::Path;
use std::sync::Arc;

use daw_contracts::codes::samples;
use daw_contracts::{AppLog, ContractError, LogEntry, SampleData, SampleDecoder};
use symphonia::core::codecs::audio::{AudioDecoder, AudioDecoderOptions};
use symphonia::core::errors::Error as SymphoniaError;
use symphonia::core::formats::probe::Hint;
use symphonia::core::formats::{FormatOptions, FormatReader, TrackType};
use symphonia::core::io::MediaSourceStream;
use symphonia::core::meta::MetadataOptions;

fn read_failed(path: &Path, error: impl std::fmt::Display) -> ContractError {
    samples::FILE_READ_FAILED.with("path", path.display()).with("details", error)
}

pub struct FileDecoder {
    log: Arc<dyn AppLog>,
}

impl FileDecoder {
    pub fn new(log: Arc<dyn AppLog>) -> Self {
        Self { log }
    }
}

/// An open file: the container, the track decoder and what is known about the sound up front.
struct Opened {
    format: Box<dyn FormatReader>,
    decoder: Box<dyn AudioDecoder>,
    track_id: u32,
    sample_rate: u32,
    channels: u16,
}

/// Detects the format and prepares the decoder of the first audio track.
fn probe(path: &Path) -> Result<Opened, ContractError> {
    let io_error = |error| read_failed(path, error);
    let file = File::open(path).map_err(|e| read_failed(path, e))?;
    let stream = MediaSourceStream::new(Box::new(file), Default::default());
    let mut hint = Hint::new();
    if let Some(extension) = path.extension().and_then(|e| e.to_str()) {
        hint.with_extension(extension);
    }
    let format =
        symphonia::default::get_probe().probe(&hint, stream, FormatOptions::default(), MetadataOptions::default()).map_err(io_error)?;
    let track = format.default_track(TrackType::Audio).ok_or_else(|| samples::NO_AUDIO_TRACK.with("path", path.display()))?;
    let track_id = track.id;
    let codec = track
        .codec_params
        .as_ref()
        .and_then(|params| params.audio())
        .ok_or_else(|| samples::CODEC_UNSUPPORTED.with("path", path.display()))?;
    let decoder = symphonia::default::get_codecs().make_audio_decoder(codec, &AudioDecoderOptions::default()).map_err(io_error)?;
    let sample_rate = codec.sample_rate.unwrap_or(0);
    let channels = codec.channels.as_ref().map_or(0, |c| c.count() as u16);
    Ok(Opened { format, decoder, track_id, sample_rate, channels })
}

/// Reads packets to the end of the file. Returns the reason if reading stopped earlier.
fn read_packets(path: &Path, opened: &mut Opened, frames: &mut Vec<f32>) -> Result<Option<String>, ContractError> {
    let mut chunk: Vec<f32> = Vec::new();
    loop {
        let packet = match opened.format.next_packet() {
            Ok(Some(packet)) => packet,
            Ok(None) => return Ok(None),
            Err(SymphoniaError::IoError(error)) => return Ok(Some(error.to_string())),
            Err(error) => return Err(read_failed(path, error)),
        };
        if packet.track_id != opened.track_id {
            continue;
        }
        match opened.decoder.decode(&packet) {
            Ok(buffer) => {
                opened.sample_rate = buffer.spec().rate();
                opened.channels = buffer.spec().channels().count() as u16;
                chunk.resize(buffer.samples_interleaved(), 0.0);
                buffer.copy_to_slice_interleaved(&mut chunk);
                frames.extend_from_slice(&chunk);
            }
            Err(SymphoniaError::DecodeError(_)) => {}
            Err(error) => return Ok(Some(error.to_string())),
        }
    }
}

impl SampleDecoder for FileDecoder {
    fn decode(&self, path: &Path) -> Result<SampleData, ContractError> {
        let mut opened = probe(path)?;
        let mut frames = Vec::new();
        let stopped = read_packets(path, &mut opened, &mut frames)?;
        if frames.is_empty() || opened.sample_rate == 0 || opened.channels == 0 {
            return Err(samples::FILE_HAS_NO_SOUND.with("path", path.display()));
        }
        if let Some(reason) = stopped {
            let message = format!("the file was not read to the end, the sound is cut short: {reason}");
            self.log.record(LogEntry::warn("samples", message).with_details(format!("file: {}", path.display())));
        }
        Ok(SampleData { sample_rate: opened.sample_rate, channels: opened.channels, frames })
    }
}

#[cfg(test)]
mod tests {
    use daw_contracts::LogLevel;

    use super::*;
    use crate::testing::MemoryLog;

    /// A PCM16 WAV with a sine.
    fn write_wav(path: &Path, channels: u16, sample_rate: u32, frames: usize) {
        let spec = hound::WavSpec { channels, sample_rate, bits_per_sample: 16, sample_format: hound::SampleFormat::Int };
        let mut writer = hound::WavWriter::create(path, spec).unwrap();
        for frame in 0..frames {
            let value = ((frame as f32 * 0.05).sin() * 16_000.0) as i16;
            for _ in 0..channels {
                writer.write_sample(value).unwrap();
            }
        }
        writer.finalize().unwrap();
    }

    fn temp_dir(name: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("daw-samples-{name}-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn decodes_a_whole_stereo_wav_without_logging() {
        let dir = temp_dir("decode");
        let path = dir.join("tone.wav");
        write_wav(&path, 2, 44_100, 1000);
        let log = Arc::new(MemoryLog::default());
        let data = FileDecoder::new(log.clone()).decode(&path).unwrap();
        assert_eq!((data.sample_rate, data.channels, data.frame_count()), (44_100, 2, 1000));
        assert!(data.frames.iter().any(|v| v.abs() > 0.1));
        assert!(data.frames.iter().all(|v| v.abs() <= 1.0));
        assert!(log.recent(10).is_empty());
        std::fs::remove_dir_all(dir).ok();
    }

    #[test]
    fn rejects_garbage_and_missing_files() {
        let dir = temp_dir("garbage");
        let path = dir.join("bad.wav");
        std::fs::write(&path, b"not audio at all").unwrap();
        let decoder = FileDecoder::new(Arc::new(MemoryLog::default()));
        assert!(matches!(decoder.decode(&path), Err(e) if e.is(samples::FILE_READ_FAILED)));
        assert!(matches!(decoder.decode(&dir.join("missing.wav")), Err(e) if e.is(samples::FILE_READ_FAILED)));
        std::fs::remove_dir_all(dir).ok();
    }

    #[test]
    fn a_file_cut_short_is_kept_with_what_was_read_and_logged() {
        let dir = temp_dir("cut");
        let path = dir.join("cut.wav");
        write_wav(&path, 1, 44_100, 2_000);
        let bytes = std::fs::read(&path).unwrap();
        std::fs::write(&path, &bytes[..bytes.len() - 2_000]).unwrap();
        let log = Arc::new(MemoryLog::default());
        let data = FileDecoder::new(log.clone()).decode(&path).unwrap();
        assert!(data.frame_count() < 2_000 && data.frame_count() > 0, "{}", data.frame_count());
        let entries = log.recent(10);
        assert_eq!(entries.len(), 1, "the cut is logged");
        assert_eq!((entries[0].source.as_str(), entries[0].level), ("samples", LogLevel::Warn));
        std::fs::remove_dir_all(dir).ok();
    }
}
