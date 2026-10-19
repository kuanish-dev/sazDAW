// Rendering a track to audio: "Bounce to audio", "Freeze", "Unfreeze". Rendering is slow, so a busy indicator is shown
// meanwhile; a backend refusal is a notification; on success the audio copy is selected.
import type { Id } from "../api/types";
import { t } from "../i18n";
import type { Core } from "../core/core";
import type { Views } from "../shell/views";

export interface BounceActions {
  /** The track's sound into an audio track below it (the original is muted); selects the copy. */
  bounce(track: Id): Promise<void>;
  /** Like `bounce`, but the instrument is frozen instead of muted (not instantiated, less load). */
  freeze(track: Id): Promise<void>;
  /** Removes a frozen track's audio copy; the instrument plays again. */
  unfreeze(track: Id): Promise<void>;
}

export function createBounceActions(core: Core, views: Views): BounceActions {
  const { api } = core;

  const copy = async (busyText: string, call: () => Promise<Id>, done: string) => {
    const created = await core.withBusy(busyText, () => core.run(call()));
    if (created === undefined) return;
    await views.showCreated({ track: created });
    core.toast("info", done);
  };

  return {
    bounce: (track) => copy(t().project.bouncing, () => api.tracks.bounce(track), t().project.bounced),
    freeze: (track) => copy(t().project.freezing, () => api.tracks.freeze(track), t().project.frozen),
    unfreeze: (track) => core.exec(api.tracks.unfreeze(track)),
  };
}
