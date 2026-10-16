// Interface session: what the UI gets (`Session`), assembled from role-based parts — the project copy, playback,
// selection and what is open, notifications, preferences and appearance — and the actions layer. Only reads and
// intents reach the UI. Depends only on the `RpcClient`, dialog and shell contracts.
import { createContext, onCleanup, useContext, type JSX } from "solid-js";
import { type Actions, createActions } from "../actions";
import { createApi } from "../api/api";
import type { SystemDialogs } from "../api/dialogs";
import type { RpcClient } from "../api/rpc";
import type { AppShell } from "../api/shell";
import type { AudioRecordStatus, Catalog, Id, Preferences, Project, Status, Tick, TrackRecordState } from "../api/types";
import { createActionContext } from "./actionContext";
import { type Appearance, createAppearance } from "./appearance";
import { createDrumKit, type DrumKit } from "./drumKit";
import { createKeyboardPiano, type KeyboardPiano } from "./keyboardPiano";
import { createLevels, createReductions, type Levels, type Reductions } from "./levels";
import { type NodeSounds, createNodeSounds } from "./nodeSounds";
import { createTakeNotice, createToasts, type Toast } from "./notifications";
import { createPlayback } from "./playback";
import { createPreferencesCopy } from "./preferences";
import { createProjectSync } from "./projectSync";
import { createRecordingStatus } from "./recordingStatus";
import { createSelection, type Selection } from "./selection";
import { createViews, type Views } from "./views";

export type { Tab } from "../actions";
export type { SettingsSection } from "./views";

/** What the UI may read and ask for, by role. */
export interface Session {
  /** Everything that changes the backend or decides how a result is shown (`actions/`). */
  readonly actions: Actions;

  // The project copy (`projectSync.ts`).
  readonly project: Project;
  status(): Status;
  catalog(): Catalog | null;
  /** The first project read is done: the views can be shown. */
  ready(): boolean;
  /** A project is open (saved in its own folder). */
  hasProject(): boolean;
  /** Label of a long operation (opening a project); `null` — none. */
  busy(): string | null;

  // App preferences and the look made from them (`preferences.ts`, `appearance.ts`).
  preferences(): Preferences;
  readonly appearance: Appearance;

  // Playback and meters (`playback.ts`, `levels.ts`).
  /** The latest backend tick (~30 per second): position, transport state, meters, load. */
  tick(): Tick;
  /** Position for canvases; `null` while stopped. Fires only when the value changes. */
  playhead(): number | null;
  /** Sample audition position in file frames; `null` when nothing is playing. */
  previewPlayhead(): number | null;
  /** Position on every screen frame (extrapolated between ticks), for light elements only. */
  smoothPlayhead(): number | null;
  readonly levels: Levels;
  /** Gain reduction of compressors, gates and limiters, dB by node. */
  readonly reductions: Reductions;
  /** The live sound of the nodes the effect pictures show (`nodeSounds.ts`). */
  readonly nodeSounds: NodeSounds;

  // Audio recording (`recordingStatus.ts`).
  /** The audio recording take in progress (null — none). */
  recordingStatus(): AudioRecordStatus | null;
  /** Audio track input: arming, channels, levels. */
  trackInput(track: Id): TrackRecordState;
  /** Why input monitoring is silent (null — all fine). */
  monitorProblem(): string | null;

  // Selection and what the window shows (`selection.ts`, `views.ts`, `drumKit.ts`).
  readonly selection: Selection;
  readonly ui: Views;
  readonly drumKit: DrumKit;

  /** The computer keyboard as a piano on the selected track (`keyboardPiano.ts`). */
  readonly keyboard: KeyboardPiano;

  // Notifications (`notifications.ts`).
  toasts(): Toast[];
  dismissToast(id: number): void;
}

export function createSession(rpc: RpcClient, dialogs: SystemDialogs, shell: AppShell): Session {
  const api = createApi(rpc);
  const toasts = createToasts();
  const playback = createPlayback(rpc);
  // `onLoaded` fires only after the first project read (`sync.start` below) — by then the selection already exists.
  const sync = createProjectSync({ api, rpc, onError: toasts.showFailure, onLoaded: (p) => selection.prune(p) });
  const selection = createSelection(sync.project);
  const ui = createViews(selection.selectTrack);
  const preferences = createPreferencesCopy();
  const recording = createRecordingStatus(api.audioRecord.status, toasts.showFailure);
  onCleanup(recording.dispose);

  const { context, busy } = createActionContext({
    backend: { api, dialogs, shell },
    project: {
      project: sync.project, status: sync.status, catalog: sync.catalog,
      refresh: sync.refresh, refreshCatalog: sync.refreshCatalog, patchProject: sync.patch,
    },
    playback: { tick: playback.tick },
    preferences,
    notices: toasts,
    selection,
    views: ui,
    recording: { followRecording: recording.follow },
  });
  const actions = createActions(context);
  createTakeNotice(() => playback.tick().record, toasts.toast);

  // Preferences first (projects folder, "open last project"), then the project, then what opening it left to say:
  // everything is in place when the views show.
  const start = async () => {
    await actions.preferences.load();
    await sync.start();
    await actions.startup.afterFirstRead();
  };
  void start();
  // Track arming lives in the backend longer than the window: fetch it right away (polling sleeps if nothing is armed).
  recording.follow();

  return {
    actions,
    project: sync.project,
    status: sync.status,
    catalog: sync.catalog,
    ready: sync.ready,
    hasProject: sync.hasProject,
    busy,
    preferences: preferences.preferences,
    appearance: createAppearance(preferences.preferences, shell),
    tick: playback.tick,
    playhead: playback.playhead,
    previewPlayhead: playback.previewPlayhead,
    smoothPlayhead: playback.smoothPlayhead,
    levels: createLevels(playback.tick),
    reductions: createReductions(playback.tick),
    nodeSounds: createNodeSounds(actions.audio),
    recordingStatus: recording.take,
    trackInput: recording.track,
    monitorProblem: () => recording.status()?.monitorProblem ?? null,
    selection,
    ui,
    drumKit: createDrumKit(),
    keyboard: createKeyboardPiano({ selectedTrack: selection.track, live: actions.live }),
    toasts: toasts.toasts,
    dismissToast: toasts.dismiss,
  };
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
