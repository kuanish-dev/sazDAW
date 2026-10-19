// The samples role: which sounds the project has (`samples.ts`), everything done to one of them in the editor
// (`sampleEditor.ts`) and auditioning a sound without changing the project (`preview.ts`).
import { defineRole, useRole } from "../core/roles";
import { shellRole } from "../shell/role";
import { createPreview, type Preview } from "./preview";
import { createSampleEditor, type SampleEditor } from "./sampleEditor";
import { createSamples, type Samples } from "./samples";

export type SamplesRole = Samples & {
  readonly editor: SampleEditor;
  readonly preview: Preview;
};

export const samplesRole = defineRole("samples", ({ core, use }): SamplesRole => ({
  ...createSamples(core),
  editor: createSampleEditor(core, use(shellRole)),
  preview: createPreview(core),
}));

export const useSamples = () => useRole(samplesRole);
