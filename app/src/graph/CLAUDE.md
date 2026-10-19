# `src/graph/` — the Graph page

Map of the whole code: [ARCHITECTURE.md](../../../ARCHITECTURE.md). Cross-layer features it takes part in: [processing-graph](../../../docs/features/processing-graph.md), [sends](../../../docs/features/sends.md).

`GraphView.tsx` assembles the page (camera, drafts, selection, whether the signal path strip is open); its header names the parts: `GraphCanvas`, `pointer.ts` (`createGraphPointer`), `graphSelection.ts`, `graphLayout.ts`, `NodeCard`/`NodeStrip`/`Inspector`, `SignalPath`.

**Signal path view** ("Signal path" in the toolbar, `SignalPath.tsx`): a strip under the canvas with the way a node's sound goes, from where it comes from to the master (`graph/signalPathRules.ts`, on the mixer's main-output convention `mixerStrips.ts: mainOutput`; sends, sidechain keys and a merging bus's other inputs are left out). Every effect is a card with its live graph (`EffectGraph`), all hearing their nodes at once through `effects/nodeSounds.ts`, up to `MAX_WATCHED_NODES` (beyond that a note says only the first ones are live); the effect graphs' handles (EQ, filter…) work right in the strip. How the path follows the selection is in the header of `SignalPath.tsx`.
