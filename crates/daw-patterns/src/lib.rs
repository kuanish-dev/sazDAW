//! Parts from ready-made patterns. Drum parts: a library of beats (`PatternBank`, the table in `data.rs`) placed one
//! after another (`DrumPartBuilder`) — grooves, fills, breaks, intros and endings become drum tracks with clips. Chord
//! parts: a chord progression strummed with a strum pattern (`ChordPartBuilder`; `strum.rs`, `voicing.rs`) on one
//! instrument track. Both can be heard before building (`DrumPartPreviewer`, `ChordPartPreviewer`; `render.rs`). What
//! the patterns mean and how to combine them is in the comments of `data.rs`, `daw_contracts::patterns` and
//! `daw_contracts::chord_part`.

mod chord_part;
mod chord_preview;
mod data;
mod kit;
mod library;
mod part;
mod plan;
mod preview;
mod render;
mod strum;
#[cfg(test)]
mod testing;
mod voicing;

pub use chord_part::ChordPartBuilder;
pub use chord_preview::ChordPartPreviewer;
pub use library::PatternBank;
pub use part::DrumPartBuilder;
pub use preview::DrumPartPreviewer;
