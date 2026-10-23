import type { Id, Project } from "../../api/types";
import { t } from "../../i18n";
import { fail } from "../errors";
import type { MockWorld } from "../world";
import type { HandlerSet } from "./types";

const MAX_MARKERS = 256;
const MAX_MARKER_NAME = 64;
/** As `MAX_CLIP_TICK` in Rust. */
const MAX_TICK = 960 * 60 * 500;

function checkedName(name: string): string {
  const trimmed = String(name ?? "").trim();
  if (!trimmed) throw fail("MK-110");
  if ([...trimmed].length > MAX_MARKER_NAME) throw fail("MK-111", { max: MAX_MARKER_NAME });
  return trimmed;
}

function checkTick(tick: number) {
  if (!Number.isInteger(tick) || tick < 0 || tick > MAX_TICK) throw fail("MK-210", { tick, max: MAX_TICK });
}

function checkFree(p: Project, tick: number, except: Id | null) {
  const other = p.markers.find((m) => m.tick === tick && m.id !== except);
  if (other) throw fail("MK-115", { name: other.name, tick });
}

const sort = (p: Project) => p.markers.sort((a, b) => a.tick - b.tick);

export function markersHandlers(w: MockWorld): HandlerSet {
  const h: HandlerSet = {};
  const marker = (p: Project, id: Id) => w.must(p.markers.find((m) => m.id === id), fail("MK-010", { id }));

  h["markers.add"] = ({ tick, name }: { tick: number; name: string }) => {
    checkTick(tick);
    const checked = checkedName(name);
    if (w.project.markers.length >= MAX_MARKERS) throw fail("MK-211", { max: MAX_MARKERS });
    checkFree(w.project, tick, null);
    return w.edit(t().mockEdits.history.addMarker, (p) => {
      const id = w.id(p);
      p.markers.push({ id, tick, name: checked });
      sort(p);
      return id;
    });
  };
  h["markers.edit"] = ({ id, tick, name }: { id: Id; tick?: number; name?: string }) => {
    marker(w.project, id);
    if (tick !== undefined) {
      checkTick(tick);
      checkFree(w.project, tick, id);
    }
    const checked = name === undefined ? undefined : checkedName(name);
    return w.edit(t().mockEdits.history.editMarker, (p) => {
      const found = marker(p, id);
      if (tick !== undefined) found.tick = tick;
      if (checked !== undefined) found.name = checked;
      sort(p);
    });
  };
  h["markers.remove"] = ({ id }: { id: Id }) => {
    marker(w.project, id);
    return w.edit(t().mockEdits.history.removeMarker, (p) => {
      p.markers = p.markers.filter((m) => m.id !== id);
    });
  };
  return h;
}
