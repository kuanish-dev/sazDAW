// Contract between the actions layer and the rest of the app, split by role: what actions get from below (`Backend`),
// how they run a call and tell the user (`Calls`), and the narrow session handles from above (the project copy, the
// playback position, the preferences copy, what the window shows, recording polling). Each action factory asks only for
// the roles it needs; `state/actionContext.ts` assembles them. The UI never sees this object.
import type { DawApi } from "../api/api";
import type { SystemDialogs } from "../api/dialogs";
import type { AppShell } from "../api/shell";
import type { Catalog, Id, Preferences, Project, Status, Tick } from "../api/types";
import type { AutomationSelection } from "../domain/automation";

export type Tab = "tracks" | "graph" | "mixer" | "stage" | "samples" | "settings";

/** What lies below: the backend, system dialogs and the window shell (zoom and app version). */
export interface Backend {
  readonly api: DawApi;
  readonly dialogs: SystemDialogs;
  readonly shell: AppShell;
}

/** How a call runs and how the user hears about it. */
export interface Calls {
  /** Runs a call; a failure is shown (`showFailure`) and the project is re-read. */
  run<T>(action: Promise<T>): Promise<T | undefined>;
  /** Like `run`, but returns whether the action succeeded. */
  attempt(action: Promise<unknown>): Promise<boolean>;
  /** Like `run`, for commands without a result: await and forget. */
  exec(action: Promise<unknown>): Promise<void>;
  /** Shows the loading indicator while the action runs. */
  withBusy<T>(text: string, action: () => Promise<T>): Promise<T>;
  /** Notification; `error` is also written to the error log. */
  toast(kind: "error" | "info", text: string): void;
  /** Shows a failed call: a backend refusal only as a toast (the backend logged it), anything else also goes to the log. */
  showFailure(error: unknown): void;
}

/** The UI's copy of the project: reads, instant display before the backend replies, and re-reads. */
export interface ProjectCopy {
  readonly project: Project;
  status(): Status;
  catalog(): Catalog | null;
  refresh(): Promise<void>;
  /** Re-reads the kinds catalog (a plugin scan adds new effects to it). */
  refreshCatalog(): Promise<void>;
  /**
   * Shows a value in the UI immediately, before the backend replies (sliders, knobs, a finished drag).
   * The real project arrives with the change event and reconciles everything. Only the actions layer changes state.
   */
  patchProject(change: (project: Project) => void): void;
}

/** Where playback is: the latest backend tick (read-only). */
export interface PlaybackPosition {
  tick(): Tick;
}

/** The app preferences as the UI sees them; only the actions layer changes them (before and after the backend replies). */
export interface PreferencesCopy {
  preferences(): Preferences;
  showPreferences(next: Preferences): void;
}

/** Something an action just made, to be selected once it is in the project copy. */
export type Created = { track: Id } | { clips: Id[] } | { sample: Id };

/** What the window shows: the open tab and the selection (UI state that lives in the session). */
export interface View {
  showTab(tab: Tab): void;
  /** Selects an automation lane row and optionally one of its clips (`null` clears); clears the clip selection. */
  selectAutomation(selection: AutomationSelection | null): void;
  /**
   * Re-reads the project, then selects what was created (a track; clips and the last one's track; a sample in the
   * editor) and, with `tab`, opens that tab. Selecting before the re-read would be undone: the selection gives way to
   * items the project copy doesn't have yet.
   */
  showCreated(created: Created, tab?: Tab): Promise<void>;
}

/** Asks the state layer to poll audio recording while it runs (after start/stop). */
export interface RecordingFollow {
  followRecording(): void;
}

/** Every role at once: what `createActions` hands out, each factory taking only its part. */
export type ActionContext = Backend & Calls & ProjectCopy & PlaybackPosition & PreferencesCopy & View & RecordingFollow;
