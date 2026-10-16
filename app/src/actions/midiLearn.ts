// MIDI learn: controller knobs on parameters. Mappings live in the project, so the list is read from the project copy
// and is as fresh as it is; the parameter waiting for a knob is backend state of this run (the backend moves the
// parameter when the knob turns).
import { createEffect, createSignal, on } from "solid-js";
import type { MidiMapping, ParamTarget } from "../api/types";
import { targetExists } from "../domain/automation";
import { t } from "../i18n";
import type { Backend, Calls, ProjectCopy } from "./context";
import type { MidiInputActions } from "./midiInput";

export interface MidiLearnActions {
  /** Knob mappings to parameters that exist (a mapping to a removed one stays in the project but does nothing). */
  mappings(): MidiMapping[];
  /** The parameter waiting for a knob; `null` — none. */
  learning(): ParamTarget | null;
  /** Wait for a knob for `target` (`label` is for the hint). */
  learn(target: ParamTarget, label: string): Promise<void>;
  cancelLearn(): Promise<void>;
  removeMapping(target: ParamTarget): Promise<void>;
}

export function createMidiLearnActions(
  ctx: Backend & Calls & ProjectCopy, input: Pick<MidiInputActions, "connection">,
): MidiLearnActions {
  const { api } = ctx;
  const [learning, setLearning] = createSignal<ParamTarget | null>(null);
  const readLearning = async () => {
    const state = await ctx.run(api.midi.mappings());
    if (state) setLearning(state.learning);
  };
  // A turned knob ends learning with a project edit (the new mapping): while waiting, any edit may be that answer.
  createEffect(on(() => ctx.status().revision, () => {
    if (learning() !== null) void readLearning();
  }, { defer: true }));

  return {
    mappings: () => (ctx.project.midiMappings ?? []).filter((m) => targetExists(ctx.project, m.target)),
    learning,
    async learn(target, label) {
      if (!(await ctx.attempt(api.midi.learn(target)))) return;
      ctx.toast("info", t().settings.midi.learnToast(label, input.connection() !== null));
      await readLearning();
    },
    async cancelLearn() {
      await ctx.exec(api.midi.cancelLearn());
      await readLearning();
    },
    removeMapping: (target) => ctx.exec(api.midi.removeMapping(target)),
  };
}
