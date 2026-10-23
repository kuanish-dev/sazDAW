import { Show, type JSX } from "solid-js";
import { t } from "../i18n";
import { IconClose } from "./icons";

export function Modal(props: { title: string; onClose: () => void; children: JSX.Element; footer?: JSX.Element; width?: string }) {
  return (
    <div
      class="anim-fade fixed inset-0 z-50 flex items-center justify-center bg-black/40"
      onMouseDown={(e) => e.target === e.currentTarget && props.onClose()}
      onKeyDown={(e) => e.key === "Escape" && props.onClose()}
    >
      <div
        class={`anim-pop origin-center flex max-h-[85vh] max-w-[94vw] flex-col rounded border border-field-line bg-bg shadow-2xl ${props.width ?? "w-[460px]"}`}
        role="dialog" aria-label={props.title}
      >
        <div class="flex h-9 shrink-0 items-center justify-between border-b border-line pl-4 pr-1.5">
          <h2 class="truncate text-sm font-semibold">{props.title}</h2>
          <button class="btn btn-icon !h-6 !w-6 !min-w-6" onClick={props.onClose} aria-label={t().common.close}><IconClose size={14} /></button>
        </div>
        <div class="min-h-0 overflow-auto p-4">{props.children}</div>
        <Show when={props.footer}>
          <div class="flex shrink-0 justify-end gap-2 border-t border-line px-4 py-2.5">{props.footer}</div>
        </Show>
      </div>
    </div>
  );
}
