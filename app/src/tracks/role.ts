// The tracks role: the tracks themselves (`tracks.ts`) and the jobs around them on the timeline — clips, folders, markers
// and rendering a track to audio.
import { automationRole } from "../automation/role";
import { defineRole, useRole } from "../core/roles";
import { shellRole } from "../shell/role";
import { type BounceActions, createBounceActions } from "./bounce";
import { type ClipsActions, createClipsActions } from "./clips";
import { createFoldersActions, type FoldersActions } from "./folders";
import { createMarkersActions, type MarkersActions } from "./markers";
import { createTracksActions, type TracksActions } from "./tracks";

export type TracksRole = TracksActions & {
  readonly clips: ClipsActions;
  readonly folders: FoldersActions;
  /** Song structure: section markers. */
  readonly markers: MarkersActions;
  /** Render a track to audio: "Bounce to audio", "Freeze", "Unfreeze". */
  readonly bounce: BounceActions;
};

export const tracksRole = defineRole("tracks", ({ core, use }): TracksRole => {
  const shell = use(shellRole);
  return {
    ...createTracksActions(core, shell, use(automationRole).touch),
    clips: createClipsActions(core, shell),
    folders: createFoldersActions(core),
    markers: createMarkersActions(core),
    bounce: createBounceActions(core, shell),
  };
});

export const useTracks = () => useRole(tracksRole);
