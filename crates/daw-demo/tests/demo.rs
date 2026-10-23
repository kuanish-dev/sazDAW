//! The `DemoProject` contract on a real session: the beat is built whole and counts as the initial state, and a
//! failure midway leaves no trace.
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    reason = "integration test: a failure with a clear message is the check itself"
)]

use daw_bootstrap::build_session;
use daw_contracts::{MAX_TRACKS, NewTrack, Params};

#[test]
fn the_demo_is_a_clean_unmodified_project_with_drums_bass_and_effects() {
    let session = build_session(false);
    let c = &session.controls;
    c.demo.fill_demo().unwrap();
    let names: Vec<String> = c.tracks.tracks().into_iter().map(|t| t.name).collect();
    assert_eq!(names, ["Kick", "Snare", "Hi-hat", "Clap", "Synth"]);
    assert_eq!(c.clips.clips().len(), 4, "the clap has no clip");
    assert_eq!(c.automation.lanes().len(), 1);
    assert!(!c.history.can_undo(), "the built beat is the initial state");
    assert!(!c.project.is_dirty());
}

#[test]
fn a_demo_that_fails_midway_leaves_the_project_as_it_was() {
    let session = build_session(false);
    let c = &session.controls;
    let filler = (0..MAX_TRACKS - 2).map(|i| NewTrack { name: format!("Empty {i}"), kind: None, default_pitch: 60, params: Params::new() });
    c.tracks.add_tracks(filler.collect()).unwrap();
    let before = (c.tracks.tracks(), c.graph.graph(), c.history.undo_label());
    assert!(c.demo.fill_demo().is_err());
    assert_eq!((c.tracks.tracks(), c.graph.graph(), c.history.undo_label()), before);
}
