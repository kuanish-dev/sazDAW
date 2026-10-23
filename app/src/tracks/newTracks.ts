import type { Id, NewTrack } from "../api/types";
import type { Core } from "../core/core";
import type { Views } from "../shell/views";

/** A batch of tracks as one edit; selects the first. Returns the ids or `undefined`. */
export async function addTracks(core: Core, views: Views, tracks: NewTrack[]): Promise<Id[] | undefined> {
  const ids = await core.calls.run(core.api.tracks.addMany(tracks));
  if (!ids || ids.length === 0) return undefined;
  await views.showCreated({ track: ids[0]! });
  return ids;
}
