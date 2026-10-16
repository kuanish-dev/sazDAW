// Kinds catalog of the mock backend (a copy of the backend's built-in instruments and effects) and the params of a new instance.
// Labels come in the interface language at request time, like the real backend's answers.
import type { Catalog, KindInfo, ParamInfo, Params } from "../api/types";
import { clamp } from "../domain/math";
import { type ErrorCode, fail } from "./errors";
import { t } from "../i18n";

const SWITCHES = new Set(["sidechain", "ping_pong", "loop", "one_shot", "reverse", "listen"]);
/** Per-band switches of the band-split effects (`band2_solo`): named by band, so matched by their ending. */
const BAND_SWITCHES = /_(solo|bypass)$/;

const p = (name: string, label: string, min: number, max: number, def: number): ParamInfo => ({
  name, label, min, max, default: def,
  kind: name === "sample" ? "sample" : SWITCHES.has(name) || BAND_SWITCHES.test(name) ? "switch" : "number",
});

/** A choice param: the value is the option index. */
const choice = (name: string, label: string, choices: string[], def: number): ParamInfo => ({
  name, label, min: 0, max: choices.length - 1, default: def, kind: "choice", choices,
});

/** Which plugin kinds the plugin scan offers (enabled ones only); kept by `MockWorld`, set by `handlers/plugins.ts`. */
export interface OfferedPlugins {
  effects: KindInfo[];
  instruments: KindInfo[];
}

/** A copy of the backend catalog in the current language, built anew on each request. */
export function createMockCatalog(plugins: OfferedPlugins): Catalog {
  const { kinds: k, params: q, choices } = t().catalog;
  const drum = (kind: string, label: string): KindInfo => ({
    kind, label,
    params: [p("gain", q.volume, 0, 1.5, 1), p("tune", q.tuneSemitones, -12, 12, 0), p("decay", q.decayTimes, 0.3, 3, 1)],
  });
  // Plucked strings (`daw-strings`): the same params, the kind's own defaults (ring s, brightness %, body %, volume %);
  // the kinds played through an amp also have Drive (`drive`, its default).
  const plucked = (kind: string, label: string, [ring, tone, body, gain]: [number, number, number, number], drive?: number): KindInfo => ({
    kind, label,
    params: [
      p("ring", q.ringS, 0.2, 12, ring), p("tone", q.brightnessPercent, 0, 100, tone),
      p("body", q.bodyPercent, 0, 100, body), p("gain", q.volumePercent, 0, 100, gain),
      ...(drive === undefined ? [] : [p("drive", q.drivePercent, 0, 100, drive)]),
    ],
  });
  return {
    instruments: [
      drum("kick", k.kick), drum("snare", k.snare), drum("hat", k.hat), drum("clap", k.clap),
      drum("openhat", k.openhat), drum("tom_hi", k.tomHi), drum("tom_mid", k.tomMid), drum("tom_lo", k.tomLo),
      drum("crash", k.crash), drum("ride", k.ride), drum("rim", k.rim),
      {
        kind: "sampler", label: k.sampler,
        params: [
          p("sample", q.sample, 0, 1_000_000, 0), p("start", q.start, 0, 1, 0), p("end", q.end, 0, 1, 1), p("reverse", q.reverse, 0, 1, 0),
          p("root", q.rootNote, 0, 127, 60), p("gain", q.volume, 0, 1.5, 0.8),
          p("attack", q.attackS, 0.0005, 4, 0.002), p("release", q.releaseS, 0.0005, 8, 0.08),
          p("loop", q.loop, 0, 1, 0), p("one_shot", q.ignoreNoteOff, 0, 1, 1),
        ],
      },
      {
        kind: "synth", label: k.synth,
        params: [
          choice("waveform", q.waveform, choices.waveform, 2), p("attack", q.attackS, 0.0005, 4, 0.005), p("hold", q.holdS, 0, 4, 0),
          p("decay", q.decayS, 0.0005, 4, 0.15), p("sustain", q.sustain, 0, 1, 0.6), p("release", q.releaseS, 0.0005, 8, 0.12),
          p("low_cut", q.lowCutHz, 20, 2000, 20), p("high_cut", q.highCutHz, 40, 18000, 1800), p("gain", q.volumePercent, 0, 100, 25),
        ],
      },
      plucked("guitar", k.guitar, [4, 55, 50, 60]), plucked("electric_guitar", k.electricGuitar, [7, 60, 40, 55], 0),
      plucked("ukulele", k.ukulele, [1.8, 65, 45, 60]), plucked("bass_guitar", k.bassGuitar, [5, 30, 15, 70], 0),
      ...plugins.instruments,
    ],
    effects: [
      {
        kind: "eq", label: k.eq,
        params: [
          p("low_cut", q.lowCutHz, 20, 1000, 20), p("low_gain", q.lowDb, -18, 18, 0), p("low_freq", q.lowHz, 40, 400, 100), p("mid_gain", q.midDb, -18, 18, 0),
          p("mid_freq", q.midHz, 200, 8000, 1000), p("mid_q", q.midQ, 0.3, 8, 1),
          p("high_gain", q.highDb, -18, 18, 0), p("high_freq", q.highHz, 2000, 16000, 8000),
          p("high_cut", q.highCutHz, 1000, 20000, 20000),
        ],
      },
      {
        kind: "compressor", label: k.compressor,
        params: [
          p("threshold", q.thresholdDb, -60, 0, -18), p("ratio", q.ratio, 1, 20, 4), p("attack", q.attackMs, 0.1, 100, 10),
          p("release", q.releaseMs, 10, 1000, 120), p("knee", q.kneeDb, 0, 12, 6), p("makeup", q.makeupDb, 0, 24, 0),
          p("sidechain", q.sidechain, 0, 1, 0),
        ],
      },
      {
        kind: "reverb", label: k.reverb,
        params: [p("room", q.room, 0, 1, 0.6), p("damp", q.damping, 0, 1, 0.5), p("width", q.width, 0, 1, 1), p("mix", q.mix, 0, 1, 0.25)],
      },
      {
        kind: "delay", label: k.delay,
        params: [
          p("time", q.timeMs, 1, 2000, 350), p("feedback", q.feedback, 0, 0.95, 0.4), p("mix", q.mix, 0, 1, 0.3), p("ping_pong", q.pingPong, 0, 1, 0),
          choice("sync", q.sync, choices.sync, 0),
        ],
      },
      {
        kind: "distortion", label: k.distortion,
        params: [
          p("drive", q.driveDb, 0, 40, 12), p("tone", q.toneHz, 200, 16000, 8000), p("mix", q.mix, 0, 1, 1), p("output", q.outputDb, -24, 6, -6),
          choice("shape", q.shape, choices.distortionShape, 0),
        ],
      },
      {
        kind: "limiter", label: k.limiter,
        params: [p("ceiling", q.ceilingDb, -12, 0, -0.3), p("release", q.releaseMs, 10, 500, 80), p("input", q.inputDb, 0, 24, 0)],
      },
      {
        kind: "chorus", label: k.chorus,
        params: [p("rate", q.rateHz, 0.05, 5, 0.8), p("depth", q.depthMs, 0, 10, 3), p("delay", q.delayMs, 5, 30, 12), p("mix", q.mix, 0, 1, 0.4)],
      },
      {
        kind: "filter", label: k.filter,
        params: [
          choice("type", q.type, choices.filterType, 0), p("cutoff", q.cutoffHz, 20, 20000, 1000),
          p("resonance", q.resonanceQ, 0.5, 12, 0.7), p("mix", q.mix, 0, 1, 1),
        ],
      },
      {
        kind: "gate", label: k.gate,
        params: [
          p("threshold", q.thresholdDb, -80, 0, -40), p("attack", q.attackMs, 0.1, 50, 1), p("hold", q.holdMs, 0, 500, 50),
          p("release", q.releaseMs, 5, 2000, 150), p("range", q.rangeDb, -80, 0, -80), p("sidechain", q.sidechain, 0, 1, 0),
        ],
      },
      {
        kind: "saturator", label: k.saturator,
        params: [
          p("drive", q.driveDb, 0, 36, 6), choice("type", q.type, choices.saturatorType, 0),
          p("output", q.outputDb, -24, 6, 0), p("mix", q.mix, 0, 1, 1),
        ],
      },
      {
        kind: "multiband", label: k.multiband,
        params: [
          p("split1", q.splitHz(1), 20, 2000, 120), p("split2", q.splitHz(2), 100, 8000, 1000), p("split3", q.splitHz(3), 500, 18000, 6000),
          choice("bands", q.bands, ["3", "4"], 1),
          ...([[20, 200], [10, 150], [5, 100], [3, 80]] as const).flatMap(([attack, release], i) => {
            const n = i + 1;
            return [
              p(`band${n}_threshold`, q.bandThresholdDb(n), -60, 0, -20), p(`band${n}_ratio`, q.bandRatio(n), 1, 20, 3),
              p(`band${n}_attack`, q.bandAttackMs(n), 0.1, 100, attack), p(`band${n}_release`, q.bandReleaseMs(n), 10, 1000, release),
              p(`band${n}_gain`, q.bandGainDb(n), -12, 12, 0), p(`band${n}_solo`, q.bandSolo(n), 0, 1, 0), p(`band${n}_bypass`, q.bandBypass(n), 0, 1, 0),
            ];
          }),
          p("output", q.outputDb, -24, 12, 0),
        ],
      },
      {
        kind: "dynamic_eq", label: k.dynamicEq,
        params: [
          ...[100, 500, 2000, 8000].flatMap((freq, i) => {
            const n = i + 1;
            return [
              p(`band${n}_freq`, q.bandFreqHz(n), 20, 20000, freq), p(`band${n}_q`, q.bandQ(n), 0.3, 10, 1),
              p(`band${n}_gain`, q.bandGainDb(n), -18, 18, 0), p(`band${n}_threshold`, q.bandThresholdDb(n), -60, 0, -24),
              p(`band${n}_ratio`, q.bandRatio(n), 1, 20, 4), p(`band${n}_attack`, q.bandAttackMs(n), 0.1, 100, 5),
              p(`band${n}_release`, q.bandReleaseMs(n), 10, 1000, 120),
            ];
          }),
          p("sidechain", q.sidechain, 0, 1, 0),
        ],
      },
      {
        kind: "de_esser", label: k.deEsser,
        params: [
          p("freq", q.frequencyHz, 2000, 12000, 6500), p("threshold", q.thresholdDb, -60, 0, -30), p("range", q.rangeDb, 0, 24, 10),
          choice("mode", q.mode, choices.deEsserMode, 0), p("listen", q.listenBand, 0, 1, 0),
        ],
      },
      {
        kind: "transient", label: k.transient,
        params: [p("attack", q.attackPercent, -100, 100, 0), p("sustain", q.sustainPercent, -100, 100, 0), p("output", q.outputDb, -24, 12, 0)],
      },
      ...plugins.effects,
    ],
    noteEffects: [
      {
        kind: "arp", label: k.arp,
        params: [
          choice("rate", q.step, ["1/4", "1/8", "1/8T", "1/16", "1/16T", "1/32"], 3),
          choice("mode", q.order, choices.arpMode, 0),
          p("octaves", q.octaves, 1, 4, 1), p("gate", q.gatePercent, 10, 100, 50), p("swing", q.swingPercent, 0, 75, 0),
        ],
      },
      {
        kind: "chord", label: k.chord,
        params: [
          choice("type", q.chord, choices.chordType, 0),
          p("inversion", q.inversion, 0, 3, 0), { ...p("spread", q.spread, 0, 1, 0), kind: "switch" },
          choice("scale", q.scale, choices.scale, 0),
          choice("root", q.root, ["C", "C#", "D", "D#", "E", "F", "F#", "G", "G#", "A", "A#", "B"], 0),
        ],
      },
    ],
  };
}

/** A value allowed for the param (like the backend's `ParamInfo::fit`): within range, a whole option index for a choice. */
export function fitParam(info: ParamInfo, value: number): number {
  const fitted = clamp(value, info.min, info.max);
  return info.kind === "choice" ? Math.round(fitted) : fitted;
}

/**
 * Params of a new instance of a kind: defaults plus `overrides`, each fitted to its range.
 * An unknown param is the backend error `unknownParam` (its code differs for instruments, effects and MIDI effects).
 */
export function paramsFor(info: KindInfo, overrides: Params, unknownParam: ErrorCode): Params {
  const params: Params = Object.fromEntries(info.params.map((p) => [p.name, p.default]));
  for (const [name, value] of Object.entries(overrides)) {
    const param = info.params.find((p) => p.name === name);
    if (!param) throw fail(unknownParam, { kind: info.kind, param: name });
    params[name] = fitParam(param, value);
  }
  return params;
}
