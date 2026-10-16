//! Strum patterns and building a chord part via RPC.
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    reason = "integration tests: a failure with a clear message is the check itself"
)]

use crate::common::*;

#[test]
fn strums_are_listed_and_a_chord_part_builds_on_a_new_guitar_track() {
    let session = build_session(false);
    let (table, c) = (RpcTable::new(), &session.controls);

    let strums = call(&table, c, "chords.strums", Value::Null);
    assert_eq!(strums[1], json!({ "id": "six", "name": "Six", "grid": "D.DU.UDU" }));

    // Defaults: from bar zero, one pass, no replacing.
    let report = call(
        &table,
        c,
        "chords.build",
        json!({ "target": { "type": "new", "kind": "guitar" }, "strum": "six", "bars": [["C"], ["G"], ["Am", "F"]], "repeat": 2 }),
    );
    assert_eq!(report["bars"], 6);
    assert_eq!(report["createdTrack"], "Acoustic guitar");
    let project = project(&table, c);
    let clip = &project["clips"][0];
    assert_eq!((clip["track"].clone(), clip["lengthTicks"].clone()), (report["track"].clone(), json!(6 * 3840)));
    assert_eq!(clip["content"]["loopTicks"], 3 * 3840, "the progression loops inside the clip");

    let error = table
        .call(c, "chords.build", json!({ "target": { "type": "new", "kind": "guitar" }, "strum": "six", "bars": [["H"]] }))
        .unwrap_err();
    assert!(error.is(codes::patterns::BAD_CHORD) && error.params().iter().any(|(_, value)| value == "H"), "{error}");
}
