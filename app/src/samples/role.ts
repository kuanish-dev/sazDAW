// The samples role: which sounds the project has (`samples.ts`), everything done to one of them in the editor
// (`sampleEditor.ts`) and auditioning a sound without changing the project (`preview.ts`).
import { defineRole, useRole } from "../core/roles";
import { shellRole } from "../shell/role";
import { createPreviewActions, type PreviewActions } from "./preview";
import { createSampleEditorActions, type SampleEditorActions } from "./sampleEditor";
import { createSamplesActions, type SamplesActions } from "./samples";

export type SamplesRole = SamplesActions & {
  readonly editor: SampleEditorActions;
  readonly preview: PreviewActions;
};

export const samplesRole = defineRole("samples", ({ core, use }): SamplesRole => ({
  ...createSamplesActions(core),
  editor: createSampleEditorActions(core, use(shellRole)),
  preview: createPreviewActions(core),
}));

export const useSamples = () => useRole(samplesRole);
