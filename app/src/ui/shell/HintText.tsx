// Hint in the status bar: explains the element under the cursor. Hints come from `title` (moved into `data-hint` so
// the system tooltip doesn't get in the way) or from `data-hint`. Mounted only while hints are on (Settings → Appearance).
import { createSignal, onCleanup, onMount } from "solid-js";
import { t } from "../../i18n";

const SELECTOR = "[data-hint], [title]";

export function HintText() {
  const [text, setText] = createSignal("");

  const hintOf = (target: EventTarget | null): string => {
    if (!(target instanceof Element)) return "";
    const element = target.closest<HTMLElement>(SELECTOR);
    if (!element) return "";
    const title = element.getAttribute("title");
    if (title) {
      element.dataset.hint = title;
      element.removeAttribute("title");
    }
    return element.dataset.hint ?? "";
  };

  onMount(() => {
    const over = (event: MouseEvent) => setText(hintOf(event.target));
    const leave = () => setText("");
    document.addEventListener("mouseover", over);
    document.addEventListener("mouseleave", leave);
    window.addEventListener("blur", leave);
    onCleanup(() => {
      document.removeEventListener("mouseover", over);
      document.removeEventListener("mouseleave", leave);
      window.removeEventListener("blur", leave);
    });
  });

  return (
    <span class="min-w-0 flex-1 truncate px-2" classList={{ "text-text": text() !== "", "text-muted": text() === "" }} role="status" aria-live="polite">
      {text() || t().shell.hintIdle}
    </span>
  );
}
