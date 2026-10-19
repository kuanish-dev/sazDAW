//! What changed in the project after a revision (`ProjectView::changes_since`): the parts that changed, whole. A reader
//! that keeps its own copy of the project (the interface) takes only them after an edit instead of the whole project:
//! a fader moved, one graph node comes, not every note of the song.

use serde::Serialize;

use crate::graph::{Edge, Graph, Node};
use crate::model::{AutomationLane, Clip, Folder, Marker, MidiMapping, Project, SampleRef, Track};
use crate::{Room, TempoMap};

/// A list of entities with ids, as it changed: the ids of the whole list in its order, and the entities that changed
/// or appeared, whole. An id that is no longer there was removed; an entity not in `changed` is as the reader has it.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ListChanges<T> {
    pub ids: Vec<u32>,
    pub changed: Vec<T>,
}

/// What changed in the project after a revision. Each field is a part of `Project` (the graph comes as its `nodes`
/// and `edges`): `None` (absent in JSON) when it did not change, otherwise its new value, whole; a list of entities
/// with ids comes as `ListChanges`. A part may come although it is the same as the reader has (the whole project, for
/// a reader that has nothing, is all parts): taking it changes nothing.
#[derive(Debug, Clone, PartialEq, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectChanges {
    /// The revision the changes bring the reader to: the `since` of its next call.
    pub revision: u64,
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
    #[serde(skip_serializing_if = "Option::is_none")]
    pub midi_mappings: Option<Vec<MidiMapping>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub next_id: Option<u32>,
}

/// A whole list as changes: every entity counts as changed.
fn whole_list<T: Clone>(items: &[T], id: fn(&T) -> u32) -> ListChanges<T> {
    ListChanges { ids: items.iter().map(id).collect(), changed: items.to_vec() }
}

/// The list the reader has (`items`) with the changes taken in: in the order of `ids`, each entity from `changed` or,
/// failing that, the reader's own.
fn apply_list<T: Clone>(items: &mut Vec<T>, changes: ListChanges<T>, id: fn(&T) -> u32) {
    let ListChanges { ids, changed } = changes;
    *items = ids.iter().filter_map(|wanted| changed.iter().chain(items.iter()).find(|item| id(item) == *wanted).cloned()).collect();
}

impl ProjectChanges {
    /// The whole project as changes: what a reader that has nothing gets.
    pub fn whole(project: &Project, revision: u64) -> Self {
        // Every field by name: a new project field does not compile until it travels here (and in `apply`).
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
            midi_mappings,
            next_id,
        } = project;
        Self {
            revision,
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
            midi_mappings: Some(midi_mappings.clone()),
            next_id: Some(*next_id),
        }
    }

    /// Takes the changes into a reader's copy of the project (the interface does the same in `domain/projectChanges.ts`).
    pub fn apply(self, project: &mut Project) {
        let Self {
            revision: _,
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
        set(&mut project.midi_mappings, midi_mappings);
        set(&mut project.next_id, next_id);
    }
}
