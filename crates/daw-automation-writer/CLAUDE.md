# `daw-automation-writer`

**`LiveAutomationWriter`** (`AutomationWriter`): "turn param moves made while the song plays into automation points" (Touch/Latch) — a poll thread follows the play position, `stretch.rs` keeps what one continuous touch wrote, lanes go through `AutomationControl`, the engine is told which lanes not to play (`AutomationHold`); one play pass = one undo step (see "Automation: editing and live writing" ([docs/features/automation.md](../../docs/features/automation.md))).

Depends on: contracts.

Map of the whole code: [ARCHITECTURE.md](../../ARCHITECTURE.md). Cross-layer features it takes part in: [automation](../../docs/features/automation.md).
