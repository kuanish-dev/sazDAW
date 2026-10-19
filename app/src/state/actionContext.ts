// Builds the `ActionContext` for the actions layer from its roles: the backend, how a call runs (failure → notification
// and a whole re-read) with the long-operation indicator, and the narrow state handles actions are allowed to touch.
import { type Accessor, createSignal } from "solid-js";
import type {
  ActionContext, Backend, Calls, PlaybackPosition, PreferencesCopy, ProjectCopy, RecordingFollow, View,
} from "../actions";
import type { Selection } from "./selection";
import type { Views } from "./views";

/** The session parts each role is made of. */
export interface ContextParts {
  backend: Backend;
  project: ProjectCopy;
  playback: PlaybackPosition;
  preferences: PreferencesCopy;
  notices: Pick<Calls, "toast" | "showFailure">;
  selection: Pick<Selection, "selectTrack" | "selectClips" | "selectSample" | "selectAutomation">;
  views: Pick<Views, "showTab">;
  recording: RecordingFollow;
}

function createCalls(parts: ContextParts, setBusy: (text: string | null) => void): Calls {
  const fail = async (error: unknown) => {
    parts.notices.showFailure(error);
    // Whole: the refused call's value may already be on screen (`patchProject`), and nothing changed to bring it back.
    await parts.project.reload();
  };

  const run = async <T>(action: Promise<T>): Promise<T | undefined> => {
    try {
      return await action;
    } catch (error) {
      await fail(error);
      return undefined;
    }
  };

  const attempt = async (action: Promise<unknown>): Promise<boolean> => {
    try {
      await action;
      return true;
    } catch (error) {
      await fail(error);
      return false;
    }
  };

  return {
    run,
    attempt,
    exec: async (action) => void (await attempt(action)),
    async withBusy(text, action) {
      setBusy(text);
      try {
        return await action();
      } finally {
        setBusy(null);
      }
    },
    toast: parts.notices.toast,
    showFailure: parts.notices.showFailure,
  };
}

function createView(parts: ContextParts): View {
  const { selection, views } = parts;
  return {
    showTab: views.showTab,
    selectAutomation: selection.selectAutomation,
    async showCreated(created, tab) {
      await parts.project.refresh();
      if ("track" in created) selection.selectTrack(created.track);
      else if ("clips" in created) selection.selectClips(created.clips);
      else selection.selectSample(created.sample);
      if (tab) views.showTab(tab);
    },
  };
}

export function createActionContext(parts: ContextParts): { context: ActionContext; busy: Accessor<string | null> } {
  /** Label of a long operation (opening a project); while set, the loading indicator is shown. */
  const [busy, setBusy] = createSignal<string | null>(null);
  const context: ActionContext = {
    ...parts.backend,
    ...createCalls(parts, setBusy),
    ...parts.project,
    ...parts.playback,
    ...parts.preferences,
    ...createView(parts),
    ...parts.recording,
  };
  return { context, busy };
}
