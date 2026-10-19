// M (mute) and S (solo) buttons of a node with a mixer strip: a track source or a bus.
import type { Id } from "../api/types";
import { t } from "../i18n";
import { useSession } from "../session";

export function MuteSolo(props: { node: Id; mute: boolean; solo: boolean; /** Button classes for the placement. */ class: string }) {
  const { graph } = useSession();
  return (
    <>
      <button
        class={`btn ${props.class}`} classList={{ "!bg-danger !text-white anim-toggle": props.mute }} title={t().controls.muteHint} aria-label={t().controls.mute} aria-pressed={props.mute}
        onPointerDown={(e) => e.stopPropagation()} onClick={() => void graph.setMute(props.node, !props.mute)}
      >M</button>
      <button
        class={`btn ${props.class}`} classList={{ "btn-on": props.solo }} title={t().controls.soloHint} aria-label={t().controls.solo} aria-pressed={props.solo}
        onPointerDown={(e) => e.stopPropagation()} onClick={() => void graph.setSolo(props.node, !props.solo)}
      >S</button>
    </>
  );
}
