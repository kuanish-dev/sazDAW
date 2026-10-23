//! What changed in the project between two copies of it (`ProjectChanges::between`): the parts that changed, whole. A
//! reader that keeps its own copy of the project (the window, through the feed in `daw-rpc/src/feed.rs`) takes only
//! them after an edit instead of the whole project: a fader moved, one graph node comes, not every note of the song.

use std::collections::HashMap;

use serde::Serialize;

use crate::graph::{Edge, Graph, Node};
use crate::model::{AutomationLane, Clip, Folder, Marker, MidiMapping, Project, SampleRef, Track};
use crate::{Room, ScaleKey, TempoMap};

/// A list of entities with ids, as it changed: the ids of the whole list in its order, and the entities that changed
/// or appeared, whole. An id that is no longer there was removed; an entity not in `changed` is as the reader has it.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ListChanges<T> {
    pub ids: Vec<u32>,
    pub changed: Vec<T>,
}

/// What changed in the project. Each field is a part of `Project` (the graph comes as its `nodes` and `edges`): `None`
/// (absent in JSON) when it did not change, otherwise its new value, whole; a list of entities with ids comes as
/// `ListChanges`. The default is "nothing changed".
#[derive(Debug, Clone, PartialEq, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectChanges {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tempo_map: Option<TempoMap>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tracks: Option<ListChanges<Track>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub clips: Option<ListChanges<Clip>>,
    /// The new loop region (`Some(None)`, `null` in JSON: the region was removed).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub loop_region: Option<Option<(u32, u32)>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub nodes: Option<ListChanges<Node>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub edges: Option<Vec<Edge>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub room: Option<Room>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub automation: Option<ListChanges<AutomationLane>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub samples: Option<ListChanges<SampleRef>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub markers: Option<ListChanges<Marker>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub folders: Option<ListChanges<Folder>>,
    /// The new song key (`Some(None)`, `null` in JSON: the key was removed).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub key: Option<Option<ScaleKey>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub midi_mappings: Option<Vec<MidiMapping>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub next_id: Option<u32>,
}

/// A whole list as changes: every entity counts as changed.
fn whole_list<T: Clone>(items: &[T], id: fn(&T) -> u32) -> ListChanges<T> {
    ListChanges { ids: items.iter().map(id).collect(), changed: items.to_vec() }
}

/// A whole part, if it differs.
fn changed<T: Clone + PartialEq>(old: &T, new: &T) -> Option<T> {
    (old != new).then(|| new.clone())
}

/// A list's changes from `old` to `new`: the entities that are new or differ, by id; `None` when no entity came, went,
/// moved or changed.
fn list_changes<T: Clone + PartialEq>(old: &[T], new: &[T], id: fn(&T) -> u32) -> Option<ListChanges<T>> {
    let before: HashMap<u32, &T> = old.iter().map(|item| (id(item), item)).collect();
    let changed: Vec<T> = new.iter().filter(|item| before.get(&id(item)).is_none_or(|seen| seen != item)).cloned().collect();
    let ids: Vec<u32> = new.iter().map(id).collect();
    let same_order = old.iter().map(id).eq(ids.iter().copied());
    (!same_order || !changed.is_empty()).then_some(ListChanges { ids, changed })
}

/// The list the reader has (`items`) with the changes taken in: in the order of `ids`, each entity from `changed` or,
/// failing that, the reader's own.
fn apply_list<T: Clone>(items: &mut Vec<T>, changes: ListChanges<T>, id: fn(&T) -> u32) {
    let ListChanges { ids, changed } = changes;
    *items = ids.iter().filter_map(|wanted| changed.iter().chain(items.iter()).find(|item| id(item) == *wanted).cloned()).collect();
}

impl ProjectChanges {
    /// The whole project as changes: what a reader that has nothing gets.
    pub fn whole(project: &Project) -> Self {
        let Project {
            name,
            tempo_map,
            tracks,
            clips,
            loop_region,
            graph: Graph { nodes, edges },
            room,
            automation,
            samples,
            markers,
            folders,
            key,
            midi_mappings,
            next_id,
        } = project;
        Self {
            name: Some(name.clone()),
            tempo_map: Some(tempo_map.clone()),
            tracks: Some(whole_list(tracks, |t| t.id)),
            clips: Some(whole_list(clips, |c| c.id)),
            loop_region: Some(*loop_region),
            nodes: Some(whole_list(nodes, |n| n.id)),
            edges: Some(edges.clone()),
            room: Some(*room),
            automation: Some(whole_list(automation, |l| l.id)),
            samples: Some(whole_list(samples, |s| s.id)),
            markers: Some(whole_list(markers, |m| m.id)),
            folders: Some(whole_list(folders, |f| f.id)),
            key: Some(*key),
            midi_mappings: Some(midi_mappings.clone()),
            next_id: Some(*next_id),
        }
    }

    /// What changed from `old` (the copy the reader has) to `new`: taken into `old` (`apply`), it makes `new`.
    pub fn between(old: &Project, new: &Project) -> Self {
        let Project {
            name,
            tempo_map,
            tracks,
            clips,
            loop_region,
            graph: Graph { nodes, edges },
            room,
            automation,
            samples,
            markers,
            folders,
            key,
            midi_mappings,
            next_id,
        } = new;
        Self {
            name: changed(&old.name, name),
            tempo_map: changed(&old.tempo_map, tempo_map),
            tracks: list_changes(&old.tracks, tracks, |t| t.id),
            clips: list_changes(&old.clips, clips, |c| c.id),
            loop_region: changed(&old.loop_region, loop_region),
            nodes: list_changes(&old.graph.nodes, nodes, |n| n.id),
            edges: changed(&old.graph.edges, edges),
            room: changed(&old.room, room),
            automation: list_changes(&old.automation, automation, |l| l.id),
            samples: list_changes(&old.samples, samples, |s| s.id),
            markers: list_changes(&old.markers, markers, |m| m.id),
            folders: list_changes(&old.folders, folders, |f| f.id),
            key: changed(&old.key, key),
            midi_mappings: changed(&old.midi_mappings, midi_mappings),
            next_id: changed(&old.next_id, next_id),
        }
    }

    /// Takes the changes into a reader's copy of the project (the interface does the same in `core/projectChanges.ts`).
    pub fn apply(self, project: &mut Project) {
        let Self {
            name,
            tempo_map,
            tracks,
            clips,
            loop_region,
            nodes,
            edges,
            room,
            automation,
            samples,
            markers,
            folders,
            key,
            midi_mappings,
            next_id,
        } = self;
        fn set<T>(part: &mut T, value: Option<T>) {
            if let Some(value) = value {
                *part = value;
            }
        }
        fn list<T: Clone>(items: &mut Vec<T>, changes: Option<ListChanges<T>>, id: fn(&T) -> u32) {
            if let Some(changes) = changes {
                apply_list(items, changes, id);
            }
        }
        set(&mut project.name, name);
        set(&mut project.tempo_map, tempo_map);
        list(&mut project.tracks, tracks, |t| t.id);
        list(&mut project.clips, clips, |c| c.id);
        set(&mut project.loop_region, loop_region);
        list(&mut project.graph.nodes, nodes, |n| n.id);
        set(&mut project.graph.edges, edges);
        set(&mut project.room, room);
        list(&mut project.automation, automation, |l| l.id);
        list(&mut project.samples, samples, |s| s.id);
        list(&mut project.markers, markers, |m| m.id);
        list(&mut project.folders, folders, |f| f.id);
        set(&mut project.key, key);
        set(&mut project.midi_mappings, midi_mappings);
        set(&mut project.next_id, next_id);
    }
}

#[cfg(test)]
mod tests {
    use super::{ListChanges, ProjectChanges};
    use crate::model::{Marker, Project};
    use crate::{Scale, ScaleKey};

    fn marker(id: u32, tick: u32) -> Marker {
        Marker { id, tick, name: format!("M{id}") }
    }

    fn with_markers(markers: Vec<Marker>) -> Project {
        Project { markers, ..Project::empty() }
    }

    /// `between` taken into `old` makes `new`.
    fn assert_makes(old: &Project, new: &Project) {
        let mut copy = old.clone();
        ProjectChanges::between(old, new).apply(&mut copy);
        assert_eq!(&copy, new);
    }

    #[test]
    fn the_same_project_brings_nothing() {
        let project = with_markers(vec![marker(1, 0)]);
        assert_eq!(ProjectChanges::between(&project, &project.clone()), ProjectChanges::default());
    }

    #[test]
    fn the_whole_project_makes_it_from_nothing() {
        let project = Project { name: "Song".into(), loop_region: Some((0, 3840)), ..with_markers(vec![marker(1, 0), marker(2, 3840)]) };
        let mut copy = Project::empty();
        ProjectChanges::whole(&project).apply(&mut copy);
        assert_eq!(copy, project);
    }

    #[test]
    fn a_changed_entity_comes_alone_with_the_ids_in_order() {
        let old = with_markers(vec![marker(1, 0), marker(2, 960), marker(3, 1920)]);
        let new = with_markers(vec![marker(1, 0), marker(2, 480), marker(3, 1920)]);
        let changes = ProjectChanges::between(&old, &new);
        assert_eq!(changes.markers, Some(ListChanges { ids: vec![1, 2, 3], changed: vec![marker(2, 480)] }));
        assert_eq!(ProjectChanges { markers: None, ..changes }, ProjectChanges::default(), "only the markers changed");
        assert_makes(&old, &new);
    }

    #[test]
    fn added_removed_and_moved_entities_come_as_the_new_ids() {
        let old = with_markers(vec![marker(1, 0), marker(2, 960)]);
        let added = with_markers(vec![marker(1, 0), marker(2, 960), marker(3, 1920)]);
        assert_eq!(ProjectChanges::between(&old, &added).markers, Some(ListChanges { ids: vec![1, 2, 3], changed: vec![marker(3, 1920)] }));
        assert_makes(&old, &added);

        let removed = with_markers(vec![marker(2, 960)]);
        assert_eq!(ProjectChanges::between(&old, &removed).markers, Some(ListChanges { ids: vec![2], changed: vec![] }));
        assert_makes(&old, &removed);

        let moved = with_markers(vec![marker(2, 960), marker(1, 0)]);
        assert_eq!(ProjectChanges::between(&old, &moved).markers, Some(ListChanges { ids: vec![2, 1], changed: vec![] }));
        assert_makes(&old, &moved);
    }

    #[test]
    fn whole_parts_come_when_they_differ() {
        let old = Project { loop_region: Some((0, 3840)), ..Project::empty() };
        let new =
            Project { name: "Other".into(), loop_region: None, key: Some(ScaleKey { root: 5, scale: Scale::Minor }), ..Project::empty() };
        let changes = ProjectChanges::between(&old, &new);
        assert_eq!(changes.name.as_deref(), Some("Other"));
        assert_eq!(changes.key, Some(new.key), "a key that was set comes whole");
        assert_eq!(changes.loop_region, Some(None), "a removed loop region comes as null");
        assert_eq!(changes.edges, None, "the connections did not change");
        assert_makes(&old, &new);
    }
}
