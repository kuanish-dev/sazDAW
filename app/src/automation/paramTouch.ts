// Live automation writing as the other actions see it: a control moved a parameter (fader, knob, send, instrument
// parameter). One tracker for the whole app; the backend writes the moves while the song plays (Touch/Latch).
import type { ParamTarget } from "../api/types";
import type { Core } from "../core/core";

export interface ParamTouch {
  /** Call right before setting the parameter; reported to the backend only while writing can happen. */
  moved(target: ParamTarget, value: number): void;
  /** The mouse button (or a key) went up: touched controls are released. */
  releaseAll(): Promise<void>;
}

export function createParamTouch(core: Core): ParamTouch {
  /** Something was touched since the last release: only then does a mouse-up go to the backend. */
  let touched = false;
  return {
    moved(target, value) {
      const tick = core.tick();
      if (tick.automationWrite === "off" || tick.state !== "playing") return;
      touched = true;
      void core.exec(core.api.automation.touch(target, value));
    },
    async releaseAll() {
      if (!touched) return;
      touched = false;
      await core.exec(core.api.automation.releaseAll());
    },
  };
}
