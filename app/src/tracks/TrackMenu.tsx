import { For, Show } from "solid-js";
import type { Track } from "../api/types";
import { t } from "../i18n";
import { ColorSwatches } from "../common/ColorSwatches";
import { ContextMenu, MenuItem, MenuLabel, type MenuPoint } from "../common/ContextMenu";
import { useProjectCopy } from "../core/roles";
import { useTracks } from "./role";

export function TrackMenu(props: { track: Track; at: MenuPoint; onClose: () => void }) {
  const { project } = useProjectCopy();
  const tracks = useTracks();
const { folders, bounce } = tracks;
  const id = () => props.track.id;
  const otherFolders = () => project.folders.filter((f) => f.id !== props.track.folder);
  return (
    <ContextMenu at={props.at} label={t().tracks.menu.label(props.track.name)} onClose={props.onClose}>
      <MenuLabel>{t().tracks.menu.color}</MenuLabel>
      <ColorSwatches current={props.track.color} onPick={(color) => { props.onClose(); void tracks.setColor(id(), color); }} />
      <MenuLabel>{t().tracks.menu.folder}</MenuLabel>
      <MenuItem onClose={props.onClose} onPick={() => void folders.create([id()])}>{t().tracks.menu.newFolder}</MenuItem>
      <For each={otherFolders()}>
        {(folder) => <MenuItem onClose={props.onClose} onPick={() => void folders.moveInto(folder.id, id())}>{t().tracks.menu.moveInto(folder.name)}</MenuItem>}
      </For>
      <Show when={props.track.folder != null}>
        <MenuItem onClose={props.onClose} onPick={() => void folders.takeOut(id())}>{t().tracks.menu.takeOut}</MenuItem>
      </Show>
      <MenuLabel>{t().tracks.menu.sound}</MenuLabel>
      <Show when={props.track.instrument?.frozen == null}>
        <MenuItem onClose={props.onClose} onPick={() => void bounce.bounce(id())}>{t().tracks.menu.bounce}</MenuItem>
      </Show>
      <Show when={props.track.instrument}>
        <Show when={props.track.instrument?.frozen != null} fallback={
          <MenuItem onClose={props.onClose} onPick={() => void bounce.freeze(id())}>{t().tracks.menu.freeze}</MenuItem>
        }>
          <MenuItem onClose={props.onClose} onPick={() => void bounce.unfreeze(id())}>{t().tracks.menu.unfreeze}</MenuItem>
        </Show>
      </Show>
    </ContextMenu>
  );
}
