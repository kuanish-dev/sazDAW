//! MP3 (LAME via `mp3lame-encoder`): stereo, constant bitrate, best quality. Float samples go to LAME as they are, so no
//! rounding or dither is needed. LAME picks the output rate: 32/44.1/48 kHz stay, anything above 48 kHz (MP3 cannot
//! store it) is resampled down inside LAME. No Xing/LAME tag is written: a CBR file does not need it, and writing it
//! would mean rewriting the file start; without it decoders hear the encoder delay (~1100 frames of leading silence).

use std::io::Write;

use daw_contracts::{ContractError, ExportRequest};
use mp3lame_encoder::{Bitrate, Builder, FlushGap, InterleavedPcm, Mode, Quality};

use super::{Encoder, Output, io_error};
use daw_contracts::codes::export;

/// Bytes to reserve for `flush`: what LAME asks for its last frames.
const FLUSH_BYTES: usize = 7_200;

pub(super) fn check(sample_rate: u32, bitrate: u32) -> Result<(), ContractError> {
    if bitrate_of(bitrate).is_none() {
        return Err(unsupported_bitrate(bitrate));
    }
    // LAME checks the rate itself when the encoder starts: what it refuses here it would refuse after the render too.
    builder(sample_rate, bitrate)?.build().map(|_| ()).map_err(lame_error)
}

fn bitrate_of(kbps: u32) -> Option<Bitrate> {
    match kbps {
        128 => Some(Bitrate::Kbps128),
        192 => Some(Bitrate::Kbps192),
        256 => Some(Bitrate::Kbps256),
        320 => Some(Bitrate::Kbps320),
        _ => None,
    }
}

fn unsupported_bitrate(kbps: u32) -> ContractError {
    export::MP3_BITRATE_UNSUPPORTED.with("kbps", kbps).with("allowed", format!("{:?}", ExportRequest::MP3_BITRATES))
}

fn lame_error(error: impl std::fmt::Display) -> ContractError {
    export::MP3_ENCODER_FAILED.with("details", error)
}

fn builder(sample_rate: u32, kbps: u32) -> Result<Builder, ContractError> {
    let bitrate = bitrate_of(kbps).ok_or_else(|| unsupported_bitrate(kbps))?;
    let mut builder = Builder::new().ok_or_else(|| lame_error("LAME did not start"))?;
    builder.set_num_channels(2).map_err(lame_error)?;
    builder.set_sample_rate(sample_rate).map_err(lame_error)?;
    builder.set_brate(bitrate).map_err(lame_error)?;
    builder.set_mode(Mode::JointStereo).map_err(lame_error)?;
    builder.set_quality(Quality::Best).map_err(lame_error)?;
    builder.set_to_write_vbr_tag(false).map_err(lame_error)?;
    Ok(builder)
}

pub(super) struct Mp3Encoder {
    file: Output,
    lame: mp3lame_encoder::Encoder,
    /// Compressed bytes of one chunk, reused between chunks.
    bytes: Vec<u8>,
}

impl Mp3Encoder {
    pub(super) fn new(file: Output, sample_rate: u32, kbps: u32) -> Result<Self, ContractError> {
        let lame = builder(sample_rate, kbps)?.build().map_err(lame_error)?;
        Ok(Self { file, lame, bytes: Vec::new() })
    }
}

impl Encoder for Mp3Encoder {
    fn write(&mut self, interleaved: &[f32]) -> Result<(), ContractError> {
        self.bytes.clear();
        self.bytes.reserve(mp3lame_encoder::max_required_buffer_size(interleaved.len() / 2));
        self.lame.encode_to_vec(InterleavedPcm(interleaved), &mut self.bytes).map_err(lame_error)?;
        self.file.write_all(&self.bytes).map_err(io_error)
    }

    fn finish(mut self: Box<Self>) -> Result<(), ContractError> {
        self.bytes.clear();
        self.bytes.reserve(FLUSH_BYTES);
        self.lame.flush_to_vec::<FlushGap>(&mut self.bytes).map_err(lame_error)?;
        self.file.write_all(&self.bytes).map_err(io_error)?;
        self.file.flush().map_err(io_error)
    }
}
