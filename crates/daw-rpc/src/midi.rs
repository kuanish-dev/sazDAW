//! `midi.*` methods: MIDI files (`MidiFileIo`), live input from devices (`MidiInputControl`) and controller knobs on
//! params (`MidiMappingControl`, MIDI learn).

use std::path::PathBuf;

use daw_contracts::{ClipId, MidiMapping, ParamTarget, TrackId};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::RpcTable;
use crate::params::Path;

#[derive(Deserialize, JsonSchema)]
struct ClipFile {
    clip: ClipId,
    path: PathBuf,
}

#[derive(Deserialize, JsonSchema)]
struct Connect {
    device: String,
    target: TrackId,
}

#[derive(Deserialize, JsonSchema)]
struct Target {
    target: TrackId,
}

#[derive(Deserialize, JsonSchema)]
struct LearnTarget {
    target: ParamTarget,
}

/// The mappings and what is being learned now.
#[derive(Serialize)]
struct Mappings {
    mappings: Vec<MidiMapping>,
    learning: Option<ParamTarget>,
}

pub(crate) fn register(table: &mut RpcTable) {
    table.add("midi.exportProject", "Saves all note tracks of the project and the tempo map to a MIDI file.", |c, p: Path| {
        c.midi_files.export_project(&p.path)
    });
    table.add("midi.exportClip", "Saves a note clip to a MIDI file.", |c, p: ClipFile| c.midi_files.export_clip(p.clip, &p.path));
    table.add("midi.importInto", "Replaces a clip's notes with the notes from a MIDI file.", |c, p: ClipFile| {
        c.midi_files.import_into(p.clip, &p.path)
    });
    table.add(
        "midi.importTempo",
        "Replaces the song's tempo and time signatures (tempo.*) with the tempo and signature events from a MIDI file; one undo step.",
        |c, p: Path| c.midi_files.import_tempo(&p.path),
    );
    table.add("midi.devices", "Lists the MIDI input devices.", |c, ()| Ok(c.midi_input.devices()));
    table.add("midi.connect", "Connects a MIDI input device to a track.", |c, p: Connect| c.midi_input.connect(&p.device, p.target));
    table.add("midi.setTarget", "Sets the track the connected MIDI keyboard plays.", |c, p: Target| Ok(c.midi_input.set_target(p.target)));
    table.add("midi.disconnect", "Disconnects the MIDI device.", |c, ()| Ok(c.midi_input.disconnect()));
    register_learn(table);
}

/// MIDI learn: a controller knob moves a project param (targets as in automation.addLane).
fn register_learn(table: &mut RpcTable) {
    table.add(
        "midi.learn",
        "Arms MIDI learn: the next knob (CC) turned on the connected MIDI device will move the param target \
         (as in automation.addLane: nodeVolume, nodePan, nodeWidth, effectParam, instrumentParam, edgeLevel). A previous mapping \
         of the param is replaced. Mappings are stored in the project (midiMappings).",
        |c, p: LearnTarget| c.midi_mappings.learn(p.target),
    );
    table.add("midi.cancelLearn", "Cancels an armed MIDI learn.", |c, ()| Ok(c.midi_mappings.cancel_learn()));
    table.add(
        "midi.mappings",
        "Returns the MIDI controller knob mappings ({ mappings: [{ cc, channel?, target, min?, max? }], learning: a target or null }).",
        |c, ()| Ok(Mappings { mappings: c.midi_mappings.mappings(), learning: c.midi_mappings.learning() }),
    );
    table.add("midi.removeMapping", "Removes the MIDI controller mapping from the param target.", |c, p: LearnTarget| {
        c.midi_mappings.remove_mapping(&p.target)
    });
}
