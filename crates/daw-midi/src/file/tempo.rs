//! Tempo map ↔ MIDI file meta events (Set Tempo, Time Signature). In MIDI the tempo changes only in steps,
//! so a ramp is written as a staircase: each step carries the average tempo of its span (the span's duration is kept
//! exactly), and on reading a staircase stays a staircase.

use daw_contracts::codes::midi;
use daw_contracts::{Bpm, ContractError, PPQ, SignatureChange, TempoChange, TempoMap, TimeSignature};
use midly::num::u24;
use midly::{MetaMessage, Smf, TrackEventKind};

use super::read::{ticks_per_beat, to_project_ticks};

/// The most steps one ramp produces (a short ramp gets one per sixteenth).
const RAMP_STEPS: u32 = 32;
const MIN_STEP_TICKS: u32 = PPQ / 4;
/// The MIDI default tempo when the file has none before its first change.
const MIDI_DEFAULT_BPM: f64 = 120.0;

fn micros(bpm: f64) -> u24 {
    u24::new(((60_000_000.0 / bpm).round() as u32).min(u24::max_value().as_int()))
}

/// Map events at project ticks: tempo (ramps as staircases) and signatures.
pub(super) fn tempo_events(map: &TempoMap) -> Vec<(u32, MetaMessage<'static>)> {
    let tempos = map.tempos();
    let mut events = Vec::new();
    for (i, change) in tempos.iter().enumerate() {
        match tempos.get(i + 1).filter(|next| next.ramp) {
            Some(next) => {
                let step = ((next.tick - change.tick) / RAMP_STEPS).max(MIN_STEP_TICKS);
                let mut tick = change.tick;
                while tick < next.tick {
                    let end = (tick + step).min(next.tick);
                    let seconds = map.seconds_at(f64::from(end)) - map.seconds_at(f64::from(tick));
                    let average = f64::from(end - tick) / f64::from(PPQ) * 60.0 / seconds;
                    events.push((tick, MetaMessage::Tempo(micros(average))));
                    tick = end;
                }
            }
            None => events.push((change.tick, MetaMessage::Tempo(micros(change.bpm.get())))),
        }
    }
    for change in map.signatures() {
        let signature = change.signature;
        let power = signature.denominator().trailing_zeros() as u8;
        events.push((change.tick, MetaMessage::TimeSignature(signature.numerator(), power, 24, 8)));
    }
    // The sort is stable: on the same tick the tempo goes before the signature.
    events.sort_by_key(|(tick, _)| *tick);
    events
}

/// The map from tempo and signature events of all file tracks. No event at tick 0 means the MIDI default (120) and 4/4;
/// of several events on one tick the last wins; equal consecutive tempos merge.
pub(super) fn read_tempo_map(smf: &Smf) -> Result<TempoMap, ContractError> {
    let per_beat = ticks_per_beat(smf.header.timing)?;
    let mut tempos: Vec<(u32, f64)> = Vec::new();
    let mut signatures: Vec<(u32, TimeSignature)> = Vec::new();
    for track in &smf.tracks {
        let mut time = 0u64;
        for event in track {
            time += u64::from(event.delta.as_int());
            let tick = to_project_ticks(time, per_beat);
            match event.kind {
                TrackEventKind::Meta(MetaMessage::Tempo(value)) => {
                    let bpm = (60_000_000.0 / f64::from(value.as_int().max(1)) * 1000.0).round() / 1000.0;
                    tempos.push((tick, bpm));
                }
                TrackEventKind::Meta(MetaMessage::TimeSignature(numerator, power, ..)) => {
                    let denominator = 1u8.checked_shl(u32::from(power)).unwrap_or(0);
                    signatures.push((tick, file_signature(numerator, denominator)?));
                }
                _ => {}
            }
        }
    }
    let tempos = last_per_tick(tempos, MIDI_DEFAULT_BPM);
    let signatures = last_per_tick(signatures, TimeSignature::FOUR_FOUR);
    let mut changes: Vec<TempoChange> = Vec::new();
    for (tick, bpm) in tempos {
        let bpm = Bpm::new(bpm).map_err(file_error)?;
        if changes.last().is_none_or(|last| last.bpm != bpm) {
            changes.push(TempoChange { tick, bpm, ramp: false });
        }
    }
    let signatures = signatures.into_iter().map(|(tick, signature)| SignatureChange { tick, signature }).collect();
    TempoMap::new(changes, signatures).map_err(file_error)
}

fn file_signature(numerator: u8, denominator: u8) -> Result<TimeSignature, ContractError> {
    TimeSignature::new(numerator, denominator).map_err(file_error)
}

fn file_error(error: ContractError) -> ContractError {
    midi::BAD_FILE_TEMPO.with("details", error)
}

/// One value per tick (the last), ascending, and always one at tick 0 (`base` if the file has none).
fn last_per_tick<T: Copy>(mut events: Vec<(u32, T)>, base: T) -> Vec<(u32, T)> {
    events.sort_by_key(|(tick, _)| *tick);
    let mut result: Vec<(u32, T)> = Vec::new();
    for (tick, value) in events {
        match result.last_mut() {
            Some(last) if last.0 == tick => last.1 = value,
            _ => result.push((tick, value)),
        }
    }
    if result.first().is_none_or(|(tick, _)| *tick != 0) {
        result.insert(0, (0, base));
    }
    result
}
