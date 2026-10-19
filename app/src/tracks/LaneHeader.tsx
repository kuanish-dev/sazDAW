// Automation lane row left of the timeline (right under its track, or in the "Buses & master" block): the parameter's
// name, a click selects the row, the trash button deletes the lane. A selected row scrolls itself into view (a lane
// picked from a knob's menu may be far down the list).
import { createEffect } from "solid-js";
import type { AutomationLane } from "../api/types";
import { targetLabel } from "../automation/automationRules";
import { t } from "../i18n";
import { useSession } from "../session";
import { IconPulse, IconTrash } from "../common/icons";

export function LaneHeader(props: { lane: AutomationLane }) {
  const session = useSession();
  const { automation } = session;
  const msg = () => t().automation;
  const selected = () => session.selection.automation()?.lane === props.lane.id;
  let row!: HTMLDivElement;
  createEffect(() => {
    if (selected()) row.scrollIntoView({ block: "nearest" });
  });

  return (
    <div ref={row}
      class="flex cursor-default items-center gap-1.5 border-t border-line pl-6 pr-2 text-xs"
      classList={{ "bg-hover": selected() }}
      style={{ height: `${session.settings.appearance.rowHeight()}px` }}
      title={msg().laneHint}
      onClick={() => automation.select(props.lane.id)}
    >
      <IconPulse size={14} class="shrink-0 text-blue" />
      <span class="min-w-0 flex-1 truncate">{targetLabel(props.lane.target, session.project, session.catalog())}</span>
      <button class="btn btn-icon !h-6 !w-6 !min-w-6 hover:!text-danger" title={msg().removeLane} aria-label={msg().removeLane}
        onClick={(e) => { e.stopPropagation(); void automation.removeLane(props.lane.id); }}><IconTrash size={14} /></button>
    </div>
  );
}

/** Title row above the lanes that belong to no track. */
export function BusLanesHeader() {
  const session = useSession();
  return (
    <div class="flex items-center border-t border-line bg-hover px-2.5 text-xs font-semibold text-muted"
      style={{ height: `${session.settings.appearance.rowHeight()}px` }}>{t().automation.busLanes}</div>
  );
}
