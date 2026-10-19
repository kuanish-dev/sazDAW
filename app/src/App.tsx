// Root component: creates the session, installs window keys and lays out the VS Code-like shell: transport strip on top,
// activity bar on the left, the chosen view, status bar at the bottom (with the hover hint), floating windows and toasts.
import { type JSX, Show, onCleanup, onMount } from "solid-js";
import { Dynamic } from "solid-js/web";
import type { SystemDialogs } from "./api/dialogs";
import type { RpcClient } from "./api/rpc";
import type { AppShell } from "./api/shell";
import { t } from "./i18n";
import { installGlobalKeys } from "./shell/globalKeys";
import { createSession, SessionProvider, type Tab } from "./session";
import { installTouchRelease } from "./automation/touchRelease";
import { Toasts } from "./common/Toasts";
import { GraphView } from "./graph/GraphView";
import { LoadingOverlay } from "./shell/LoadingOverlay";
import { MixerView } from "./mixer/MixerView";
import { SampleEditorView } from "./samples/SampleEditorView";
import { SettingsView } from "./settings/SettingsView";
import { StageView } from "./stage/StageView";
import { ActivityBar } from "./shell/ActivityBar";
import { StatusBar } from "./shell/StatusBar";
import { InstrumentWindow } from "./instruments/InstrumentWindow";
import { TracksView } from "./tracks/TracksView";
import { TransportBar } from "./transport/TransportBar";
import { WelcomeScreen } from "./shell/WelcomeScreen";

/** The main view of each activity bar tab. */
const VIEWS: Record<Tab, () => JSX.Element> = {
  tracks: TracksView,
  graph: GraphView,
  mixer: MixerView,
  stage: StageView,
  samples: SampleEditorView,
  settings: SettingsView,
};

export function App(props: { rpc: RpcClient; dialogs: SystemDialogs; shell: AppShell }) {
  const session = createSession(props.rpc, props.dialogs, props.shell);

  // Window-wide listeners: keys, the computer keyboard piano, the release of live-written controls.
  onMount(() => {
    onCleanup(installGlobalKeys(session));
    onCleanup(session.keyboard.install());
    onCleanup(installTouchRelease(session));
  });

  return (
    <SessionProvider session={session}>
      <Show when={session.ready() && !session.hasProject()}><WelcomeScreen /></Show>
      <LoadingOverlay text={!session.ready() ? t().shell.openingProject : session.busy()} />
      <div class="flex h-full flex-col">
        <TransportBar />
        <div class="flex min-h-0 flex-1">
          <ActivityBar />
          <main class="min-h-0 min-w-0 flex-1 overflow-auto bg-bg">
            <Show when={session.ready()} fallback={null}>
              {/* Keyed by the tab: the wrapper is created anew on a switch, so the new view eases in (`anim-view`). */}
              <Show when={session.ui.tab()} keyed>
                {(tab) => <div class="anim-view h-full"><Dynamic component={VIEWS[tab]} /></div>}
              </Show>
            </Show>
          </main>
        </div>
        <StatusBar />
        <Show when={session.ui.instrumentOpen()}><InstrumentWindow /></Show>
        <Toasts />
      </div>
    </SessionProvider>
  );
}
