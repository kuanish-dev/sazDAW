//! Reading a MIDI file: note on/off pairs into notes, controllers (pitch bend, CC, channel pressure) into lanes, and
//! file ticks into project ticks.

use daw_contracts::codes::midi;
use daw_contracts::{ContractError, ControllerKind, ControllerPoint, MAX_CC_NUMBER, NewNote, PPQ};
use midly::{MidiMessage, Timing, TrackEvent, TrackEventKind};

use crate::values::{bend_from_14bit, from_7bit};

/// A note in file ticks (their resolution comes from the file header).
#[derive(Debug, PartialEq)]
pub(super) struct RawNote {
    pub pitch: u8,
    pub start: u64,
    pub length: u64,
    pub velocity: u8,
}

pub(super) fn extract_notes(track: &[TrackEvent]) -> Vec<RawNote> {
    let mut time = 0u64;
    let mut open: Vec<(u8, u64, u8)> = Vec::new();
    let mut notes = Vec::new();
    for event in track {
        time += u64::from(event.delta.as_int());
        let TrackEventKind::Midi { message, .. } = event.kind else {
            continue;
        };
        match message {
            MidiMessage::NoteOn { key, vel } if vel.as_int() > 0 => {
                open.push((key.as_int(), time, vel.as_int()));
            }
            MidiMessage::NoteOn { key, .. } | MidiMessage::NoteOff { key, .. } => {
                if let Some(index) = open.iter().position(|(pitch, ..)| *pitch == key.as_int()) {
                    let (pitch, start, velocity) = open.remove(index);
                    notes.push(RawNote { pitch, start, length: (time - start).max(1), velocity });
                }
            }
            _ => {}
        }
    }
    notes
}

/// A controller event in file ticks.
#[derive(Debug, PartialEq)]
pub(super) struct RawController {
    pub time: u64,
    pub kind: ControllerKind,
    pub value: f32,
}

/// Controller events of a file track in order (all channels together, like notes). CC 120–127 are channel mode
/// messages, not lanes.
pub(super) fn extract_controllers(track: &[TrackEvent]) -> Vec<RawController> {
    let mut time = 0u64;
    let mut out = Vec::new();
    for event in track {
        time += u64::from(event.delta.as_int());
        let TrackEventKind::Midi { message, .. } = event.kind else {
            continue;
        };
        let (kind, value) = match message {
            MidiMessage::Controller { controller, value } if controller.as_int() <= MAX_CC_NUMBER => {
                (ControllerKind::Cc { number: controller.as_int() }, from_7bit(value.as_int()))
            }
            MidiMessage::PitchBend { bend } => (ControllerKind::PitchBend, bend_from_14bit(bend.0.as_int())),
            MidiMessage::ChannelAftertouch { vel } => (ControllerKind::Aftertouch, from_7bit(vel.as_int())),
            _ => continue,
        };
        out.push(RawController { time, kind, value });
    }
    out
}

/// Controller lanes in project ticks: points past `limit` are dropped, and so are consecutive repeats of the same value
/// (files have many, and the value holds anyway).
pub(super) fn imported_controllers(raw: &[RawController], per_beat: u64, limit: u32) -> Vec<(ControllerKind, Vec<ControllerPoint>)> {
    let mut lanes: Vec<(ControllerKind, Vec<ControllerPoint>)> = Vec::new();
    for event in raw {
        let tick = to_project_ticks(event.time, per_beat);
        if tick >= limit {
            continue;
        }
        let index = if let Some(index) = lanes.iter().position(|(kind, _)| *kind == event.kind) {
            index
        } else {
            lanes.push((event.kind, Vec::new()));
            lanes.len() - 1
        };
        let points = &mut lanes[index].1;
        if points.last().is_none_or(|last| last.value != event.value) {
            points.push(ControllerPoint { tick, value: event.value });
        }
    }
    lanes
}

pub(super) fn ticks_per_beat(timing: Timing) -> Result<u64, ContractError> {
    match timing {
        Timing::Metrical(ppq) if ppq.as_int() > 0 => Ok(u64::from(ppq.as_int())),
        _ => Err(midi::SMPTE_UNSUPPORTED.err()),
    }
}

/// File ticks → project ticks. Saturating rather than wrapping: a huge time in the file
/// must not turn into a small one and land inside the clip.
pub(super) fn to_project_ticks(ticks: u64, per_beat: u64) -> u32 {
    u32::try_from(ticks.saturating_mul(u64::from(PPQ)) / per_beat).unwrap_or(u32::MAX)
}

/// File notes in project ticks; anything starting past `limit` is dropped, lengths are cut at it.
pub(super) fn imported_notes(raw: &[RawNote], per_beat: u64, limit: u32) -> Vec<NewNote> {
    raw.iter()
        .filter_map(|n| {
            let start = to_project_ticks(n.start, per_beat);
            (start < limit).then(|| NewNote {
                pitch: n.pitch,
                start_tick: start,
                length_ticks: to_project_ticks(n.length, per_beat).clamp(1, limit - start),
                velocity: n.velocity.clamp(1, 127),
            })
        })
        .collect()
}
