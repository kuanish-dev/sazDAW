# `daw-demo`

**`DemoBeat`** (`DemoProject::fill_demo`): "build the starter beat" through the track, clip (created with notes), graph and automation interfaces in one edit group (`HistoryControl::begin_group`; on failure — `cancel_group`), then `mark_baseline`; on the frontend — "New project with demo" (`newProject(true)`: create → `project.fillDemo` → save).

Depends on: contracts.

Map of the whole code: [ARCHITECTURE.md](../../ARCHITECTURE.md).
