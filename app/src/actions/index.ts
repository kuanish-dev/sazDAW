// The actions layer: the only thing the UI sees (`session.actions.*`). Each job has its own interface;
// implementations delegate to the layer below (`api`) and decide themselves how to show errors and what to refresh.
import { createAboutActions, type AboutActions } from "./about";
import { createAudioActions, type AudioActions } from "./audio";
import { createAudioExportActions, type AudioExportActions } from "./audioExport";
import { createAudioRecordActions, type AudioRecordActions } from "./audioRecord";
import { createAutomationActions, type AutomationActions } from "./automation";
import { createBounceActions, type BounceActions } from "./bounce";
import { createChordsActions, type ChordsActions } from "./chords";
import { createClipsActions, type ClipsActions } from "./clips";
import type { ActionContext } from "./context";
import { createDrumsActions, type DrumsActions } from "./drums";
import { createFoldersActions, type FoldersActions } from "./folders";
import { createGraphActions, type GraphActions } from "./graph";
import { createHistoryActions, type HistoryActions } from "./history";
import { createLiveActions, type LiveActions } from "./live";
import { createLogActions, type LogActions } from "./log";
import { createMarkersActions, type MarkersActions } from "./markers";
import { createMidiInputActions, type MidiInputActions } from "./midiInput";
import { createMidiLearnActions, type MidiLearnActions } from "./midiLearn";
import { createNoteEffectsActions, type NoteEffectsActions } from "./noteEffects";
import { createNoteRecordActions, type NoteRecordActions } from "./noteRecord";
import { createNotesActions, type NotesActions } from "./notes";
import { createParamTouch } from "./paramTouch";
import { createPluginEditorActions, type PluginEditorActions } from "./pluginEditor";
import { createPluginsActions, type PluginsActions } from "./plugins";
import { createPreferencesActions, type PreferencesActions } from "./preferences";
import { createPresetsActions, type PresetsActions } from "./presets";
import { createPreviewActions, type PreviewActions } from "./preview";
import { createProjectActions, type ProjectActions } from "./project";
import { createSampleEditorActions, type SampleEditorActions } from "./sampleEditor";
import { createSamplesActions, type SamplesActions } from "./samples";
import { createSoundBrowserActions, type SoundBrowserActions } from "./soundBrowser";
import { createSoundPlacementActions, type SoundPlacementActions } from "./soundPlacement";
import { createStageActions, type StageActions } from "./stage";
import { createStartupActions, type StartupActions } from "./startup";
import { createTemplatesActions, type TemplatesActions } from "./templates";
import { createTempoActions, type TempoActions } from "./tempo";
import { createTracksActions, type TracksActions } from "./tracks";
import { createTransportActions, type TransportActions } from "./transport";

export type {
  ActionContext, Backend, Calls, Created, PlaybackPosition, PreferencesCopy, ProjectCopy, RecordingFollow, Tab, View,
} from "./context";

export interface Actions {
  /** Play, position, cue, loop region, song loop, metronome. */
  readonly transport: TransportActions;
  /** Undo and redo. */
  readonly history: HistoryActions;
  /** Song tempo and meter: tempo points, meter changes. */
  readonly tempo: TempoActions;
  /** Project files: new, open, save, autosave recovery. */
  readonly project: ProjectActions;
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
  readonly preferences: PreferencesActions;
  readonly about: AboutActions;
  /** Render a track to audio: "Bounce to audio", "Freeze", "Unfreeze". */
  readonly bounce: BounceActions;
  /** Project templates (a new project from a template is `project.newFromTemplate`). */
  readonly templates: TemplatesActions;
  /** Instrument and effect presets. */
  readonly presets: PresetsActions;
}

export function createActions(ctx: ActionContext): Actions {
  const project = createProjectActions(ctx);
  const plugins = createPluginsActions(ctx);
  const samples = createSamplesActions(ctx);
  const noteRecord = createNoteRecordActions(ctx);
  const midiInput = createMidiInputActions(ctx);
  // One live-writing tracker for every control that moves a parameter (faders, knobs, sends, instrument params).
  const touch = createParamTouch(ctx);
  return {
    transport: createTransportActions(ctx),
    history: createHistoryActions(ctx),
    tempo: createTempoActions(ctx),
    project,
    audioExport: createAudioExportActions(ctx),
    startup: createStartupActions(project, plugins),
    tracks: createTracksActions(ctx, touch),
    folders: createFoldersActions(ctx),
    markers: createMarkersActions(ctx),
    clips: createClipsActions(ctx),
    notes: createNotesActions(ctx),
    noteEffects: createNoteEffectsActions(ctx),
    automation: createAutomationActions(ctx, touch),
    graph: createGraphActions(ctx, touch),
    stage: createStageActions(ctx, touch),
    samples,
    sampleEditor: createSampleEditorActions(ctx),
    soundPlacement: createSoundPlacementActions(ctx, samples),
    soundBrowser: createSoundBrowserActions(ctx),
    preview: createPreviewActions(ctx),
    live: createLiveActions(ctx),
    log: createLogActions(ctx),
    audio: createAudioActions(ctx),
    noteRecord,
    audioRecord: createAudioRecordActions(ctx, noteRecord),
    plugins,
    pluginEditor: createPluginEditorActions(ctx),
    drums: createDrumsActions(ctx),
    chords: createChordsActions(ctx),
    midiInput,
    midiLearn: createMidiLearnActions(ctx, midiInput),
    preferences: createPreferencesActions(ctx),
    about: createAboutActions(ctx),
    bounce: createBounceActions(ctx),
    templates: createTemplatesActions(ctx),
    presets: createPresetsActions(ctx),
  };
}
