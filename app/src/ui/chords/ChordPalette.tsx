// The chords to pick from: the key's own chords as buttons, and any chord by root and type. A picked chord goes to the
// dialog, which puts it into the selected bar.
import { For, createSignal } from "solid-js";
import { type KeyMode, QUALITIES, ROOTS, chordSymbol, keyChords } from "../../domain/chordPart";
import { t } from "../../i18n";

export function ChordPalette(props: {
  root: number;
  mode: KeyMode;
  onKey: (root: number, mode: KeyMode) => void;
  onPick: (symbol: string) => void;
}) {
  const msg = () => t().chords;
  const [root, setRoot] = createSignal(0);
  const [quality, setQuality] = createSignal("");
  const rootOptions = () => <For each={ROOTS}>{(name, i) => <option value={i()}>{name}</option>}</For>;

  return (
    <div class="flex flex-col gap-2">
      <div class="flex items-center gap-2 text-xs">
        <span class="label">{msg().key}</span>
        <select class="field !h-7 text-xs" aria-label={msg().key} value={props.root} onChange={(e) => props.onKey(Number(e.currentTarget.value), props.mode)}>
          {rootOptions()}
        </select>
        <select class="field !h-7 text-xs" value={props.mode} onChange={(e) => props.onKey(props.root, e.currentTarget.value as KeyMode)}>
          <option value="major">{msg().major}</option>
          <option value="minor">{msg().minor}</option>
        </select>
      </div>
      <div class="label" title={msg().keyChordsHint}>{msg().keyChords}</div>
      <div class="flex flex-wrap gap-1.5">
        <For each={keyChords(props.root, props.mode)}>
          {(symbol) => <button class="btn !h-8 min-w-11 text-sm font-semibold" title={msg().keyChordsHint} onClick={() => props.onPick(symbol)}>{symbol}</button>}
        </For>
      </div>
      <div class="label">{msg().anyChord}</div>
      <div class="flex flex-wrap items-center gap-1.5 text-xs">
        <select class="field !h-7 text-xs" aria-label={msg().rootLabel} value={root()} onChange={(e) => setRoot(Number(e.currentTarget.value))}>
          {rootOptions()}
        </select>
        <select class="field !h-7 text-xs" aria-label={msg().typeLabel} value={quality()} onChange={(e) => setQuality(e.currentTarget.value)}>
          <For each={QUALITIES}>{(suffix, i) => <option value={suffix}>{msg().qualities[i()] ?? suffix}</option>}</For>
        </select>
        <button class="btn !h-7 text-xs" onClick={() => props.onPick(chordSymbol(root(), quality()))}>
          {msg().use}: <b>{chordSymbol(root(), quality())}</b>
        </button>
      </div>
    </div>
  );
}
