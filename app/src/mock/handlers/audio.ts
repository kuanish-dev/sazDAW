import { type AudioDevice, type AudioSettings, type AudioStatus, CAPTURE_LEASE_SECONDS, type Id, MAX_WATCHED_NODES, NODE_WINDOW_FRAMES, type NodeCursor } from "../../api/types";
import { MAX_RECORD_LATENCY_MS, isRecordLatencyValid } from "../../settings/audioSettings";
import { t } from "../../i18n";
import { fail } from "../errors";
import { MASTER_WINDOW_FRAMES, mockLiveSamples } from "../liveSound";
import { masterMeterReading, resetMasterMeter } from "../masterMeter";
import { type MockNodeWindow, encodeMasterWindow, encodeNodeWindows } from "../wire";
import type { MockWorld } from "../world";
import type { HandlerSet } from "./types";

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

/** When a capture read now stops, ms on the `performance.now` clock. */
const leaseEnd = (now: number) => now + CAPTURE_LEASE_SECONDS * 1000;

export function audioHandlers(w: MockWorld): HandlerSet {
  let settings: AudioSettings = { output: null, input: null, bufferFrames: null, recordLatencyMs: null };
  let watching = false;
  /** When the master capture stops unless read again (ms). */
  let masterUntil = 0;
  /** The captured nodes, like `NodeScope`: each with its watch number, when the watch began (ms, so its frame count runs
   *  with the clock) and when it stops unless read again. */
  const captured = new Map<Id, { watch: number; since: number; until: number }>();
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
    if (next.output === t().mock.audio.broken) throw fail("AU-440", { details: "the device does not support the stream configuration" });
    if (next.recordLatencyMs !== null && !isRecordLatencyValid(next.recordLatencyMs)) {
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
  h["audio.masterWindow"] = (): ArrayBuffer => {
    const now = performance.now();
    const running = now < masterUntil;
    masterUntil = leaseEnd(now);
    return encodeMasterWindow(running ? mockLiveSamples(w, current.sampleRate, MASTER_WINDOW_FRAMES) : [], current.sampleRate);
  };
  h["audio.masterMeter"] = () => masterMeterReading(w, current.sampleRate);
  h["audio.resetMasterMeter"] = () => {
    resetMasterMeter(w);
    return null;
  };
  h["audio.setMono"] = ({ on }: { on: boolean }) => {
    w.mono = on;
    return null;
  };
  h["audio.nodeWindows"] = ({ nodes, seen = [] }: { nodes: Id[]; seen?: NodeCursor[] }): ArrayBuffer => {
    const now = performance.now();
    for (const [node, capture] of captured) if (capture.until <= now) captured.delete(node);
    const read = new Set<Id>();
    const windows: MockNodeWindow[] = [];
    for (const node of nodes) {
      if (read.has(node)) continue;
      let capture = captured.get(node);
      if (!capture) {
        if (captured.size >= MAX_WATCHED_NODES) {
          const oldest = [...captured].filter(([id]) => !read.has(id)).sort((a, b) => a[1].until - b[1].until)[0];
          if (!oldest) continue;
          captured.delete(oldest[0]);
        }
        capture = { watch: nextWatch++, since: now, until: now };
        captured.set(node, capture);
      }
      capture.until = leaseEnd(now);
      read.add(node);
      const end = Math.floor(((now - capture.since) / 1000) * current.sampleRate);
      const watch = capture.watch;
      const cursor = seen.find((c) => c.node === node && c.watch === watch && c.end <= end);
      const samples = mockLiveSamples(w, current.sampleRate, cursor ? Math.min(end - cursor.end, NODE_WINDOW_FRAMES) : NODE_WINDOW_FRAMES);
      windows.push({ node, watch, end, input: samples, output: samples, sampleRate: current.sampleRate });
    }
    return encodeNodeWindows(windows);
  };
  return h;
}
