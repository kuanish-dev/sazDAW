// Mock sound for the live pictures: the master window for the visualizer (`audio.masterWindow`) and a node's window for
// the effect visuals (`audio.nodeWindows`). Both get a stretch of plausible "music" synthesized at the transport position —
// a bass with harmonics, a chord, a little air and noise, and a kick on every beat. The mock does not run the node's
// effect: its input and output are the same music, so an EQ shows it whatever its bands and a distortion's trace lies on
// the 1:1 line.
import { secondsAt, tempoAt } from "../domain/tempoMap";
import type { MockWorld } from "./world";

/** Frames in the master window, as `MASTER_WINDOW_FRAMES` in Rust. */
export const MASTER_WINDOW_FRAMES = 2048;
/** Tones as [Hz, amplitude]: an A bass with its harmonics, an A major chord above it, a little high content. */
const TONES: [number, number][] = [
  [55, 0.22], [110, 0.12], [165, 0.05], [277.2, 0.06], [329.6, 0.05], [440, 0.04], [1760, 0.012], [3520, 0.006],
];
const NOISE = 0.015;
/** The kick: its pitch falls from KICK_TOP_HZ + KICK_LOW_HZ to KICK_LOW_HZ, both decays in seconds. */
const KICK_LEVEL = 0.5;
const KICK_LOW_HZ = 50;
const KICK_TOP_HZ = 110;
const KICK_PITCH_DECAY = 0.03;
const KICK_DECAY = 0.12;

/** A stretch of `frames` ending at the transport position, as loud as the master meter; silence while stopped. */
export function mockLiveSamples(w: MockWorld, sampleRate: number, frames: number): number[] {
  const samples = new Array<number>(frames).fill(0);
  const master = w.project.graph.nodes.find((node) => node.kind.type === "master");
  const level = w.state === "playing" && master ? (w.peaks.get(master.id) ?? 0) : 0;
  if (level === 0) return samples;
  const map = w.project.tempoMap;
  const end = secondsAt(map, w.position);
  // Beats counted in seconds from the start: exact for a constant tempo, close enough for a picture otherwise.
  const beat = 60 / tempoAt(map, w.position);
  for (let i = 0; i < frames; i++) {
    const t = end - (frames - i) / sampleRate;
    let value = (Math.random() * 2 - 1) * NOISE;
    for (const [hz, amplitude] of TONES) value += amplitude * Math.sin(2 * Math.PI * hz * t);
    const sinceBeat = ((t % beat) + beat) % beat;
    const kickHz = KICK_LOW_HZ + KICK_TOP_HZ * Math.exp(-sinceBeat / KICK_PITCH_DECAY);
    value += KICK_LEVEL * Math.exp(-sinceBeat / KICK_DECAY) * Math.sin(2 * Math.PI * kickHz * sinceBeat);
    samples[i] = value * level;
  }
  return samples;
}
