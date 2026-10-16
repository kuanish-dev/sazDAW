//! The `FolderControl` contract: track folders — membership, the "in a row" order, mute/solo, color; plus the track
//! color (`TrackControl::set_color`).

use super::*;

/// Five synth tracks "a"…"e" in order.
fn rig_with_tracks() -> (Rig, Vec<TrackId>) {
    let rig = Rig::new();
    let ids = ["a", "b", "c", "d", "e"].iter().map(|name| rig.tracks.add_track(name, Some("synth"), 60).unwrap()).collect();
    (rig, ids)
}

fn order(rig: &Rig) -> String {
    rig.tracks.tracks().iter().map(|t| t.name.as_str()).collect()
}

fn members(rig: &Rig, folder: FolderId) -> String {
    rig.tracks.tracks().iter().filter(|t| t.folder == Some(folder)).map(|t| t.name.as_str()).collect()
}

fn strip(rig: &Rig, track: TrackId) -> Strip {
    *rig.graph.graph().node(source(rig, track)).unwrap().kind.strip().unwrap()
}

#[test]
fn create_folder_gathers_its_tracks_at_the_topmost_one_in_one_undo_step() {
    let (rig, t) = rig_with_tracks();
    let folder = rig.folders.create_folder(" Drums ", &[t[3], t[1]]).unwrap();
    assert_eq!(order(&rig), "adbce", "the tracks stand in a row, in list order, at the place of the topmost");
    assert_eq!(members(&rig, folder), "db");
    assert_eq!(rig.folders.folders(), [Folder { id: folder, name: "Drums".into(), collapsed: false, color: None }]);

    assert!(rig.service.undo(), "creating is one step");
    assert_eq!(order(&rig), "abcde");
    assert_eq!(rig.folders.folders(), []);
    assert!(rig.tracks.tracks().iter().all(|t| t.folder.is_none()));
}

#[test]
fn moving_tracks_keeps_folders_contiguous() {
    let (rig, t) = rig_with_tracks();
    let folder = rig.folders.create_folder("Folder", &[t[1], t[2]]).unwrap();
    rig.tracks.move_track(t[2], 4).unwrap();
    assert_eq!(order(&rig), "abcde", "a folder track moved down comes back into the block");
    rig.tracks.move_track(t[2], 0).unwrap();
    assert_eq!(order(&rig), "cbade", "moved above the folder, it lifts the whole folder");
    rig.tracks.move_track(t[4], 1).unwrap();
    assert_eq!(order(&rig), "cbead", "another track does not get inside the block");
    assert_eq!(members(&rig, folder), "cb", "reordering does not change membership");
}

#[test]
fn set_folder_tracks_sets_members_and_order_in_one_step() {
    let (rig, t) = rig_with_tracks();
    let folder = rig.folders.create_folder("Folder", &[t[1], t[2], t[3]]).unwrap();
    rig.folders.set_folder_tracks(folder, &[t[4], t[1], t[3]]).unwrap();
    assert_eq!(members(&rig, folder), "ebd", "the new track went first at the block's place");
    assert_eq!(order(&rig), "aebdc", "the removed \"c\" stays outside the folder next to it");
    assert!(rig.service.undo());
    assert_eq!(order(&rig), "abcde");
    assert_eq!(members(&rig, folder), "bcd");
}

#[test]
fn a_track_lives_in_one_folder_and_empty_folders_disappear() {
    let (rig, t) = rig_with_tracks();
    let first = rig.folders.create_folder("First", &[t[0], t[1]]).unwrap();
    let second = rig.folders.create_folder("Second", &[t[1], t[0]]).unwrap();
    assert_eq!(members(&rig, second), "ba");
    assert!(rig.folders.folders().iter().all(|f| f.id != first), "the first one emptied and went away");

    let lone = rig.folders.create_folder("Lone", &[t[4]]).unwrap();
    rig.tracks.remove_track(t[4]).unwrap();
    assert!(rig.folders.folders().iter().all(|f| f.id != lone), "removing the last track removes the folder");
    rig.tracks.remove_track(t[0]).unwrap();
    assert_eq!(members(&rig, second), "b", "the removed track left the folder");
    assert!(rig.service.undo());
    assert_eq!(members(&rig, second), "ba");
}

#[test]
fn remove_folder_keeps_tracks_in_place() {
    let (rig, t) = rig_with_tracks();
    let folder = rig.folders.create_folder("Folder", &[t[3], t[4]]).unwrap();
    rig.folders.remove_folder(folder).unwrap();
    assert_eq!(order(&rig), "abcde");
    assert!(rig.tracks.tracks().iter().all(|t| t.folder.is_none()));
    assert_eq!(rig.folders.folders(), []);
}

#[test]
fn edit_folder_changes_only_given_fields() {
    let (rig, t) = rig_with_tracks();
    let folder = rig.folders.create_folder("Folder", &[t[0]]).unwrap();
    rig.folders
        .edit_folder(folder, FolderEdit { collapsed: Some(true), color: Some(Some("teal".into())), ..FolderEdit::default() })
        .unwrap();
    let edited = rig.folders.folders()[0].clone();
    assert_eq!((edited.name.as_str(), edited.collapsed, edited.color.as_deref()), ("Folder", true, Some("teal")));
    rig.folders.edit_folder(folder, FolderEdit { name: Some("Bass".into()), color: Some(None), ..FolderEdit::default() }).unwrap();
    let edited = rig.folders.folders()[0].clone();
    assert_eq!((edited.name.as_str(), edited.collapsed, edited.color), ("Bass", true, None));

    let revision = rig.service.revision();
    rig.folders.edit_folder(folder, FolderEdit::default()).unwrap();
    assert_eq!(rig.service.revision(), revision, "an empty edit records no step");
}

#[test]
fn folder_mute_and_solo_set_every_member_in_one_step() {
    let (rig, t) = rig_with_tracks();
    let folder = rig.folders.create_folder("Folder", &[t[1], t[2]]).unwrap();
    rig.folders.set_folder_mute(folder, true).unwrap();
    rig.folders.set_folder_solo(folder, true).unwrap();
    assert!(strip(&rig, t[1]).mute && strip(&rig, t[2]).mute && strip(&rig, t[1]).solo && strip(&rig, t[2]).solo);
    assert!(!strip(&rig, t[0]).mute && !strip(&rig, t[0]).solo, "tracks outside the folder are untouched");
    assert!(rig.service.undo());
    assert!(!strip(&rig, t[1]).solo && !strip(&rig, t[2]).solo, "folder solo is one step");
    assert!(strip(&rig, t[2]).mute);
}

#[test]
fn bad_folder_requests_fail_and_change_nothing() {
    let (rig, t) = rig_with_tracks();
    let folder = rig.folders.create_folder("Folder", &[t[0]]).unwrap();
    let before = rig.service.snapshot();
    use codes::markers::{FOLDER_NAME_EMPTY, FOLDER_NOT_FOUND, FOLDER_TRACK_TWICE, FOLDER_WITHOUT_TRACKS};
    assert!(matches!(rig.folders.create_folder("Empty", &[]), Err(e) if e.is(FOLDER_WITHOUT_TRACKS)));
    assert!(matches!(rig.folders.create_folder("Twice", &[t[1], t[1]]), Err(e) if e.is(FOLDER_TRACK_TWICE)));
    assert!(matches!(rig.folders.create_folder("", &[t[1]]), Err(e) if e.is(FOLDER_NAME_EMPTY)));
    assert!(matches!(rig.folders.create_folder("None", &[9_999]), Err(e) if e.is(codes::tracks::NOT_FOUND)));
    assert!(matches!(rig.folders.set_folder_tracks(folder, &[]), Err(e) if e.is(FOLDER_WITHOUT_TRACKS)));
    assert!(matches!(rig.folders.set_folder_tracks(9_999, &[t[0]]), Err(e) if e.is(FOLDER_NOT_FOUND)));
    let bad_color = FolderEdit { color: Some(Some("#ff0000".into())), ..FolderEdit::default() };
    assert!(matches!(rig.folders.edit_folder(folder, bad_color), Err(e) if e.is(codes::tracks::UNKNOWN_COLOR)));
    assert!(matches!(rig.folders.remove_folder(9_999), Err(e) if e.is(FOLDER_NOT_FOUND)));
    assert!(matches!(rig.folders.set_folder_mute(9_999, true), Err(e) if e.is(FOLDER_NOT_FOUND)));
    assert_eq!(rig.service.snapshot(), before);
}

#[test]
fn track_color_is_a_named_palette_color_and_undoable() {
    let (rig, t) = rig_with_tracks();
    rig.tracks.set_color(t[0], Some("violet")).unwrap();
    assert_eq!(rig.tracks.tracks()[0].color.as_deref(), Some("violet"));
    rig.tracks.set_color(t[0], None).unwrap();
    assert_eq!(rig.tracks.tracks()[0].color, None);
    assert!(rig.service.undo());
    assert_eq!(rig.tracks.tracks()[0].color.as_deref(), Some("violet"));

    assert!(matches!(rig.tracks.set_color(t[0], Some("bordeaux")), Err(e) if e.is(codes::tracks::UNKNOWN_COLOR)));
    assert!(matches!(rig.tracks.set_color(9_999, Some("red")), Err(e) if e.is(codes::tracks::NOT_FOUND)));
    let revision = rig.service.revision();
    rig.tracks.set_color(t[0], Some("violet")).unwrap();
    assert_eq!(rig.service.revision(), revision, "the same color records no step");
}
