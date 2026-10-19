//! Contract tests of the project entities, one file per job: tracks, clips, notes, markers, folders, graph, the channel
//! utility, the stage, automation, history, sends, samples, files, DSP sync and batch operations. Shared helpers live
//! here.

mod automation;
mod batch;
mod bounce;
mod changes;
mod channel_utility;
mod clear;
mod clip_processing;
mod clips;
mod controllers;
mod dsp_sync;
mod files;
mod folders;
mod graph;
mod graph_insert;
mod groove;
mod history;
mod markers;
mod midi_mappings;
mod note_effects;
mod note_transforms;
mod notes;
mod plugin_states;
mod samples;
mod sends;
mod sidechain;
mod stage;
mod takes;
mod tempo;
mod tracks;
mod variation;
mod warp;

pub(super) use std::path::{Path, PathBuf};

pub(super) use daw_contracts::*;

pub(super) use crate::testing::{FakeInstruments, Rig};

pub(super) fn note(pitch: u8, start: u32, length: u32) -> NewNote {
    NewNote { pitch, start_tick: start, length_ticks: length, velocity: 100 }
}

/// An instrument track with one empty note clip (one bar from zero).
pub(super) fn rig_with_clip() -> (Rig, TrackId, ClipId) {
    let rig = Rig::new();
    let track = rig.tracks.add_track("Synth", Some("synth"), 60).unwrap();
    let clip = rig.clips.add_note_clip(NewNoteClip::empty(track, 0, None)).unwrap().id;
    (rig, track, clip)
}

pub(super) fn source(rig: &Rig, track: TrackId) -> NodeId {
    rig.graph.graph().source_of(track).expect("the track's source node").id
}

pub(super) fn master_volume(rig: &Rig) -> f32 {
    match rig.graph.graph().node(MASTER).unwrap().kind {
        NodeKind::Master { volume } => volume,
        _ => unreachable!("node 0 is the master"),
    }
}

pub(super) fn edge_count(rig: &Rig, from: NodeId, to: NodeId) -> usize {
    rig.graph.graph().edges.iter().filter(|e| e.from == from && e.to == to).count()
}

pub(super) fn effect_params(rig: &Rig, id: NodeId) -> Params {
    match rig.graph.graph().node(id).unwrap().kind.clone() {
        NodeKind::Effect { params, .. } => params,
        _ => unreachable!("the node is not an effect"),
    }
}

pub(super) fn content(rig: &Rig, clip: ClipId) -> NoteContent {
    match rig.clips.clips().into_iter().find(|c| c.id == clip).unwrap().content {
        ClipContent::Notes(content) => content,
        ClipContent::Audio { .. } => unreachable!("the clip is not a note clip"),
    }
}

pub(super) fn loop_ticks(rig: &Rig, clip: ClipId) -> u32 {
    content(rig, clip).loop_ticks
}
