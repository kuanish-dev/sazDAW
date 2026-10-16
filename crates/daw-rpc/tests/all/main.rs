//! All integration tests of the crate are one test program: each separate one rebuilt and relinked the whole
//! application stack, and building the tests took minutes. Only tests with global process state are kept apart.

#[path = "../common/mod.rs"]
mod common;

mod analysis;
mod arrangement;
mod audio;
mod automation;
mod chords;
mod controllers;
mod drums;
mod editing;
mod graph;
mod live;
mod methods;
mod note_effects;
mod note_transforms;
mod project;
mod settings;
mod stage;
mod tempo;
mod warp;
