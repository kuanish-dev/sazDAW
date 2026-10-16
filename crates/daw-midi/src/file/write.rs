//! Writing a MIDI file: clip notes and controllers in absolute time, event tracks and assembling the file.

use daw_contracts::codes::general;
use daw_contracts::{Clip, ContractError, ControllerKind, NoteContent, PPQ, TempoMap};
use midly::num::{u4, u7, u14, u15, u28};
use midly::{Format, Header, MetaMessage, MidiMessage, PitchBend, Smf, Timing, TrackEvent, TrackEventKind};

use super::tempo::tempo_events;
use crate::values::{bend_to_14bit, to_7bit};

/// A file note in project ticks.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(super) struct FileNote {
    pub start: u32,
    pub length: u32,
    pub pitch: u8,
    pub velocity: u8,
}

/// A file controller point in project ticks.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(super) struct FileController {
    pub tick: u32,
    pub kind: ControllerKind,
    pub value: f32,
}

/// A file track: name, notes and controllers.
pub(super) struct FileTrack {
    pub name: String,
    pub notes: Vec<FileNote>,
    pub controllers: Vec<FileController>,
}

/// A tick that does not fit `u32` saturates: wrapping would put the note at the project start.
fn saturating_tick(ticks: u64) -> u32 {
    u32::try_from(ticks).unwrap_or(u32::MAX)
}

/// The clip's notes with content repeats unrolled: time from the project start, cut at the clip end.
pub(super) fn unrolled(clip: &Clip, content: &NoteContent) -> Vec<FileNote> {
    let loop_ticks = u64::from(content.loop_ticks.max(1));
    let (offset, length) = (u64::from(clip.offset_ticks), u64::from(clip.length_ticks));
    let mut out = Vec::new();
    let mut cycle = 0u64;
    // Repeat k starts in the clip at k·loop − offset.
    while cycle * loop_ticks < length + offset {
        for note in &content.notes {
            let start = (cycle * loop_ticks + u64::from(note.start_tick)).checked_sub(offset);
            let Some(start) = start.filter(|start| *start < length) else {
                continue;
            };
            let end = (start + u64::from(note.length_ticks)).min(length);
            out.push(FileNote {
                start: saturating_tick(u64::from(clip.start_tick) + start),
                length: saturating_tick(end - start),
                pitch: note.pitch,
                velocity: note.velocity,
            });
        }
        cycle += 1;
    }
    out
}

/// The clip's controller points with content repeats unrolled: time from the project start, only inside the clip.
pub(super) fn unrolled_controllers(clip: &Clip, content: &NoteContent) -> Vec<FileController> {
    let loop_ticks = u64::from(content.loop_ticks.max(1));
    let (offset, length) = (u64::from(clip.offset_ticks), u64::from(clip.length_ticks));
    let mut out = Vec::new();
    let mut cycle = 0u64;
    while cycle * loop_ticks < length + offset {
        for lane in &content.controllers {
            for point in lane.points.iter().filter(|p| u64::from(p.tick) < loop_ticks) {
                let start = (cycle * loop_ticks + u64::from(point.tick)).checked_sub(offset);
                if let Some(start) = start.filter(|start| *start < length) {
                    out.push(FileController {
                        tick: saturating_tick(u64::from(clip.start_tick) + start),
                        kind: lane.kind,
                        value: point.value,
                    });
                }
            }
        }
        cycle += 1;
    }
    out
}

/// The clip content as it is (no repeats): notes and controller points.
pub(super) fn raw_content(name: &str, content: &NoteContent) -> FileTrack {
    let notes =
        content.notes.iter().map(|n| FileNote { start: n.start_tick, length: n.length_ticks, pitch: n.pitch, velocity: n.velocity });
    let controllers = content
        .controllers
        .iter()
        .flat_map(|lane| lane.points.iter().map(|p| FileController { tick: p.tick, kind: lane.kind, value: p.value }));
    FileTrack { name: name.to_string(), notes: notes.collect(), controllers: controllers.collect() }
}

/// The MIDI message of a controller.
fn controller_message(kind: ControllerKind, value: f32) -> MidiMessage {
    match kind {
        ControllerKind::Cc { number } => MidiMessage::Controller { controller: u7::new(number), value: u7::new(to_7bit(value)) },
        ControllerKind::PitchBend => MidiMessage::PitchBend { bend: PitchBend(u14::new(bend_to_14bit(value))) },
        ControllerKind::Aftertouch => MidiMessage::ChannelAftertouch { vel: u7::new(to_7bit(value)) },
    }
}

/// The channel for track number `index`. Channel 10 (index 9) is drums in General MIDI:
/// a melodic track on it would sound as drums, so it is skipped.
pub(super) fn melodic_channel(index: usize) -> u4 {
    const DRUMS: usize = 9;
    let channel = index % 15;
    let channel = if channel >= DRUMS { channel + 1 } else { channel };
    u4::new(channel as u8)
}

/// The meta track: tempo map and (if set) the author; by the SMF standard the Copyright event goes in the first track.
fn meta_track<'a>(map: &TempoMap, author: &'a str) -> Vec<TrackEvent<'a>> {
    let meta = |delta: u32, message| TrackEvent { delta: u28::new(delta), kind: TrackEventKind::Meta(message) };
    let mut events = Vec::new();
    if !author.is_empty() {
        events.push(meta(0, MetaMessage::Copyright(author.as_bytes())));
    }
    let mut last = 0u32;
    for (tick, message) in tempo_events(map) {
        events.push(meta(tick - last, message));
        last = tick;
    }
    events.push(meta(0, MetaMessage::EndOfTrack));
    events
}

fn note_track(track: &FileTrack, channel: u4) -> Vec<TrackEvent<'_>> {
    // (tick, order, message): on one tick note offs first, then controllers (bend and pedal already apply to the note),
    // then note ons.
    let mut timeline: Vec<(u32, u8, MidiMessage)> = Vec::new();
    for note in &track.notes {
        timeline.push((note.start, 2, MidiMessage::NoteOn { key: u7::new(note.pitch), vel: u7::new(note.velocity) }));
        timeline.push((note.start.saturating_add(note.length), 0, MidiMessage::NoteOff { key: u7::new(note.pitch), vel: u7::new(0) }));
    }
    for point in &track.controllers {
        timeline.push((point.tick, 1, controller_message(point.kind, point.value)));
    }
    timeline.sort_by_key(|(tick, order, _)| (*tick, *order));

    let name = TrackEventKind::Meta(MetaMessage::TrackName(track.name.as_bytes()));
    let mut events = vec![TrackEvent { delta: u28::new(0), kind: name }];
    let mut last = 0u32;
    for (tick, _, message) in timeline {
        events.push(TrackEvent { delta: u28::new(tick - last), kind: TrackEventKind::Midi { channel, message } });
        last = tick;
    }
    events.push(TrackEvent { delta: u28::new(0), kind: TrackEventKind::Meta(MetaMessage::EndOfTrack) });
    events
}

/// The file bytes: the meta track with the tempo map and author (empty means no event), then one track per
/// note and controller track.
pub(super) fn write_file(map: &TempoMap, author: &str, named: &[FileTrack]) -> Result<Vec<u8>, ContractError> {
    let mut tracks = vec![meta_track(map, author)];
    for (index, track) in named.iter().enumerate() {
        tracks.push(note_track(track, melodic_channel(index)));
    }
    let smf = Smf { header: Header::new(Format::Parallel, Timing::Metrical(u15::new(PPQ as u16))), tracks };
    let mut bytes = Vec::new();
    smf.write_std(&mut bytes).map_err(|e| general::INTERNAL.with("details", e))?;
    Ok(bytes)
}
