// Preset row above instrument or effect parameters: the kind's preset list (picking one applies it), "Save…"
// (the name is typed right here) and "✕" (delete the selected one). Shared by the instrument window and `EffectPanel`.
import { For, Show, createResource, createSignal } from "solid-js";
import type { EditorTarget } from "../api/types";
import { t } from "../i18n";
import { useSession } from "../session";
import { IconClose } from "./icons";

export function PresetBar(props: { target: EditorTarget; kind: string }) {
  const { presets } = useSession();
  const [names, { refetch }] = createResource(() => props.kind, (kind) => presets.list(kind));
  /** The last applied or saved preset: the one "✕" deletes. */
  const [current, setCurrent] = createSignal("");
  /** The new preset name while the save field is open; `null` when closed. */
  const [draft, setDraft] = createSignal<string | null>(null);

  const apply = (name: string) => {
    setCurrent(name);
    if (name) void presets.apply(props.target, name);
  };
  const save = async () => {
    const name = draft() ?? "";
    if (await presets.save(props.target, props.kind, name)) {
      setDraft(null);
      setCurrent(name.trim());
      void refetch();
    }
  };
  const remove = async () => {
    if (await presets.remove(props.kind, current())) {
      setCurrent("");
      void refetch();
    }
  };

  const msg = () => t().presets.bar;

  return (
    <div class="flex items-center gap-1">
      <Show
        when={draft() !== null}
        fallback={
          <>
            <select class="field !h-7 min-w-0 flex-1 text-xs" aria-label={msg().preset} value={current()} onChange={(e) => apply(e.currentTarget.value)}>
              <option value="">{(names() ?? []).length > 0 ? msg().choose : msg().empty}</option>
              <For each={names() ?? []}>{(name) => <option value={name} selected={name === current()}>{name}</option>}</For>
            </select>
            <button class="btn !h-7 shrink-0 text-xs" title={msg().saveHint} onClick={() => setDraft(current())}>{msg().save}</button>
            <button class="btn btn-icon shrink-0 hover:!text-danger" title={msg().removeHint} aria-label={msg().remove}
              disabled={!current()} onClick={() => void remove()}><IconClose size={14} /></button>
          </>
        }
      >
        <input
          class="field !h-7 min-w-0 flex-1 text-xs" placeholder={msg().name} aria-label={msg().name} value={draft() ?? ""}
          ref={(el) => queueMicrotask(() => el.focus())}
          onInput={(e) => setDraft(e.currentTarget.value)}
          onKeyDown={(e) => {
            if (e.key === "Enter") void save();
            if (e.key === "Escape") setDraft(null);
          }}
        />
        <button class="btn btn-primary !h-7 shrink-0 text-xs" onClick={() => void save()}>{t().common.ok}</button>
        <button class="btn !h-7 shrink-0 text-xs" onClick={() => setDraft(null)}>{t().common.cancel}</button>
      </Show>
    </div>
  );
}
