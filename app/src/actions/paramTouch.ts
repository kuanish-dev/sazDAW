// Live automation writing as the other actions see it: a control moved a parameter (fader, knob, send, instrument
// parameter). One tracker for the whole app; the backend writes the moves while the song plays (Touch/Latch).
import type { ParamTarget } from "../api/types";
import type { Backend, Calls, PlaybackPosition } from "./context";

export interface ParamTouch {
  /** Call right before setting the parameter; reported to the backend only while writing can happen. */
  moved(target: ParamTarget, value: number): void;
  /** The mouse button (or a key) went up: touched controls are released. */
  releaseAll(): Promise<void>;
}

export function createParamTouch(ctx: Backend & Calls & PlaybackPosition): ParamTouch {
  /** Something was touched since the last release: only then does a mouse-up go to the backend. */
  let touched = false;
  return {
    moved(target, value) {
      const tick = ctx.tick();
      if (tick.automationWrite === "off" || tick.state !== "playing") return;
      touched = true;
      void ctx.exec(ctx.api.automation.touch(target, value));
    },
    async releaseAll() {
      if (!touched) return;
      touched = false;
      await ctx.exec(ctx.api.automation.releaseAll());
    },
  };
}
