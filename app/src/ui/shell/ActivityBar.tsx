// Activity bar: the narrow icon column on the left (as in VS Code) that switches the main view; Settings sits at the
// bottom. The active view has an accent bar on its left edge; the name shows as a tooltip on hover and keyboard focus.
import { For, type JSX } from "solid-js";
import { t } from "../../i18n";
import { type Tab, useSession } from "../../state/session";
import { IconGear, IconGraph, IconMixer, IconStage, IconTracks, IconWave, type IconProps } from "../common/icons";

const VIEWS: { tab: Tab; icon: (p: IconProps) => JSX.Element }[] = [
  { tab: "tracks", icon: IconTracks },
  { tab: "graph", icon: IconGraph },
  { tab: "mixer", icon: IconMixer },
  { tab: "stage", icon: IconStage },
  { tab: "samples", icon: IconWave },
];

function ActivityButton(props: { tab: Tab; icon: (p: IconProps) => JSX.Element }) {
  const session = useSession();
  const active = () => session.ui.tab() === props.tab;
  const name = () => t().shell.tabs[props.tab];
  return (
    <button
      class="group relative flex h-12 w-12 cursor-pointer items-center justify-center"
      classList={{ "text-text": active(), "text-muted hover:text-text": !active() }}
      aria-label={name()} aria-current={active() ? "page" : undefined}
      onClick={() => session.ui.showTab(props.tab)}
    >
      {/* The accent bar grows from the middle when the view becomes active. */}
      <span class="absolute inset-y-0 left-0 w-0.5 bg-accent transition-[scale] duration-200 ease-out" classList={{ "scale-y-100": active(), "scale-y-0": !active() }} />
      <props.icon size={24} />
      <span class="anim-fade pointer-events-none absolute left-full top-1/2 z-50 ml-1 hidden -translate-y-1/2 whitespace-nowrap rounded border border-field-line bg-panel px-2 py-0.5 text-xs text-text shadow-md group-hover:block group-focus-visible:block">
        {name()}
      </span>
    </button>
  );
}

export function ActivityBar() {
  return (
    <nav class="flex w-12 shrink-0 flex-col border-r border-line bg-panel" aria-label={t().shell.sections}>
      <For each={VIEWS}>{(view) => <ActivityButton tab={view.tab} icon={view.icon} />}</For>
      <div class="mt-auto">
        <ActivityButton tab="settings" icon={IconGear} />
      </div>
    </nav>
  );
}
