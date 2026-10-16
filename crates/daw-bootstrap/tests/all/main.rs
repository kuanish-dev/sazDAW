//! All integration tests of the crate are one test program: each separate one rebuilds and relinks the whole app stack,
//! and building the tests took minutes. Only tests with process-global state are kept separate (panic_log.rs, language.rs).

#[path = "../common/mod.rs"]
mod common;

mod architecture;
mod chord_part;
mod drum_part;
mod plugin_state;
mod plugins;
mod presets_templates;
mod render_analysis;
mod render_bounce;
mod render_comp;
mod render_drums;
mod render_export;
mod render_files;
mod render_meters;
mod render_note_effects;
mod render_playback;
mod render_recording;
mod render_samples;
mod render_tempo;
mod render_warp;
mod session;
