//! Timeline playback: for every segment decides what to play — the notes of note clips (`NoteSchedule`), their
//! controllers (`ControllerSchedule`) and the voices of audio clips (`AudioVoices`) — and catches start, stop and seek,
//! after which everything is cut off and the controllers catch up with the values at the new position.
//! There are no modes: the timeline always plays; muting tracks (mute/solo) is done by the mixer on the source node.

use daw_contracts::{BlockTiming, EngineSnapshot};

use crate::audio_voices::AudioVoices;
use crate::controllers::{ControllerEvent, ControllerSchedule};
use crate::link::GarbageBin;
use crate::mixer::GraphMixer;
use crate::notes::{Event, NoteSchedule};

pub struct Playback {
    notes: NoteSchedule,
    controllers: ControllerSchedule,
    voices: AudioVoices,
    expected_tick: f64,
    was_playing: bool,
}

/// The result of preparing a segment.
pub struct Plan {
    /// All notes must be cut off and the controllers reset: start, pause, stop or seek.
    /// The controller catch-up (if playing) is already among the segment events.
    pub release_all: bool,
    /// The transport has just started.
    pub started: bool,
}

impl Playback {
    pub fn new() -> Self {
        Self {
            notes: NoteSchedule::new(),
            controllers: ControllerSchedule::new(),
            voices: AudioVoices::new(),
            expected_tick: 0.0,
            was_playing: false,
        }
    }

    pub fn events(&self) -> &[Event] {
        self.notes.events()
    }

    pub fn controller_events(&self) -> &[ControllerEvent] {
        self.controllers.events()
    }

    pub fn note_started(&mut self, event: &Event) {
        self.notes.note_started(event);
    }

    pub fn note_released(&mut self, event: &Event) {
        self.notes.note_released(event);
    }

    /// Prepares the events and voices for the segment.
    pub fn plan(&mut self, snapshot: Option<&EngineSnapshot>, timing: &BlockTiming, sample_rate: u32, bin: &mut GarbageBin) -> Plan {
        self.notes.clear_events();
        self.controllers.clear_events();
        let started = timing.playing && !self.was_playing;
        let jumped = (timing.start_tick - self.expected_tick).abs() > 1.0;
        let release_all = if timing.playing { started || jumped } else { self.was_playing };
        if release_all {
            self.notes.forget_active();
            self.voices.drop_all(bin);
        }
        self.was_playing = timing.playing;
        if timing.playing {
            self.expected_tick = timing.end_tick();
            if let Some(snapshot) = snapshot {
                let from = timing.start_tick.ceil() as i64;
                let to = timing.end_tick().ceil() as i64;
                self.notes.collect(snapshot, from, to);
                self.controllers.collect(snapshot, from, to, release_all);
                self.voices.start(snapshot, timing, sample_rate, bin);
            }
        }
        Plan { release_all, started }
    }

    /// Adds the sound of the audio clip voices to the buffers of the `Audio` nodes.
    pub fn render_audio(&mut self, mixer: &mut GraphMixer, timing: &BlockTiming, frames: usize, sample_rate: u32, bin: &mut GarbageBin) {
        self.voices.render(mixer, timing, frames, sample_rate, bin);
    }
}
