// MIDI: one made-up keyboard for live input (connecting checks the device and track); export checks the clip.
// MIDI learn: there is no knob, so it is "turned" at once and the mapping gets the first free CC from 20.
import type { MidiMapping, ParamTarget, Project } from "../../api/types";
import { sameTarget, targetRange } from "../../automation/automationRules";
import { t } from "../../i18n";
import { fail } from "../errors";
import type { MockWorld } from "../world";
import { targetProblem, validateTarget } from "./automation";
import type { HandlerSet } from "./types";

const FIRST_MOCK_CC = 20;

/** Mappings to existing targets (as `MidiMappingControl::mappings`). */
function liveMappings(p: Project): MidiMapping[] {
  return (p.midiMappings ?? []).filter((m) => targetProblem(p, m.target) === null);
}

const DEVICES = ["Mock Keys 49"];

export function midiHandlers(w: MockWorld): HandlerSet {
  const h: HandlerSet = {};
  h["midi.devices"] = () => [...DEVICES];
  // Like the backend: a project without note clips exports a file with only the tempo map.
  h["midi.exportProject"] = () => undefined;
  h["midi.exportClip"] = ({ clip }) => {
    if (w.clip(w.project, clip).content.type !== "notes") throw fail("NT-110", { id: clip });
  };
  // The browser has no files: the chosen "file" never has notes.
  h["midi.importInto"] = ({ clip }) => {
    if (w.clip(w.project, clip).content.type !== "notes") throw fail("NT-110", { id: clip });
    throw fail("MD-171");
  };
  h["midi.connect"] = ({ device, target }) => {
    if (!DEVICES.includes(device)) throw fail("MD-070", { name: device });
    w.must(w.project.tracks.find((x) => x.id === target), fail("TR-001", { id: target }));
  };
  h["midi.setTarget"] = () => undefined;
  h["midi.learn"] = ({ target }: { target: ParamTarget }) => {
    validateTarget(w.project, target);
    w.edit(t().mockEdits.history.assignMidiCc, (p) => {
      const others = (p.midiMappings ?? []).filter((m) => !sameTarget(m.target, target));
      let cc = FIRST_MOCK_CC;
      while (others.some((m) => m.cc === cc)) cc += 1;
      const { min, max } = targetRange(target, p, w.catalog);
      p.midiMappings = [...others, { cc, channel: 0, target, min, max }];
    });
  };
  h["midi.cancelLearn"] = () => undefined;
  h["midi.mappings"] = () => ({ mappings: liveMappings(w.project), learning: null });
  h["midi.removeMapping"] = ({ target }: { target: ParamTarget }) => {
    if (!(w.project.midiMappings ?? []).some((m) => sameTarget(m.target, target))) throw fail("MD-110");
    w.edit(t().mockEdits.history.removeMidiCc, (p) => {
      p.midiMappings = (p.midiMappings ?? []).filter((m) => !sameTarget(m.target, target));
    });
  };
  h["midi.disconnect"] = () => undefined;
  return h;
}
