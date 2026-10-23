import { createSignal } from "solid-js";
import type { TempoMap } from "../api/types";
import { menuPoint, type MenuPoint } from "../common/ContextMenu";
import { localPoint } from "../common/pointer";
import { flagAt } from "./tempoLayout";

/** The signature change menu opened on flag `index`. */
export interface TempoMenu {
  index: number;
  at: MenuPoint;
}

export function createTempoPointer(deps: { surface: () => HTMLElement; map: () => TempoMap; pxPerTick: () => number }) {
  const [menu, setMenu] = createSignal<TempoMenu | null>(null);

  const flag = (event: MouseEvent) => {
    const { x, y } = localPoint(deps.surface(), event);
    return flagAt(deps.map(), x, y, deps.pxPerTick());
  };
  const open = (event: MouseEvent): boolean => {
    const index = flag(event);
    if (index === null) return false;
    setMenu({ index, at: menuPoint(event) });
    event.preventDefault();
    return true;
  };

  return {
    menu,
    closeMenu: () => setMenu(null),
    onDouble: open,
    onContext: open,
  };
}
