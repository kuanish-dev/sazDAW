// The interface session: the app's roles, created and wired once, and what the UI gets of them (`Session`). Every role
// holds the shared core (`core/core.ts`) and asks it, the way the entities on the backend hold the project core; a role
// keeps its data, its work and its pictures in its own folder. The UI gets the core's reads and the roles, never the
// core itself, so no component calls the backend. Depends only on the `RpcClient`, dialog and shell contracts.
import { createContext, onCleanup, useContext, type JSX } from "solid-js";
import type { SystemDialogs } from "./api/dialogs";
import type { RpcClient } from "./api/rpc";
import type { AppShell } from "./api/shell";
import { createParamTouch } from "./automation/paramTouch";
import { createCore, type CoreReads } from "./core/core";
import { createTakeNotice } from "./core/notifications";
import { createDrumKit, type DrumKit } from "./drums/drumKit";
import { type NodeSounds, createNodeSounds } from "./effects/nodeSounds";
import { createKeyboardPiano, type KeyboardPiano } from "./keyboard/keyboardPiano";
import { createLevels, createReductions, type Levels, type Reductions } from "./mixer/levels";
import { createMasterActions, type MasterActions } from "./mixer/master";
import { createRecordingStatus, type RecordingReads } from "./recording/recordingStatus";
import { createSelection, type Selection } from "./selection/selection";
import { type Appearance, createAppearance } from "./settings/appearance";
import { type AppPreferences, createPreferences } from "./settings/preferences";
import { createViews, type Views } from "./shell/views";
import { createAutomationActions, type AutomationActions } from "./automation/automation";
import { createChordsActions, type ChordsActions } from "./chords/chords";
import { createDrumsActions, type DrumsActions } from "./drums/drums";
import { createGraphActions, type GraphActions } from "./graph/graph";
import { createHistoryActions, type HistoryActions } from "./history/history";
import { createNoteEffectsActions, type NoteEffectsActions } from "./instruments/noteEffects";
import { createLiveActions, type LiveActions } from "./keyboard/live";
import { createMidiInputActions, type MidiInputActions } from "./midi/midiInput";
import { createMidiLearnActions, type MidiLearnActions } from "./midi/midiLearn";
import { createNotesActions, type NotesActions } from "./pianoroll/notes";
import { createPluginEditorActions, type PluginEditorActions } from "./plugins/pluginEditor";
import { createPluginsActions, type PluginsActions } from "./plugins/plugins";
import { createPresetsActions, type PresetsActions } from "./presets/presets";
import { createAudioExportActions, type AudioExportActions } from "./project/audioExport";
import { createProjectActions, type ProjectActions } from "./project/project";
import { createStartupActions, type StartupActions } from "./project/startup";
import { createTemplatesActions, type TemplatesActions } from "./project/templates";
import { createAudioRecordActions, type AudioRecordActions } from "./recording/audioRecord";
import { createNoteRecordActions, type NoteRecordActions } from "./recording/noteRecord";
import { createPreviewActions, type PreviewActions } from "./samples/preview";
import { createSampleEditorActions, type SampleEditorActions } from "./samples/sampleEditor";
import { createSamplesActions, type SamplesActions } from "./samples/samples";
import { createAboutActions, type AboutActions } from "./settings/about";
import { createAudioActions, type AudioActions } from "./settings/audio";
import { createLogActions, type LogActions } from "./settings/log";
import { createSoundBrowserActions, type SoundBrowserActions } from "./sounds/library";
import { createSoundPlacementActions, type SoundPlacementActions } from "./sounds/soundPlacement";
import { createStageActions, type StageActions } from "./stage/stage";
import { createBounceActions, type BounceActions } from "./tracks/bounce";
import { createClipsActions, type ClipsActions } from "./tracks/clips";
import { createFoldersActions, type FoldersActions } from "./tracks/folders";
import { createMarkersActions, type MarkersActions } from "./tracks/markers";
import { createTracksActions, type TracksActions } from "./tracks/tracks";
import { createTempoActions, type TempoActions } from "./transport/tempo";
import { createTransportActions, type TransportActions } from "./transport/transport";


export type { SettingsSection, Tab } from "./shell/views";

/** What the UI may read and ask for: the core's reads and every role. */
export type Session = CoreReads &
  RecordingReads & {
    /** App preferences and the look made from them (`settings/`). */
    readonly preferences: AppPreferences;
    readonly appearance: Appearance;
    /** Selection and what the window shows (`selection/`, `shell/views.ts`, `drums/drumKit.ts`). */
    readonly selection: Selection;
    readonly ui: Views;
    readonly drumKit: DrumKit;
    /** The computer keyboard as a piano on the selected track (`keyboard/`). */
    readonly keyboard: KeyboardPiano;
    /** Meters and gain reduction by node (`mixer/levels.ts`). */
    readonly levels: Levels;
    readonly reductions: Reductions;
    /** The live sound of the nodes the effect pictures show (`effects/nodeSounds.ts`). */
    readonly nodeSounds: NodeSounds;
    /** The master's window for the visualizer, the live master meter and the mono check (`mixer/master.ts`). */
    readonly master: MasterActions;
  /** Play, position, cue, loop region, song loop, metronome. */
  readonly transport: TransportActions;
  /** Moving through the edit history: undo, redo, the list of steps, a jump to one of them. */
  readonly history: HistoryActions;
  /** Song tempo and meter: tempo points, meter changes. */
  readonly tempo: TempoActions;
  /** Project files: new, open, save, autosave recovery. */
  readonly files: ProjectActions;
  /** Rendering the song to an audio file. */
  readonly audioExport: AudioExportActions;
  /** What the app says and asks once after the first project read. */
  readonly startup: StartupActions;
  readonly tracks: TracksActions;
  readonly folders: FoldersActions;
  /** Song structure: section markers. */
  readonly markers: MarkersActions;
  readonly clips: ClipsActions;
  readonly notes: NotesActions;
  /** Track MIDI effects (arpeggiator, chord) before the instrument. */
  readonly noteEffects: NoteEffectsActions;
  readonly automation: AutomationActions;
  readonly graph: GraphActions;
  /** The virtual stage: where each channel stands in the shared room, and the room. */
  readonly stage: StageActions;
  /** Which sounds the project has: list, import, removal. */
  readonly samples: SamplesActions;
  /** Everything done to a single sound in the "Samples" tab. */
  readonly sampleEditor: SampleEditorActions;
  /** A sound from the browser to the timeline, a track or the editor. */
  readonly soundPlacement: SoundPlacementActions;
  /** What is in the project folder and in packs. */
  readonly soundBrowser: SoundBrowserActions;
  readonly preview: PreviewActions;
  readonly live: LiveActions;
  readonly log: LogActions;
  readonly audio: AudioActions;
  /** Note recording: arming and the record quantize. */
  readonly noteRecord: NoteRecordActions;
  readonly audioRecord: AudioRecordActions;
  readonly plugins: PluginsActions;
  readonly pluginEditor: PluginEditorActions;
  readonly drums: DrumsActions;
  readonly chords: ChordsActions;
  /** Live MIDI input from keyboards. */
  readonly midiInput: MidiInputActions;
  /** MIDI learn: controller knobs on parameters. */
  readonly midiLearn: MidiLearnActions;
  readonly about: AboutActions;
  /** Render a track to audio: "Bounce to audio", "Freeze", "Unfreeze". */
  readonly bounce: BounceActions;
  /** Project templates (a new project from a template is `files.newFromTemplate`). */
  readonly templates: TemplatesActions;
  /** Instrument and effect presets. */
  readonly presets: PresetsActions;
  };

export function createSession(rpc: RpcClient, dialogs: SystemDialogs, shell: AppShell): Session {
  const { core, reads, sync, toasts } = createCore(rpc, dialogs, shell);
  const selection = createSelection(core.project);
  sync.onLoaded(selection.prune);
  const ui = createViews(selection, core.refresh);
  const preferences = createPreferences(core);
  const recording = createRecordingStatus(core.api.audioRecord.status, core.showFailure);
  onCleanup(recording.dispose);
  createTakeNotice(() => core.tick().record, toasts.toast);

  // One live-writing tracker for every control that moves a parameter (faders, knobs, sends, instrument params).
  const touch = createParamTouch(core);
  const files = createProjectActions(core, preferences);
  const plugins = createPluginsActions(core, preferences);
  const samples = createSamplesActions(core);
  const noteRecord = createNoteRecordActions(core);
  const midiInput = createMidiInputActions(core);
  const live = createLiveActions(core);

  const session: Session = {
    ...reads,
    ...recording,
    preferences,
    appearance: createAppearance(preferences.current, shell),
    selection,
    ui,
    drumKit: createDrumKit(),
    keyboard: createKeyboardPiano({ selectedTrack: selection.track, live }),
    levels: createLevels(core.tick),
    reductions: createReductions(core.tick),
    nodeSounds: createNodeSounds(core),
    master: createMasterActions(core),
    transport: createTransportActions(core),
    history: createHistoryActions(core),
    tempo: createTempoActions(core),
    files,
    audioExport: createAudioExportActions(core),
    startup: createStartupActions(files, plugins),
    tracks: createTracksActions(core, ui, touch),
    folders: createFoldersActions(core),
    markers: createMarkersActions(core),
    clips: createClipsActions(core, ui),
    notes: createNotesActions(core),
    noteEffects: createNoteEffectsActions(core),
    automation: createAutomationActions(core, selection, ui, touch),
    graph: createGraphActions(core, touch),
    stage: createStageActions(core, touch),
    samples,
    sampleEditor: createSampleEditorActions(core, ui),
    soundPlacement: createSoundPlacementActions(core, ui, samples),
    soundBrowser: createSoundBrowserActions(core),
    preview: createPreviewActions(core),
    live,
    log: createLogActions(core),
    audio: createAudioActions(core),
    noteRecord,
    audioRecord: createAudioRecordActions(core, ui, recording, noteRecord),
    plugins,
    pluginEditor: createPluginEditorActions(core),
    drums: createDrumsActions(core, ui),
    chords: createChordsActions(core, ui),
    midiInput,
    midiLearn: createMidiLearnActions(core, midiInput),
    about: createAboutActions(core),
    bounce: createBounceActions(core, ui),
    templates: createTemplatesActions(core),
    presets: createPresetsActions(core),
  };

  void startSession(session, sync);
  // Track arming lives in the backend longer than the window: fetch it right away (polling sleeps if nothing is armed).
  recording.followRecording();
  return session;
}

/** Preferences first (projects folder, "open last project"), then the project, then what opening it left to say:
 *  everything is in place when the views show. */
async function startSession(session: Session, sync: { start(): Promise<void> }) {
  await session.preferences.load();
  await sync.start();
  await session.startup.afterFirstRead();
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
