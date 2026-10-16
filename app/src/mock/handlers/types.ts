// Mock backend handler types: an RPC method → a function over `MockWorld`.
/** Handler of one RPC method: JSON params → result (or a thrown backend error, `fail` in `../errors.ts`). */
// biome-ignore lint/suspicious/noExplicitAny: RPC params arrive as untyped JSON; each handler takes its own fields.
export type Handler = (params: any) => unknown;

/** Handlers of one area: method name → handler. */
export type HandlerSet = Record<string, Handler>;
