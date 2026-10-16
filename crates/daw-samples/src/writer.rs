//! `WavWriter`: "write audio to a WAV file" (24 bits).

use std::path::Path;

use daw_contracts::codes::samples;
use daw_contracts::{ContractError, SampleData, SampleWriter};

/// The largest 24-bit sample value.
const I24_MAX: f32 = 8_388_607.0;

pub struct WavWriter;

impl SampleWriter for WavWriter {
    fn write(&self, path: &Path, data: &SampleData) -> Result<(), ContractError> {
        let spec = hound::WavSpec {
            channels: data.channels.max(1),
            sample_rate: data.sample_rate,
            bits_per_sample: 24,
            sample_format: hound::SampleFormat::Int,
        };
        let failed = |e: &dyn std::fmt::Display| samples::WAV_WRITE_FAILED.with("path", path.display()).with("details", e);
        let io = |e: hound::Error| failed(&e);
        if let Some(dir) = path.parent().filter(|d| !d.as_os_str().is_empty()) {
            std::fs::create_dir_all(dir).map_err(|e| failed(&e))?;
        }
        let mut writer = hound::WavWriter::create(path, spec).map_err(io)?;
        for value in &data.frames {
            let sample = (value.clamp(-1.0, 1.0) * I24_MAX).round() as i32;
            writer.write_sample(sample).map_err(io)?;
        }
        writer.finalize().map_err(io)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::FileDecoder;
    use daw_contracts::SampleDecoder;
    use std::sync::Arc;

    #[test]
    fn written_files_decode_back_to_the_same_audio() {
        let dir = std::env::temp_dir().join(format!("daw-writer-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("out.wav");
        let frames: Vec<f32> = (0..2_000).map(|i| (i as f32 * 0.03).sin() * 0.7).collect();
        let original = SampleData { sample_rate: 44_100, channels: 2, frames };
        WavWriter.write(&path, &original).unwrap();

        let decoded = FileDecoder::new(Arc::new(crate::testing::MemoryLog::default())).decode(&path).unwrap();
        assert_eq!((decoded.sample_rate, decoded.channels, decoded.frame_count()), (44_100, 2, 1_000));
        let worst = original.frames.iter().zip(&decoded.frames).map(|(a, b)| (a - b).abs()).fold(0.0, f32::max);
        assert!(worst < 1e-5, "24 bits are almost lossless: {worst}");
        // Missing folders (for example the project's `exports/`) are created.
        let nested = dir.join("exports/stems/out.wav");
        WavWriter.write(&nested, &original).unwrap();
        assert!(nested.is_file());
        // A path that cannot be a folder (under a file) is an error.
        let under_file = WavWriter.write(&path.join("inside/out.wav"), &original);
        assert!(matches!(under_file, Err(e) if e.is(samples::WAV_WRITE_FAILED)));
        std::fs::remove_dir_all(dir).ok();
    }
}
