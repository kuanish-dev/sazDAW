// The channel utility of a track source or a bus, used by the mixer strip and the graph inspector: polarity (Ø both
// sides, L and R one side), mono, the width knob (right-click: automation, MIDI) and the fine time alignment knob with
// its value in milliseconds and in samples at the stream rate; ‹ › nudge it by one sample.
import type { Id, Strip } from "../../api/types";
import {
  DELAY_RANGE, WIDTH_RANGE, flipBoth, formatDelayMs, formatDelaySamples, formatWidth, nudgeDelay,
} from "../../domain/channelUtility";
import { t } from "../../i18n";
import { useSession } from "../../state/session";
import { Knob } from "./Knob";
import { ParamMenu } from "./ParamMenu";

const BUTTON = "btn !h-5 !min-w-0 !px-1 text-[10px]";
const NUDGE = "btn btn-icon !h-4 !w-4 !min-w-4 text-[10px]";

export function ChannelUtility(props: { node: Id; strip: Strip; sampleRate: number; knobSize: number }) {
  const { graph } = useSession().actions;
  const text = () => t().mixer.utility;
  const polarity = () => props.strip.polarity;
  const both = () => polarity().invertLeft && polarity().invertRight;
  const either = () => polarity().invertLeft || polarity().invertRight;
  const setPolarity = (invertLeft: boolean, invertRight: boolean) => void graph.setPolarity(props.node, { invertLeft, invertRight });
  const nudge = (samples: number) => void graph.setDelay(props.node, nudgeDelay(props.strip.delayMs, samples, props.sampleRate));

  return (
    <div class="flex flex-col gap-1" role="group" aria-label={text().label} title={text().hint}>
      <div class="flex gap-0.5" onPointerDown={(e) => e.stopPropagation()}>
        <button
          class={`${BUTTON} flex-1`} classList={{ "btn-on": both() }} title={text().polarityHint} aria-label={text().polarityHint}
          aria-pressed={both() ? true : either() ? "mixed" : false} onClick={() => void graph.setPolarity(props.node, flipBoth(polarity()))}
        >{text().polarity}</button>
        <button
          class={BUTTON} classList={{ "btn-on": polarity().invertLeft }} title={text().invertLeft} aria-label={text().invertLeft}
          aria-pressed={polarity().invertLeft} onClick={() => setPolarity(!polarity().invertLeft, polarity().invertRight)}
        >{text().left}</button>
        <button
          class={BUTTON} classList={{ "btn-on": polarity().invertRight }} title={text().invertRight} aria-label={text().invertRight}
          aria-pressed={polarity().invertRight} onClick={() => setPolarity(polarity().invertLeft, !polarity().invertRight)}
        >{text().right}</button>
        <button
          class={`${BUTTON} flex-[2]`} classList={{ "btn-on": props.strip.mono }} title={text().monoHint} aria-label={text().monoHint}
          aria-pressed={props.strip.mono} onClick={() => void graph.setMono(props.node, !props.strip.mono)}
        >{text().mono}</button>
      </div>
      <div class="flex items-start gap-1">
        <div class="min-w-0 flex-1" classList={{ "opacity-50": props.strip.mono }}>
          <ParamMenu target={{ type: "nodeWidth", node: props.node }}>
            <Knob
              info={{ ...WIDTH_RANGE, label: text().width }} value={props.strip.width} size={props.knobSize} format={formatWidth}
              onChange={(width) => void graph.setWidth(props.node, width)}
            />
          </ParamMenu>
        </div>
        <div class="flex min-w-0 flex-1 flex-col items-center gap-0.5">
          <Knob
            info={{ ...DELAY_RANGE, label: text().delay }} value={props.strip.delayMs} size={props.knobSize} format={formatDelayMs}
            onChange={(ms) => void graph.setDelay(props.node, ms)}
          />
          <div
            class="flex max-w-full items-center gap-0.5" onPointerDown={(e) => e.stopPropagation()}
            title={text().delayHint(formatDelayMs(props.strip.delayMs), formatDelaySamples(props.strip.delayMs, props.sampleRate))}
          >
            <button class={NUDGE} aria-label={text().earlier} title={text().earlier} onClick={() => nudge(-1)}>‹</button>
            <span class="truncate font-mono text-[10px] leading-3 text-muted">{formatDelaySamples(props.strip.delayMs, props.sampleRate)}</span>
            <button class={NUDGE} aria-label={text().later} title={text().later} onClick={() => nudge(1)}>›</button>
          </div>
        </div>
      </div>
    </div>
  );
}
