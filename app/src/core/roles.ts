// The app's roles, each made once, on first use, for the whole window. A role is defined next to its work
// (`defineRole` in its folder's `role.ts`) and says itself what it is made from: the shared core and the other roles it
// asks for (`use`). So no central list wires them, and the order roles come up in is theirs to know. A component takes
// its role through the role's hook (`usePianoroll`), and what it reads of the core through the hooks here
// (`useProjectCopy`, `usePlayback`, `useToasts`, `useBusy`); it never gets the core or the registry itself.
import { type JSX, createComponent, createContext, getOwner, runWithOwner, useContext } from "solid-js";
import type { ToastReads } from "./notifications";
import type { PlaybackReads } from "./playback";
import type { ProjectReads } from "./projectSync";
import type { Core } from "./core";

/** What a role is made from: the shared core and the other roles it asks for. */
export interface RoleDeps {
  readonly core: Core;
  use<T>(role: Role<T>): T;
}

/** A role of the app: its name (to report a cycle) and how it is made. */
export interface Role<T> {
  readonly name: string;
  readonly create: (deps: RoleDeps) => T;
}

export function defineRole<T>(name: string, create: (deps: RoleDeps) => T): Role<T> {
  return { name, create };
}

/** The window's roles, and the core they are made from (for the hooks of what the UI reads of it). */
export interface Roles {
  use<T>(role: Role<T>): T;
  readonly core: Core;
}

/**
 * The registry of one window. A role is made under the owner this is created in (the window's root), not under the
 * component that asked first: its effects and cleanups live as long as the window. Two roles that need each other are
 * a mistake in their definitions and stop with both names.
 */
export function createRoles(core: Core): Roles {
  const owner = getOwner();
  const made = new Map<Role<unknown>, unknown>();
  const making: Role<unknown>[] = [];
  const use = <T,>(role: Role<T>): T => {
    // Only `use` puts a value under a role, and it is that role's own `T`.
    if (made.has(role)) return made.get(role) as T;
    if (making.includes(role)) throw new Error(`Roles need each other: ${[...making, role].map((r) => r.name).join(" → ")}`);
    making.push(role);
    try {
      const value = runWithOwner(owner, () => role.create({ core, use }));
      if (value === undefined) throw new Error(`The ${role.name} role failed to start`);
      made.set(role, value);
      return value;
    } finally {
      making.pop();
    }
  };
  return { use, core };
}

const RolesContext = createContext<Roles>();

export function RolesProvider(props: { roles: Roles; children: JSX.Element }) {
  return createComponent(RolesContext.Provider, {
    value: props.roles,
    get children() {
      return props.children;
    },
  });
}

function roles(): Roles {
  const value = useContext(RolesContext);
  if (!value) throw new Error("RolesProvider is missing");
  return value;
}

/** A role for a component (through the role's own hook, like `usePianoroll`). */
export const useRole = <T,>(role: Role<T>): T => roles().use(role);

/** The project copy as the UI reads it: the project, its status, the catalog, whether one is open. */
export const useProjectCopy = (): ProjectReads => roles().core.copy;

/** The playback tick and the positions for drawing. */
export const usePlayback = (): PlaybackReads => roles().core.playback;

/** The notifications in the window corner. */
export const useToasts = (): ToastReads => roles().core.notifications;

/** The label of a long operation (opening a project); `null` — none. */
export const useBusy = (): (() => string | null) => roles().core.calls.busy;
