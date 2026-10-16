# `daw-summary`

**Project summary** (`Summarizer`: `ProjectSummary`): "describe the project briefly" — sections, tracks with their channel, effect path and where they play, buses, shared effects, automation; pure function of the project, mainly for AI (see "Project summary").

Depends on: contracts.

Map of the whole code: [ARCHITECTURE.md](../../ARCHITECTURE.md).

## Project summary: orienting without reading the whole project (`project.summary`)

Why: `project.get` of a real song carries every note, automation point and parameter; an AI that reads it to answer "what plays in the chorus" fills its context before it starts working. `project.summary` answers "what is here and where" in a few hundred lines, and the MCP instructions tell the AI to start with it.

* **Contract** (`daw-contracts/src/summary.rs`): `ProjectSummary::summarize(project) → Summary` — "describe the project briefly". The project is passed in (RPC takes `ProjectView::snapshot`, as for `analysis.*`); nothing is rendered or changed, it never fails. Levels are not part of it (they need a render): `analysis.levels`.
* **What is in it:** name, starting tempo and meter (+ whether they change), the song end (the end of the last clip) and its bar count, the loop; **sections** from the markers (to the next marker; the last one to the song end) with the tracks that have clips in them; every **track** — instrument kind or audio, frozen, folder, MIDI effects, its source node and channel (volume, pan, mute, solo — inline), inserted effects and output, number of clips and the merged spans where they play, notes in the clips' content and their pitch range, sound names for audio clips; **buses** with their channel, the tracks whose sound reaches them (sidechain keys do not count), their inserts and output; **shared effects** (not an insert of any track or bus path: several inputs, only a send, or none) with the effects after them; master volume; **automation** lanes with their target and spans. Ticks are absolute, spans `[start, end)`, bars from 1 (as in `analysis.levels`).
* **Implementation** (`daw-summary`, `Summarizer`): pure functions over the project. What counts as an insert is the graph's own path rule `Graph::insert_path` (the mixer view uses the same), so the summary and the mixer agree. Tests on a synthetic project with inserts, a send, a sidechain and a shared limiter — `daw-summary/src/tests.rs`; wire — `daw-rpc/tests/all/project.rs`.
* **Similar** (a new field): the field in the contract type → a line in `Summarizer` → a test. The frontend does not call it (the interface has the whole project).
