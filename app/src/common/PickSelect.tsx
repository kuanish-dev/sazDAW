import { For } from "solid-js";

export interface PickOption {
  value: string;
  label: string;
}

export function PickSelect(props: {
  placeholder: string;
  options: PickOption[];
  onPick: (value: string) => void;
  class?: string;
  title?: string;
  disabled?: boolean;
}) {
  return (
    <select
      class={`field ${props.class ?? ""}`}
      value=""
      disabled={props.disabled}
      aria-label={props.placeholder}
      title={props.title}
      onChange={(e) => {
        const value = e.currentTarget.value;
        e.currentTarget.value = "";
        if (value) props.onPick(value);
      }}
    >
      <option value="">{props.placeholder}</option>
      <For each={props.options}>{(o) => <option value={o.value}>{o.label}</option>}</For>
    </select>
  );
}
