import { For } from "solid-js";
import { t } from "../i18n";
import { IconClose, IconError, IconInfo } from "./icons";
import { useToasts } from "../core/roles";

export function Toasts() {
  const { dismissToast, toasts } = useToasts();
  return (
    <div class="pointer-events-none fixed bottom-8 right-3 z-[60] flex w-96 flex-col gap-2">
      <For each={toasts()}>
        {(toast) => (
          <div
            class="anim-toast pointer-events-auto flex items-start gap-2 rounded border border-field-line bg-panel py-2 pl-3 pr-1.5 text-sm shadow-xl"
            classList={{ "border-l-2 !border-l-danger": toast.kind === "error", "border-l-2 !border-l-accent": toast.kind === "info" }}
            role="alert"
          >
            <span class="mt-0.5" classList={{ "text-danger": toast.kind === "error", "text-accent": toast.kind === "info" }}>
              {toast.kind === "error" ? <IconError /> : <IconInfo />}
            </span>
            <span class="flex-1 break-words">{toast.text}</span>
            <button class="btn btn-icon !h-6 !w-6 !min-w-6" onClick={() => dismissToast(toast.id)} aria-label={t().common.close}><IconClose size={14} /></button>
          </div>
        )}
      </For>
    </div>
  );
}
