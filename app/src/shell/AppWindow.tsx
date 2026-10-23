import { type JSX, Show, onCleanup, onMount } from "solid-js";
import { Dynamic } from "solid-js/web";
import { t } from "../i18n";
import { installGlobalKeys } from "./globalKeys";
import { useAutomation } from "../automation/role";
import { installTouchRelease } from "../automation/touchRelease";
import { Toasts } from "../common/Toasts";
import { GraphView } from "../graph/GraphView";
import { LoadingOverlay } from "./LoadingOverlay";
import { MixerView } from "../mixer/MixerView";
import { SampleEditorView } from "../samples/SampleEditorView";
import { SettingsView } from "../settings/SettingsView";
import { StageView } from "../stage/StageView";
import { ActivityBar } from "./ActivityBar";
import { StatusBar } from "./StatusBar";
import { InstrumentWindow } from "../instruments/InstrumentWindow";
import { TracksView } from "../tracks/TracksView";
import { TransportBar } from "../transport/TransportBar";
import { WelcomeScreen } from "./WelcomeScreen";
import { useBusy, useProjectCopy } from "../core/roles";
import { useFiles } from "../files/role";
import { useHistory } from "../history/role";
import { useKeyboard } from "../keyboard/role";
import { useSelection } from "../selection/role";
import { useTracks } from "../tracks/role";
import { useTransport } from "../transport/role";
import { useShell } from "./role";
import type { Tab } from "./views";

/** The main view of each activity bar tab. */
const VIEWS: Readonly<Record<Tab, () => JSX.Element>> = {
  tracks: TracksView,
  graph: GraphView,
  mixer: MixerView,
  stage: StageView,
  samples: SampleEditorView,
  settings: SettingsView,
} as const;

export function AppWindow() {
  const copy = useProjectCopy();
  const busy = useBusy();
  const shell = useShell();
  const keyboard = useKeyboard();
  const automation = useAutomation();
  const keys = {
    copy, shell, automation, transport: useTransport(), history: useHistory(), files: useFiles(), tracks: useTracks(), selection: useSelection(),
  };

  onMount(() => {
    onCleanup(installGlobalKeys(keys));
    onCleanup(keyboard.install());
    onCleanup(installTouchRelease(automation));
  });

  return (
    <>
      <Show when={copy.ready() && !copy.hasProject()}><WelcomeScreen /></Show>
      <LoadingOverlay text={!copy.ready() ? t().shell.openingProject : busy()} />
      <div class="flex h-full flex-col">
        <TransportBar />
        <div class="flex min-h-0 flex-1">
          <ActivityBar />
          <main class="min-h-0 min-w-0 flex-1 overflow-auto bg-bg">
            <Show when={copy.ready()} fallback={null}>
              <Show when={shell.tab()} keyed>
                {(tab) => <div class="anim-view h-full"><Dynamic component={VIEWS[tab]} /></div>}
              </Show>
            </Show>
          </main>
        </div>
        <StatusBar />
        <Show when={shell.instrumentOpen()}><InstrumentWindow /></Show>
        <Toasts />
      </div>
    </>
  );
}
