//! `tracks.*` methods: thin adapters over `TrackControl`.

use daw_contracts::{MonitorMode, NewTrack, Params, TrackId};
use schemars::JsonSchema;
use serde::Deserialize;

use crate::RpcTable;
use crate::params::{Id, IdName};

#[derive(Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
struct Add {
    name: String,
    /// `null` for an audio track.
    kind: Option<String>,
    default_pitch: u8,
}

#[derive(Deserialize, JsonSchema)]
struct AddMany {
    tracks: Vec<NewTrack>,
}

#[derive(Deserialize, JsonSchema)]
struct SetParam {
    id: TrackId,
    name: String,
    value: f32,
}

#[derive(Deserialize, JsonSchema)]
struct SetInstrument {
    id: TrackId,
    kind: String,
    /// Param values of the new kind on top of the defaults (e.g. `sample` for the sampler).
    #[serde(default)]
    params: Params,
    /// New track name; without it the name stays.
    #[serde(default)]
    name: Option<String>,
}

#[derive(Deserialize, JsonSchema)]
struct SetMonitor {
    track: TrackId,
    /// `off` never monitors, `on` always monitors, `auto` only while recording is armed on the track.
    mode: MonitorMode,
}

#[derive(Deserialize, JsonSchema)]
struct SetColor {
    id: TrackId,
    /// One of red, orange, amber, yellow, lime, green, teal, cyan, blue, indigo, violet, pink; `null` for the color by row number.
    color: Option<String>,
}

#[derive(Deserialize, JsonSchema)]
struct SetHeight {
    id: TrackId,
    /// Pixels, 36..=400; `null` for the height from the interface setting.
    height: Option<u16>,
}

#[derive(Deserialize, JsonSchema)]
struct Move {
    id: TrackId,
    index: usize,
}

pub(crate) fn register(table: &mut RpcTable) {
    table.add(
        "tracks.add",
        "Creates a track: an instrument of a catalog kind (or null for an audio track), a name and the step-input pitch.",
        |c, p: Add| c.tracks.add_track(&p.name, p.kind.as_deref(), p.default_pitch),
    );
    table.add("tracks.addMany", "Creates several tracks in one undo step, with instrument params.", |c, p: AddMany| {
        c.tracks.add_tracks(p.tracks)
    });
    table.add("tracks.remove", "Removes a track together with its clips, graph node and automation.", |c, p: Id| {
        c.tracks.remove_track(p.id)
    });
    table.add(
        "tracks.move",
        "Moves a track to a position in the list (0 is the top); folder tracks stay together (folders.setTracks changes a folder's contents).",
        |c, p: Move| c.tracks.move_track(p.id, p.index),
    );
    table.add("tracks.rename", "Renames a track (and the label of its graph node).", |c, p: IdName| c.tracks.rename_track(p.id, &p.name));
    table.add(
        "tracks.setInstrument",
        "Changes the instrument kind of a track (params are the defaults plus params, the name is optional, notes stay).",
        |c, p: SetInstrument| c.tracks.set_instrument(p.id, &p.kind, p.params, p.name.as_deref()),
    );
    table.add("tracks.setParam", "Sets a param of a track's instrument (the value is clamped to its range).", |c, p: SetParam| {
        c.tracks.set_instrument_param(p.id, &p.name, p.value)
    });
    table.add(
        "tracks.setMonitor",
        "Sets input monitoring on an audio track: off, on or auto (only while recording is armed); the input goes through the track's fader and effects. With speakers, feedback is possible.",
        |c, p: SetMonitor| c.tracks.set_monitor(p.track, p.mode),
    );
    table.add(
        "tracks.setColor",
        "Sets a track's color on the timeline and in the mixer (a palette name), or null for the color by row number.",
        |c, p: SetColor| c.tracks.set_color(p.id, p.color.as_deref()),
    );
    table.add(
        "tracks.setHeight",
        "Sets a track's own row height on the timeline in pixels (36..=400), or null to follow the interface setting.",
        |c, p: SetHeight| c.tracks.set_height(p.id, p.height),
    );
}
