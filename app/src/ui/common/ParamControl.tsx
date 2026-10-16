// A catalog parameter's control by its kind: knob, switch, choice or project sample picker.
// With `target`, right-click assigns a MIDI controller knob (not for the sample picker: it isn't a continuous value).
import { For, type JSX, Match, Show, Switch } from "solid-js";
import type { ParamInfo, ParamTarget } from "../../api/types";
import { t } from "../../i18n";
import { useSession } from "../../state/session";
import { Knob } from "./Knob";
import { ParamMenu } from "./ParamMenu";

export function ParamControl(props: { info: ParamInfo; value: number; target?: ParamTarget; onChange: (value: number) => void }) {
  const session = useSession();
  const label = () => props.info.label.replace(/,.*$/, "");
  const on = () => props.value >= 0.5;

  /** A non-knob control with its label below. */
  const Labeled = (inner: { wide?: boolean; children: JSX.Element }) => (
    <div class="flex w-full flex-col items-center gap-1" classList={{ "col-span-2": inner.wide }}>
      {inner.children}
      <span class="w-full truncate text-center text-[10px] leading-3 text-muted">{label()}</span>
    </div>
  );

  const control = () => (
    <Switch fallback={<Knob info={props.info} value={props.value} onChange={props.onChange} />}>
      <Match when={props.info.kind === "switch"}>
        <Labeled>
          <button type="button" class="btn !h-8 w-full text-[11px]" classList={{ "btn-on": on() }} aria-pressed={on()} onClick={() => props.onChange(on() ? 0 : 1)}>
            {on() ? t().common.on : t().common.off}
          </button>
        </Labeled>
      </Match>
      <Match when={props.info.kind === "choice"}>
        <Labeled>
          <select class="field !h-8 w-full text-[11px]" value={Math.round(props.value)} aria-label={props.info.label} onChange={(e) => props.onChange(Number(e.currentTarget.value))}>
            <For each={props.info.choices ?? []}>{(name, index) => <option value={index()}>{name}</option>}</For>
          </select>
        </Labeled>
      </Match>
      <Match when={props.info.kind === "sample"}>
        <Labeled wide>
          <select class="field !h-8 w-full text-[11px]" value={props.value} aria-label={props.info.label} onChange={(e) => props.onChange(Number(e.currentTarget.value))}>
            <option value={0}>{t().controls.noSample}</option>
            <For each={session.project.samples}>{(s) => <option value={s.id}>{s.name}</option>}</For>
          </select>
        </Labeled>
      </Match>
    </Switch>
  );
  const learnable = () => (props.info.kind === "sample" ? undefined : props.target);
  return (
    <Show when={learnable()} fallback={control()}>
      {(target) => <ParamMenu target={target()}>{control()}</ParamMenu>}
    </Show>
  );
}
