// Right-click menu of an audio clip on the timeline (`timelinePointer.ts` opens it): normalize (all selected audio clips
// when the clip is among them), reverse, fade shapes (on a fade only that fade's shapes), delete.
import { For, Show } from "solid-js";
import type { FadeShape } from "../api/types";
import { clipLabel } from "./clipRules";
import { FADE_SHAPES, type FadeEdge } from "./fades";
import { clipById } from "./trackRules";
import { t } from "../i18n";
import { useSession } from "../session";
import { ContextMenu, MenuCheck, MenuItem, MenuLabel } from "../common/ContextMenu";
import type { ClipMenu as Menu } from "./timelinePointer";

export function ClipMenu(props: { menu: Menu; onClose: () => void }) {
  const session = useSession();
  const clips = session.clips;
  const msg = () => t().tracks.clipEdit;
  const close = props.onClose;
  const clip = () => clipById(session.project, props.menu.clip);
  /** Normalize takes the whole selection when the clicked clip is part of it. */
  const normalizeIds = () => {
    const selected = session.selection.clips();
    const group = selected.includes(props.menu.clip) ? selected : [props.menu.clip];
    return group.filter((id) => clipById(session.project, id)?.content.type === "audio");
  };
  const edges = (): FadeEdge[] => (props.menu.fade ? [props.menu.fade] : ["in", "out"]);

  const shapeItems = (edge: FadeEdge, current: FadeShape) => (
    <For each={FADE_SHAPES}>
      {(shape) => (
        <MenuItem onClose={close} onPick={() => void clips.setFadeShape(props.menu.clip, edge, shape)}>
          <MenuCheck on={shape === current}>{msg().shapes[shape]}</MenuCheck>
        </MenuItem>
      )}
    </For>
  );

  return (
    <Show when={clip()}>
      {(c) => (
        <ContextMenu at={props.menu.at} label={msg().menu(clipLabel(c(), session.project))} onClose={close}>
          <Show when={!props.menu.fade}>
            <MenuItem onClose={close} onPick={() => void clips.normalize(normalizeIds())}>{msg().normalize}</MenuItem>
            <MenuItem onClose={close} onPick={() => void clips.reverse(props.menu.clip)}>{msg().reverse}</MenuItem>
          </Show>
          <For each={edges()}>
            {(edge) => (
              <>
                <MenuLabel>{edge === "in" ? msg().fadeInShape : msg().fadeOutShape}</MenuLabel>
                {shapeItems(edge, edge === "in" ? c().fadeInShape : c().fadeOutShape)}
              </>
            )}
          </For>
          <Show when={!props.menu.fade}>
            <MenuItem onClose={close} onPick={() => void clips.remove(props.menu.clip)}>{msg().remove}</MenuItem>
          </Show>
        </ContextMenu>
      )}
    </Show>
  );
}
