//! `graph.*` methods: node JSON, connections, inserting and removing effects.
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    reason = "integration tests: a failure with a clear message is the check itself"
)]

use crate::common::*;

#[test]
fn project_graph_json_has_tagged_node_kinds() {
    let session = build_session(false);
    let (table, c) = (RpcTable::new(), &session.controls);
    add_synth(&table, c);
    // An audio track gives a source node of kind `audio`.
    call(&table, c, "tracks.add", json!({ "name": "Audio", "kind": null, "defaultPitch": 60 }));
    let bus = call(&table, c, "graph.addBus", json!({ "name": "Bass", "x": 10.0, "y": 20.0 }));
    let effect = call(&table, c, "graph.addEffect", json!({ "effect": "reverb", "x": 1.0, "y": 2.0 }));

    let graph = call(&table, c, "project.get", Value::Null)["graph"].clone();
    let nodes = graph["nodes"].as_array().unwrap();
    let kind_of = |id: &Value| nodes.iter().find(|n| n["id"] == *id).unwrap()["kind"].clone();
    assert_eq!(nodes[0]["id"], 0);
    assert_eq!(nodes[0]["kind"]["type"], "master");
    assert!(nodes[0]["kind"]["volume"].is_number());
    let types: Vec<&str> = nodes.iter().map(|n| n["kind"]["type"].as_str().unwrap()).collect();
    assert!(types.contains(&"audio") && types.contains(&"instrument"));
    let bus_node = nodes.iter().find(|n| n["id"] == bus).unwrap();
    assert_eq!(bus_node["name"], "Bass");
    assert_eq!(bus_node["x"], 10.0);
    assert_eq!(kind_of(&bus)["type"], "bus");
    for key in ["volume", "pan", "mute", "solo"] {
        assert!(!kind_of(&bus)[key].is_null(), "a bus has {key}");
    }
    let effect_kind = kind_of(&effect);
    assert_eq!(effect_kind["type"], "effect");
    assert_eq!(effect_kind["effect"], "reverb");
    assert_eq!(effect_kind["bypass"], false);
    assert!(effect_kind["params"].is_object());

    let edge = graph["edges"].as_array().unwrap().iter().find(|e| e["from"] == bus).expect("bus → master");
    assert_eq!(edge["to"], 0);
    assert!(edge["level"].is_number() && edge["sidechain"] == false);
}

#[test]
fn channel_utility_controls_travel_in_the_node_json() {
    let session = build_session(false);
    let (table, c) = (RpcTable::new(), &session.controls);
    let bus = call(&table, c, "graph.addBus", json!({ "name": "Drums", "x": 0.0, "y": 0.0 }));
    let kind = |table: &RpcTable| {
        let graph = call(table, c, "project.get", Value::Null)["graph"].clone();
        graph["nodes"].as_array().unwrap().iter().find(|n| n["id"] == bus).unwrap()["kind"].clone()
    };
    let fresh = kind(&table);
    assert_eq!(fresh["polarity"], json!({ "invertLeft": false, "invertRight": false }));
    assert_eq!((fresh["width"].as_f64(), fresh["mono"].as_bool(), fresh["delayMs"].as_f64()), (Some(1.0), Some(false), Some(0.0)));

    call(&table, c, "graph.setPolarity", json!({ "id": bus, "invertLeft": true, "invertRight": false }));
    call(&table, c, "graph.setWidth", json!({ "id": bus, "width": 1.5 }));
    call(&table, c, "graph.setMono", json!({ "id": bus, "mono": true }));
    call(&table, c, "graph.setDelay", json!({ "id": bus, "ms": -2.5 }));
    let set = kind(&table);
    assert_eq!(set["polarity"], json!({ "invertLeft": true, "invertRight": false }));
    assert_eq!((set["width"].as_f64(), set["mono"].as_bool(), set["delayMs"].as_f64()), (Some(1.5), Some(true), Some(-2.5)));

    let refused = table.call(c, "graph.setWidth", json!({ "id": 0, "width": 1.0 })).unwrap_err();
    assert!(refused.is(codes::graph::NO_UTILITY), "the master has no utility controls: {refused}");
    call(&table, c, "automation.addLane", json!({ "target": { "type": "nodeWidth", "node": bus } }));
}

#[test]
fn graph_add_bus_returns_id_and_connect_changes_edges() {
    let session = build_session(false);
    let (table, c) = (RpcTable::new(), &session.controls);
    let a = call(&table, c, "graph.addBus", json!({ "name": "A", "x": 0.0, "y": 0.0 }));
    let b = call(&table, c, "graph.addBus", json!({ "name": "B", "x": 0.0, "y": 0.0 }));
    assert!(a.is_number() && b.is_number() && a != b);

    call(&table, c, "graph.connect", json!({ "from": a, "to": b, "level": 0.5, "sidechain": false }));
    let has_edge = |level: f64| {
        let graph = call(&table, c, "project.get", Value::Null)["graph"].clone();
        graph["edges"].as_array().unwrap().iter().any(|e| e["from"] == a && e["to"] == b && e["level"] == level)
    };
    assert!(has_edge(0.5));
    call(&table, c, "graph.connect", json!({ "from": a, "to": b, "level": 0.25, "sidechain": false }));
    assert!(has_edge(0.25), "connecting again changes the level");
    call(&table, c, "graph.disconnect", json!({ "from": a, "to": b, "sidechain": false }));
    assert!(!has_edge(0.25));

    call(&table, c, "graph.renameNode", json!({ "id": a, "name": "New" }));
    call(&table, c, "graph.moveNode", json!({ "id": a, "x": 5.0, "y": 6.0 }));
    call(&table, c, "graph.removeNode", json!({ "id": b }));
    let graph = call(&table, c, "project.get", Value::Null)["graph"].clone();
    assert!(graph["nodes"].as_array().unwrap().iter().all(|n| n["id"] != b));
}

#[test]
fn graph_connect_errors_carry_their_codes() {
    let session = build_session(false);
    let (table, c) = (RpcTable::new(), &session.controls);
    let bus = call(&table, c, "graph.addBus", json!({ "name": "A", "x": 0.0, "y": 0.0 }));
    let effect = call(&table, c, "graph.addEffect", json!({ "effect": "reverb", "x": 0.0, "y": 0.0 }));

    // Bus → effect is fine (a compressor on a bus); back into the bus after that is a loop.
    call(&table, c, "graph.connect", json!({ "from": bus, "to": effect, "level": 1.0, "sidechain": false }));
    let back = table.call(c, "graph.connect", json!({ "from": effect, "to": bus, "level": 1.0, "sidechain": false })).unwrap_err();
    assert!(back.is(codes::graph::EDGE_MAKES_LOOP), "{back}");
    let master = table.call(c, "graph.connect", json!({ "from": 0, "to": bus, "level": 1.0, "sidechain": false })).unwrap_err();
    assert!(master.is(codes::graph::MASTER_HAS_NO_OUTPUT), "{master}");

    // A loop: effect → effect → the first one.
    let other = call(&table, c, "graph.addEffect", json!({ "effect": "reverb", "x": 0.0, "y": 0.0 }));
    call(&table, c, "graph.connect", json!({ "from": effect, "to": other, "level": 1.0, "sidechain": false }));
    let cycle = table.call(c, "graph.connect", json!({ "from": other, "to": effect, "level": 1.0, "sidechain": false })).unwrap_err();
    assert!(cycle.is(codes::graph::EDGE_MAKES_LOOP), "{cycle}");

    assert!(table.call(c, "graph.connect", json!({ "from": 0, "to": bus })).unwrap_err().is(codes::rpc::BAD_PARAMS));
    assert!(table.call(c, "graph.removeNode", json!({ "id": 0 })).is_err(), "the master cannot be removed");
}

#[test]
fn graph_insert_effect_set_output_and_remove_keep_path_round_trip() {
    let session = build_session(false);
    let (table, c) = (RpcTable::new(), &session.controls);
    let track = add_synth(&table, c);
    let graph = || project(&table, c)["graph"].clone();
    let edges = || graph()["edges"].as_array().unwrap().clone();
    let source = graph()["nodes"]
        .as_array()
        .unwrap()
        .iter()
        .find(|n| n["kind"]["type"] == "instrument" && n["kind"]["track"] == track)
        .expect("the track's source node")["id"]
        .clone();
    assert!(edges().iter().any(|e| e["from"] == source && e["to"] == 0), "source → master");

    // Inserting an effect: source → effect → master, the direct connection is gone.
    let effect = call(&table, c, "graph.insertEffect", json!({ "after": source, "effect": "reverb", "x": 1.0, "y": 2.0 }));
    assert!(effect.is_number());
    assert!(edges().iter().any(|e| e["from"] == source && e["to"] == effect));
    assert!(edges().iter().any(|e| e["from"] == effect && e["to"] == 0));
    assert!(!edges().iter().any(|e| e["from"] == source && e["to"] == 0));

    // Changing the output: source → bus; the connection to the effect is replaced.
    let bus = call(&table, c, "graph.addBus", json!({ "name": "Bass", "x": 0.0, "y": 0.0 }));
    call(&table, c, "graph.setOutput", json!({ "node": source, "target": bus }));
    assert!(edges().iter().any(|e| e["from"] == source && e["to"] == bus));
    assert!(!edges().iter().any(|e| e["from"] == source && e["to"] == effect));

    // Removing an effect without breaking the path: after routing the output back to the effect and removing it, sound flows on.
    call(&table, c, "graph.setOutput", json!({ "node": source, "target": effect }));
    call(&table, c, "graph.removeEffectKeepPath", json!({ "id": effect }));
    assert!(graph()["nodes"].as_array().unwrap().iter().all(|n| n["id"] != effect));
    assert!(edges().iter().any(|e| e["from"] == source && e["to"] == 0), "the source → master path is restored");
    assert!(edges().iter().all(|e| e["from"] != effect && e["to"] != effect));

    assert!(table.call(c, "graph.setOutput", json!({ "node": source, "target": 9999 })).is_err());
    assert!(table.call(c, "graph.insertEffect", json!({ "after": 9999, "effect": "reverb", "x": 0.0, "y": 0.0 })).is_err());
}

#[test]
fn graph_insert_effect_before_master_takes_its_inputs() {
    let session = build_session(false);
    let (table, c) = (RpcTable::new(), &session.controls);
    add_synth(&table, c);
    let bus = call(&table, c, "graph.addBus", json!({ "name": "Bus", "x": 0.0, "y": 0.0 }));
    let edges = || project(&table, c)["graph"]["edges"].as_array().unwrap().clone();
    let inputs_before = edges().iter().filter(|e| e["to"] == 0).count();
    assert!(inputs_before >= 2, "the source and the bus go into the master");

    let limiter = call(&table, c, "graph.insertEffectBefore", json!({ "node": 0, "effect": "reverb", "x": 1.0, "y": 2.0 }));
    assert!(limiter.is_number());
    let into_master: Vec<_> = edges().into_iter().filter(|e| e["to"] == 0).collect();
    assert_eq!(into_master.len(), 1);
    assert_eq!(into_master[0]["from"], limiter);
    assert_eq!(edges().iter().filter(|e| e["to"] == limiter).count(), inputs_before);
    assert!(edges().iter().any(|e| e["from"] == bus && e["to"] == limiter));

    // A compressor after the bus.
    let comp = call(&table, c, "graph.insertEffect", json!({ "after": bus, "effect": "reverb", "x": 0.0, "y": 0.0 }));
    assert!(edges().iter().any(|e| e["from"] == bus && e["to"] == comp));
    assert!(edges().iter().any(|e| e["from"] == comp && e["to"] == limiter));

    let source =
        project(&table, c)["graph"]["nodes"].as_array().unwrap().iter().find(|n| n["kind"]["type"] == "instrument").unwrap()["id"].clone();
    let err = table.call(c, "graph.insertEffectBefore", json!({ "node": source, "effect": "reverb", "x": 0.0, "y": 0.0 })).unwrap_err();
    assert!(err.is(codes::graph::SOURCE_HAS_NO_INPUT), "{err}");
    assert!(table.call(c, "graph.insertEffectBefore", json!({ "node": 0 })).unwrap_err().is(codes::rpc::BAD_PARAMS));
}

#[test]
fn graph_set_edge_pre_fader_and_edge_level_automation_wire() {
    let session = build_session(false);
    let (table, c) = (RpcTable::new(), &session.controls);
    add_synth(&table, c);
    let bus = call(&table, c, "graph.addBus", json!({ "name": "Reverb", "x": 0.0, "y": 0.0 }));
    let source =
        project(&table, c)["graph"]["nodes"].as_array().unwrap().iter().find(|n| n["kind"]["type"] == "instrument").unwrap()["id"].clone();
    call(&table, c, "graph.connect", json!({ "from": source, "to": bus, "level": 0.5, "sidechain": false }));
    let send = || project(&table, c)["graph"]["edges"].as_array().unwrap().iter().find(|e| e["from"] == source && e["to"] == bus).cloned();
    assert_eq!(send().unwrap()["preFader"], false, "a connection's JSON always has preFader");

    let done = call(&table, c, "graph.setEdgePreFader", json!({ "from": source, "to": bus, "preFader": true }));
    assert_eq!(done, Value::Null);
    assert_eq!(send().unwrap()["preFader"], true);
    let effect = call(&table, c, "graph.insertEffect", json!({ "after": bus, "effect": "reverb", "x": 0.0, "y": 0.0 }));
    let err = table.call(c, "graph.setEdgePreFader", json!({ "from": effect, "to": 0, "preFader": true })).unwrap_err();
    assert!(err.is(codes::graph::PRE_FADER_NEEDS_STRIP), "{err}");

    let lane = call(&table, c, "automation.addLane", json!({ "target": { "type": "edgeLevel", "from": source, "to": bus } }));
    let target = project(&table, c)["automation"][0]["target"].clone();
    assert_eq!(target, json!({ "type": "edgeLevel", "from": source, "to": bus, "sidechain": false }));
    let clip = call(&table, c, "automation.addClip", json!({ "lane": lane, "startTick": 0, "lengthTicks": 960 }));
    call(&table, c, "automation.setPoints", json!({ "lane": lane, "clip": clip, "points": [{ "tick": 0, "value": 0.2 }] }));
    call(&table, c, "graph.disconnect", json!({ "from": source, "to": bus, "sidechain": false }));
    assert!(send().is_none());
    assert_eq!(project(&table, c)["automation"], json!([]), "the lane went away with the connection");
}

#[test]
fn graph_set_sidechain_picks_and_clears_the_key_source() {
    let session = build_session(false);
    let (table, c) = (RpcTable::new(), &session.controls);
    add_synth(&table, c);
    let source =
        project(&table, c)["graph"]["nodes"].as_array().unwrap().iter().find(|n| n["kind"]["type"] == "instrument").unwrap()["id"].clone();
    let bus = call(&table, c, "graph.addBus", json!({ "name": "Bass", "x": 0.0, "y": 0.0 }));
    let comp = call(&table, c, "graph.insertEffect", json!({ "after": bus, "effect": "compressor", "x": 0.0, "y": 0.0 }));
    let keys =
        || project(&table, c)["graph"]["edges"].as_array().unwrap().iter().filter(|e| e["to"] == comp && e["sidechain"] == true).count();
    let switch = || {
        project(&table, c)["graph"]["nodes"].as_array().unwrap().iter().find(|n| n["id"] == comp).unwrap()["kind"]["params"]["sidechain"]
            .clone()
    };

    assert_eq!(call(&table, c, "graph.setSidechain", json!({ "effect": comp, "source": source })), Value::Null);
    assert_eq!((keys(), switch()), (1, json!(1.0)));
    call(&table, c, "graph.setSidechain", json!({ "effect": comp, "source": null }));
    assert_eq!((keys(), switch()), (0, json!(0.0)));

    let reverb = call(&table, c, "graph.addEffect", json!({ "effect": "reverb", "x": 0.0, "y": 0.0 }));
    let err = table.call(c, "graph.setSidechain", json!({ "effect": reverb, "source": source })).unwrap_err();
    assert!(err.is(codes::graph::NO_SIDECHAIN_INPUT), "{err}");
}

#[test]
fn new_effects_take_params_through_rpc() {
    let session = build_session(false);
    let (table, c) = (RpcTable::new(), &session.controls);
    let params_of = |id: &Value| {
        let graph = call(&table, c, "project.get", Value::Null)["graph"].clone();
        graph["nodes"].as_array().unwrap().iter().find(|n| n["id"] == *id).unwrap()["kind"]["params"].clone()
    };

    let reverb =
        call(&table, c, "graph.addEffect", json!({ "effect": "reverb", "params": { "mix": 1.0, "room": 0.9 }, "x": 0.0, "y": 0.0 }));
    let params = params_of(&reverb);
    assert_eq!(params["mix"], 1.0);
    assert!((params["room"].as_f64().unwrap() - 0.9).abs() < 1e-6, "{params}");
    let limiter = call(
        &table,
        c,
        "graph.insertEffectBefore",
        json!({ "node": 0, "effect": "limiter", "params": { "ceiling": -1.0 }, "x": 0.0, "y": 0.0 }),
    );
    assert_eq!(params_of(&limiter)["ceiling"], -1.0);
    let plain = call(&table, c, "graph.addEffect", json!({ "effect": "reverb", "x": 0.0, "y": 0.0 }));
    assert!(params_of(&plain)["mix"].is_number(), "params are optional");

    let error = table.call(c, "graph.addEffect", json!({ "effect": "reverb", "params": { "nope": 1.0 }, "x": 0.0, "y": 0.0 })).unwrap_err();
    assert!(error.is(codes::graph::UNKNOWN_EFFECT_PARAM), "{error}");
}
