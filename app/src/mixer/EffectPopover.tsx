import { Show } from "solid-js";
import type { Id } from "../api/types";
import { findNode, nodeLabel } from "../graph/graphRules";
import { t } from "../i18n";
import { IconClose, IconTrash } from "../common/icons";
import { EffectPanel } from "../effects/EffectPanel";
import { useProjectCopy } from "../core/roles";
import { useGraph } from "../graph/role";

export interface OpenEffect {
  node: Id;
  /** Where the insert button is: the panel opens below it. */
  anchor: DOMRect;
}

const PANEL_WIDTH = 288;
const GAP = 4;

export function EffectPopover(props: { open: OpenEffect; onClose: () => void }) {
  const { project } = useProjectCopy();
  const graph = useGraph();
  const node = () => findNode(project.graph, props.open.node);
  const left = () => Math.max(GAP, Math.min(props.open.anchor.left, window.innerWidth - PANEL_WIDTH - GAP));
  const top = () => Math.min(props.open.anchor.bottom + GAP, window.innerHeight / 2);

  const remove = () => {
    void graph.removeEffectKeepPath(props.open.node);
    props.onClose();
  };

  return (
    <>
      <div class="fixed inset-0 z-40" onPointerDown={props.onClose} />
      <div
        class="anim-pop fixed z-50 max-h-[45vh] overflow-y-auto rounded border border-field-line bg-panel shadow-xl"
        style={{ left: `${left()}px`, top: `${top()}px`, width: `${PANEL_WIDTH}px` }}
        role="dialog"
        tabIndex={-1}
        ref={(el) => queueMicrotask(() => el.focus())}
        onKeyDown={(e) => e.key === "Escape" && props.onClose()}
      >
        <Show when={node()} fallback={<p class="p-3 text-sm text-muted">{t().mixer.effectRemoved}</p>}>
          {(n) => (
            <>
              <div class="sticky top-0 z-10 flex h-9 items-center justify-between gap-2 border-b border-line bg-panel pl-3 pr-1.5">
                <h2 class="label truncate">{nodeLabel(n())}</h2>
                <div class="flex shrink-0 gap-0.5">
                  <button class="btn btn-icon !h-6 !w-6 !min-w-6 hover:!text-danger" title={t().mixer.removeEffectHint} aria-label={t().common.remove} onClick={remove}><IconTrash size={14} /></button>
                  <button class="btn btn-icon !h-6 !w-6 !min-w-6" title={t().common.close} aria-label={t().common.close} onClick={() => props.onClose()}><IconClose size={14} /></button>
                </div>
              </div>
              <EffectPanel node={n()} />
            </>
          )}
        </Show>
      </div>
    </>
  );
}
