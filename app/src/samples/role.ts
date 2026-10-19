// The samples role as the session holds it: which sounds the project has (`samples.ts`), everything done to one of them
// in the editor (`sampleEditor.ts`) and auditioning a sound without changing the project (`preview.ts`).
import type { Core } from "../core/core";
import type { Views } from "../shell/views";
import { createPreviewActions, type PreviewActions } from "./preview";
import { createSampleEditorActions, type SampleEditorActions } from "./sampleEditor";
import { createSamplesActions, type SamplesActions } from "./samples";

export type SamplesRole = SamplesActions & {
  readonly editor: SampleEditorActions;
  readonly preview: PreviewActions;
};

export function createSamplesRole(core: Core, views: Views): SamplesRole {
  return { ...createSamplesActions(core), editor: createSampleEditorActions(core, views), preview: createPreviewActions(core) };
}
