//! The `MarkerControl` contract: the song layout.

use super::*;

fn ticks(rig: &Rig) -> Vec<u32> {
    rig.markers.markers().iter().map(|m| m.tick).collect()
}

#[test]
fn markers_stay_sorted_by_tick_after_add_and_move() {
    let rig = Rig::new();
    let drop = rig.markers.add_marker(BAR_TICKS * 32, "Drop").unwrap();
    let intro = rig.markers.add_marker(0, "  Intro ").unwrap();
    rig.markers.add_marker(BAR_TICKS * 8, "Verse").unwrap();
    assert_eq!(ticks(&rig), [0, BAR_TICKS * 8, BAR_TICKS * 32]);
    assert_eq!(rig.markers.markers()[0].name, "Intro", "the name is trimmed");

    rig.markers.edit_marker(drop, Some(BAR_TICKS * 4), None).unwrap();
    assert_eq!(ticks(&rig), [0, BAR_TICKS * 4, BAR_TICKS * 8]);
    assert_eq!(rig.markers.markers()[1].id, drop);

    rig.markers.edit_marker(intro, None, Some("Opening")).unwrap();
    assert_eq!(rig.markers.markers()[0].name, "Opening");
    assert_eq!(rig.markers.markers()[0].tick, 0, "without a tick the marker does not move");
}

#[test]
fn every_marker_edit_is_one_undo_step() {
    let rig = Rig::new();
    let id = rig.markers.add_marker(0, "Intro").unwrap();
    rig.markers.edit_marker(id, Some(BAR_TICKS), Some("Input")).unwrap();
    rig.markers.remove_marker(id).unwrap();
    assert_eq!(rig.markers.markers(), []);

    assert!(rig.service.undo());
    assert_eq!(rig.markers.markers(), [Marker { id, tick: BAR_TICKS, name: "Input".into() }]);
    assert!(rig.service.undo(), "the move and the name are one step");
    assert_eq!(rig.markers.markers(), [Marker { id, tick: 0, name: "Intro".into() }]);
    assert!(rig.service.undo());
    assert_eq!(rig.markers.markers(), []);

    let id = rig.markers.add_marker(0, "Intro").unwrap();
    let revision = rig.service.revision();
    rig.markers.edit_marker(id, Some(0), Some("Intro")).unwrap();
    assert_eq!(rig.service.revision(), revision, "no change, no undo step");
}

#[test]
fn bad_marker_requests_fail_and_change_nothing() {
    let rig = Rig::new();
    let id = rig.markers.add_marker(0, "Intro").unwrap();
    let other = rig.markers.add_marker(BAR_TICKS, "Verse").unwrap();
    let before = rig.markers.markers();

    use codes::markers::{MARKER_NAME_EMPTY, MARKER_NAME_TOO_LONG, MARKER_NOT_FOUND, MARKER_PLACE_TAKEN, MARKER_TICK_OUT_OF_RANGE};
    assert!(matches!(rig.markers.add_marker(0, "More"), Err(e) if e.is(MARKER_PLACE_TAKEN)), "a taken tick");
    assert!(matches!(rig.markers.edit_marker(other, Some(0), None), Err(e) if e.is(MARKER_PLACE_TAKEN)), "moving onto another marker");
    assert!(matches!(rig.markers.add_marker(BAR_TICKS * 2, "   "), Err(e) if e.is(MARKER_NAME_EMPTY)));
    let long = "я".repeat(MAX_MARKER_NAME + 1);
    assert!(matches!(rig.markers.add_marker(BAR_TICKS * 2, &long), Err(e) if e.is(MARKER_NAME_TOO_LONG)));
    assert!(matches!(rig.markers.add_marker(MAX_CLIP_TICK + 1, "Far"), Err(e) if e.is(MARKER_TICK_OUT_OF_RANGE)));
    assert!(matches!(rig.markers.edit_marker(9_999, Some(5), None), Err(e) if e.is(MARKER_NOT_FOUND)));
    assert!(matches!(rig.markers.remove_marker(9_999), Err(e) if e.is(MARKER_NOT_FOUND)));
    assert!(matches!(rig.markers.edit_marker(id, None, Some("")), Err(e) if e.is(MARKER_NAME_EMPTY)));
    assert_eq!(rig.markers.markers(), before);
}

#[test]
fn markers_are_limited() {
    let rig = Rig::new();
    for i in 0..MAX_MARKERS as u32 {
        rig.markers.add_marker(i * BAR_TICKS, "Section").unwrap();
    }
    let extra = rig.markers.add_marker(MAX_CLIP_TICK, "Extra");
    assert_eq!(extra, Err(codes::markers::TOO_MANY_MARKERS.with("max", MAX_MARKERS)));
}
