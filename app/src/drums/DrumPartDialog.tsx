// "Drum part": picking ready-made patterns (grooves, fills, breaks…) and building a rhythm from them on the timeline.
// This file assembles the dialog: part, start, replace and request; patterns are `PatternBrowser`, order `PartList`, sounds `DrumKitPicker`.
import { Show, createMemo, createSignal, onMount } from "solid-js";
import type { DrumPattern, PatternSlot } from "../api/types";
import { drumRequest, lanesOf, totalBars } from "./drumPart";
import { parseStartBar } from "../common/partFields";
import { t } from "../i18n";
import { useSession } from "../session";
import { createLoopPreview } from "../common/loopPreview";
import { Modal } from "../common/Modal";
import { DrumKitPicker } from "./DrumKitPicker";
import { PartList } from "./PartList";
import { PatternBrowser } from "./PatternBrowser";

/** Preview key of the whole part (a pattern's key is its id). */
const WHOLE_PART = "part";

export function DrumPartDialog(props: { onClose: () => void }) {
  const session = useSession();
  const { drums } = session;
  const [patterns, setPatterns] = createSignal<DrumPattern[]>([]);
  const [slots, setSlots] = createSignal<PatternSlot[]>([]);
  const [startBar, setStartBar] = createSignal(0);
  const [replace, setReplace] = createSignal(false);

  onMount(() => void drums.patterns().then(setPatterns));

  const nameOf = (id: string) => patterns().find((p) => p.id === id)?.name ?? id;
  const lanes = createMemo(() => lanesOf(slots(), patterns()));
  const requestFor = (list: PatternSlot[]) =>
    drumRequest({ startBar: startBar(), slots: list, replace: replace() }, session.drums.kit.choices(), patterns(), session.project);
  const preview = createLoopPreview(drums);

  const build = async () => {
    await preview.stop();
    if (await drums.build(requestFor(slots()))) props.onClose();
  };

  const msg = () => t().drums;

  return (
    <Modal
      title={msg().title}
      width="w-[940px]"
      onClose={props.onClose}
      footer={
        <>
          <button class="btn" onClick={props.onClose}>{t().common.cancel}</button>
          <button class="btn btn-primary" disabled={slots().length === 0} onClick={() => void build()}>{msg().build(totalBars(slots(), patterns()))}</button>
        </>
      }
    >
      <div class="grid grid-cols-[1fr_340px] gap-4">
        <PatternBrowser
          patterns={patterns()} playing={preview.playing()}
          onAdd={(id) => setSlots((list) => [...list, { pattern: id, repeat: 1 }])}
          onListen={(id) => void preview.toggle(id, requestFor([{ pattern: id, repeat: 1 }]))}
        />
        <div class="flex min-w-0 flex-col gap-2">
          <div class="label">{msg().partOrder}</div>
          <PartList slots={slots()} nameOf={nameOf} onChange={setSlots} />
          <Show when={lanes().length > 0}><DrumKitPicker lanes={lanes()} /></Show>
          <label class="flex items-center gap-2 text-xs"><span class="label">{msg().fromBar}</span>
            <input type="number" class="field !h-7 w-16 text-xs" min="1" value={startBar() + 1}
              onChange={(e) => setStartBar(parseStartBar(e.currentTarget.value))} /></label>
          <label class="flex items-center gap-2 text-xs" title={msg().replaceHint}>
            <input type="checkbox" checked={replace()} onChange={(e) => setReplace(e.currentTarget.checked)} /> {msg().replace}</label>
          <Show when={slots().length > 0}>
            <div class="flex gap-2">
              <button class="btn !h-7 text-xs" classList={{ "btn-on": preview.playing() === WHOLE_PART }} title={msg().listenPartHint} onClick={() => void preview.toggle(WHOLE_PART, requestFor(slots()))}>
                {preview.playing() === WHOLE_PART ? msg().stop : msg().listenPart}
              </button>
              <button class="btn !h-7 text-xs" onClick={() => setSlots([])}>{msg().clearPart}</button>
            </div>
          </Show>
        </div>
      </div>
    </Modal>
  );
}
