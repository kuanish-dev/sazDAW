// Interface session: what the UI gets (`Session`). It is put together from role-based parts; a part the UI reads in
// pieces says itself what those are (its `…Reads` type, next to its code: the project copy, playback, recording,
// notifications), the others the UI uses as whole objects. This file only creates the parts, wires them and starts the
// session. Only reads and intents reach the UI. Depends only on the `RpcClient`, dialog and shell contracts.
import { createContext, onCleanup, useContext, type JSX } from "solid-js";
import { type Actions, createActions } from "../actions";
import { createApi } from "../api/api";
import type { SystemDialogs } from "../api/dialogs";
import type { RpcClient } from "../api/rpc";
import type { AppShell } from "../api/shell";
import type { Preferences } from "../api/types";
import { createActionContext } from "./actionContext";
import { type Appearance, createAppearance } from "./appearance";
import { createDrumKit, type DrumKit } from "./drumKit";
import { createKeyboardPiano, type KeyboardPiano } from "./keyboardPiano";
import { createLevels, createReductions, type Levels, type Reductions } from "./levels";
import { type NodeSounds, createNodeSounds } from "./nodeSounds";
import { createTakeNotice, createToasts, type ToastReads } from "./notifications";
import { createPlayback, type PlaybackReads } from "./playback";
import { createPreferencesCopy } from "./preferences";
import { createProjectSync, type ProjectReads } from "./projectSync";
import { createRecordingStatus, type RecordingReads } from "./recordingStatus";
import { createSelection, type Selection } from "./selection";
import { createViews, type Views } from "./views";

export type { Tab } from "../actions";
export type { SettingsSection } from "./views";

/** What the UI may read and ask for. */
export type Session = ProjectReads &
  PlaybackReads &
  RecordingReads &
  ToastReads & {
    /** Everything that changes the backend or decides how a result is shown (`actions/`). */
    readonly actions: Actions;
    /** Label of a long operation (opening a project); `null` — none. */
    busy(): string | null;
    /** App preferences (`preferences.ts`) and the look made from them (`appearance.ts`). */
    preferences(): Preferences;
    readonly appearance: Appearance;
    /** Meters and gain reduction by node (`levels.ts`). */
    readonly levels: Levels;
    readonly reductions: Reductions;
    /** The live sound of the nodes the effect pictures show (`nodeSounds.ts`). */
    readonly nodeSounds: NodeSounds;
    /** Selection and what the window shows (`selection.ts`, `views.ts`, `drumKit.ts`). */
    readonly selection: Selection;
    readonly ui: Views;
    readonly drumKit: DrumKit;
    /** The computer keyboard as a piano on the selected track (`keyboardPiano.ts`). */
    readonly keyboard: KeyboardPiano;
  };

export function createSession(rpc: RpcClient, dialogs: SystemDialogs, shell: AppShell): Session {
  const api = createApi(rpc);
  const toasts = createToasts();
  const playback = createPlayback(rpc);
  const sync = createProjectSync({ api, rpc, onError: toasts.showFailure });
  const selection = createSelection(sync.project);
  sync.onLoaded(selection.prune);
  const ui = createViews(selection.selectTrack);
  const preferences = createPreferencesCopy();
  const recording = createRecordingStatus(api.audioRecord.status, toasts.showFailure);
  onCleanup(recording.dispose);

  const { context, busy } = createActionContext({
    backend: { api, dialogs, shell },
    project: sync,
    playback,
    preferences,
    notices: toasts,
    selection,
    views: ui,
    recording,
  });
  const actions = createActions(context);
  createTakeNotice(() => playback.tick().record, toasts.toast);

  const appearance = createAppearance(preferences.preferences, shell);
  const levels = createLevels(playback.tick);
  const reductions = createReductions(playback.tick);
  const nodeSounds = createNodeSounds(actions.audio);
  const drumKit = createDrumKit();
  const keyboard = createKeyboardPiano({ selectedTrack: selection.track, live: actions.live });

  void startSession(actions, sync);
  // Track arming lives in the backend longer than the window: fetch it right away (polling sleeps if nothing is armed).
  recording.followRecording();

  return {
    // Spread whole: `Session` lets the UI see only their `…Reads`; the rest of them is for the wiring above.
    ...sync,
    ...playback,
    ...recording,
    ...toasts,
    actions,
    busy,
    preferences: preferences.preferences,
    appearance,
    levels,
    reductions,
    nodeSounds,
    selection,
    ui,
    drumKit,
    keyboard,
  };
}

/** Preferences first (projects folder, "open last project"), then the project, then what opening it left to say:
 *  everything is in place when the views show. */
async function startSession(actions: Actions, sync: { start(): Promise<void> }) {
  await actions.preferences.load();
  await sync.start();
  await actions.startup.afterFirstRead();
}

const SessionContext = createContext<Session>();

export function SessionProvider(props: { session: Session; children: JSX.Element }) {
  return <SessionContext.Provider value={props.session}>{props.children}</SessionContext.Provider>;
}

export function useSession(): Session {
  const session = useContext(SessionContext);
  if (!session) throw new Error("SessionProvider is missing");
  return session;
}
