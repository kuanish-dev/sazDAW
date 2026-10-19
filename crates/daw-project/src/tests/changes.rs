//! `ProjectView::changes_since`: the whole project for a reader that has nothing, then only the parts that changed,
//! whatever changed them, so a copy kept from the replies stays equal to the project, also with several readers.

use daw_contracts::{ProjectChanges, ProjectView};

use super::*;

/// A reader's copy of the project kept from the replies, like the interface's.
struct Copy {
    project: Project,
    revision: Option<u64>,
}

impl Copy {
    fn new() -> Self {
        Self { project: Project::empty(), revision: None }
    }

    /// Takes the next reply in and returns it.
    fn read(&mut self, rig: &Rig) -> ProjectChanges {
        let changes = rig.service.changes_since(self.revision);
        self.revision = Some(changes.revision);
        changes.clone().apply(&mut self.project);
        changes
    }

    fn assert_equal(&self, rig: &Rig) {
        assert_eq!(self.project, rig.service.snapshot());
    }
}

#[test]
fn a_reader_with_nothing_gets_the_whole_project() {
    let (rig, _, clip) = rig_with_clip();
    rig.notes.add_note(clip, note(60, 0, 240)).unwrap();
    let mut copy = Copy::new();
    copy.read(&rig);
    copy.assert_equal(&rig);
}

#[test]
fn a_moved_fader_brings_only_its_node_and_nothing_new_brings_nothing() {
    let (rig, _, clip) = rig_with_clip();
    rig.notes.add_note(clip, note(60, 0, 240)).unwrap();
    let mut copy = Copy::new();
    copy.read(&rig);

    rig.graph.set_volume(MASTER, 0.5).unwrap();
    let changes = copy.read(&rig);
    let nodes = changes.nodes.as_ref().expect("the master changed");
    assert_eq!(nodes.changed.iter().map(|n| n.id).collect::<Vec<_>>(), [MASTER], "only the master node");
    assert_eq!(
        ProjectChanges { nodes: None, ..changes.clone() },
        ProjectChanges { revision: changes.revision, ..ProjectChanges::default() }
    );
    copy.assert_equal(&rig);

    let nothing = copy.read(&rig);
    assert_eq!(nothing, ProjectChanges { revision: nothing.revision, ..ProjectChanges::default() });
}

#[test]
fn removals_undo_and_redo_come_like_any_edit() {
    let (rig, track, _) = rig_with_clip();
    let mut copy = Copy::new();
    copy.read(&rig);

    let other = rig.tracks.add_track("Other", Some("synth"), 60).unwrap();
    let added = copy.read(&rig);
    assert_eq!(added.tracks.as_ref().map(|t| t.ids.clone()), Some(vec![track, other]));
    copy.assert_equal(&rig);

    assert!(rig.service.undo());
    let undone = copy.read(&rig);
    let tracks = undone.tracks.as_ref().expect("the track list changed");
    assert_eq!((tracks.ids.clone(), tracks.changed.len()), (vec![track], 0), "the removed track is gone, the other is as it was");
    copy.assert_equal(&rig);

    assert!(rig.service.redo());
    copy.read(&rig);
    copy.assert_equal(&rig);
}

#[test]
fn two_readers_stay_equal_to_the_project_whoever_reads_in_between() {
    let (rig, track, clip) = rig_with_clip();
    let (mut first, mut second) = (Copy::new(), Copy::new());
    first.read(&rig);
    rig.graph.set_volume(MASTER, 0.5).unwrap();
    first.read(&rig);

    // The value goes back to what the first reader saw before, while only the second reader reads.
    rig.graph.set_volume(MASTER, 0.25).unwrap();
    second.read(&rig);
    rig.graph.set_volume(MASTER, 0.5).unwrap();
    rig.notes.add_note(clip, note(64, 240, 240)).unwrap();
    second.read(&rig);
    rig.tracks.rename_track(track, "Lead").unwrap();

    first.read(&rig);
    first.assert_equal(&rig);
    second.read(&rig);
    second.assert_equal(&rig);
}
