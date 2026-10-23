import type { MasterWindow, NodeWindow } from "./types";

/** `sampleRate: u32`, `frames: u32`, then `frames` samples. */
export function decodeMasterWindow(buffer: ArrayBuffer): MasterWindow {
  const view = new DataView(buffer);
  return { sampleRate: view.getUint32(0, true), samples: new Float32Array(buffer, 8, view.getUint32(4, true)) };
}

/** `count: u32`, then per window `node: u32`, `watch: u32`, `end: u64`, `sampleRate: u32`, `frames: u32`, `frames`
 *  samples of the input and `frames` of the output. */
export function decodeNodeWindows(buffer: ArrayBuffer): NodeWindow[] {
  const view = new DataView(buffer);
  const windows: NodeWindow[] = [];
  let at = 4;
  for (let left = view.getUint32(0, true); left > 0; left--) {
    const frames = view.getUint32(at + 20, true);
    windows.push({
      node: view.getUint32(at, true),
      watch: view.getUint32(at + 4, true),
      end: view.getUint32(at + 8, true) + view.getUint32(at + 12, true) * 2 ** 32,
      sampleRate: view.getUint32(at + 16, true),
      input: new Float32Array(buffer, at + 24, frames),
      output: new Float32Array(buffer, at + 24 + 4 * frames, frames),
    });
    at += 24 + 8 * frames;
  }
  return windows;
}
