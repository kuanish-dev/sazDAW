// Saving under a name the user typed (presets, project templates): the name is checked by the backend's rules before
// sending, a taken name asks before replacing, and the result is a toast.
import { presetNameProblem } from "../domain/presets";
import type { Backend, Calls } from "./context";

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
export async function saveUnderName(ctx: Calls & Pick<Backend, "dialogs">, name: string, how: NamedSave): Promise<boolean> {
  const problem = presetNameProblem(name);
  if (problem) {
    ctx.toast("error", how.problem(problem));
    return false;
  }
  const trimmed = name.trim();
  if ((await how.taken()).includes(trimmed) && !(await ctx.dialogs.confirm(how.exists(trimmed), how.title))) return false;
  const saved = await ctx.attempt(how.save(trimmed));
  if (saved) ctx.toast("info", how.saved(trimmed));
  return saved;
}
