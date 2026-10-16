//! `stage.*` methods: placing a channel shows in the project JSON, the room round-trips, errors keep their codes.
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    reason = "integration tests: a failure with a clear message is the check itself"
)]

use crate::common::*;

#[test]
fn a_placed_channel_carries_its_place_in_the_project_json() {
    let session = build_session(false);
    let (table, c) = (RpcTable::new(), &session.controls);
    let bus = call(&table, c, "graph.addBus", json!({ "name": "Strings", "x": 0.0, "y": 0.0 }));
    let kind = |table: &RpcTable| {
        let graph = call(table, c, "project.get", Value::Null)["graph"].clone();
        graph["nodes"].as_array().unwrap().iter().find(|n| n["id"] == bus).unwrap()["kind"].clone()
    };
    assert!(kind(&table).get("stage").is_none(), "off the stage: no `stage` field");

    // Behind the listener and a little below the ears.
    call(&table, c, "stage.place", json!({ "node": bus, "pan": 0.5, "depth": -0.75, "height": -0.25 }));
    let placed = kind(&table);
    assert_eq!(placed["pan"], 0.5);
    assert_eq!(placed["stage"], json!({ "depth": -0.75, "height": -0.25 }));

    call(&table, c, "stage.takeOff", json!({ "node": bus }));
    assert!(kind(&table).get("stage").is_none());
}

#[test]
fn the_room_round_trips_and_shows_in_the_project() {
    let session = build_session(false);
    let (table, c) = (RpcTable::new(), &session.controls);
    let hall = json!({ "size": 0.9, "reverb": 0.6, "listening": "headphones" });
    call(&table, c, "stage.setRoom", hall.clone());
    assert_eq!(call(&table, c, "stage.room", Value::Null), hall);
    assert_eq!(call(&table, c, "project.get", Value::Null)["room"], hall);
}

#[test]
fn a_node_that_cannot_stand_on_the_stage_is_refused_with_its_code() {
    let session = build_session(false);
    let (table, c) = (RpcTable::new(), &session.controls);
    let error = table.call(c, "stage.place", json!({ "node": 0, "pan": 0.0, "depth": 0.5, "height": 0.0 })).unwrap_err();
    assert!(error.is(codes::graph::NO_STAGE_PLACE), "the master: {error}");
}
