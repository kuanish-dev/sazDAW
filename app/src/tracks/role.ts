import { automationRole } from "../automation/role";
import { defineRole, useRole } from "../core/roles";
import { shellRole } from "../shell/role";
import { type Bounce, createBounce } from "./bounce";
import { type Clips, createClips } from "./clips";
import { createFolders, type Folders } from "./folders";
import { createMarkers, type Markers } from "./markers";
import { createTracks, type Tracks } from "./tracks";

export type TracksRole = Tracks & {
  readonly clips: Clips;
  readonly folders: Folders;
  /** Song structure: section markers. */
  readonly markers: Markers;
  /** Render a track to audio: "Bounce to audio", "Freeze", "Unfreeze". */
  readonly bounce: Bounce;
};

export const tracksRole = defineRole("tracks", ({ core, use }): TracksRole => {
  const shell = use(shellRole);
  return {
    ...createTracks(core, shell, use(automationRole).touch),
    clips: createClips(core, shell),
    folders: createFolders(core),
    markers: createMarkers(core),
    bounce: createBounce(core, shell),
  };
});

export const useTracks = () => useRole(tracksRole);
