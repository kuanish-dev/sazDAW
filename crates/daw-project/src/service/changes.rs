//! What changed in the project after a revision (`ProjectView::changes_since`). The core does not follow edits one by
//! one: a read compares the model with the copy it handed out last and stamps every part that differs with the current
//! revision, and the reader gets the parts stamped after its own revision. Why at reads and by comparing: every way the
//! model changes (an edit, undo, a group cancelled, another project opened) is covered without any of them saying what
//! it touched, so nothing can be forgotten; and the comparing runs at most as often as the interface reads (about 30
//! times a second), not after each of an AI's hundred edits.
//!
//! Why a reader gets everything it lacks: stamps only grow, and any part that differs between two reads was stamped at
//! or after the later one, so all parts stamped after the reader's revision cover every difference since it, also when
//! other readers read in between.

use std::collections::{HashMap, HashSet};

use daw_contracts::graph::{Edge, Graph, Node};
use daw_contracts::model::{AutomationLane, Clip, Folder, Marker, MidiMapping, SampleRef, Track};
use daw_contracts::{ListChanges, Project, ProjectChanges, Room, TempoMap};

/// A part kept whole: as last handed out, and the revision it last changed at.
struct Whole<T> {
    seen: T,
    at: u64,
}

impl<T: Clone + PartialEq> Whole<T> {
    fn new(value: &T) -> Self {
        Self { seen: value.clone(), at: 0 }
    }

    fn stamp(&mut self, current: &T, revision: u64) {
        if *current != self.seen {
            self.seen = current.clone();
            self.at = revision;
        }
    }

    fn since(&self, since: u64) -> Option<T> {
        (self.at > since).then(|| self.seen.clone())
    }
}

/// A list of entities with ids: the ids in order and each entity as last handed out, with the revisions they last
/// changed at.
struct List<T> {
    id: fn(&T) -> u32,
    ids: Vec<u32>,
    /// When an entity last came, went or moved.
    order_at: u64,
    items: HashMap<u32, (T, u64)>,
}

impl<T: Clone + PartialEq> List<T> {
    fn new(items: &[T], id: fn(&T) -> u32) -> Self {
        Self {
            id,
            ids: items.iter().map(id).collect(),
            order_at: 0,
            items: items.iter().map(|item| (id(item), (item.clone(), 0))).collect(),
        }
    }

    fn stamp(&mut self, current: &[T], revision: u64) {
        let ids: Vec<u32> = current.iter().map(self.id).collect();
        if ids != self.ids {
            let kept: HashSet<u32> = ids.iter().copied().collect();
            self.items.retain(|id, _| kept.contains(id));
            self.ids = ids;
            self.order_at = revision;
        }
        for item in current {
            match self.items.get_mut(&(self.id)(item)) {
                Some((seen, _)) if seen == item => {}
                Some(entry) => *entry = (item.clone(), revision),
                None => _ = self.items.insert((self.id)(item), (item.clone(), revision)),
            }
        }
    }

    fn since(&self, since: u64) -> Option<ListChanges<T>> {
        let changed: Vec<T> =
            self.ids.iter().filter_map(|id| self.items.get(id)).filter(|(_, at)| *at > since).map(|(item, _)| item.clone()).collect();
        (self.order_at > since || !changed.is_empty()).then(|| ListChanges { ids: self.ids.clone(), changed })
    }
}

/// Every part of the project as last handed out, with its stamp.
pub(super) struct Stamps {
    /// The revision of the last comparing: a read at the same revision has nothing to compare.
    revision: u64,
    name: Whole<String>,
    tempo_map: Whole<TempoMap>,
    tracks: List<Track>,
    clips: List<Clip>,
    loop_region: Whole<Option<(u32, u32)>>,
    nodes: List<Node>,
    edges: Whole<Vec<Edge>>,
    room: Whole<Room>,
    automation: List<AutomationLane>,
    samples: List<SampleRef>,
    markers: List<Marker>,
    folders: List<Folder>,
    midi_mappings: Whole<Vec<MidiMapping>>,
    next_id: Whole<u32>,
}

impl Stamps {
    /// Stamps for `project` as it is at revision 0.
    pub(super) fn new(project: &Project) -> Self {
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
            revision: 0,
            name: Whole::new(name),
            tempo_map: Whole::new(tempo_map),
            tracks: List::new(tracks, |t| t.id),
            clips: List::new(clips, |c| c.id),
            loop_region: Whole::new(loop_region),
            nodes: List::new(nodes, |n| n.id),
            edges: Whole::new(edges),
            room: Whole::new(room),
            automation: List::new(automation, |l| l.id),
            samples: List::new(samples, |s| s.id),
            markers: List::new(markers, |m| m.id),
            folders: List::new(folders, |f| f.id),
            midi_mappings: Whole::new(midi_mappings),
            next_id: Whole::new(next_id),
        }
    }

    /// The changes of `project` (at `revision`) after `since` (see `ProjectView::changes_since`).
    pub(super) fn changes(&mut self, project: &Project, revision: u64, since: Option<u64>) -> ProjectChanges {
        if revision != self.revision {
            self.stamp(project, revision);
        }
        let Some(since) = since else {
            return ProjectChanges::whole(project, revision);
        };
        ProjectChanges {
            revision,
            name: self.name.since(since),
            tempo_map: self.tempo_map.since(since),
            tracks: self.tracks.since(since),
            clips: self.clips.since(since),
            loop_region: self.loop_region.since(since),
            nodes: self.nodes.since(since),
            edges: self.edges.since(since),
            room: self.room.since(since),
            automation: self.automation.since(since),
            samples: self.samples.since(since),
            markers: self.markers.since(since),
            folders: self.folders.since(since),
            midi_mappings: self.midi_mappings.since(since),
            next_id: self.next_id.since(since),
        }
    }

    fn stamp(&mut self, project: &Project, revision: u64) {
        // Every field by name: a new project field does not compile until it is stamped here.
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
        self.revision = revision;
        self.name.stamp(name, revision);
        self.tempo_map.stamp(tempo_map, revision);
        self.tracks.stamp(tracks, revision);
        self.clips.stamp(clips, revision);
        self.loop_region.stamp(loop_region, revision);
        self.nodes.stamp(nodes, revision);
        self.edges.stamp(edges, revision);
        self.room.stamp(room, revision);
        self.automation.stamp(automation, revision);
        self.samples.stamp(samples, revision);
        self.markers.stamp(markers, revision);
        self.folders.stamp(folders, revision);
        self.midi_mappings.stamp(midi_mappings, revision);
        self.next_id.stamp(next_id, revision);
    }
}
