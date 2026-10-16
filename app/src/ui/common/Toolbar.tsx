// View toolbars in the VS Code manner: a thin strip with a hairline bottom border, square icon buttons (optionally
// with a text label) and hairline dividers between groups. Shared by the timeline, graph, piano roll and sample editor.
import { Show, type JSX } from "solid-js";

/** The strip itself; `class` adds placement (e.g. `sticky top-0`). */
export function Toolbar(props: { label?: string; class?: string; children: JSX.Element }) {
  return (
    <div
      role="toolbar" aria-label={props.label}
      class={`flex min-h-9 shrink-0 flex-wrap items-center gap-x-1 gap-y-1 border-b border-line bg-panel px-2 py-1 ${props.class ?? ""}`}
    >
      {props.children}
    </div>
  );
}

/** A thin vertical divider between groups of a toolbar. */
export const ToolbarDivider = () => <div class="mx-1 h-4 w-px shrink-0 bg-line" aria-hidden="true" />;

/**
 * A toolbar button with an icon: square without `text`, or the icon followed by the label. `pressed` makes it a toggle
 * (lit while on). The label is always the accessible name; `title` is the hover hint.
 */
export function ToolButton(props: {
  icon: JSX.Element;
  label: string;
  title?: string;
  text?: boolean;
  pressed?: boolean;
  disabled?: boolean;
  class?: string;
  onClick: (event: MouseEvent) => void;
}) {
  return (
    <button
      class={`btn ${props.text ? "btn-ghost !px-2" : "btn-icon"} ${props.class ?? ""}`}
      classList={{ "btn-on !text-on-accent": props.pressed === true }}
      aria-pressed={props.pressed}
      aria-label={props.label}
      title={props.title ?? props.label}
      disabled={props.disabled}
      onClick={(e) => props.onClick(e)}
    >
      {props.icon}
      <Show when={props.text}><span>{props.label}</span></Show>
    </button>
  );
}
