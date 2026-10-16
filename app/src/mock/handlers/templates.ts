// Project templates (`templates.*`, `project.createFromTemplate`) in mock memory: the project structure without clips,
// automation, loop, markers and samples (the mock does not tell sampler sounds apart, so it drops them all).
import type { Project } from "../../api/types";
import { fail } from "../errors";
import type { MockWorld } from "../world";
import { checkedName } from "./presets";
import type { HandlerSet } from "./types";

const stem = (path: string) => path.slice(path.lastIndexOf("/") + 1).replace(/\.dawproj$/, "");

export function templatesHandlers(w: MockWorld): HandlerSet {
  const h: HandlerSet = {};
  const templates = new Map<string, string>();

  const existing = (name: string) => {
    const saved = templates.get(checkedName(name));
    if (saved === undefined) throw fail("PR-070", { name: name.trim() });
    return saved;
  };

  h["templates.list"] = () => [...templates.keys()].sort((a, b) => a.localeCompare(b));
  h["templates.save"] = ({ name }) => {
    const structure: Project = w.clone(w.project);
    structure.clips = [];
    structure.automation = [];
    structure.loopRegion = null;
    structure.samples = [];
    structure.markers = [];
    for (const track of structure.tracks) if (track.instrument) track.instrument.frozen = null;
    templates.set(checkedName(name), JSON.stringify(structure));
  };
  h["templates.remove"] = ({ name }) => {
    existing(name);
    templates.delete(name.trim());
  };
  h["project.createFromTemplate"] = ({ path, template }) => {
    const project = JSON.parse(existing(template)) as Project;
    if (w.saved.has(path)) throw fail("PJ-110", { path });
    project.name = stem(path);
    w.saved.set(path, JSON.stringify(project));
    w.install(project, path, false);
    w.remember(path);
  };
  return h;
}
