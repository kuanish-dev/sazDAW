//! Drum patterns and building a drum part via RPC.
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    reason = "integration tests: a failure with a clear message is the check itself"
)]

use crate::common::*;

#[test]
fn drum_patterns_are_listed_with_readable_grids_and_build_a_part() {
    let session = build_session(false);
    let (table, c) = (RpcTable::new(), &session.controls);

    let all = call(&table, c, "patterns.list", Value::Null);
    assert!(all.as_array().unwrap().len() >= 40, "there should be many patterns");
    let fills = call(&table, c, "patterns.list", json!({ "role": "fill" }));
    assert!(fills.as_array().unwrap().iter().all(|p| p["role"] == "fill" && p["bars"] == 1));
    let rock = call(&table, c, "patterns.list", json!({ "style": "rock" }));
    assert!(rock.as_array().unwrap().iter().all(|p| p["style"] == "rock"));

    let basic = call(&table, c, "patterns.get", json!({ "id": "rock_basic" }));
    assert_eq!(basic["lanes"][0], json!({ "lane": "kick", "grid": "X... .... X... ...." }));
    assert_eq!(basic["lanes"][1]["grid"], ".... X... .... X...");

    // Default params: from bar zero, one repeat, no replacing.
    let report =
        call(&table, c, "drums.build", json!({ "slots": [{ "pattern": "rock_basic", "repeat": 2 }, { "pattern": "fill_snare_roll" }] }));
    assert_eq!(report["bars"], 3);
    assert_eq!(report["createdTracks"], json!(["Kick", "Snare", "Hi-hat"]));
    assert_eq!(report["clips"].as_array().unwrap().len(), 6);

    let error = table.call(c, "drums.build", json!({ "slots": [{ "pattern": "nope" }] })).unwrap_err();
    assert!(error.params().iter().any(|(_, value)| value == "nope"), "{error}");
}
