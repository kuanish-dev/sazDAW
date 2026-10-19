// Instrument and effect presets (`presets.*`) in mock memory: a kind's params under a name (the mock has no plugin state).
import type { EditorTarget, Params, Project } from "../../api/types";
import { MAX_PRESET_NAME_CHARS } from "../../presets/presetRules";
import { t } from "../../i18n";
import { fail } from "../errors";
import type { MockWorld } from "../world";
import type { HandlerSet } from "./types";

/** A preset or template name, trimmed (as `check_preset_name` in `daw-contracts/src/presets.rs`). */
export function checkedName(name: string): string {
  const trimmed = name.trim();
  if (trimmed === "") throw fail("PR-103");
  if ([...trimmed].length > MAX_PRESET_NAME_CHARS) throw fail("PR-104", { name: trimmed, max: MAX_PRESET_NAME_CHARS });
  // biome-ignore lint/suspicious/noControlCharactersInRegex: control characters in a file name are exactly what is forbidden.
  if (trimmed.startsWith(".") || /[/\\:\u0000-\u001f\u007f]/.test(trimmed)) throw fail("PR-105", { name: trimmed });
  return trimmed;
}

/** Kind and params of the target in the project (an object from `p`, editable). */
function target(w: MockWorld, p: Project, of: EditorTarget): { kind: string; params: Params } {
  if (of.type === "instrument") {
    const instrument = w.track(p, of.track).instrument;
    if (!instrument) throw fail("TR-160", { id: of.track });
    return instrument;
  }
  const kind = w.must(p.graph.nodes.find((n) => n.id === of.node), fail("GR-001", { id: of.node })).kind;
  if (kind.type !== "effect") throw fail("GR-160", { id: of.node });
  return { kind: kind.effect, params: kind.params };
}

export function presetsHandlers(w: MockWorld): HandlerSet {
  const h: HandlerSet = {};
  const presets = new Map<string, Map<string, Params>>();
  const ofKind = (kind: string) => presets.get(kind) ?? presets.set(kind, new Map()).get(kind)!;
  h["presets.list"] = ({ kind }) => [...ofKind(kind).keys()].sort((a, b) => a.localeCompare(b));
  h["presets.save"] = ({ target: of, name }) => {
    const { kind, params } = target(w, w.project, of);
    ofKind(kind).set(checkedName(name), { ...params });
  };
  h["presets.apply"] = ({ target: of, name }) => {
    const { kind } = target(w, w.project, of);
    const preset = ofKind(kind).get(checkedName(name));
    if (!preset) throw fail("PR-071", { name: name.trim() });
    return w.edit(t().mock.undo.preset(name.trim()), (p) => {
      const { params } = target(w, p, of);
      for (const [param, value] of Object.entries(preset)) if (param in params) params[param] = value;
    });
  };
  h["presets.remove"] = ({ kind, name }) => {
    if (!ofKind(kind).delete(checkedName(name))) throw fail("PR-071", { name: name.trim() });
  };
  return h;
}
