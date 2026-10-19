// Pan knob of a node (a track source or a bus): −1 left, 0 center, 1 right.
import type { Id } from "../api/types";
import { formatPan } from "../mixer/mixer";
import { t } from "../i18n";
import { useSession } from "../session";
import { Knob } from "./Knob";
import { ParamMenu } from "./ParamMenu";

const PAN_RANGE = { min: -1, max: 1, default: 0 } as const;

export function PanKnob(props: { node: Id; pan: number; size: number }) {
  const { graph } = useSession();
  return (
    <ParamMenu target={{ type: "nodePan", node: props.node }}>
      <Knob info={{ ...PAN_RANGE, label: t().controls.pan }} value={props.pan} size={props.size} format={formatPan} onChange={(pan) => void graph.setPan(props.node, pan)} />
    </ParamMenu>
  );
}
