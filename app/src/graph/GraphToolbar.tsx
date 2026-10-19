// Toolbar strip above the graph canvas: add an effect or bus, delete the selection, arrange nodes, show the signal path,
// zoom.
import type { KindInfo } from "../api/types";
import { kindChoices } from "../common/catalog";
import { t } from "../i18n";
import { IconAdd, IconFit, IconGraph, IconTrash, IconWave } from "../common/icons";
import { PickSelect } from "../common/PickSelect";
import { ToolButton, Toolbar, ToolbarDivider } from "../common/Toolbar";

export function GraphToolbar(props: {
  effects: KindInfo[];
  canRemove: boolean;
  /** A node is selected — an effect can be inserted at it (after the node, before the master). */
  canInsert: boolean;
  /** An effect is selected — it can be removed keeping the path. */
  canKeepPath: boolean;
  zoom: number;
  /** The signal path view is open. */
  pathOpen: boolean;
  onAddEffect: (kind: string) => void;
  onInsertEffect: (kind: string) => void;
  onAddBus: () => void;
  onRemove: () => void;
  onRemoveKeepPath: () => void;
  onArrange: () => void;
  onResetView: () => void;
  onTogglePath: () => void;
}) {
  const effects = () => kindChoices(props.effects);
  return (
    <Toolbar>
      <PickSelect class="text-xs" options={effects()} placeholder={t().graph.toolbar.addEffect} title={t().graph.toolbar.addEffectHint} onPick={props.onAddEffect} />
      <PickSelect
        class="text-xs" options={effects()} placeholder={t().graph.toolbar.insertEffect}
        title={t().graph.toolbar.insertEffectHint} disabled={!props.canInsert} onPick={props.onInsertEffect}
      />
      <ToolButton icon={<IconAdd />} label={t().graph.toolbar.addBus} title={t().graph.toolbar.addBusHint} text onClick={() => props.onAddBus()} />
      <ToolbarDivider />
      <ToolButton icon={<IconTrash />} label={t().common.delete} title={t().graph.toolbar.removeHint} disabled={!props.canRemove} onClick={() => props.onRemove()} />
      <button class="btn btn-ghost !px-2 text-xs" disabled={!props.canKeepPath} title={t().graph.toolbar.keepPathHint} onClick={() => props.onRemoveKeepPath()}>{t().graph.toolbar.keepPath}</button>
      <ToolbarDivider />
      <ToolButton icon={<IconGraph />} label={t().graph.toolbar.arrange} title={t().graph.toolbar.arrangeHint} text onClick={() => props.onArrange()} />
      <ToolButton
        icon={<IconWave />} label={t().graph.toolbar.signalPath} title={t().graph.toolbar.signalPathHint} text
        pressed={props.pathOpen} onClick={() => props.onTogglePath()}
      />
      <span class="flex-1" />
      <span class="hidden truncate text-[11px] text-muted xl:inline">{t().graph.toolbar.navHint}</span>
      <button class="btn btn-ghost !px-2 font-mono text-xs text-muted hover:text-text" title={t().graph.toolbar.resetView} onClick={() => props.onResetView()}>
        <IconFit size={14} />{Math.round(props.zoom * 100)}%
      </button>
    </Toolbar>
  );
}
