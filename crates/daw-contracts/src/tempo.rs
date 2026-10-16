//! The contract "song tempo and time signature": editing the project's tempo map.

use crate::{Bpm, ContractError, SignatureChange, TempoChange, TempoMap, TimeSignature};

/// Song tempo and time signature. Job: hold the project's tempo map (`Project::tempo_map`): which tempo and time
/// signature are where.
///
/// The map is part of the project: every edit is an undo step, marks the project modified and is heard at once (the
/// engine receives it in the snapshot with the rest of the project). The transport does not store the tempo: the clock
/// takes it from the map in the snapshot. Lists are replaced whole (checked by [`TempoMap::new`], codes
/// `tempo::TEMPO_MAP_*`, `SIGNATURE_OFF_BAR`); on error the project does not change.
pub trait TempoControl: Send + Sync {
    /// The current map.
    fn tempo_map(&self) -> TempoMap;
    /// Replaces the tempo points (the first at tick 0).
    fn set_tempo_changes(&self, changes: Vec<TempoChange>) -> Result<(), ContractError>;
    /// Replaces the time signature changes (the first at tick 0, the others at bar starts).
    fn set_signature_changes(&self, changes: Vec<SignatureChange>) -> Result<(), ContractError>;
    /// Replaces the whole map in one undo step (e.g. a map from a MIDI file).
    fn set_tempo_map(&self, map: TempoMap) -> Result<(), ContractError>;
    /// Tempo at the song start (the point at tick 0); the other points stay.
    fn set_tempo(&self, tempo: Bpm) -> Result<(), ContractError>;
    /// Time signature at the song start; the other changes stay if they still sit at bar starts (otherwise an error).
    fn set_time_signature(&self, signature: TimeSignature) -> Result<(), ContractError>;
}
