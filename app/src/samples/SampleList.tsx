// List of the project's sounds on the left of the Samples tab: a click opens the sound in the editor.
import { For } from "solid-js";
import { t } from "../i18n";
import { useSession } from "../session";
import { IconWave } from "../common/icons";

export function SampleList() {
  const session = useSession();
  const { selection } = session;
  return (
    <section class="flex w-56 shrink-0 flex-col border-r border-line bg-panel">
      <h2 class="section-header !h-[30px]">{t().samples.projectSounds}</h2>
      <div class="min-h-0 flex-1 overflow-auto">
        <For each={session.project.samples} fallback={<p class="px-3 py-2 text-xs text-muted">{t().samples.emptyList}</p>}>
          {(s) => (
            <button
              class="flex w-full cursor-pointer items-center gap-2 px-3 py-1 text-left"
              classList={{ "bg-select": s.id === selection.sample(), "hover:bg-hover": s.id !== selection.sample() }}
              aria-current={s.id === selection.sample() ? "true" : undefined}
              onClick={() => selection.selectSample(s.id)}
            >
              <IconWave size={14} class="text-muted" />
              <span class="min-w-0 flex-1">
                <span class="block truncate text-sm">{s.name}</span>
                <span class="block text-[11px] text-muted">{t().samples.markers(s.markers.length)}</span>
              </span>
            </button>
          )}
        </For>
      </div>
    </section>
  );
}
