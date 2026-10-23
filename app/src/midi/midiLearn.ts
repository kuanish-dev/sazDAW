import { createSignal, onCleanup } from "solid-js";
import type { MidiMapping, ParamTarget } from "../api/types";
import { targetExists } from "../automation/automationRules";
import { t } from "../i18n";
import type { Core } from "../core/core";
import type { MidiInput } from "./midiInput";

export interface MidiLearn {
  /** Knob mappings to parameters that exist (a mapping to a removed one stays in the project but does nothing). */
  mappings(): MidiMapping[];
  /** The parameter waiting for a knob; `null` — none. */
  learning(): ParamTarget | null;
  /** Wait for a knob for `target` (`label` is for the hint). */
  learn(target: ParamTarget, label: string): Promise<void>;
  cancelLearn(): Promise<void>;
  removeMapping(target: ParamTarget): Promise<void>;
}

export function createMidiLearn(core: Core, input: Pick<MidiInput, "connection">,
): MidiLearn {
  const { api } = core;
  const [learning, setLearning] = createSignal<ParamTarget | null>(null);
  const readLearning = async () => {
    const state = await core.calls.run(api.midi.mappings());
    if (state) setLearning(state.learning);
  };
  onCleanup(core.copy.onLoaded(() => {
    if (learning() !== null) void readLearning();
  }));

  return {
    mappings: () => (core.copy.project.midiMappings ?? []).filter((m) => targetExists(core.copy.project, m.target)),
    learning,
    async learn(target, label) {
      if (!(await core.calls.attempt(api.midi.learn(target)))) return;
      core.notifications.toast("info", t().settings.midi.learnToast(label, input.connection() !== null));
      await readLearning();
    },
    async cancelLearn() {
      await core.calls.exec(api.midi.cancelLearn());
      await readLearning();
    },
    removeMapping: (target) => core.calls.exec(api.midi.removeMapping(target)),
  };
}
