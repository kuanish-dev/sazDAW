// The live graph of a built-in effect, chosen by its kind: what the effect does, drawn from its params (it follows the
// knobs at once), with the node's live sound over it (`nodeSound.ts`, faded out when the sound stops). Shown above the
// knobs in the effect panel and for every effect along the signal path on the Graph page; nothing for a kind without a
// graph (plugins). The EQ and filter handles change params like a knob.
import { Match, Switch, createMemo } from "solid-js";
import type { GraphNode, ParamInfo, Params } from "../../api/types";
import { effectInfo } from "../../domain/catalog";
import { compressorCurve } from "../../domain/compressorCurve";
import type { DynamicsCurve } from "../../domain/dynamicsGraph";
import { gateCurve } from "../../domain/gateCurve";
import { limiterCurve } from "../../domain/limiterCurve";
import { type Transfer, distortionTransfer, saturatorTransfer, tapeTransfer } from "../../domain/shaperCurve";
import { t } from "../../i18n";
import { useSession } from "../../state/session";
import { ChorusGraph } from "./ChorusGraph";
import { DeEsserGraph } from "./DeEsserGraph";
import { DelayGraph } from "./DelayGraph";
import { DynamicEqGraph } from "./DynamicEqGraph";
import { DynamicsGraph } from "./DynamicsGraph";
import { EqGraph } from "./EqGraph";
import { FilterGraph } from "./FilterGraph";
import { LinearEqGraph } from "./LinearEqGraph";
import { MultibandGraph } from "./MultibandGraph";
import { ReverbGraph } from "./ReverbGraph";
import { ShaperGraph } from "./ShaperGraph";
import { TransientGraph } from "./TransientGraph";

type Curve<T> = (infos: ParamInfo[], values: Params) => T | null;

/** The dynamics effects: their static curve and the graph's name and hint. */
const DYNAMICS: Record<string, { curve: Curve<DynamicsCurve>; label: () => string; hint: () => string }> = {
  compressor: { curve: compressorCurve, label: () => t().graph.effect.compressorGraph, hint: () => t().graph.effect.compressorHint },
  limiter: { curve: limiterCurve, label: () => t().graph.effect.limiterGraph, hint: () => t().graph.effect.limiterHint },
  gate: { curve: gateCurve, label: () => t().graph.effect.gateGraph, hint: () => t().graph.effect.gateHint },
};

/** The waveshapers: their transfer curve. */
const SHAPERS: Record<string, Curve<Transfer>> = { distortion: distortionTransfer, saturator: saturatorTransfer, tape: tapeTransfer };

/** The kinds with a graph of their own (the rest). */
const OWN_GRAPHS = new Set(["eq", "linear_eq", "filter", "delay", "chorus", "reverb", "multiband", "dynamic_eq", "de_esser", "transient"]);

/** Whether effect kind `kind` has a graph. */
export const hasEffectGraph = (kind: string) => OWN_GRAPHS.has(kind) || kind in DYNAMICS || kind in SHAPERS;

export function EffectGraph(props: { node: GraphNode }) {
  const session = useSession();
  const { graph } = session.actions;
  const kind = () => props.node.kind as Extract<GraphNode["kind"], { type: "effect" }>;
  const params = () => effectInfo(session.catalog(), kind().effect)?.params ?? [];
  const setParam = (name: string, value: number) => void graph.setEffectParam(props.node.id, name, value);
  const dynamics = () => DYNAMICS[kind().effect];
  const dynamicsCurve = createMemo(() => dynamics()?.curve(params(), kind().params) ?? null);
  const transfer = createMemo(() => SHAPERS[kind().effect]?.(params(), kind().params) ?? null);
  return (
    <Switch>
      <Match when={kind().effect === "eq"}>
        <EqGraph node={props.node.id} infos={params()} values={kind().params} onChange={setParam} />
      </Match>
      <Match when={kind().effect === "linear_eq"}>
        <LinearEqGraph node={props.node.id} infos={params()} values={kind().params} onChange={setParam} />
      </Match>
      <Match when={kind().effect === "filter"}>
        <FilterGraph node={props.node.id} infos={params()} values={kind().params} onChange={setParam} />
      </Match>
      <Match when={dynamicsCurve()}>
        {(curve) => (
          <DynamicsGraph
            node={props.node.id} curve={curve()} label={dynamics()?.label() ?? ""} hint={dynamics()?.hint() ?? ""}
            reduction={session.reductions[props.node.id] ?? 0}
          />
        )}
      </Match>
      <Match when={transfer()}>
        {(shape) => <ShaperGraph node={props.node.id} transfer={shape()} />}
      </Match>
      <Match when={kind().effect === "delay"}>
        <DelayGraph node={props.node.id} infos={params()} values={kind().params} />
      </Match>
      <Match when={kind().effect === "chorus"}>
        <ChorusGraph node={props.node.id} infos={params()} values={kind().params} />
      </Match>
      <Match when={kind().effect === "reverb"}>
        <ReverbGraph node={props.node.id} infos={params()} values={kind().params} />
      </Match>
      <Match when={kind().effect === "multiband"}>
        <MultibandGraph node={props.node.id} infos={params()} values={kind().params} onChange={setParam} />
      </Match>
      <Match when={kind().effect === "dynamic_eq"}>
        <DynamicEqGraph node={props.node.id} infos={params()} values={kind().params} onChange={setParam} />
      </Match>
      <Match when={kind().effect === "de_esser"}>
        <DeEsserGraph
          node={props.node.id} infos={params()} values={kind().params} reduction={session.reductions[props.node.id] ?? 0} onChange={setParam}
        />
      </Match>
      <Match when={kind().effect === "transient"}>
        <TransientGraph node={props.node.id} infos={params()} values={kind().params} />
      </Match>
    </Switch>
  );
}
