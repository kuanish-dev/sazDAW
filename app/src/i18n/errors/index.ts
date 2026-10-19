// Backend errors in the interface language. The backend sends `{ code, params, message }` (`daw_contracts::ContractError`):
// English is its `message`; other languages translate by `code` with the dictionaries here.
import { language } from "..";
import { ruErrors } from "./ru";

/** A backend error as it arrives over RPC. */
export interface BackendError {
  code: string;
  params: Record<string, string>;
  message: string;
}

const DICTIONARIES: Readonly<Record<string, Record<string, string>>> = { ru: ruErrors } as const;

export function isBackendError(error: unknown): error is BackendError {
  const e = error as BackendError | null;
  return typeof e === "object" && e !== null && typeof e.code === "string" && typeof e.message === "string";
}

/** Fills `{name}` placeholders with params; unknown ones stay visible. */
export function fillTemplate(template: string, params: Record<string, string>): string {
  return template.replace(/\{(\w+)\}/g, (whole, name: string) => params[name] ?? whole);
}

/**
 * Text of any failure for a person: a backend error in the current language (English or a missing translation —
 * its own `message`) followed by its code, so a screenshot names the exact failure; anything else as a string.
 */
export function errorText(error: unknown): string {
  if (!isBackendError(error)) return error instanceof Error ? error.message : String(error);
  const template = DICTIONARIES[language()]?.[error.code];
  const text = template ? fillTemplate(template, error.params ?? {}) : error.message;
  return `${text} (${error.code})`;
}
