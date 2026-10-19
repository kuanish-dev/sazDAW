// Project menu (the button with the project name): files, templates, recent projects, export and jumps to settings sections (audio, plugins, error log).
import { createSignal, For, onMount, Show } from "solid-js";
import type { RecentProject } from "../api/types";
import { t } from "../i18n";
import { closeOnOutside } from "../common/pointer";
import { ExportDialog } from "./ExportDialog";
import { TemplatesDialog } from "./TemplatesDialog";
import { useProjectCopy } from "../core/roles";
import { useFiles } from "./role";
import { useShell } from "../shell/role";

export function ProjectMenu(props: { onClose: () => void }) {
  const { status } = useProjectCopy();
  const files = useFiles();
  const shell = useShell();
  const [exporting, setExporting] = createSignal(false);
  const [templatesOpen, setTemplatesOpen] = createSignal(false);
  const [recent, setRecent] = createSignal<RecentProject[]>([]);
  let root!: HTMLDivElement;

  onMount(() => void files.recent().then(setRecent));
  // While a dialog is open on top, the menu under it stays open.
  closeOnOutside(() => root, () => !exporting() && !templatesOpen() && props.onClose());

  const choose = (action: () => Promise<void>) => () => {
    props.onClose();
    void action();
  };

  const item = "menu-item group";
  const hint = "text-[11px] text-muted group-hover:text-on-accent";
  const Separator = () => <div class="mx-1 my-1 h-px bg-line" />;
  const msg = () => t().project.menu;
  return (
    <div ref={root} class="anim-pop absolute left-0 top-8 z-40 w-64 rounded border border-field-line bg-bg p-1 shadow-xl" role="menu">
      <button class={item} onClick={choose(() => files.newProject())}>{msg().newEmpty}<span class={hint}>⌘N</span></button>
      <button class={item} onClick={choose(() => files.newProject(true))}>{msg().newDemo}</button>
      <button class={item} onClick={() => setTemplatesOpen(true)}>{msg().newFromTemplate}</button>
      <button class={item} onClick={choose(() => files.openProject())}>{msg().open}<span class={hint}>⌘O</span></button>
      <button class={item} onClick={choose(() => files.saveProject())}>{msg().save}<span class={hint}>⌘S</span></button>
      <button class={item} onClick={choose(() => files.saveProjectAs())}>{msg().saveAs}<span class={hint}>⇧⌘S</span></button>
      <button class={item} onClick={() => setTemplatesOpen(true)}>{msg().saveAsTemplate}</button>
      <Show when={recent().filter((r) => r.path !== status().path).length > 0}>
        <div>
          <Separator />
          <div class="px-2.5 pb-0.5 pt-1 text-[11px] font-semibold uppercase tracking-wide text-muted">{msg().recent}</div>
          <For each={recent().filter((r) => r.path !== status().path).slice(0, 6)}>
            {(entry) => (
              <button class={item} title={entry.path} onClick={choose(() => files.openRecent(entry.path))}>
                <span class="truncate">{entry.name}</span>
              </button>
            )}
          </For>
        </div>
      </Show>
      <Separator />
      <button class={item} onClick={choose(async () => shell.showSettings("audio"))}>{msg().audio}</button>
      <button class={item} onClick={choose(async () => shell.showSettings("plugins"))}>{msg().plugins}</button>
      <button class={item} onClick={choose(async () => shell.showSettings("log"))}>{msg().errorLog}</button>
      <Separator />
      <button class={item} onClick={() => setExporting(true)}>{msg().exportAudio}</button>
      <Show when={status().path}>
        {(path) => <div class="mt-1 truncate border-t border-line px-2.5 pb-0.5 pt-1.5 text-[11px] text-muted" title={path()}>{path()}</div>}
      </Show>
      <Show when={templatesOpen()}>
        <TemplatesDialog canSave onClose={() => { setTemplatesOpen(false); props.onClose(); }} />
      </Show>
      <Show when={exporting()}>
        <ExportDialog onClose={() => { setExporting(false); props.onClose(); }} />
      </Show>
    </div>
  );
}
