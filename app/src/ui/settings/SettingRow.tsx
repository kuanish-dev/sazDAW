// Shared building blocks of the settings page: section title, group of rows, a "label — value" row,
// a switch and a Copy button. Layout only: values and actions come from the section.
import { Show, createSignal, type JSX } from "solid-js";
import { t } from "../../i18n";


export function SectionTitle(props: { title: string; hint?: string }) {
  return (
    <header class="mb-2 border-b border-line pb-3">
      <h2 class="text-[22px] font-semibold leading-tight">{props.title}</h2>
      <Show when={props.hint}><p class="mt-1 text-sm text-muted">{props.hint}</p></Show>
    </header>
  );
}

export function SettingGroup(props: { title?: string; children: JSX.Element }) {
  return (
    <section class="mt-5">
      <Show when={props.title}><h3 class="mb-1 px-3 text-[15px] font-semibold">{props.title}</h3></Show>
      <div class="flex flex-col">{props.children}</div>
    </section>
  );
}

/** A setting: label and hint on the left, control on the right. */
export function SettingRow(props: { label: string; hint?: string; children: JSX.Element }) {
  return (
    <div class="flex flex-wrap items-center justify-between gap-x-6 gap-y-2 rounded px-3 py-2.5 hover:bg-hover/60">
      <div class="min-w-0 flex-1 basis-56">
        <div class="text-sm font-semibold">{props.label}</div>
        <Show when={props.hint}><div class="mt-0.5 text-[12px] text-muted">{props.hint}</div></Show>
      </div>
      <div class="flex shrink-0 items-center gap-2">{props.children}</div>
    </div>
  );
}

export function Toggle(props: { checked: boolean; label: string; onChange: (checked: boolean) => void }) {
  return (
    <button
      role="switch" aria-checked={props.checked} aria-label={props.label}
      class="relative h-[18px] w-8 shrink-0 cursor-pointer rounded-full border transition-colors"
      classList={{ "border-accent bg-accent": props.checked, "border-field-line bg-field": !props.checked }}
      onClick={() => props.onChange(!props.checked)}
    >
      <span
        class="absolute top-[2px] h-3 w-3 rounded-full transition-all"
        classList={{ "left-[16px] bg-on-accent": props.checked, "left-[2px] bg-muted": !props.checked }}
      />
    </button>
  );
}

/** Copies text to the clipboard; after success the caption reads "Copied" (a failure goes to the log). */
export function CopyButton(props: { text: () => string; label?: string; disabled?: boolean }) {
  const [copied, setCopied] = createSignal(false);
  const copy = async () => {
    try {
      await navigator.clipboard.writeText(props.text());
      setCopied(true);
    } catch (error) {
      console.error("clipboard", error);
      setCopied(false);
    }
  };
  return <button class="btn !h-7 text-xs" disabled={props.disabled} onClick={() => void copy()}>{copied() ? t().settings.copied : (props.label ?? t().settings.copy)}</button>;
}
