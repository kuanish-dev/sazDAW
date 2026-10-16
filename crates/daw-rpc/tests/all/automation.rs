//! `automation.*`: clips, shapes over a pattern range, curves in points, the live write mode and touches.
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    reason = "integration tests: a failure with a clear message is the check itself"
)]

use crate::common::*;

#[test]
fn draw_shape_takes_flat_params_and_points_keep_their_curve() {
    let session = build_session(false);
    let (table, c) = (RpcTable::new(), &session.controls);
    let lane = call(&table, c, "automation.addLane", json!({ "target": { "type": "nodeVolume", "node": 0 } }));
    let clip = call(&table, c, "automation.addClip", json!({ "lane": lane, "startTick": 0, "lengthTicks": 3840 }));
    call(
        &table,
        c,
        "automation.setPoints",
        json!({ "lane": lane, "clip": clip, "points": [{ "tick": 0, "value": 0.2, "curve": 0.5 }, { "tick": 10, "value": 1.0 }] }),
    );
    let points = |table: &RpcTable| project(table, c)["automation"][0]["clips"][0]["points"].clone();
    assert_eq!(points(&table)[0]["curve"], json!(0.5));
    assert_eq!(points(&table)[1]["curve"], json!(0.0), "a missing curve is straight");

    let shape = json!({
        "lane": lane, "clip": clip, "fromTick": 960, "toTick": 2880, "shape": "triangle", "periodTicks": 960, "min": 0.0, "max": 1.0
    });
    call(&table, c, "automation.drawShape", shape);
    let ticks: Vec<Value> = points(&table).as_array().unwrap().iter().map(|p| p["tick"].clone()).collect();
    assert_eq!(ticks, [json!(0), json!(10), json!(960), json!(1440), json!(1920), json!(2400), json!(2880)]);

    let empty =
        json!({ "lane": lane, "clip": clip, "fromTick": 5, "toTick": 5, "shape": "rampUp", "periodTicks": 0, "min": 0.0, "max": 1.0 });
    let err = table.call(c, "automation.drawShape", empty).unwrap_err();
    assert!(err.is(codes::automation::SHAPE_EMPTY_RANGE), "{err}");
}

#[test]
fn clips_are_added_edited_duplicated_and_removed() {
    let session = build_session(false);
    let (table, c) = (RpcTable::new(), &session.controls);
    let lane = call(&table, c, "automation.addLane", json!({ "target": { "type": "nodeVolume", "node": 0 } }));
    let clip = call(&table, c, "automation.addClip", json!({ "lane": lane, "startTick": 3840, "lengthTicks": 1920 }));
    let edited = call(&table, c, "automation.editClip", json!({ "lane": lane, "clip": clip, "lengthTicks": 3840, "loopTicks": 960 }));
    assert_eq!(
        (edited["startTick"].clone(), edited["lengthTicks"].clone(), edited["loopTicks"].clone()),
        (json!(3840), json!(3840), json!(960))
    );
    let copy = call(&table, c, "automation.duplicateClip", json!({ "lane": lane, "clip": clip }));
    assert_eq!(project(&table, c)["automation"][0]["clips"][1]["startTick"], json!(7680), "right after the original");
    call(&table, c, "automation.removeClip", json!({ "lane": lane, "clip": clip }));
    let clips = project(&table, c)["automation"][0]["clips"].clone();
    assert_eq!(clips.as_array().map(Vec::len), Some(1));
    assert_eq!(clips[0]["id"], copy);
    let err = table.call(c, "automation.addClip", json!({ "lane": lane, "startTick": 7000, "lengthTicks": 1000 })).unwrap_err();
    assert!(err.is(codes::automation::CLIP_OVERLAPS), "{err}");
}

#[test]
fn the_write_mode_is_set_read_and_shown_in_the_tick_and_touches_while_stopped_write_nothing() {
    let session = build_session(false);
    let (table, c) = (RpcTable::new(), &session.controls);
    assert_eq!(call(&table, c, "automation.writeMode", Value::Null), json!("off"));
    call(&table, c, "automation.setWriteMode", json!({ "mode": "latch" }));
    assert_eq!(call(&table, c, "automation.writeMode", Value::Null), json!("latch"));
    assert_eq!(daw_rpc::Telemetry::new().poll(c).tick["automationWrite"], json!("latch"));

    let target = json!({ "type": "nodeVolume", "node": 0 });
    call(&table, c, "automation.touch", json!({ "target": target, "value": 0.5 }));
    call(&table, c, "automation.release", json!({ "target": target }));
    call(&table, c, "automation.releaseAll", Value::Null);
    assert_eq!(project(&table, c)["automation"], json!([]), "the transport is stopped: nothing is written");
    call(&table, c, "automation.setWriteMode", json!({ "mode": "off" }));
}

#[test]
fn add_drawn_clip_automates_a_param_in_one_call() {
    let session = build_session(false);
    let (table, c) = (RpcTable::new(), &session.controls);
    let drawn = call(
        &table,
        c,
        "automation.addDrawnClip",
        json!({
            "target": { "type": "nodeVolume", "node": 0 }, "startTick": 0, "lengthTicks": 3840,
            "drawing": { "type": "shape", "shape": "rampUp", "periodTicks": 3840, "min": 0.1, "max": 0.9 }
        }),
    );
    let lanes = project(&table, c)["automation"].clone();
    assert_eq!(drawn["lane"], lanes[0]["id"]);
    assert_eq!(drawn["clip"], lanes[0]["clips"][0]["id"]);
    let points = lanes[0]["clips"][0]["points"].as_array().unwrap().clone();
    assert!(
        (points[0]["value"].as_f64().unwrap() - 0.1).abs() < 1e-6 && (points.last().unwrap()["value"].as_f64().unwrap() - 0.9).abs() < 1e-6
    );

    let points = json!({ "type": "points", "points": [{ "tick": 0, "value": 0.5 }] });
    let next = call(
        &table,
        c,
        "automation.addDrawnClip",
        json!({ "target": { "type": "nodeVolume", "node": 0 }, "startTick": 3840, "lengthTicks": 960, "drawing": points }),
    );
    assert_eq!(next["lane"], drawn["lane"], "the same lane");
}
