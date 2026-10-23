import { For } from "solid-js";
import { GRID_CHOICES } from "../common/time";
import { t } from "../i18n";
import { IconCopy, IconDuplicate, IconExport, IconImport, IconKeyboard, IconPaste, IconTrash } from "../common/icons";
import { ToolButton, Toolbar, ToolbarDivider } from "../common/Toolbar";
import { NoteTools, type NoteToolsProps } from "./NoteTools";
import { ZOOMS } from "./pianoRollLayout";
import { useKeyboard } from "../keyboard/role";

export interface ToolbarProps {
  hasNotes: boolean;
  snap: number;
  zoom: number;
  onSnap(ticks: number): void;
  onZoom(zoom: number): void;
  onImportMidi(): void;
  onExportMidi(): void;
  selectedCount: number;
  canPaste: boolean;
  onCopy(): void;
  onPaste(): void;
  onDuplicate(): void;
  onRemove(): void;
  tools: NoteToolsProps;
  onClear(): void;
}

export function PianoRollToolbar(props: ToolbarProps) {
  const keyboard = useKeyboard();
  const hasSelection = () => props.selectedCount > 0;
  const msg = () => t().pianoRoll;
  return (
    <>
      <Toolbar label={msg().toolbar}>
        <span class="flex h-6 items-center gap-1 rounded border border-field-line bg-bg px-2 text-xs text-muted" title={msg().keyboardHint}>
          <IconKeyboard size={14} />{msg().keyboard} C{keyboard.octave()} <span class="font-mono text-text">[ ]</span>
        </span>
        <ToolbarDivider />
        <label class="flex items-center gap-1.5"><span class="label">{msg().grid}</span>
          <select class="field text-xs" title={msg().gridHint} value={props.snap} onChange={(e) => props.onSnap(Number(e.currentTarget.value))}>
            <For each={GRID_CHOICES}>{(s) => <option value={s.ticks}>{s.label}</option>}</For>
          </select></label>
        <label class="flex items-center gap-1.5"><span class="label">{msg().zoom}</span>
          <select class="field text-xs" title={msg().zoomHint} value={props.zoom} onChange={(e) => props.onZoom(Number(e.currentTarget.value))}>
            <For each={ZOOMS}>{(z) => <option value={z}>{z * 100}%</option>}</For>
          </select></label>
        <div class="ml-auto flex items-center gap-0.5">
          <ToolButton icon={<IconCopy />} label={msg().copy} title={msg().copyHint} disabled={!hasSelection()} onClick={props.onCopy} />
          <ToolButton icon={<IconPaste />} label={msg().paste} title={msg().pasteHint} disabled={!props.canPaste} onClick={props.onPaste} />
          <ToolButton icon={<IconDuplicate />} label={msg().duplicate} title={msg().duplicateHint} disabled={!hasSelection()} onClick={props.onDuplicate} />
          <ToolButton icon={<IconTrash />} label={t().common.delete} title={msg().removeHint} disabled={!hasSelection()} onClick={props.onRemove} />
          <ToolbarDivider />
          <ToolButton icon={<IconImport />} label={msg().importMidi} title={msg().importMidiHint} onClick={props.onImportMidi} />
          <ToolButton icon={<IconExport />} label={msg().exportMidi} title={msg().exportMidiHint} onClick={props.onExportMidi} />
        </div>
      </Toolbar>

      <Toolbar label={msg().editToolbar}>
        <NoteTools {...props.tools} />
        <ToolbarDivider />
        <span class="min-w-0 flex-1 truncate text-xs text-muted">{hasSelection() ? msg().selectedNotes(props.selectedCount) : msg().selectionHint}</span>
        <button class="btn btn-ghost !px-2 text-xs" title={msg().clearClipHint} disabled={!props.hasNotes} onClick={props.onClear}>{msg().clearClip}</button>
      </Toolbar>
    </>
  );
}
