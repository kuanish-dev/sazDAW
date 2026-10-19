// The interface session: the app's roles, created and wired once, and what the UI gets of them (`Session`). Every role
// holds the shared core (`core/core.ts`) and asks it, the way the entities on the backend hold the project core; a role
// keeps its data, its work and its pictures in its own folder. The UI gets the core's reads and the roles, never the
// core itself, so no component calls the backend. Depends only on the `RpcClient`, dialog and shell contracts.
import { createContext, onCleanup, useContext, type JSX } from "solid-js";
import type { SystemDialogs } from "./api/dialogs";
import type { RpcClient } from "./api/rpc";
import type { AppShell } from "./api/shell";
import { type AutomationActions, createAutomationActions } from "./automation/automation";
import { createParamTouch } from "./automation/paramTouch";
import { type ChordsActions, createChordsActions } from "./chords/chords";
import { createCore, type CoreReads } from "./core/core";
import { createTakeNotice } from "./core/notifications";
import { createDrumsRole, type DrumsRole } from "./drums/role";
import { createEffectsRole, type EffectsRole } from "./effects/role";
import { createFilesRole, type FilesRole } from "./files/role";
import { createGraphActions, type GraphActions } from "./graph/graph";
import { createHistoryActions, type HistoryActions } from "./history/history";
import { createInstrumentsRole, type InstrumentsRole } from "./instruments/role";
import { createKeyboardRole, type KeyboardRole } from "./keyboard/role";
import { createMidiRole, type MidiRole } from "./midi/role";
import { createMixerRole, type MixerRole } from "./mixer/role";
import { createNotesActions, type NotesActions } from "./pianoroll/notes";
import { createPluginsRole, type PluginsRole } from "./plugins/role";
import { createPresetsActions, type PresetsActions } from "./presets/presets";
import { createRecordingRole, type RecordingRole } from "./recording/role";
import { createSamplesRole, type SamplesRole } from "./samples/role";
import { createSelection, type Selection } from "./selection/selection";
import { createSettingsRole, type SettingsRole } from "./settings/role";
import { createViews, type Views } from "./shell/views";
import { createSoundsRole, type SoundsRole } from "./sounds/role";
import { createStageActions, type StageActions } from "./stage/stage";
import { createTracksRole, type TracksRole } from "./tracks/role";
import { createTransportRole, type TransportRole } from "./transport/role";

export type { SettingsSection, Tab } from "./shell/views";

/** What the UI may read and ask for: the core's reads and one object per role, named as its folder. */
export type Session = CoreReads & {
  /** What the window shows: the tab, the settings section, the instrument window (`shell/`). */
  readonly shell: Views;
  /** What is selected: the track, clips, a sample, automation (`selection/`). */
  readonly selection: Selection;
  /** Play, position, cue, loop region, song loop, metronome; the tempo map (`transport/`). */
  readonly transport: TransportRole;
  /** Moving through the edit history: undo, redo, the list of steps, a jump to one of them (`history/`). */
  readonly history: HistoryActions;
  /** Project files, templates, audio export, startup (`files/`). */
  readonly files: FilesRole;
  /** Tracks, clips, folders, markers, bounce (`tracks/`). */
  readonly tracks: TracksRole;
  /** The notes of a note clip (`pianoroll/`). */
  readonly pianoroll: NotesActions;
  /** Automation lanes and their clips (`automation/`). */
  readonly automation: AutomationActions;
  /** A track's MIDI effects (`instruments/`). */
  readonly instruments: InstrumentsRole;
  /** The computer keyboard as a piano, live notes (`keyboard/`). */
  readonly keyboard: KeyboardRole;
  /** Audio and note recording, track inputs (`recording/`). */
  readonly recording: RecordingRole;
  /** MIDI keyboards and MIDI learn (`midi/`). */
  readonly midi: MidiRole;
  /** The processing graph: effects, buses, connections, faders (`graph/`). */
  readonly graph: GraphActions;
  /** The live sound of the nodes the effect pictures show (`effects/`). */
  readonly effects: EffectsRole;
  /** Meters, gain reduction, the master (`mixer/`). */
  readonly mixer: MixerRole;
  /** The virtual stage: where each channel stands in the shared room, and the room (`stage/`). */
  readonly stage: StageActions;
  /** The project's sounds, the sample editor, auditioning (`samples/`). */
  readonly samples: SamplesRole;
  /** The sound browser and putting a sound to work (`sounds/`). */
  readonly sounds: SoundsRole;
  /** Drum parts from patterns and the drum kit (`drums/`). */
  readonly drums: DrumsRole;
  /** Chord parts from strums (`chords/`). */
  readonly chords: ChordsActions;
  /** Third-party plugins and their windows (`plugins/`). */
  readonly plugins: PluginsRole;
  /** Instrument and effect presets (`presets/`). */
  readonly presets: PresetsActions;
  /** Preferences and the look, audio devices, the error log, about (`settings/`). */
  readonly settings: SettingsRole;
};

export function createSession(rpc: RpcClient, dialogs: SystemDialogs, shell: AppShell): Session {
  const { core, reads, sync, toasts } = createCore(rpc, dialogs, shell);
  const selection = createSelection(core.project);
  sync.onLoaded(selection.prune);
  const views = createViews(selection, core.refresh);
  const settings = createSettingsRole(core);
  const recording = createRecordingRole(core, views);
  onCleanup(recording.dispose);
  createTakeNotice(() => core.tick().record, toasts.toast);
  // One live-writing tracker for every control that moves a parameter (faders, knobs, sends, instrument params).
  const touch = createParamTouch(core);
  const plugins = createPluginsRole(core, settings.preferences);
  const samples = createSamplesRole(core, views);

  const session: Session = {
    ...reads,
    shell: views,
    selection,
    transport: createTransportRole(core),
    history: createHistoryActions(core),
    files: createFilesRole(core, settings.preferences, plugins),
    tracks: createTracksRole(core, views, touch),
    pianoroll: createNotesActions(core),
    automation: createAutomationActions(core, selection, views, touch),
    instruments: createInstrumentsRole(core),
    keyboard: createKeyboardRole(core, selection),
    recording,
    midi: createMidiRole(core),
    graph: createGraphActions(core, touch),
    effects: createEffectsRole(core),
    mixer: createMixerRole(core),
    stage: createStageActions(core, touch),
    samples,
    sounds: createSoundsRole(core, views, samples),
    drums: createDrumsRole(core, views),
    chords: createChordsActions(core, views),
    plugins,
    presets: createPresetsActions(core),
    settings,
  };

  void startSession(session, sync);
  // Track arming lives in the backend longer than the window: fetch it right away (polling sleeps if nothing is armed).
  recording.followRecording();
  return session;
}

/** Preferences first (projects folder, "open last project"), then the project, then what opening it left to say:
 *  everything is in place when the views show. */
async function startSession(session: Session, sync: { start(): Promise<void> }) {
  await session.settings.preferences.load();
  await sync.start();
  await session.files.startup.afterFirstRead();
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
