// What changed in the project after a revision (`project.changes`), taken into the interface's copy: the backend sends
// only the parts that changed, the copy keeps the rest. The same rules as `ProjectChanges::apply` in Rust
// (`daw-contracts/src/project_changes.rs`); a new project field is a new line here and there.
import type { ListChanges, Project, ProjectChanges } from "../api/types";

/** The list rebuilt in the order of `changes.ids`: each entity from `changed`, otherwise the one the copy has. */
function applyList<T extends { id: number }>(items: T[], changes: ListChanges<T> | undefined): T[] {
  if (!changes) return items;
  const byId = new Map(items.map((item) => [item.id, item]));
  for (const item of changes.changed) byId.set(item.id, item);
  return changes.ids.flatMap((id) => byId.get(id) ?? []);
}

/** `project` with `changes` taken in: a new object whose unchanged parts are the very objects `project` has, so a
 *  store reconciling it skips them at once. */
export function applyChanges(project: Project, changes: ProjectChanges): Project {
  const c = changes;
  return {
    name: c.name ?? project.name,
    tempoMap: c.tempoMap ?? project.tempoMap,
    tracks: applyList(project.tracks, c.tracks),
    clips: applyList(project.clips, c.clips),
    loopRegion: c.loopRegion === undefined ? project.loopRegion : c.loopRegion,
    graph: { nodes: applyList(project.graph.nodes, c.nodes), edges: c.edges ?? project.graph.edges },
    room: c.room ?? project.room,
    automation: applyList(project.automation, c.automation),
    samples: applyList(project.samples, c.samples),
    markers: applyList(project.markers, c.markers),
    folders: applyList(project.folders, c.folders),
    midiMappings: c.midiMappings ?? project.midiMappings,
    nextId: c.nextId ?? project.nextId,
  };
}
