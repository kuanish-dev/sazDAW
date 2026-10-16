//! Undo history. Codes `HS-KNN`.

crate::codes! {
    STEP_LABEL_EMPTY = "HS-101", "An undo step needs a name";
    CHECKPOINT_GONE = "HS-301", "Checkpoint {mark} is no longer in the undo history: it was undone past, a project was opened, or too many steps were made since";
    GROUP_OPEN = "HS-302", "A compound edit (a recording, a part being built) is in progress; try again when it ends";
}
