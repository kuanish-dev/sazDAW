// Folder row left of the timeline: collapse/expand chevron, color stripe, name (double-click — rename), track count
// and M/S for all its tracks at once. Right-click — color and "Remove folder" (tracks stay in place).
import { Show, createSignal } from "solid-js";
import type { Folder, Track } from "../api/types";
import { folderFlags } from "./trackRows";
import { t } from "../i18n";
import { ColorSwatches } from "../common/ColorSwatches";
import { ContextMenu, MenuItem, MenuLabel, type MenuPoint, menuPoint } from "../common/ContextMenu";
import { IconChevronDown, IconChevronRight, IconFolder } from "../common/icons";
import { InlineRename } from "../common/InlineRename";
import { TRACK_COLOR_HEX } from "../common/trackColors";
import { useProjectCopy } from "../core/roles";
import { useSettings } from "../settings/role";
import { useTracks } from "./role";

export function FolderHeader(props: { folder: Folder; tracks: Track[] }) {
  const { project } = useProjectCopy();
  const settings = useSettings();
  const tracks = useTracks();
  const { folders } = tracks;
  const [renaming, setRenaming] = createSignal(false);
  const [menuAt, setMenuAt] = createSignal<MenuPoint | null>(null);
  /** A button is lit when the flag is set on every track of the folder; a click sets or clears it on all. */
  const flags = () => folderFlags(project.graph, props.tracks);
  const allMuted = () => flags().muted;
  const allSoloed = () => flags().soloed;
  const edit = (change: Parameters<typeof folders.edit>[1]) => void folders.edit(props.folder.id, change);

  return (
    <div
      class="relative flex items-center gap-1 border-t border-line bg-hover px-2 pl-2.5 text-sm"
      style={{ height: `${settings.appearance.rowHeight()}px` }}
      onContextMenu={(e) => { e.preventDefault(); setMenuAt(menuPoint(e)); }}
    >
      <div class="pointer-events-none absolute inset-y-0 left-0 w-1"
        style={{ background: props.folder.color ? TRACK_COLOR_HEX[props.folder.color] : "var(--color-muted)" }} />
      <button
        class="btn btn-icon !h-6 !w-6 !min-w-6" aria-expanded={!props.folder.collapsed}
        title={props.folder.collapsed ? t().tracks.folder.expand : t().tracks.folder.collapse} aria-label={props.folder.collapsed ? t().tracks.folder.expand : t().tracks.folder.collapse}
        onClick={() => edit({ collapsed: !props.folder.collapsed })}
      >{props.folder.collapsed ? <IconChevronRight size={14} /> : <IconChevronDown size={14} />}</button>
      <InlineRename name={props.folder.name} editing={renaming()} class="!h-6 min-w-0 flex-1 text-sm"
        onRename={(name) => edit({ name })} onClose={() => setRenaming(false)}>
        <span class="flex min-w-0 flex-1 items-center gap-1.5 font-semibold" title={t().tracks.folder.nameHint}
          onDblClick={() => setRenaming(true)}><IconFolder size={14} class="text-muted" /><span class="truncate">{props.folder.name}</span></span>
      </InlineRename>
      <span class="shrink-0 rounded-full bg-panel-3 px-1.5 text-[10px] leading-4 text-muted">{props.tracks.length}</span>
      <button class="btn !h-6 !min-w-6 !px-0 text-[10px]" classList={{ "!bg-danger !text-white": allMuted() }}
        title={t().tracks.folder.muteAll} aria-label={t().tracks.folder.mute} aria-pressed={allMuted()}
        onClick={() => void folders.setMute(props.folder.id, !allMuted())}>M</button>
      <button class="btn !h-6 !min-w-6 !px-0 text-[10px]" classList={{ "btn-on": allSoloed() }}
        title={t().tracks.folder.soloAll} aria-label={t().tracks.folder.solo} aria-pressed={allSoloed()}
        onClick={() => void folders.setSolo(props.folder.id, !allSoloed())}>S</button>
      <Show when={menuAt()}>
        {(at) => (
          <ContextMenu at={at()} label={t().tracks.folder.menu(props.folder.name)} onClose={() => setMenuAt(null)}>
            <MenuLabel>{t().tracks.folder.color}</MenuLabel>
            <ColorSwatches current={props.folder.color} onPick={(color) => { setMenuAt(null); edit({ color }); }} />
            <MenuItem onClose={() => setMenuAt(null)} onPick={() => setRenaming(true)}>{t().common.rename}</MenuItem>
            <MenuItem onClose={() => setMenuAt(null)} onPick={() => void folders.remove(props.folder.id)}>{t().tracks.folder.remove}</MenuItem>
          </ContextMenu>
        )}
      </Show>
    </div>
  );
}
