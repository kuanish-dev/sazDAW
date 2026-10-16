//! FLAC (flacenc, pure Rust): blocks of `BLOCK_FRAMES` are encoded one by one as the render goes and written right away.
//! The header (`fLaC` + STREAMINFO: frame count, sizes, audio MD5) is known only at the end, so a placeholder of the
//! same size is written at the file start and `finish` overwrites it with the real one.

use std::io::{Seek, SeekFrom, Write};

use daw_contracts::ContractError;
use daw_contracts::codes::export;
use flacenc::bitsink::MemSink;
use flacenc::component::{BitRepr, Stream};
use flacenc::config;
use flacenc::error::{Verified, Verify};
use flacenc::source::{Context, Fill, FrameBuf};

use super::pcm::Quantizer;
use super::{Encoder, Output, io_error};

/// Frames per FLAC block, as the reference `flac` encoder uses by default.
const BLOCK_FRAMES: usize = 4096;
const CHANNELS: usize = 2;

pub(super) struct FlacEncoder {
    file: Output,
    config: Verified<config::Encoder>,
    /// A stream without frames: only its STREAMINFO, updated after every frame.
    header: Stream,
    block: FrameBuf,
    /// Frame count and audio MD5 for STREAMINFO.
    context: Context,
    /// Interleaved samples waiting for a full block.
    pending: Vec<i32>,
    quantizer: Quantizer,
    bytes: MemSink<u8>,
}

fn flac_error(error: impl std::fmt::Display) -> ContractError {
    export::FLAC_ENCODER_FAILED.with("details", error)
}

impl FlacEncoder {
    pub(super) fn new(mut file: Output, sample_rate: u32, bits: u32, dither: bool) -> Result<Self, ContractError> {
        let config = config::Encoder::default().into_verified().map_err(|(_, error)| flac_error(error))?;
        let mut header = Stream::new(sample_rate as usize, CHANNELS, bits as usize).map_err(flac_error)?;
        header.stream_info_mut().set_block_sizes(BLOCK_FRAMES, BLOCK_FRAMES).map_err(flac_error)?;
        let mut bytes = MemSink::new();
        header.write(&mut bytes).map_err(flac_error)?;
        file.write_all(bytes.as_slice()).map_err(io_error)?;
        Ok(Self {
            file,
            config,
            header,
            block: FrameBuf::with_size(CHANNELS, BLOCK_FRAMES).map_err(flac_error)?,
            context: Context::new(bits as usize, CHANNELS),
            pending: Vec::with_capacity(BLOCK_FRAMES * CHANNELS),
            quantizer: Quantizer::new(bits, dither),
            bytes,
        })
    }

    /// Encodes what is pending (a full block, or the shorter last one) and writes the frame to the file.
    fn encode_pending(&mut self) -> Result<(), ContractError> {
        self.block.fill_interleaved(&self.pending).map_err(flac_error)?;
        self.context.fill_interleaved(&self.pending).map_err(flac_error)?;
        let number = self.context.current_frame_number().unwrap_or(0);
        let frame = flacenc::encode_fixed_size_frame(&self.config, &self.block, number, self.header.stream_info()).map_err(flac_error)?;
        self.header.stream_info_mut().update_frame_info(&frame);
        self.bytes.clear();
        frame.write(&mut self.bytes).map_err(flac_error)?;
        self.file.write_all(self.bytes.as_slice()).map_err(io_error)?;
        self.pending.clear();
        Ok(())
    }
}

impl Encoder for FlacEncoder {
    fn write(&mut self, interleaved: &[f32]) -> Result<(), ContractError> {
        for &sample in interleaved {
            self.pending.push(self.quantizer.quantize(sample));
            if self.pending.len() == BLOCK_FRAMES * CHANNELS {
                self.encode_pending()?;
            }
        }
        Ok(())
    }

    fn finish(mut self: Box<Self>) -> Result<(), ContractError> {
        if !self.pending.is_empty() {
            self.encode_pending()?;
        }
        let info = self.header.stream_info_mut();
        // The block size is fixed (only the last block is shorter), as with the reference `flac`: `update_frame_info`
        // would store the last block length as the minimum, and such a stream reads as a variable-block stream.
        info.set_block_sizes(BLOCK_FRAMES, BLOCK_FRAMES).map_err(flac_error)?;
        info.set_md5_digest(&self.context.md5_digest());
        info.set_total_samples(self.context.total_samples());
        self.bytes.clear();
        self.header.write(&mut self.bytes).map_err(flac_error)?;
        self.file.seek(SeekFrom::Start(0)).map_err(io_error)?;
        self.file.write_all(self.bytes.as_slice()).map_err(io_error)?;
        self.file.flush().map_err(io_error)
    }
}
