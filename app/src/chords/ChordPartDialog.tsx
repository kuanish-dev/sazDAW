import { For, Show, createEffect, createSignal, on, onMount } from "solid-js";
import type { ChordPartRequest, StrumPattern } from "../api/types";
import {
  type ChordCell, type KeyMode, NEW_TRACK_KINDS, choiceTarget, clampCell, defaultTarget, nextCell, setChord, startingProgression, targetChoice,
} from "./chordPart";
import { MAX_PART_REPEAT, parseRepeat, parseStartBar } from "../common/partFields";
import { instrumentTracks } from "../tracks/trackRules";
import { t } from "../i18n";
import { createLoopPreview } from "../common/loopPreview";
import { Modal } from "../common/Modal";
import { ChordPalette } from "./ChordPalette";
import { ProgressionBars } from "./ProgressionBars";
import { StrumPicker } from "./StrumPicker";
import { useProjectCopy } from "../core/roles";
import { useChords } from "./role";
import { useSelection } from "../selection/role";

/** The strum a new part starts with: "six" (D DU UDU) suits most songs. */
const FIRST_STRUM = "six";

export function ChordPartDialog(props: { onClose: () => void }) {
  const { catalog, project } = useProjectCopy();
  const chords = useChords();
  const selection = useSelection();
  const [strums, setStrums] = createSignal<StrumPattern[]>([]);
  const [strum, setStrum] = createSignal(FIRST_STRUM);
  const [keyRoot, setKeyRoot] = createSignal(0);
  const [mode, setMode] = createSignal<KeyMode>("major");
  const [bars, setBars] = createSignal(startingProgression(0, "major"));
  /** Until the progression is edited, it follows the key. */
  const [edited, setEdited] = createSignal(false);
  const [selected, setSelected] = createSignal<ChordCell>({ bar: 0, half: 0 });
  const [target, setTarget] = createSignal(defaultTarget(project, selection.track()));
  const [startBar, setStartBar] = createSignal(0);
  const [repeat, setRepeat] = createSignal(1);
  const [replace, setReplace] = createSignal(false);

  onMount(() => void chords.strums().then(setStrums));

  const changeBars = (next: string[][]) => {
    setEdited(true);
    setBars(next);
    setSelected((cell) => clampCell(next, cell));
  };
  /** A picked chord replaces the selected one, and the selection moves on: clicking chords fills the bars in order. */
  const pick = (symbol: string) => {
    const cell = selected();
    const next = setChord(bars(), cell, symbol);
    changeBars(next);
    setSelected(nextCell(next, cell));
  };
  const changeKey = (root: number, nextMode: KeyMode) => {
    setKeyRoot(root);
    setMode(nextMode);
    if (!edited()) setBars(startingProgression(root, nextMode));
  };

  const request = (): ChordPartRequest => ({
    target: target(), startBar: startBar(), strum: strum(), bars: bars(), repeat: repeat(), replace: replace(),
  });
  /** The chord part loops as one piece. */
  const PART = "part";
  const preview = createLoopPreview(chords);
  createEffect(on(request, (next) => void preview.replay(next), { defer: true }));

  const build = async () => {
    await preview.stop();
    if (await chords.build(request())) props.onClose();
  };

  const kindLabel = (kind: string) => catalog()?.instruments.find((k) => k.kind === kind)?.label ?? kind;
  const msg = () => t().chords;

  return (
    <Modal
      title={msg().title}
      width="w-[780px]"
      onClose={props.onClose}
      footer={
        <>
          <button class="btn" classList={{ "btn-on": preview.playing() === PART }} title={msg().listenHint} onClick={() => void preview.toggle(PART, request())}>
            {preview.playing() === PART ? msg().stop : msg().listen}
          </button>
          <button class="btn" onClick={props.onClose}>{t().common.cancel}</button>
          <button class="btn btn-primary" disabled={bars().length === 0} onClick={() => void build()}>{msg().build(bars().length * repeat())}</button>
        </>
      }
    >
      <div class="grid grid-cols-[1fr_270px] gap-5">
        <div class="flex min-w-0 flex-col gap-4">
          <ProgressionBars bars={bars()} selected={selected()} onSelect={setSelected} onChange={changeBars} />
          <StrumPicker strums={strums()} value={strum()} onChange={setStrum} />
        </div>
        <div class="flex min-w-0 flex-col gap-3">
          <ChordPalette root={keyRoot()} mode={mode()} onKey={changeKey} onPick={pick} />
          <label class="flex flex-col gap-1 text-xs">
            <span class="label">{msg().track}</span>
            <select class="field text-xs" value={targetChoice(target())} onChange={(e) => setTarget(choiceTarget(e.currentTarget.value))}>
              <optgroup label={msg().newTracks}>
                <For each={NEW_TRACK_KINDS}>{(kind) => <option value={targetChoice({ type: "new", kind })}>{kindLabel(kind)}</option>}</For>
              </optgroup>
              <Show when={instrumentTracks(project).length > 0}>
                <optgroup label={msg().yourTracks}>
                  <For each={instrumentTracks(project)}>{(track) => <option value={targetChoice({ type: "track", id: track.id })}>{track.name}</option>}</For>
                </optgroup>
              </Show>
            </select>
          </label>
          <div class="flex flex-wrap gap-3">
            <label class="flex items-center gap-2 text-xs"><span class="label">{msg().fromBar}</span>
              <input type="number" class="field !h-7 w-16 text-xs" min="1" value={startBar() + 1}
                onChange={(e) => setStartBar(parseStartBar(e.currentTarget.value))} /></label>
            <label class="flex items-center gap-2 text-xs"><span class="label">{msg().repeat}</span>
              <input type="number" class="field !h-7 w-14 text-xs" min="1" max={MAX_PART_REPEAT} value={repeat()}
                onChange={(e) => setRepeat(parseRepeat(e.currentTarget.value))} /></label>
          </div>
          <label class="flex items-center gap-2 text-xs" title={msg().replaceHint}>
            <input type="checkbox" checked={replace()} onChange={(e) => setReplace(e.currentTarget.checked)} /> {msg().replace}</label>
        </div>
      </div>
    </Modal>
  );
}
