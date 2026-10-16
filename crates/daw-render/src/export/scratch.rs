//! Temporary file with the mix audio during loudness normalization: the render writes float samples here as they are
//! (and measures the loudness), then the same audio is read back with the found gain into the export file.
//!
//! Why a file, not memory and not a second render: 20 minutes of 48 kHz stereo take ~460 MB in memory (an hour at
//! 192 kHz takes 5.5 GB), and a second render would double the time with heavy plugins and could sound slightly
//! different (randomness in plugins), so the file's loudness and peak would differ from the measured ones. The file
//! lies next to the export (`<file>.render-tmp`) and is removed after writing and on failure.

use std::fs::File;
use std::io::{BufReader, BufWriter, Read, Write};
use std::path::{Path, PathBuf};

use daw_contracts::ContractError;
use daw_contracts::codes::export;

use crate::CHUNK_FRAMES;
use crate::encode::io_error;

const SAMPLE_BYTES: usize = 4;

pub(crate) struct Scratch {
    path: PathBuf,
    writer: BufWriter<File>,
    samples: u64,
}

impl Scratch {
    pub(crate) fn path_for(export: &Path) -> PathBuf {
        let mut name = export.as_os_str().to_owned();
        name.push(".render-tmp");
        PathBuf::from(name)
    }

    pub(crate) fn create(path: &Path) -> Result<Self, ContractError> {
        if let Some(dir) = path.parent().filter(|d| !d.as_os_str().is_empty()) {
            std::fs::create_dir_all(dir).map_err(io_error)?;
        }
        let writer = BufWriter::new(File::create(path).map_err(io_error)?);
        Ok(Self { path: path.to_path_buf(), writer, samples: 0 })
    }

    pub(crate) fn write(&mut self, interleaved: &[f32]) -> Result<(), ContractError> {
        for sample in interleaved {
            self.writer.write_all(&sample.to_le_bytes()).map_err(io_error)?;
        }
        self.samples += interleaved.len() as u64;
        Ok(())
    }

    /// Reads back what was written in chunks of `CHUNK_FRAMES` frames; `progress` gets the share read.
    pub(crate) fn read(
        mut self,
        mut each: impl FnMut(&mut [f32]) -> Result<(), ContractError>,
        progress: &dyn Fn(f32),
    ) -> Result<(), ContractError> {
        self.writer.flush().map_err(io_error)?;
        let mut reader = BufReader::new(File::open(&self.path).map_err(io_error)?);
        let mut bytes = vec![0u8; CHUNK_FRAMES * 2 * SAMPLE_BYTES];
        let mut chunk = vec![0.0f32; CHUNK_FRAMES * 2];
        let mut done = 0u64;
        while done < self.samples {
            let count = (self.samples - done).min(chunk.len() as u64) as usize;
            reader.read_exact(&mut bytes[..count * SAMPLE_BYTES]).map_err(io_error)?;
            for (sample, raw) in chunk.iter_mut().zip(bytes.chunks_exact(SAMPLE_BYTES)).take(count) {
                *sample = f32::from_le_bytes([raw[0], raw[1], raw[2], raw[3]]);
            }
            each(&mut chunk[..count])?;
            done += count as u64;
            progress(done as f32 / self.samples as f32);
        }
        Ok(())
    }
}

/// Removes the temporary file after the work that produced `result`. Failing to remove it is an error (gigabytes must
/// not be left behind silently); a missing file (the failure happened before it was created) is not.
pub(crate) fn remove<T>(path: &Path, result: Result<T, ContractError>) -> Result<T, ContractError> {
    let removal = match std::fs::remove_file(path) {
        Err(error) if error.kind() != std::io::ErrorKind::NotFound => Some(error),
        _ => None,
    };
    match (result, removal) {
        (result, None) => result,
        (Ok(_), Some(error)) => Err(export::TEMP_FILE_NOT_REMOVED.with("path", path.display()).with("details", error)),
        (Err(failure), Some(error)) => {
            Err(export::FAILED_TEMP_FILE_LEFT.with("cause", failure).with("path", path.display()).with("details", error))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn samples_come_back_unchanged_in_chunks_and_the_file_is_removed() {
        let dir = std::env::temp_dir().join(format!("daw-render-scratch-{}", std::process::id()));
        let path = Scratch::path_for(&dir.join("mix.wav"));
        assert!(path.ends_with("mix.wav.render-tmp"));
        let audio: Vec<f32> = (0..(CHUNK_FRAMES * 2 + 10) * 2).map(|i| (i as f32 * 0.001).sin()).collect();
        let mut scratch = Scratch::create(&path).unwrap();
        scratch.write(&audio[..1000]).unwrap();
        scratch.write(&audio[1000..]).unwrap();
        let mut back = Vec::new();
        let result = scratch.read(
            |chunk| {
                back.extend_from_slice(chunk);
                Ok(())
            },
            &|_| {},
        );
        assert_eq!(remove(&path, result).map(|()| back.len()), Ok(audio.len()));
        assert_eq!(back, audio);
        assert!(!path.exists());
        assert_eq!(remove(&path, Ok(1)), Ok(1), "an already removed file is not an error");
        std::fs::remove_dir_all(dir).ok();
    }
}
