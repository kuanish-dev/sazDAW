//! Audio files: the decoder (`FileDecoder`, symphonia), WAV writing (`WavWriter`), samples in memory (`SampleBank`),
//! preview (`Previewer`) and the project sound browser (`FolderSounds`).

mod bank;
mod browser;
mod decoder;
mod preview;
#[cfg(test)]
mod testing;
mod writer;

pub use bank::SampleBank;
pub use browser::FolderSounds;
pub use decoder::FileDecoder;
pub use preview::Previewer;
pub use writer::WavWriter;
