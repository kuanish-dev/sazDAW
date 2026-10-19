// Saving under a name the user typed (presets, project templates): the name is checked by the backend's rules before
// sending, a taken name asks before replacing, and the result is a toast.
import { presetNameProblem } from "../presets/presetRules";
import type { Core } from "../core/core";

/** One kind of named thing: its texts (read in the current language at the call) and backend calls. */
export interface NamedSave {
  /** Title of the "replace?" question. */
  title: string;
  problem(problem: string): string;
  exists(name: string): string;
  saved(name: string): string;
  /** Names already taken. */
  taken(): Promise<string[]>;
  save(name: string): Promise<unknown>;
}

/** Saves under the trimmed `name`; `false` — a bad name (a toast), the user kept the old one, or a backend refusal. */
export async function saveUnderName(core: Core, name: string, how: NamedSave): Promise<boolean> {
  const problem = presetNameProblem(name);
  if (problem) {
    core.notifications.toast("error", how.problem(problem));
    return false;
  }
  const trimmed = name.trim();
  if ((await how.taken()).includes(trimmed) && !(await core.dialogs.confirm(how.exists(trimmed), how.title))) return false;
  const saved = await core.calls.attempt(how.save(trimmed));
  if (saved) core.notifications.toast("info", how.saved(trimmed));
  return saved;
}
