// The backend's in-memory state: the project with undo history (the tempo map lives in it), transport, kinds catalog,
// a file "system", recording. Method handlers (`handlers/`) read and change only it; it knows nothing about RPC.
import type { AutomationWriteMode, Catalog, Id, Preferences, Project, SampleInfo, TransportState } from "../api/types";
import { DEFAULT_PREFERENCES } from "../domain/preferences";
import { t } from "../i18n";
import type { BackendError } from "../i18n/errors";
import { MOCK_FRAME_COUNT, MOCK_SAMPLE_RATE } from "./audio";
import { createMockCatalog, type OfferedPlugins } from "./catalog";
import { emptyProject } from "./demo";
import { fail } from "./errors";
import type { MeterState } from "./masterMeter";

const HISTORY_LIMIT = 100;
/** Consecutive edits with one key within this time are one undo step (the backend's `COALESCE_WINDOW_MS`). */
const MERGE_WINDOW_MS = 800;
/** Folder of the browser's start project; named in the language current when the mock is created. */
const startFolder = () => `/home/user/Documents/DAW Projects/${t().mock.projectName}`;

interface HistoryEntry {
  label: string;
  project: string;
}

export class MockWorld {
  project: Project = emptyProject();
  undoStack: HistoryEntry[] = [];
  redoStack: HistoryEntry[] = [];
  revision = 1;
  dirty = false;
  path: string | null = null;
  saved = new Map<string, string>();
  recent: string[] = [];
  autosave: string | null = null;
  /** App preferences (in the browser they live until the page reloads). */
  preferences: Preferences = structuredClone(DEFAULT_PREFERENCES);
  /** Plugins the catalog offers besides the built-in kinds; the plugin scan changes them. */
  plugins: OfferedPlugins = { effects: [], instruments: [] };
  state: TransportState = "stopped";
  position = 0;
  /** The start position (cue): play from pause/stop and stop return to it; `null` — none. */
  cue: number | null = null;
  /** The song loop mode (on by default) and whether an audio take runs (it holds the song loop): `songLoop.ts`. */
  songLoop = true;
  recordingAudio = false;
  metronome = false;
  /** The mono check, a monitoring mode like the song loop; the live master meter while it is read (`masterMeter.ts`). */
  mono = false;
  meter: MeterState | null = null;
  /** Live automation write mode and whether this play pass has already written (one undo step per pass). */
  automationWrite: AutomationWriteMode = "off";
  automationPass = false;
  armed = false;
  recordQuantize = 0;
  takeNotes = 0;
  takeOpen = false;
  /** A recording pass is the clip length; pass number per clip, so a new pass replaces the previous ones. */
  takePasses = new Map<Id, number>();
  takeRecorded: { pass: number; clip: Id; id: Id }[] = [];
  heldNotes = new Map<string, { tick: number; velocity: number }>();
  /** Held meter peaks per node (smooth fall-off and bursts from live notes). */
  peaks = new Map<number, number>();
  projectListeners = new Set<(r: number) => void>();
  files: Record<string, { name: string; isDir: boolean }[]> = {
    "/home/user": [{ name: "Samples", isDir: true }, { name: "Projects", isDir: true }, { name: "notes.wav", isDir: false }],
    "/home/user/Samples": [{ name: "Kicks", isDir: true }, { name: "kick.wav", isDir: false }, { name: "snare.wav", isDir: false }, { name: "loop.wav", isDir: false }],
    "/home/user/Samples/Kicks": [{ name: "kick-deep.wav", isDir: false }],
    "/home/user/Projects": [],
  };
  soundSources: string[] = [];
  private startFolder = startFolder();
  /** Contents of the project folder (samples/, exports/): added packs land here. */
  projectFiles: Record<string, { name: string; isDir: boolean }[]> = {
    [this.startFolder]: [{ name: "samples", isDir: true }, { name: "exports", isDir: true }],
    [`${this.startFolder}/samples`]: [],
    [`${this.startFolder}/exports`]: [],
  };
  /** Which file outside the project became which copy inside it (re-import is idempotent). */
  copies = new Map<string, string>();
  /** Metadata of imported samples; markers live in the project (as in the backend). */
  sampleMeta = new Map<Id, { frameCount: number; sampleRate: number }>();
  previewState: { from: number; to: number; looping: boolean; startedAt: number } | null = null;

  /** Asks to start the `tick` stream (set by `MockRpc`); called when playback starts. */
  needTicks: () => void = () => undefined;

  /** The app's start project in the browser: already saved in its folder and listed in recent projects. */
  openStartProject() {
    this.project.name = t().mock.projectName;
    this.path = `${this.startFolder}/${this.project.name}.dawproj`;
    this.saved.set(this.path, JSON.stringify(this.project));
    this.recent = [this.path];
  }

  /** Folder of the open project; `null` while the project is not saved. */
  folder(): string | null {
    return this.path ? this.path.slice(0, this.path.lastIndexOf("/")) : null;
  }

  /** Kinds catalog in the current language (a fresh object each time). */
  get catalog(): Catalog {
    return createMockCatalog(this.plugins);
  }

  /** The project folder, or `unsaved` (the backend's error of the method that needs the folder). */
  requireFolder(unsaved: BackendError): string {
    const folder = this.folder();
    if (!folder) throw unsaved;
    return folder;
  }

  remember(path: string) {
    this.recent = [path, ...this.recent.filter((p) => p !== path)];
  }

  install(project: Project, path: string | null, dirty: boolean) {
    this.project = project;
    this.undoStack = [];
    this.redoStack = [];
    this.dirty = dirty;
    this.path = path;
    // As in the backend: the old project's cue means nothing in the new one.
    this.cue = null;
    this.touch();
  }

  sampleInfos(): SampleInfo[] {
    return this.project.samples.map((s) => {
      const meta = this.sampleMeta.get(s.id) ?? { frameCount: MOCK_FRAME_COUNT, sampleRate: MOCK_SAMPLE_RATE };
      return { id: s.id, name: s.name, path: s.path, markers: s.markers, ...meta };
    });
  }

  sampleInfo(id: Id): SampleInfo {
    return this.must(this.sampleInfos().find((s) => s.id === id), fail("SM-001", { id }));
  }

  /** A free path in the project's `samples/`: `name.ext`, or if taken `name 2.ext`, `name 3.ext`… */
  uniqueSamplePath(folder: string, stem: string, ext: string): string {
    const taken = (path: string) => this.sampleInfos().some((s) => s.path === path);
    let path = `${folder}/samples/${stem}${ext}`;
    for (let n = 2; taken(path); n++) path = `${folder}/samples/${stem} ${n}${ext}`;
    return path;
  }

  /** Preview position in file frames; at the end of the region (without loop) the voice stops. */
  previewPosition(): number | null {
    const p = this.previewState;
    if (!p) return null;
    const elapsed = ((performance.now() - p.startedAt) / 1000) * MOCK_SAMPLE_RATE;
    const length = p.to - p.from;
    if (elapsed >= length && !p.looping) {
      this.previewState = null;
      return null;
    }
    return p.from + (p.looping ? elapsed % length : elapsed);
  }

  clone<T>(value: T): T {
    return JSON.parse(JSON.stringify(value)) as T;
  }

  /** The key and time of the last keyed edit (`edit`'s `key`). */
  lastKey: { key: string; at: number } | null = null;

  /** A project edit with undo history and notification; `label` is the localized undo label. Consecutive edits with one
   * `key` (a drag, a slider) merge into one undo step, like the backend's `edit_keyed`. */
  edit<R>(label: string, change: (p: Project) => R, key?: string): R {
    const before = JSON.stringify(this.project);
    const draft = this.clone(this.project);
    const result = change(draft);
    const now = Date.now();
    const merge = key !== undefined && this.lastKey?.key === key && now - this.lastKey.at < MERGE_WINDOW_MS;
    this.lastKey = key === undefined ? null : { key, at: now };
    if (!merge) {
      this.undoStack.push({ label, project: before });
      if (this.undoStack.length > HISTORY_LIMIT) this.undoStack.shift();
    }
    this.redoStack = [];
    this.project = draft;
    this.dirty = true;
    this.touch();
    return result;
  }

  /** A history step: the snapshot from `from` becomes the project, the current one goes to `to`. `false`: nowhere to step. */
  step(from: HistoryEntry[], to: HistoryEntry[]): boolean {
    const entry = from.pop();
    if (!entry) return false;
    to.push({ label: entry.label, project: JSON.stringify(this.project) });
    const next = JSON.parse(entry.project) as Project;
    // Ids are never reused, even after undo.
    next.nextId = Math.max(next.nextId, this.project.nextId);
    this.project = next;
    this.touch();
    return true;
  }

  touch() {
    this.revision += 1;
    for (const listener of this.projectListeners) listener(this.revision);
  }

  id(p: Project = this.project): Id {
    return p.nextId++;
  }

  /** `item`, or `missing` (the backend's "… not found" error). */
  must<T>(item: T | undefined, missing: BackendError): T {
    if (item === undefined) throw missing;
    return item;
  }

  track(p: Project, id: Id) {
    return this.must(p.tracks.find((track) => track.id === id), fail("TR-001", { id }));
  }

  clip(p: Project, id: Id) {
    return this.must(p.clips.find((c) => c.id === id), fail("CL-001", { id }));
  }
}
