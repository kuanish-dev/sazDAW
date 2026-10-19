// A file row in the sound browser: an "in project / pack" badge, the name, dragging and (when selected) action buttons.
import { Show } from "solid-js";
import type { LibraryEntry, SampleInfo } from "../api/types";
import { t } from "../i18n";
import { startSampleDrag } from "../common/dragSample";
import { IconAdd, IconChevronRight, IconClose, IconNote } from "../common/icons";

export interface SoundRowActions {
  /** A sampler track (the file is copied into the project if needed). */
  addTrack: (entry: LibraryEntry) => void;
  /** Packs only: copy into the project and nothing else. */
  toProject: (entry: LibraryEntry) => void;
  toTimeline: (entry: LibraryEntry) => void;
  toEditor: (entry: LibraryEntry) => void;
  remove: (sample: SampleInfo) => void;
}

export function SoundRow(props: {
  entry: LibraryEntry;
  /** The file is in the project folder (otherwise it is a pack: preview only). */
  project: boolean;
  selected: boolean;
  /** The project sample for this file, if already registered (without it there is nothing to remove). */
  sample: SampleInfo | undefined;
  actions: SoundRowActions;
  onPick: () => void;
}) {
  const e = () => props.entry;
  const btn = "btn !h-6 text-xs";
  const msg = () => t().sounds;
  return (
    <div
      data-path={e().path} role="option" aria-selected={props.selected}
      class="group cursor-default rounded-sm px-2 py-0.5 text-sm" classList={{ "bg-select": props.selected, "hover:bg-hover": !props.selected }}
      onPointerDown={(ev) => !e().isDir && startSampleDrag(ev, { kind: "file", path: e().path }, e().name)}
      title={e().isDir ? msg().openFolder : msg().fileHint}
      onClick={props.onPick}
    >
      <div class="flex items-center gap-1.5">
        <span class="flex w-4 shrink-0 justify-center text-muted">{e().isDir ? <IconChevronRight size={14} /> : <IconNote size={14} />}</span>
        <span class="min-w-0 flex-1 truncate">{e().name}</span>
        <Show when={!e().isDir}>
          <span
            class="shrink-0 rounded px-1 text-[10px]" classList={{ "bg-accent/15 text-accent": props.project, "bg-panel-2 text-muted": !props.project }}
            title={props.project ? msg().inProjectHint : msg().packFileHint}
          >{props.project ? msg().inProject : msg().packPreview}</span>
          <button
            class="btn btn-icon !h-5 !w-5 !min-w-5 opacity-0 group-hover:opacity-100" title={msg().addTrackHint} aria-label={msg().addTrackAria(e().name)}
            onClick={(ev) => { ev.stopPropagation(); props.actions.addTrack(e()); }}
          ><IconAdd size={14} /></button>
        </Show>
      </div>
      <Show when={props.selected && !e().isDir}>
        <div class="mt-1 flex flex-wrap gap-1 pl-5" onClick={(ev) => ev.stopPropagation()}>
          <Show
            when={props.project}
            fallback={<button class={btn} title={msg().toProjectHint} onClick={() => props.actions.toProject(e())}>{msg().toProject}</button>}
          >
            <button class={btn} title={msg().toTimelineHint} onClick={() => props.actions.toTimeline(e())}>{msg().toTimeline}</button>
            <button class={btn} title={msg().toEditorHint} onClick={() => props.actions.toEditor(e())}>{msg().toEditor}</button>
            <Show when={props.sample}>
              {(sample) => (
                <button class={`${btn} text-muted hover:text-danger`} title={msg().removeHint} aria-label={msg().removeAria(e().name)} onClick={() => props.actions.remove(sample())}><IconClose size={12} /></button>
              )}
            </Show>
          </Show>
          <button class={btn} title={msg().addTrackRowHint} onClick={() => props.actions.addTrack(e())}>{msg().addTrack}</button>
        </div>
      </Show>
    </div>
  );
}
