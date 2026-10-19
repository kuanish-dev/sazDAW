// Context menu of any parameter control (knob, fader, pan, switch): right-click for its automation lane — add it (a
// timeline row) or select its row, remove it — and MIDI learn ("Assign MIDI controller" / "Remove assignment", with the CC number).
// The automation and MIDI learn roles do all the work.
import { type JSX, Show, createSignal } from "solid-js";
import type { ParamTarget } from "../api/types";
import { sameTarget, targetLabel } from "../automation/automationRules";
import { t } from "../i18n";
import { useSession } from "../session";
import { ContextMenu, MenuItem, MenuLabel, type MenuPoint, menuPoint } from "./ContextMenu";

export function ParamMenu(props: { target: ParamTarget; children: JSX.Element }) {
  const session = useSession();
  const { midiLearn: midi, automation } = session;
  const [menu, setMenu] = createSignal<MenuPoint | null>(null);
  const mapping = () => midi.mappings().find((m) => sameTarget(m.target, props.target));
  const lane = () => session.project.automation.find((l) => sameTarget(l.target, props.target));
  const waiting = () => {
    const target = midi.learning();
    return target !== null && sameTarget(target, props.target);
  };
  const label = () => targetLabel(props.target, session.project, session.catalog());
  const close = () => setMenu(null);
  const msg = () => t().controls;

  return (
    <div
      class="contents"
      onContextMenu={(e) => {
        e.preventDefault();
        e.stopPropagation();
        setMenu(menuPoint(e));
      }}
    >
      {props.children}
      <Show when={menu()}>
        {(at) => (
          <ContextMenu at={at()} label={label()} onClose={close}>
            <MenuLabel>{label()}</MenuLabel>
            <MenuItem onClose={close} onPick={() => void automation.showFor(props.target)}>
              {lane() ? msg().automation.show : msg().automation.add}
            </MenuItem>
            <Show when={lane()}>
              {(l) => <MenuItem onClose={close} onPick={() => void automation.removeLane(l().id)}>{msg().automation.remove}</MenuItem>}
            </Show>
            <MenuLabel>{msg().midi.title}</MenuLabel>
            <Show
              when={!waiting()}
              fallback={<MenuItem onClose={close} onPick={() => void midi.cancelLearn()}>{msg().midi.stopWaiting}</MenuItem>}
            >
              <MenuItem onClose={close} onPick={() => void midi.learn(props.target, label())}>
                {mapping() ? msg().midi.assignOther : msg().midi.assign}
              </MenuItem>
            </Show>
            <Show when={mapping()}>
              {(m) => <MenuItem onClose={close} onPick={() => void midi.removeMapping(props.target)}>{msg().midi.remove(m().cc)}</MenuItem>}
            </Show>
          </ContextMenu>
        )}
      </Show>
    </div>
  );
}
