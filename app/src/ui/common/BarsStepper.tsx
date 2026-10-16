// "−  2.5 bars  +": a length shown in bars and changed by whole bars (a clip's content length, an automation clip and its
// pattern). A length that is a fraction of bars (after live writing, an imported file) steps to the nearest whole bar.
import { t } from "../../i18n";
import { IconAdd, IconMinus } from "./icons";

/** A length in whole bars one step up or down from `ticks`. */
const stepBars = (ticks: number, bar: number, up: boolean): number =>
  (up ? Math.floor(ticks / bar) + 1 : Math.ceil(ticks / bar) - 1) * bar;

/** `bar` — ticks in a bar (of the signature at the clip start); steps stay within one bar and `max` ticks. */
export function BarsStepper(props: { label: string; hint: string; ticks: number; bar: number; max: number; onSet(ticks: number): void }) {
  const set = (up: boolean) => {
    const next = stepBars(props.ticks, props.bar, up);
    if (next >= props.bar && next <= props.max) props.onSet(next);
  };
  const bars = () => Math.round((props.ticks / props.bar) * 100) / 100;
  return (
    <div class="flex items-center gap-1" title={props.hint}>
      <span class="label mr-1">{props.label}</span>
      <button class="btn btn-icon !h-6 !w-6 !min-w-6" aria-label={t().pianoRoll.shorter} title={t().pianoRoll.shorter} onClick={() => set(false)}><IconMinus size={14} /></button>
      <span class="w-16 text-center text-xs tabular-nums">{t().pianoRoll.bars(bars())}</span>
      <button class="btn btn-icon !h-6 !w-6 !min-w-6" aria-label={t().pianoRoll.longer} title={t().pianoRoll.longer} onClick={() => set(true)}><IconAdd size={14} /></button>
    </div>
  );
}
