// Musical note edits of the piano roll: Quantize / Humanize / Transpose / Fit to scale / Legato / Groove / Variation,
// with small parameter panels. They work on the selected notes, or on all notes of the clip without a selection. The edit
// logic itself is in the backend (`notes.quantize` etc.); here are only view parameters and `session.actions.notes` calls.
import { For, type JSX, Show, createMemo, createSignal } from "solid-js";
import type { Id, Variation } from "../../api/types";
import { GROOVE_PRESETS } from "../../domain/groove";
import { clipNotes } from "../../domain/noteClip";
import { MAX_HUMANIZE_TICKS, MAX_HUMANIZE_VELOCITY, SCALES, isScale, type ScaleChoice } from "../../domain/noteTransforms";
import { pitchClassName } from "../../domain/pitch";
import { barIndex } from "../../domain/tempoMap";
import { t } from "../../i18n";
import { useSession } from "../../state/session";
import { IconChevronDown, IconDice } from "../common/icons";
import { NumberField } from "../common/NumberField";
import { closeOnOutside } from "../common/pointer";

type Panel = "quantize" | "humanize" | "transpose" | "scale" | "groove" | "vary";

export interface NoteToolsProps {
  clip: Id;
  /** Selected notes; `undefined` edits all notes of the clip. */
  ids: Id[] | undefined;
  hasNotes: boolean;
  /** Quantize step in ticks (the panel grid) and its label. */
  grid: number;
  gridLabel: string;
  scale: ScaleChoice;
  onScale(choice: ScaleChoice): void;
  highlight: boolean;
  onHighlight(on: boolean): void;
}

const ROOTS = Array.from({ length: 12 }, (_, i) => i);

const newSeed = () => Math.floor(Math.random() * 2 ** 32);

export function NoteTools(props: NoteToolsProps) {
  const session = useSession();
  const notes = session.actions.notes;
  const [open, setOpen] = createSignal<Panel | null>(null);
  const [strength, setStrength] = createSignal(100);
  const [ends, setEnds] = createSignal(false);
  const [timing, setTiming] = createSignal(20);
  const [velocity, setVelocity] = createSignal(10);
  const [semitones, setSemitones] = createSignal(12);
  const [preset, setPreset] = createSignal(1);
  const [grooveAmount, setGrooveAmount] = createSignal(100);
  const [sourceClip, setSourceClip] = createSignal<Id | null>(null);
  const [density, setDensity] = createSignal(0);
  const [varyPitch, setVaryPitch] = createSignal(30);
  const [rhythm, setRhythm] = createSignal(30);
  const [varyVelocity, setVaryVelocity] = createSignal(20);
  const [seed, setSeed] = createSignal(newSeed());
  const [keepScale, setKeepScale] = createSignal(false);

  /** Other note clips of the project with notes — sources for "Take groove from clip". */
  const sources = createMemo(() =>
    session.project.clips
      .filter((c) => c.id !== props.clip && clipNotes(c).length > 0)
      .map((c) => {
        const track = session.project.tracks.find((tr) => tr.id === c.track)?.name ?? t().noteTools.trackFallback;
        return { id: c.id, label: t().noteTools.sourceClip(track, barIndex(session.project.tempoMap, c.startTick) + 1) };
      }),
  );
  const source = () => sources().find((c) => c.id === sourceClip())?.id ?? sources()[0]?.id;
  const variation = (): Variation => ({
    seed: seed(), density: density() / 100, pitch: varyPitch() / 100, rhythm: rhythm() / 100, velocity: varyVelocity() / 100,
    scale: keepScale() ? { root: props.scale.root, scale: props.scale.scale } : undefined,
  });

  const toggle = (panel: Panel) => setOpen(open() === panel ? null : panel);
  const apply = (run: () => Promise<unknown>) => {
    setOpen(null);
    void run();
  };
  const msg = () => t().noteTools;

  return (
    <div class="flex flex-wrap gap-0.5" title={msg().editsApplyTo(props.ids?.length)}>
      <ToolButton label={msg().quantize} open={open() === "quantize"} disabled={!props.hasNotes} onToggle={() => toggle("quantize")} onClose={() => setOpen(null)}>
        <p class="text-muted">{msg().gridInfo(props.gridLabel)}</p>
        <Row label={msg().strengthPercent}><NumberField value={strength()} min={0} max={100} label={msg().quantizeStrength} class="w-16" onCommit={setStrength} /></Row>
        <label class="flex items-center gap-2"><input type="checkbox" checked={ends()} onChange={(e) => setEnds(e.currentTarget.checked)} />{msg().endsToo}</label>
        <ApplyButton onClick={() => apply(() => notes.quantize(props.clip, props.ids, props.grid, strength() / 100, ends()))} />
      </ToolButton>

      <ToolButton label={msg().humanize} open={open() === "humanize"} disabled={!props.hasNotes} onToggle={() => toggle("humanize")} onClose={() => setOpen(null)}>
        <Row label={msg().timing}><NumberField value={timing()} min={0} max={MAX_HUMANIZE_TICKS} label={msg().timingSpread} class="w-16" onCommit={setTiming} /></Row>
        <Row label={msg().velocityPlusMinus}><NumberField value={velocity()} min={0} max={MAX_HUMANIZE_VELOCITY} label={msg().velocitySpread} class="w-16" onCommit={setVelocity} /></Row>
        <p class="text-muted">{msg().ticksHint}</p>
        <ApplyButton onClick={() => apply(() => notes.humanize(props.clip, props.ids, timing(), velocity()))} />
      </ToolButton>

      <ToolButton label={msg().transpose} open={open() === "transpose"} disabled={!props.hasNotes} onToggle={() => toggle("transpose")} onClose={() => setOpen(null)}>
        <div class="flex gap-1">
          <For each={[-12, -1, 1, 12]}>
            {(step) => (
              <button class="btn !h-6 flex-1 text-xs" onClick={() => apply(() => notes.transpose(props.clip, props.ids, step))}>
                {step > 0 ? `+${step}` : step}
              </button>
            )}
          </For>
        </div>
        <Row label={msg().semitones}><NumberField value={semitones()} min={-127} max={127} label={msg().semitoneShift} class="w-16" onCommit={setSemitones} /></Row>
        <ApplyButton onClick={() => apply(() => notes.transpose(props.clip, props.ids, semitones()))} />
      </ToolButton>

      <ToolButton label={msg().fitToScale} open={open() === "scale"} disabled={false} onToggle={() => toggle("scale")} onClose={() => setOpen(null)}>
        <Row label={msg().root}>
          <select class="field !h-6 text-xs" value={props.scale.root} onChange={(e) => props.onScale({ ...props.scale, root: Number(e.currentTarget.value) })}>
            <For each={ROOTS}>{(r) => <option value={r}>{pitchClassName(r)}</option>}</For>
          </select>
        </Row>
        <Row label={msg().scale}>
          <select
            class="field !h-6 text-xs" value={props.scale.scale}
            onChange={(e) => { const value = e.currentTarget.value; if (isScale(value)) props.onScale({ ...props.scale, scale: value }); }}
          >
            <For each={SCALES}>{(s) => <option value={s}>{msg().scales[s]}</option>}</For>
          </select>
        </Row>
        <label class="flex items-center gap-2"><input type="checkbox" checked={props.highlight} onChange={(e) => props.onHighlight(e.currentTarget.checked)} />{msg().highlight}</label>
        <p class="text-muted">{msg().scaleHint}</p>
        <button class="btn !h-7 text-xs" disabled={!props.hasNotes} onClick={() => apply(() => notes.fitToScale(props.clip, props.ids, props.scale.root, props.scale.scale))}>
          {msg().snapToScale}
        </button>
      </ToolButton>

      <ToolButton label={msg().groove} open={open() === "groove"} disabled={!props.hasNotes} onToggle={() => toggle("groove")} onClose={() => setOpen(null)}>
        <Row label={msg().template}>
          <select class="field !h-6 w-32 text-xs" value={preset()} onChange={(e) => setPreset(Number(e.currentTarget.value))}>
            <For each={GROOVE_PRESETS}>{(p, i) => <option value={i()}>{p.label()}</option>}</For>
          </select>
        </Row>
        <Slider label={msg().amount} value={grooveAmount()} min={0} max={100} onInput={setGrooveAmount} />
        <ApplyButton onClick={() => apply(() => notes.groove(props.clip, props.ids, GROOVE_PRESETS[preset()]!.template, grooveAmount() / 100))} />
        <div class="flex flex-col gap-1 border-t border-line pt-2">
          <Row label={msg().fromClip}>
            <select class="field !h-6 w-32 text-xs" value={source() ?? ""} disabled={sources().length === 0} onChange={(e) => setSourceClip(Number(e.currentTarget.value))}>
              <For each={sources()} fallback={<option value="">{msg().noSourceClips}</option>}>{(c) => <option value={c.id}>{c.label}</option>}</For>
            </select>
          </Row>
          <button
            class="btn !h-7 text-xs" disabled={source() === undefined}
            onClick={() => { const from = source(); if (from !== undefined) apply(() => notes.grooveFromClip(props.clip, props.ids, from, grooveAmount() / 100)); }}
          >{msg().takeGroove}</button>
        </div>
        <p class="text-muted">{msg().grooveHint}</p>
      </ToolButton>

      <ToolButton label={msg().variation} open={open() === "vary"} disabled={!props.hasNotes} onToggle={() => toggle("vary")} onClose={() => setOpen(null)}>
        <Slider label={msg().density} value={density()} min={-100} max={100} onInput={setDensity} />
        <Slider label={msg().pitch} value={varyPitch()} min={0} max={100} onInput={setVaryPitch} />
        <Slider label={msg().rhythm} value={rhythm()} min={0} max={100} onInput={setRhythm} />
        <Slider label={msg().velocity} value={varyVelocity()} min={0} max={100} onInput={setVaryVelocity} />
        <Row label={msg().seed}>
          <span class="flex items-center gap-1">
            <NumberField value={seed()} min={0} max={2 ** 32 - 1} label={msg().seedField} class="w-24" onCommit={setSeed} />
            <button class="btn btn-icon !h-6 !w-6 !min-w-6" title={msg().newSeed} aria-label={msg().newSeed} onClick={() => setSeed(newSeed())}><IconDice size={14} /></button>
          </span>
        </Row>
        <label class="flex items-center gap-2">
          <input type="checkbox" checked={keepScale()} onChange={(e) => setKeepScale(e.currentTarget.checked)} />
          {msg().inScale(pitchClassName(props.scale.root), msg().scales[props.scale.scale])}
        </label>
        <p class="text-muted">{msg().variationHint}</p>
        <div class="flex gap-1">
          <button class="btn btn-primary flex-1 text-xs" onClick={() => apply(() => notes.vary(props.clip, props.ids, variation()))}>{t().common.apply}</button>
          <button
            class="btn !h-7 flex-1 text-xs" title={msg().toNewClipHint}
            onClick={() => apply(() => notes.varyToNewClip(props.clip, props.ids, variation()))}
          >{msg().toNewClip}</button>
        </div>
      </ToolButton>

      <button
        class="btn btn-ghost !px-2 text-xs" disabled={!props.hasNotes} onClick={() => void notes.legato(props.clip, props.ids)}
        title={msg().legatoHint}
      >{msg().legato}</button>
    </div>
  );
}

/** A button with a parameter panel under it; a click outside or Esc closes the panel. */
function ToolButton(props: { label: string; open: boolean; disabled: boolean; onToggle(): void; onClose(): void; children: JSX.Element }) {
  return (
    <span class="relative">
      <button class="btn btn-ghost !gap-0.5 !pl-2 !pr-1 text-xs" classList={{ "!bg-hover": props.open }} disabled={props.disabled} aria-expanded={props.open} onClick={() => props.onToggle()}>
        {props.label}<IconChevronDown size={14} class="text-muted" />
      </button>
      <Show when={props.open}>
        <ToolPanel label={props.label} onClose={props.onClose}>{props.children}</ToolPanel>
      </Show>
    </span>
  );
}

function ToolPanel(props: { label: string; onClose(): void; children: JSX.Element }) {
  let panel: HTMLDivElement | undefined;
  // The panel opens to the right of its button: the buttons sit at the editor's left edge, so there is no room to the left.
  // "Inside" includes the panel's own button: otherwise pressing the button would close the panel and the click reopen it.
  closeOnOutside(() => panel?.parentElement ?? undefined, props.onClose);
  return (
    <div
      ref={panel} role="dialog" aria-label={props.label}
      class="anim-pop absolute left-0 top-8 z-50 flex w-64 flex-col gap-2 rounded border border-field-line bg-bg p-2.5 text-xs shadow-lg"
    >
      {props.children}
    </div>
  );
}

function Row(props: { label: string; children: JSX.Element }) {
  return <label class="flex items-center justify-between gap-2"><span class="text-muted">{props.label}</span>{props.children}</label>;
}

/** A slider with a label and its value (in percent). */
function Slider(props: { label: string; value: number; min: number; max: number; onInput(value: number): void }) {
  return (
    <label class="flex items-center justify-between gap-2">
      <span class="w-16 text-muted">{props.label}</span>
      <input
        type="range" class="flex-1" min={props.min} max={props.max} value={props.value} aria-label={props.label}
        onInput={(e) => props.onInput(Number(e.currentTarget.value))}
      />
      <span class="w-11 whitespace-nowrap text-right tabular-nums">{props.value} %</span>
    </label>
  );
}

function ApplyButton(props: { onClick(): void }) {
  return <button class="btn btn-primary text-xs" onClick={() => props.onClick()}>{t().common.apply}</button>;
}
