# `daw-summary`

**Project summary** (`Summarizer`: `ProjectSummary`): "describe the project briefly" — sections, tracks with their channel, effect path and where they play, buses, shared effects, automation; pure function of the project, mainly for AI (see "Project summary").

Depends on: contracts.

Map of the whole code: [ARCHITECTURE.md](../../ARCHITECTURE.md).

## Project summary: orienting without reading the whole project (`project.summary`)

Why: `project.get` of a real song carries every note, automation point and parameter; an AI that reads it fills its context before it starts working. `project.summary` answers "what is here and where" in a few hundred lines, and the MCP instructions tell the AI to start with it.

* **Contract** (`daw-contracts/src/summary.rs`): `ProjectSummary::summarize(project) → Summary`; the fields are documented on the type. RPC passes `ProjectView::snapshot` (as for `analysis.*`); it never renders, changes or fails. Levels are not part of it: `analysis.levels`.
* **Implementation** (`src/lib.rs`): pure functions over the project. What counts as an insert is the graph's own path rule `Graph::insert_path` (the mixer view uses the same), so the summary and the mixer agree.
* **Tests:** `src/tests.rs` (a synthetic project with inserts, a send, a sidechain and a shared limiter); wire — `daw-rpc/tests/all/project.rs`.
* **A new field:** the field in the contract type → a line in `Summarizer` → a test. The frontend does not call it (the interface has the whole project).
