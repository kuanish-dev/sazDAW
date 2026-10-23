import type { JSX } from "solid-js";
import { IconCheck } from "./icons";
import { closeOnOutside } from "./pointer";

/** Point where the menu was opened (window coordinates). */
export interface MenuPoint {
  x: number;
  y: number;
}

/** Width and height by which the menu is pushed away from the right and bottom window edges. */
const EDGE_PX = 220;

export const menuPoint = (event: MouseEvent): MenuPoint => ({
  x: Math.min(event.clientX, window.innerWidth - EDGE_PX),
  y: Math.min(event.clientY, window.innerHeight - EDGE_PX),
});

export function ContextMenu(props: { at: MenuPoint; label: string; onClose: () => void; children: JSX.Element }) {
  let menu: HTMLDivElement | undefined;
  closeOnOutside(() => menu, props.onClose);
  return (
    <div
      ref={menu} role="menu" aria-label={props.label}
      class="anim-pop fixed z-50 flex min-w-44 flex-col rounded border border-field-line bg-bg p-1 text-sm shadow-lg"
      style={{ left: `${props.at.x}px`, top: `${props.at.y}px` }}
      onPointerDown={(e) => e.stopPropagation()}
    >
      {props.children}
    </div>
  );
}

/** Menu item: picking it closes the menu and runs the action. */
export function MenuItem(props: { onPick: () => void; onClose: () => void; children: JSX.Element }) {
  return (
    <button role="menuitem" class="menu-item" onClick={() => { props.onClose(); props.onPick(); }}>
      {props.children}
    </button>
  );
}

/** Heading for a group of items. */
export const MenuLabel = (props: { children: JSX.Element }) => <div class="px-2.5 pt-1.5 pb-0.5 text-[11px] font-semibold uppercase tracking-wide text-muted">{props.children}</div>;

/** Content of a menu item with a check mark column (radio-like choices: fade shapes, ramp on/off). */
export const MenuCheck = (props: { on: boolean; children: JSX.Element }) => (
  <span class="flex items-center gap-2">
    <span class="w-3.5 shrink-0">{props.on ? <IconCheck size={14} /> : null}</span>
    {props.children}
  </span>
);
