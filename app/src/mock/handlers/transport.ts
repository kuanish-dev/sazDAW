import { resetMasterMeter } from "../masterMeter";
import { capturing, closeTake, startTake } from "../take";
import type { MockWorld } from "../world";
import type { HandlerSet } from "./types";

export function transportHandlers(w: MockWorld): HandlerSet {
  const h: HandlerSet = {};
  h["transport.play"] = () => {
    if (w.state !== "playing" && w.cue !== null) w.position = w.cue;
    if (w.state !== "playing" && w.position === 0) resetMasterMeter(w);
    w.state = "playing";
    w.needTicks();
    if (capturing(w)) startTake(w);
  };
  h["transport.pause"] = () => { closeTake(w); w.state = "paused"; };
  h["transport.stop"] = () => { closeTake(w); w.state = "stopped"; w.position = w.cue ?? 0; };
  h["transport.seek"] = ({ tick }) => { w.position = tick; };
  h["transport.setCue"] = ({ tick }) => { w.cue = tick; };
  h["transport.setSongLoop"] = ({ enabled }) => { w.songLoop = enabled; };
  h["metronome.setEnabled"] = ({ enabled }) => { w.metronome = enabled; };
  return h;
}
