// In-place rename: an input replaces the label; Enter or losing focus saves, Esc cancels.
import { Show, type JSX } from "solid-js";
import { t } from "../i18n";

export function InlineRename(props: {
  name: string;
  editing: boolean;
  /** Input classes (sized for where the label sits). */
  class: string;
  /** The new name, if non-empty and different from the old one. */
  onRename: (name: string) => void;
  /** Editing finished (saved or cancelled). */
  onClose: () => void;
  /** The label while not editing. */
  children: JSX.Element;
}) {
  const finish = (value: string) => {
    props.onClose();
    const trimmed = value.trim();
    if (trimmed && trimmed !== props.name) props.onRename(trimmed);
  };

  return (
    <Show when={props.editing} fallback={props.children}>
      <input
        class={`field ${props.class}`}
        value={props.name}
        aria-label={t().controls.newName}
        ref={(el) => queueMicrotask(() => { el.focus(); el.select(); })}
        // A press in the field is text input, not a drag or selection of what the field sits on.
        onPointerDown={(e) => e.stopPropagation()}
        onBlur={(e) => finish(e.currentTarget.value)}
        onKeyDown={(e) => {
          if (e.key === "Enter") e.currentTarget.blur();
          if (e.key === "Escape") props.onClose();
        }}
      />
    </Show>
  );
}
