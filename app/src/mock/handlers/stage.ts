// The virtual stage: where a channel stands (its strip's pan and `stage`) and the shared room, with the backend's checks
// and codes (`daw-project/src/stage.rs`). The mock plays no sound, so placing changes only the project.
import { type Id, MAX_ON_STAGE, type Project, type Room } from "../../api/types";
import { hasStrip } from "../../graph/graphRules";
import { clamp } from "../../common/math";
import { t } from "../../i18n";
import { fail } from "../errors";
import { nodeOf } from "../graphOps";
import type { MockWorld } from "../world";
import type { HandlerSet } from "./types";

function number(value: number): number {
  if (!Number.isFinite(value)) throw fail("GR-125", { value });
  return value;
}

/** The strip of a node that can stand on the stage (a source or a bus). */
function strip(p: Project, id: Id) {
  const kind = nodeOf(p, id).kind;
  if (!hasStrip(kind)) throw fail("GR-123", { id });
  return kind;
}

const onStage = (p: Project) => p.graph.nodes.filter(({ kind }) => hasStrip(kind) && kind.stage !== undefined).length;

export function stageHandlers(w: MockWorld): HandlerSet {
  const h: HandlerSet = {};
  h["stage.place"] = ({ node, pan, depth, height }) => {
    const left = clamp(number(pan), -1, 1);
    const place = { depth: clamp(number(depth), -1, 1), height: clamp(number(height), -1, 1) };
    return w.edit(t().mock.undo.stagePlace, (p) => {
      const placed = onStage(p);
      const channel = strip(p, node);
      if (!channel.stage && placed >= MAX_ON_STAGE) throw fail("GR-212", { max: MAX_ON_STAGE });
      channel.pan = left;
      channel.stage = place;
    }, `stage:${node}`);
  };
  h["stage.takeOff"] = ({ node }) => {
    // Off the stage already: nothing changes, no undo step.
    if (!strip(w.project, node).stage) return null;
    return w.edit(t().mock.undo.stageTakeOff, (p) => {
      strip(p, node).stage = undefined;
      p.automation = p.automation.filter(({ target }) => !((target.type === "stageDepth" || target.type === "stageHeight") && target.node === node));
    });
  };
  h["stage.room"] = () => w.project.room;
  h["stage.setRoom"] = ({ size, reverb, listening }: Room) => {
    const room: Room = { size: clamp(number(size), 0, 1), reverb: clamp(number(reverb), 0, 1), listening };
    return w.edit(t().mock.undo.stageRoom, (p) => { p.room = room; }, "stage:room");
  };
  return h;
}
