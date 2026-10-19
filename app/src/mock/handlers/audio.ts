// Audio settings in the browser without Rust: fake devices, status and input level (a pseudo sine), the master window
// for the visualizer and the nodes' windows for the effect visuals (both synthesized in `../liveSound.ts`, answered as
// bytes, `../wire.ts`), the live master meter (`../masterMeter.ts`) and the mono check. The windows keep the backend's
// leases (`CAPTURE_LEASE_SECONDS`), counted in wall time.
import { type AudioDevice, type AudioSettings, type AudioStatus, CAPTURE_LEASE_SECONDS, type Id, MAX_WATCHED_NODES, NODE_WINDOW_FRAMES, type NodeCursor } from "../../api/types";
import { MAX_RECORD_LATENCY_MS, isRecordLatencyValid } from "../../domain/audioSettings";
import { t } from "../../i18n";
import { fail } from "../errors";
import { MASTER_WINDOW_FRAMES, mockLiveSamples } from "../liveSound";
import { masterMeterReading, resetMasterMeter } from "../masterMeter";
import { type MockNodeWindow, encodeMasterWindow, encodeNodeWindows } from "../wire";
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
  // In the output device's rate, like the engine stream; the read that starts the capture has no samples yet.
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
  // Monitoring only: the mock plays no sound, the tick shows the switch.
  h["audio.setMono"] = ({ on }: { on: boolean }) => {
    w.mono = on;
    return null;
  };
  // Like `NodeScope::node_windows`: reading a node renews its lease; a new node takes a free slot or the one read longest
  // ago (not one read in this call). Only the frames after a cursor of the same watch, otherwise a whole window. The mock
  // runs no effects: what comes out of a node is what came in.
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
