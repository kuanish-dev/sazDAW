# `src/mixer/` — the Mixer page

Map of the whole code: [ARCHITECTURE.md](../../../ARCHITECTURE.md). Cross-layer features it takes part in: [master-meter](../../../docs/features/master-meter.md), [master-visualizer](../../../docs/features/master-visualizer.md).

**The "Mixer" page**: track strips in track order, then buses, with the master pinned to the right edge. `MixerView.tsx` — layout and the open effect; `ChannelStrip.tsx` — a strip (name, kind, the channel utility `common/ChannelUtility.tsx` — see "Channel utility" ([docs/features/channel-utility.md](../../../docs/features/channel-utility.md)) — pan, fader, meter from `useMixer().levels`, M/S; a click selects the track); `StripRouting.tsx` — inserts, sends, output; `EffectPopover.tsx` — the insert's parameters with the same `EffectPanel` as the graph inspector, and "Remove" (`removeEffectKeepPath`).

**What a strip shows.** The graph is free-form, so what counts as an insert, the output and a send is a display convention in the pure `mixer/mixerStrips.ts` (rules in its header): the main path is the node's first regular post-fader edge, an insert is an effect on it that nothing else feeds (an effect shared by several tracks is their output, not an insert), the output and sends leave from the last insert (`tail`), a pre-fader edge is always a send. The strip's buttons are graph actions on `tail` (`insertEffectAt` — the same as "＋ Effect" in the graph, `connect`, `setEdgePreFader`, `routeOutput` — why it takes several undo steps: its doc comment in `graph/graph.ts`). Send rows use `Index`, not `For` (why: the comment in `StripRouting.tsx`).

A new strip column is a field in `StripModel` and a component in `StripRouting`/`ChannelStrip`.
