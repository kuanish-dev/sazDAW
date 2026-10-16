//! WAV (hound): 16/24-bit integers (through `Quantizer`, dithered on request) or 32-bit float.

use daw_contracts::{BitDepth, ContractError};

use super::pcm::Quantizer;
use super::{Encoder, Output, io_error};

pub(super) struct WavEncoder {
    writer: hound::WavWriter<Output>,
    /// `None`: float samples are written as they are.
    quantizer: Option<Quantizer>,
}

impl WavEncoder {
    pub(super) fn new(file: Output, sample_rate: u32, depth: BitDepth, dither: bool) -> Result<Self, ContractError> {
        let (bits_per_sample, sample_format, quantizer) = match depth {
            BitDepth::Int16 => (16, hound::SampleFormat::Int, Some(Quantizer::new(16, dither))),
            BitDepth::Int24 => (24, hound::SampleFormat::Int, Some(Quantizer::new(24, dither))),
            BitDepth::Float32 => (32, hound::SampleFormat::Float, None),
        };
        let spec = hound::WavSpec { channels: 2, sample_rate, bits_per_sample, sample_format };
        Ok(Self { writer: hound::WavWriter::new(file, spec).map_err(io_error)?, quantizer })
    }
}

impl Encoder for WavEncoder {
    fn write(&mut self, interleaved: &[f32]) -> Result<(), ContractError> {
        for &sample in interleaved {
            match &mut self.quantizer {
                Some(quantizer) => self.writer.write_sample(quantizer.quantize(sample)),
                None => self.writer.write_sample(sample.clamp(-1.0, 1.0)),
            }
            .map_err(io_error)?;
        }
        Ok(())
    }

    fn finish(self: Box<Self>) -> Result<(), ContractError> {
        self.writer.finalize().map_err(io_error)
    }
}
