//! The real-time audio engine.
//!
//! For every block it advances the transport clock, picks up a fresh project snapshot, turns the tracks' note clips
//! into notes, plays instruments and audio clips into the source node buffers, runs the processing graph (effects,
//! buses) in the snapshot's order and mixes everything into the master. The callback has no locks and no allocations:
//! tables are allocated in advance, and ownership of DSP objects is passed through queues.
//!
//! Modules by role: `engine` is the audio thread (the conductor of a segment), `link` the connection to the control
//! side (queues, telemetry, the input table, the mono check), `meters` the level meters, `scope` the live windows of
//! sound (the master for the visualizer, the captured nodes for the effect pictures, every master frame for the live
//! loudness meter), `instruments` the track
//! instruments, `note_effects` the track MIDI effects (notes before the instruments), `playback` (`notes`,
//! `controllers`, `audio_voices`) what to play from the timeline, `automation` the automation lanes, `mixer` the
//! processing graph, `stage` the virtual stage (a channel's place in the shared room and the room's tail), `monitor`
//! input monitoring, `preview` the sample preview voice.

mod audio_voices;
mod automation;
mod controllers;
mod engine;
mod instruments;
mod link;
mod meters;
mod mixer;
mod monitor;
mod note_effects;
mod notes;
mod params;
mod playback;
mod preview;
mod scope;
mod stage;

use std::sync::Arc;

use daw_contracts::{AudioProcessor, AudioRenderer, TransportClock};

pub use link::EngineLink;

/// The length of one processing segment: automation and params are updated at this step (~10 ms).
/// The mixer node buffers are sized for it too.
pub(crate) const SEGMENT_FRAMES: usize = 512;

/// What the app gets: a renderer for the audio output and a link for the control side.
pub struct EngineParts {
    pub renderer: Box<dyn AudioRenderer>,
    pub link: Arc<EngineLink>,
}

/// Creates the engine. `post_processors` play on top of the finished sound (the metronome) and see the same transport position.
pub fn create_engine(clock: Box<dyn TransportClock>, post_processors: Vec<Box<dyn AudioProcessor>>) -> EngineParts {
    let end = EngineLink::create();
    let link = Arc::clone(&end.link);
    EngineParts { renderer: Box::new(engine::Engine::new(end, clock, post_processors)), link }
}

#[cfg(test)]
mod tests;
