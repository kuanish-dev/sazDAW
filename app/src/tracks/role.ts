// The tracks role as the session holds it: the tracks themselves (`tracks.ts`) and the jobs around them on the timeline —
// clips, folders, markers and rendering a track to audio.
import type { ParamTouch } from "../automation/paramTouch";
import type { Core } from "../core/core";
import type { Views } from "../shell/views";
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

export function createTracksRole(core: Core, views: Views, touch: ParamTouch): TracksRole {
  return {
    ...createTracksActions(core, views, touch),
    clips: createClipsActions(core, views),
    folders: createFoldersActions(core),
    markers: createMarkersActions(core),
    bounce: createBounceActions(core, views),
  };
}
