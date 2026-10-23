import type { Id, ParamInfo, Params } from "../api/types";
import { linearPhaseLatencyMs } from "./linearPhase";
import { t } from "../i18n";
import { createStreamRate } from "../common/streamRate";
import { EqGraph } from "./EqGraph";

export function LinearEqGraph(props: { node: Id; infos: ParamInfo[]; values: Params; onChange: (name: string, value: number) => void }) {
  const rate = createStreamRate();
  return (
    <EqGraph
      node={props.node} infos={props.infos} values={props.values} onChange={props.onChange}
      caption={t().graph.effect.linearPhaseLatency(Math.round(linearPhaseLatencyMs(rate())))}
    />
  );
}
