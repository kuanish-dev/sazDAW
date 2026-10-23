import { Show } from "solid-js";

export function LoadingOverlay(props: { text: string | null }) {
  return (
    <Show when={props.text}>
      {(text) => (
        <div class="anim-fade fixed inset-0 z-50 flex items-center justify-center bg-bg/70" role="status" aria-live="polite" aria-label={text()}>
          <div class="w-72 overflow-hidden rounded border border-field-line bg-panel shadow-xl">
            <div class="relative h-0.5 overflow-hidden bg-line"><div class="loading-bar absolute inset-y-0 w-1/3 bg-accent" /></div>
            <div class="flex items-center gap-2.5 px-4 py-3">
              <div class="h-3.5 w-3.5 shrink-0 animate-spin rounded-full border-[1.5px] border-field-line border-t-accent" />
              <div class="truncate text-sm">{text()}</div>
            </div>
          </div>
        </div>
      )}
    </Show>
  );
}
