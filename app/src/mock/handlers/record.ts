// Keyboard recording: live notes go into the track's clip under the cursor, lap by lap.
import { BAR_TICKS } from "../../api/types";
import { sourceOfTrack, upstream } from "../../graph/graphRules";
import { fail } from "../errors";
import { capturing, closeTake, enterPass, recordNote, startTake } from "../take";
import type { MockWorld } from "../world";
import type { HandlerSet } from "./types";

/** Meter peak from a live note (the browser has no sound, so this shows the note "played"). */
const LIVE_NOTE_PEAK = 0.6;

export function recordHandlers(w: MockWorld): HandlerSet {
  const h: HandlerSet = {};
  h["record.setArmed"] = ({ armed }) => { w.armed = armed; if (!armed) closeTake(w); else if (capturing(w)) startTake(w); };
  h["record.setQuantize"] = ({ ticks }) => { if (ticks < 0 || ticks > BAR_TICKS) throw fail("RC-240", { ticks, max: BAR_TICKS }); w.recordQuantize = ticks; };
  h["live.noteOn"] = ({ track, pitch, velocity }) => {
    // A meter spike on everything that hears this instrument (the source, buses and master downstream).
    const source = sourceOfTrack(w.project.graph, track);
    if (source) w.peaks.set(source.id, LIVE_NOTE_PEAK);
    for (const node of w.project.graph.nodes) {
      if (source && (node.kind.type === "bus" || node.kind.type === "master") && upstream(w.project.graph, node.id).includes(source.id)) w.peaks.set(node.id, LIVE_NOTE_PEAK);
    }
    if (capturing(w)) {
      enterPass(w, track, w.position);
      w.heldNotes.set(`${track}:${pitch}`, { tick: w.position, velocity });
    }
  };
  h["live.noteOff"] = ({ track, pitch }) => {
    const held = w.heldNotes.get(`${track}:${pitch}`);
    if (held && capturing(w)) recordNote(w, track, pitch, held.tick, w.position, held.velocity);
    w.heldNotes.delete(`${track}:${pitch}`);
  };
  return h;
}
