// Welcome screen while no project is open (VS Code "Welcome" page): Start links on the left, recent projects on the right,
// over drifting waveforms (`WelcomeBackdrop`).
import { For, Show, createSignal, onMount } from "solid-js";
import type { RecentProject } from "../api/types";
import { t } from "../i18n";
import { useSession } from "../state/session";
import { IconAdd, IconFolder, IconNote, IconTracks } from "./common/icons";
import { TemplatesDialog } from "./transport/TemplatesDialog";
import { WelcomeBackdrop } from "./WelcomeBackdrop";

/**
 * Shown without an open project (first run, or the project was deleted by hand): nothing else can be opened
 * until there is a project. Disappears as soon as a project is created or opened.
 */
export function WelcomeScreen() {
  const session = useSession();
  const [recent, setRecent] = createSignal<RecentProject[]>([]);
  const [templatesOpen, setTemplatesOpen] = createSignal(false);

  onMount(() => void session.actions.project.recent().then(setRecent));

  const link = "flex w-full cursor-pointer items-center gap-2 rounded px-1.5 py-1 text-left text-sm text-accent hover:bg-hover";
  const project = session.actions.project;

  return (
    <div class="fixed inset-0 z-50 flex items-center justify-center overflow-auto bg-bg p-8" role="dialog" aria-label={t().shell.welcome.noProject}>
      <WelcomeBackdrop />
      {/* The blocks rise in one after another (`anim-rise`, staggered by `animation-delay`). */}
      <div class="relative w-[760px] max-w-full">
        <h1 class="anim-rise text-[32px] font-light leading-tight">{t().shell.welcome.title}</h1>
        <p class="anim-rise mb-8 mt-2 max-w-xl text-sm text-muted" style={{ "animation-delay": "70ms" }}>{t().shell.welcome.intro}</p>
        <div class="grid gap-10 sm:grid-cols-2">
          <section class="anim-rise" style={{ "animation-delay": "140ms" }}>
            <h2 class="mb-2 text-[15px] font-semibold">{t().shell.welcome.start}</h2>
            <button class={link} onClick={() => void project.newProject()}><IconAdd />{t().shell.welcome.empty}</button>
            <button class={link} title={t().shell.welcome.demoHint} onClick={() => void project.newProject(true)}><IconNote />{t().shell.welcome.demo}</button>
            <button class={link} onClick={() => setTemplatesOpen(true)}><IconTracks />{t().shell.welcome.fromTemplate}</button>
            <button class={link} onClick={() => void project.openProject()}><IconFolder />{t().shell.welcome.openExisting}</button>
          </section>
          <Show when={recent().length > 0}>
            <section class="anim-rise min-w-0" style={{ "animation-delay": "210ms" }}>
              <h2 class="mb-2 text-[15px] font-semibold">{t().shell.welcome.recent}</h2>
              <For each={recent().slice(0, 8)}>
                {(entry) => (
                  <button
                    class="flex w-full cursor-pointer items-baseline gap-3 rounded px-1.5 py-1 text-left hover:bg-hover"
                    title={entry.path}
                    onClick={() => void project.openRecent(entry.path)}
                  >
                    <span class="shrink-0 text-sm text-accent">{entry.name}</span>
                    <span class="min-w-0 truncate text-xs text-muted">{entry.path}</span>
                  </button>
                )}
              </For>
            </section>
          </Show>
        </div>
      </div>
      <Show when={templatesOpen()}>
        <TemplatesDialog canSave={false} onClose={() => setTemplatesOpen(false)} />
      </Show>
    </div>
  );
}
