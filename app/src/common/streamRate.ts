import { type Accessor, createSignal, onMount } from "solid-js";
import { useSettings } from "../settings/role";

/** `StreamConfig::DEFAULT` in Rust: the engine runs at this rate until a device opens. */
const DEFAULT_SAMPLE_RATE = 48_000;

export function createStreamRate(): Accessor<number> {
  const settings = useSettings();
  const [rate, setRate] = createSignal(DEFAULT_SAMPLE_RATE);
  onMount(async () => {
    const status = await settings.audio.status();
    if (status && status.sampleRate > 0) setRate(status.sampleRate);
  });
  return rate;
}
