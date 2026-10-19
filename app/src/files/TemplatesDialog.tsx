// "Project templates" dialog: save the current project's structure as a template, create a new project from a template,
// delete a template. Opened from the project menu ("New from template…", "Save as template…") and the welcome screen.
import { For, Show, createResource, createSignal } from "solid-js";
import { t } from "../i18n";
import { Modal } from "../common/Modal";
import { IconClose } from "../common/icons";
import { useFiles } from "./role";

export function TemplatesDialog(props: { onClose: () => void; canSave: boolean }) {
  const files = useFiles();
const { templates } = useFiles();
  const [names, { refetch }] = createResource(() => templates.list());
  const [name, setName] = createSignal("");

  const save = async () => {
    if (await templates.saveCurrent(name())) {
      setName("");
      void refetch();
    }
  };
  const create = (template: string) => {
    props.onClose();
    void files.newFromTemplate(template);
  };
  const remove = async (template: string) => {
    if (await templates.remove(template)) void refetch();
  };

  const msg = () => t().presets.templates;

  return (
    <Modal title={msg().title} onClose={props.onClose}>
      <Show when={props.canSave}>
        <div class="mb-4">
          <div class="label mb-1">{msg().saveCurrent}</div>
          <p class="mb-2 text-[11px] text-muted">{msg().saveHint}</p>
          <div class="flex gap-2">
            <input class="field min-w-0 flex-1 text-sm" placeholder={msg().name} aria-label={msg().name} value={name()}
              onInput={(e) => setName(e.currentTarget.value)} onKeyDown={(e) => e.key === "Enter" && void save()} />
            <button class="btn btn-primary" disabled={!name().trim()} onClick={() => void save()}>{t().common.save}</button>
          </div>
        </div>
      </Show>
      <div class="label mb-1">{msg().newFrom}</div>
      <Show when={(names() ?? []).length > 0} fallback={<p class="text-sm text-muted">{msg().empty}</p>}>
        <div class="flex flex-col">
          <For each={names()}>
            {(template) => (
              <div class="flex items-center gap-2 rounded px-2 py-1 hover:bg-hover">
                <span class="min-w-0 flex-1 truncate text-sm">{template}</span>
                <button class="btn !h-7 text-xs" onClick={() => create(template)}>{msg().create}</button>
                <button class="btn btn-icon hover:!text-danger" title={msg().removeHint} aria-label={msg().remove(template)}
                  onClick={() => void remove(template)}><IconClose size={14} /></button>
              </div>
            )}
          </For>
        </div>
      </Show>
    </Modal>
  );
}
