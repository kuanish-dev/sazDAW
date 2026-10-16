//! `audio.*` methods: audio devices, their selection, stream status and input level (`AudioDevices`), the master
//! window for the visualizer (`EngineStats`), the watched nodes' windows for their pictures (`NodeScope`), the live
//! master meter (`MasterMeter`) and the mono check (`MonoCheck`).

use daw_contracts::{AudioDeviceInfo, AudioSettings, NodeCursor, NodeId};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::RpcTable;

#[derive(Serialize)]
struct Devices {
    outputs: Vec<AudioDeviceInfo>,
    inputs: Vec<AudioDeviceInfo>,
}

#[derive(Deserialize, JsonSchema)]
struct Watch {
    on: bool,
}

#[derive(Deserialize, JsonSchema)]
struct WatchNodes {
    nodes: Vec<NodeId>,
}

#[derive(Deserialize, JsonSchema)]
struct NodeWindows {
    /// What the reader already has: the `node`, `watch` and `end` of its previous windows.
    #[serde(default)]
    seen: Vec<NodeCursor>,
}

pub(crate) fn register(table: &mut RpcTable) {
    table.add("audio.devices", "Lists the audio output and input devices.", |c, ()| {
        Ok(Devices { outputs: c.audio.outputs(), inputs: c.audio.inputs() })
    });
    table.add("audio.settings", "Returns the selected devices, buffer size and recording latency.", |c, ()| Ok(c.audio.settings()));
    table.add("audio.status", "Returns the audio state: whether sound is running, the sample rate and the error.", |c, ()| {
        Ok(c.audio.status())
    });
    table.add(
        "audio.apply",
        "Applies audio settings (devices, buffer, recording latency); if the device fails to open, returns an error and sound stays on the previous device.",
        |c, p: AudioSettings| c.audio.apply(p),
    );
    table.add("audio.watchInput", "Turns input level monitoring on or off.", |c, p: Watch| c.audio.watch_input(p.on));
    table.add("audio.inputLevel", "Returns the current input level.", |c, ()| Ok(c.audio.input_level()));
    table.add("audio.watchMaster", "Turns capturing of the master output for the visualizer on or off.", |c, p: Watch| {
        c.stats.watch_master(p.on);
        Ok(())
    });
    table.add(
        "audio.masterWindow",
        "Returns the latest 2048 frames of the master output in mono and their sample rate (empty while not watched).",
        |c, ()| Ok(c.stats.master_window()),
    );
    table.add(
        "audio.watchMasterMeter",
        "Starts or stops the live master meter (loudness, true peak, phase); starting begins a fresh measurement.",
        |c, p: Watch| {
            c.master_meter.watch(p.on);
            Ok(())
        },
    );
    table.add(
        "audio.masterMeter",
        "Returns the live master meter: momentary (400 ms), short-term (3 s) and integrated LUFS, loudness range (LU), true peak of the last 400 ms and held since the reset (dBTP), whether it clipped, phase correlation (-1..1), seconds measured, and the newest stereo frames for a vectorscope; null while not watched.",
        |c, ()| Ok(c.master_meter.reading()),
    );
    table.add(
        "audio.resetMasterMeter",
        "Starts the integrated loudness, the loudness range and the held true peak over (playback from the song start does it too).",
        |c, ()| {
            c.master_meter.reset();
            Ok(())
        },
    );
    table.add(
        "audio.setMono",
        "Sums the master to mono for listening (both speakers get (L + R) / 2) or back to stereo: a monitoring mode, not saved with the project and not exported; the meters keep measuring the stereo mix.",
        |c, p: Watch| {
            c.mono.set_mono(p.on);
            Ok(())
        },
    );
    table.add(
        "audio.watchNodes",
        "Replaces the graph nodes whose sound is captured for their live pictures (an effect editor, a signal path): at most 16; a node that stays keeps its window, a new one starts from silence, an empty list stops capturing.",
        |c, p: WatchNodes| {
            c.node_scope.watch_nodes(&p.nodes);
            Ok(())
        },
    );
    table.add(
        "audio.nodeWindows",
        "Returns, for each watched node, its input (before the effect or fader) and output in mono, paired frame by frame, with its watch number, frame count and sample rate: only the frames after a cursor in `seen` from the same watch, otherwise the latest 4096.",
        |c, p: NodeWindows| Ok(c.node_scope.node_windows(&p.seen)),
    );
}
