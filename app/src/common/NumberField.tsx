// Numeric field: the value applies on Enter or blur, the wheel changes it, Esc cancels the input.
import { createEffect, createSignal } from "solid-js";
import { clamp } from "./math";

export function NumberField(props: {
  value: number;
  min: number;
  max: number;
  step?: number;
  decimals?: number;
  class?: string;
  label?: string;
  onCommit: (value: number) => void;
}) {
  const decimals = () => props.decimals ?? 0;
  const [text, setText] = createSignal(props.value.toFixed(decimals()));
  let focused = false;

  createEffect(() => {
    const next = props.value.toFixed(decimals());
    if (!focused) setText(next);
  });

  const commit = () => {
    const parsed = Number(text().replace(",", "."));
    if (Number.isFinite(parsed)) {
      const clamped = clamp(parsed, props.min, props.max);
      if (clamped !== props.value) props.onCommit(clamped);
      setText(clamped.toFixed(decimals()));
    } else {
      setText(props.value.toFixed(decimals()));
    }
  };

  return (
    <input
      class={`field text-center font-mono ${props.class ?? "w-20"}`}
      inputMode="decimal"
      aria-label={props.label}
      value={text()}
      onFocus={(e) => { focused = true; e.currentTarget.select(); }}
      onBlur={() => { focused = false; commit(); }}
      onInput={(e) => setText(e.currentTarget.value)}
      onKeyDown={(e) => {
        if (e.key === "Enter") e.currentTarget.blur();
        if (e.key === "Escape") { setText(props.value.toFixed(decimals())); e.currentTarget.blur(); }
      }}
      onWheel={(e) => {
        e.preventDefault();
        const step = props.step ?? 1;
        const next = clamp(props.value + (e.deltaY < 0 ? step : -step), props.min, props.max);
        if (next !== props.value) props.onCommit(next);
      }}
    />
  );
}
