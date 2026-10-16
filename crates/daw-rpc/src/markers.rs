//! `markers.*` methods: thin adapters over `MarkerControl` (song sections).

use daw_contracts::MarkerId;
use schemars::JsonSchema;
use serde::Deserialize;

use crate::RpcTable;
use crate::params::Id;

#[derive(Deserialize, JsonSchema)]
struct Add {
    /// Section start in ticks (a 4/4 bar = 3840: bar 33 = 32 × 3840 = 122880).
    tick: u32,
    /// Section name: "Intro", "Verse", "Chorus", "Drop", "Bridge", "Outro"…
    name: String,
}

#[derive(Deserialize, JsonSchema)]
struct Edit {
    id: MarkerId,
    /// New start in ticks; when omitted the marker stays in place.
    #[serde(default)]
    tick: Option<u32>,
    /// New name; when omitted the name stays.
    #[serde(default)]
    name: Option<String>,
}

pub(crate) fn register(table: &mut RpcTable) {
    table.add(
        "markers.add",
        "Creates a song section marker on the timeline (e.g. \"Intro\" at tick 0, \"Drop\" at bar 33 = tick 122880); returns the id. Markers are in project.get → markers by ascending tick and don't affect sound; only one marker per tick.",
        |c, p: Add| c.markers.add_marker(p.tick, &p.name),
    );
    table.add("markers.edit", "Moves a section marker (tick) and/or renames it (name); omitted fields don't change.", |c, p: Edit| {
        c.markers.edit_marker(p.id, p.tick, p.name.as_deref())
    });
    table.add("markers.remove", "Removes a song section marker.", |c, p: Id| c.markers.remove_marker(p.id));
}
