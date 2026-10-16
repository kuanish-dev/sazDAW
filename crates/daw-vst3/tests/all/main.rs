//! All integration tests of the crate are one test program: each separate one rebuilds and relinks the whole
//! app stack, and building the tests took minutes. Only tests with process-global state would be kept apart.

#[path = "../support/mod.rs"]
mod support;

mod distortion;
mod editors;
mod isolated_scan;
mod state;
mod surge;
mod synth;
