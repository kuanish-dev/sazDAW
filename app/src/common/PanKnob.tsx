import type { Id } from "../api/types";
import { formatPan } from "../mixer/mixer";
import { t } from "../i18n";
import { Knob } from "./Knob";
import { ParamMenu } from "./ParamMenu";
import { useGraph } from "../graph/role";

const PAN_RANGE = { min: -1, max: 1, default: 0 } as const;

export function PanKnob(props: { node: Id; pan: number; size: number }) {
  const graph = useGraph();
  return (
    <ParamMenu target={{ type: "nodePan", node: props.node }}>
      <Knob info={{ ...PAN_RANGE, label: t().controls.pan }} value={props.pan} size={props.size} format={formatPan} onChange={(pan) => void graph.setPan(props.node, pan)} />
    </ParamMenu>
  );
}
