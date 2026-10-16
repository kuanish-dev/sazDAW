// Shared step for sound actions: sampler tracks are created in one batch as a single edit, and the first one is selected.
import type { Id, NewTrack } from "../api/types";
import type { Backend, Calls, View } from "./context";

/** A batch of tracks as one edit; selects the first. Returns the ids or `undefined`. */
export async function addTracks(ctx: Backend & Calls & View, tracks: NewTrack[]): Promise<Id[] | undefined> {
  const ids = await ctx.run(ctx.api.tracks.addMany(tracks));
  if (!ids || ids.length === 0) return undefined;
  await ctx.showCreated({ track: ids[0]! });
  return ids;
}
