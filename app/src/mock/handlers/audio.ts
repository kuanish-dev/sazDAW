// Audio settings in the browser without Rust: fake devices, status and input level (a pseudo sine), the master window
// for the visualizer and the watched nodes' windows for the effect visuals (both synthesized in `../liveSound.ts`), the
// live master meter (`../masterMeter.ts`) and the mono check.
import { type AudioDevice, type AudioSettings, type AudioStatus, type Id, MAX_WATCHED_NODES, type MasterWindow, NODE_WINDOW_FRAMES, type NodeCursor, type NodeWindow } from "../../api/types";
import { MAX_RECORD_LATENCY_MS, isRecordLatencyValid } from "../../domain/audioSettings";
import { t } from "../../i18n";
import { fail } from "../errors";
import { MASTER_WINDOW_FRAMES, mockLiveSamples } from "../liveSound";
import { masterMeterReading, resetMasterMeter, watchMasterMeter } from "../masterMeter";
import type { MockWorld } from "../world";
import type { HandlerSet } from "./types";

// Device names are localized like the system's own names would be; the "broken" device always fails to open.
const outputs = (): AudioDevice[] => [
  { name: t().mock.audio.speakers, isDefault: true, channels: 2 },
  { name: t().mock.audio.headphones, isDefault: false, channels: 2 },
  { name: t().mock.audio.usb, isDefault: false, channels: 2 },
  { name: t().mock.audio.broken, isDefault: false, channels: 2 },
];
const inputs = (): AudioDevice[] => [
  { name: t().mock.audio.mic, isDefault: true, channels: 1 },
  { name: t().mock.audio.usb, isDefault: false, channels: 2 },
];

/** The USB interface runs at 48 kHz, the others at 44.1. */
const rateOf = (name: string) => (name === t().mock.audio.usb ? 48000 : 44100);

export function audioHandlers(w: MockWorld): HandlerSet {
  let settings: AudioSettings = { output: null, input: null, bufferFrames: null, recordLatencyMs: null };
  let watching = false;
  let watchingMaster = false;
  /** The watched nodes, like `NodeScope`: each with its watch number and when the watch began (ms), so its frame count
   *  runs with the clock. */
  let watchedNodes = new Map<Id, { watch: number; since: number }>();
  let nextWatch = 0;
  const outputName = (s: AudioSettings) => s.output ?? outputs().find((d) => d.isDefault)!.name;
  const status = (s: AudioSettings): AudioStatus => ({
    running: true, outputName: outputName(s), sampleRate: rateOf(outputName(s)), channels: 2, bufferRange: [32, 4096], error: null,
  });
  let current = status(settings);

  const h: HandlerSet = {};
  h["audio.devices"] = () => ({ outputs: outputs(), inputs: inputs() });
  h["audio.settings"] = () => settings;
  h["audio.status"] = () => current;
  h["audio.apply"] = (next: AudioSettings) => {
    // Like the real backend: if the device fails to open, keep the previous audio and settings.
    if (next.output === t().mock.audio.broken) throw fail("AU-440", { details: "the device does not support the stream configuration" });
    if (next.recordLatencyMs !== null && !isRecordLatencyValid(next.recordLatencyMs)) {
      // The backend has no code for this limit; a value it cannot take is "invalid params".
      throw fail("RP-101", { method: "audio.apply", details: `recordLatencyMs ${next.recordLatencyMs} is outside 0..=${MAX_RECORD_LATENCY_MS}` });
    }
    settings = { ...next };
    current = status(settings);
    return current;
  };
  h["audio.watchInput"] = ({ on }) => {
    watching = on;
    return null;
  };
  h["audio.inputLevel"] = () => {
    if (!watching) return { left: 0, right: 0 };
    const t = performance.now();
    return {
      left: 0.1 + 0.4 * Math.abs(Math.sin(t / 300)),
      right: 0.1 + 0.4 * Math.abs(Math.sin(t / 420 + 1)),
    };
  };
  h["audio.watchMaster"] = ({ on }) => {
    watchingMaster = on;
    return null;
  };
  // In the output device's rate, like the engine stream.
  h["audio.masterWindow"] = (): MasterWindow => ({
    samples: watchingMaster ? mockLiveSamples(w, current.sampleRate, MASTER_WINDOW_FRAMES) : [],
    sampleRate: current.sampleRate,
  });
  h["audio.watchMasterMeter"] = ({ on }: { on: boolean }) => {
    watchMasterMeter(w, on);
    return null;
  };
  h["audio.masterMeter"] = () => masterMeterReading(w, current.sampleRate);
  h["audio.resetMasterMeter"] = () => {
    resetMasterMeter(w);
    return null;
  };
  // Monitoring only: the mock plays no sound, the tick shows the switch.
  h["audio.setMono"] = ({ on }: { on: boolean }) => {
    w.mono = on;
    return null;
  };
  // The set replaces the old one, like `NodeScope::watch_nodes`: a node that stays keeps its watch, at most MAX_WATCHED_NODES.
  h["audio.watchNodes"] = ({ nodes }: { nodes: Id[] }) => {
    const next = new Map<Id, { watch: number; since: number }>();
    for (const node of nodes) {
      if (next.has(node) || next.size >= MAX_WATCHED_NODES) continue;
      next.set(node, watchedNodes.get(node) ?? { watch: nextWatch++, since: performance.now() });
    }
    watchedNodes = next;
    return null;
  };
  // Only the frames after a cursor of the same watch, otherwise a whole window. The mock runs no effects: what comes
  // out of a node is what came in.
  h["audio.nodeWindows"] = ({ seen = [] }: { seen?: NodeCursor[] }): NodeWindow[] =>
    [...watchedNodes].map(([node, { watch, since }]) => {
      const end = Math.floor(((performance.now() - since) / 1000) * current.sampleRate);
      const cursor = seen.find((c) => c.node === node && c.watch === watch && c.end <= end);
      const samples = mockLiveSamples(w, current.sampleRate, cursor ? Math.min(end - cursor.end, NODE_WINDOW_FRAMES) : NODE_WINDOW_FRAMES);
      return { node, watch, end, input: samples, output: samples, sampleRate: current.sampleRate };
    });
  return h;
}
