import type { Id } from "../api/types";
import { t } from "../i18n";
import type { Core } from "../core/core";
import type { Views } from "../shell/views";

export interface Bounce {
  /** The track's sound into an audio track below it (the original is muted); selects the copy. */
  bounce(track: Id): Promise<void>;
  /** Like `bounce`, but the instrument is frozen instead of muted (not instantiated, less load). */
  freeze(track: Id): Promise<void>;
  /** Removes a frozen track's audio copy; the instrument plays again. */
  unfreeze(track: Id): Promise<void>;
}

export function createBounce(core: Core, views: Views): Bounce {
  const { api } = core;

  const copy = async (busyText: string, call: () => Promise<Id>, done: string) => {
    const created = await core.calls.withBusy(busyText, () => core.calls.run(call()));
    if (created === undefined) return;
    await views.showCreated({ track: created });
    core.notifications.toast("info", done);
  };

  return {
    bounce: (track) => copy(t().project.bouncing, () => api.tracks.bounce(track), t().project.bounced),
    freeze: (track) => copy(t().project.freezing, () => api.tracks.freeze(track), t().project.frozen),
    unfreeze: (track) => core.calls.exec(api.tracks.unfreeze(track)),
  };
}
