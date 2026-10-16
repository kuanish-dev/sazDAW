//! Sound: audio stream buffers and processors, input and output devices, input capture.

use crate::BlockTiming;

/// Format of an audio stream: output, input and captured sound.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StreamConfig {
    pub sample_rate: u32,
    pub channels: u16,
}

impl StreamConfig {
    /// Settings before the first `prepare`: DSP nodes are created with them and run with them in tests.
    pub const DEFAULT: StreamConfig = StreamConfig { sample_rate: 48_000, channels: 2 };
}

/// Mutable interleaved buffer of one block.
pub struct AudioBlock<'a> {
    samples: &'a mut [f32],
    channels: usize,
}

impl<'a> AudioBlock<'a> {
    pub fn new(samples: &'a mut [f32], channels: u16) -> Self {
        let channels = usize::from(channels.max(1));
        Self { samples, channels }
    }

    pub fn channels(&self) -> usize {
        self.channels
    }

    pub fn frames(&self) -> usize {
        self.samples.len() / self.channels
    }

    /// Adds the same value to every channel of a frame.
    pub fn add_to_frame(&mut self, frame: usize, value: f32) {
        let start = frame * self.channels;
        for sample in &mut self.samples[start..start + self.channels] {
            *sample += value;
        }
    }

    pub fn as_mut_slice(&mut self) -> &mut [f32] {
        self.samples
    }

    pub fn as_slice(&self) -> &[f32] {
        self.samples
    }
}

/// A sound source or effect. Runs on the realtime thread.
/// Contract: **adds** its sound to the buffer without overwriting what is already there.
pub trait AudioProcessor: Send {
    fn prepare(&mut self, _config: StreamConfig) {}
    fn process(&mut self, block: &mut AudioBlock, timing: &BlockTiming);
}

/// Fills the buffer requested by the output device.
pub trait AudioRenderer: Send {
    fn prepare(&mut self, config: StreamConfig);
    fn render(&mut self, interleaved: &mut [f32]);
}

/// An audio input or output device, as named by the system.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AudioDeviceInfo {
    pub name: String,
    pub is_default: bool,
    /// The device's default channel count (0 if the system did not say). The UI uses it to offer track input choices.
    pub channels: u16,
}

/// The user's choice; `None` means the default device or buffer size (the system decides).
#[derive(Debug, Clone, PartialEq, Default, serde::Serialize, serde::Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct AudioSettings {
    pub output: Option<String>,
    pub input: Option<String>,
    /// Buffer size in frames: smaller means lower latency but a higher risk of clicks.
    pub buffer_frames: Option<u32>,
    /// Recording latency in ms: how far recorded sound lags behind what the program played (input and output buffers,
    /// the audio interface). `None` estimates it from the buffer size. The recorder shifts the recording earlier by
    /// this amount.
    pub record_latency_ms: Option<f32>,
}

/// What is currently happening with audio.
#[derive(Debug, Clone, PartialEq, Eq, Default, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AudioStatus {
    /// The output stream is open and playing.
    pub running: bool,
    pub output_name: Option<String>,
    pub sample_rate: u32,
    pub channels: u16,
    /// Allowed buffer size of the selected output device (min, max), if the device reports it.
    pub buffer_range: Option<(u32, u32)>,
    /// Why the stream is not playing or what last went wrong with it (no device, unsupported format, overload…).
    pub error: Option<crate::ContractError>,
}

/// Input peak level, 0…1 per channel.
#[derive(Debug, Clone, Copy, PartialEq, Default, serde::Serialize)]
pub struct InputLevel {
    pub left: f32,
    pub right: f32,
}

/// Audio devices. Job: manage the user's audio device setup: list the input and output devices, remember the user's
/// choice, reopen the output stream for it and show the input level (to check a microphone or interface).
pub trait AudioDevices: Send + Sync {
    fn outputs(&self) -> Vec<AudioDeviceInfo>;
    fn inputs(&self) -> Vec<AudioDeviceInfo>;
    /// The user's saved choice.
    fn settings(&self) -> AudioSettings;
    fn status(&self) -> AudioStatus;
    /// Remembers the choice and reopens the output stream. If the device fails to open, an error is returned and
    /// sound goes back to the previous device (or else the system one); the choice is not saved.
    fn apply(&self, settings: AudioSettings) -> Result<AudioStatus, crate::ContractError>;
    /// Turns the input for the level meter on or off. The input is open only during that time, so the system does
    /// not ask for microphone access until the user checks the input.
    fn watch_input(&self, on: bool) -> Result<(), crate::ContractError>;
    fn input_level(&self) -> InputLevel;
}

/// Where the device choice is kept between launches (next to the rest of the application state).
pub trait AudioSettingsStore: Send + Sync {
    /// The stored choice; a missing or corrupted file gives the default choice.
    fn load(&self) -> AudioSettings;
    fn save(&self, settings: &AudioSettings) -> Result<(), crate::ContractError>;
}

/// Input capture. Job: hand over what came from the input device without losing frames between requests.
/// The implementation keeps a preallocated ring buffer: nothing is allocated or locked on the audio thread.
/// The same input stream feeds the level meter (`AudioDevices::watch_input`).
pub trait AudioCapture: Send + Sync {
    /// Opens the input of the selected (or else the system) device and starts collecting frames.
    fn start(&self) -> Result<StreamConfig, crate::ContractError>;
    /// Appends the collected frames (interleaved channels) to `out` and returns their count in frames.
    fn drain(&self, out: &mut Vec<f32>) -> usize;
    /// Closes the input (unless the level meter still holds it).
    fn stop(&self);
    /// Frames lost to queue overflow since `start` (0 means the recording is intact).
    fn dropped_frames(&self) -> u64;
    /// The read side of the input for monitoring (`LiveInput`): handed out once, to the engine; `None` afterwards.
    fn take_live_input(&self) -> Option<Box<dyn LiveInput>>;
}

/// Input for realtime monitoring: the read side owned by the audio output thread (the engine).
/// Sound arrives as stereo frames (channels 1–2; equal for a mono input) at the input sample rate. The audio host
/// opens and closes the input on `set_wanted` requests; without one the input is not kept open. No method allocates
/// or locks.
pub trait LiveInput: Send {
    /// The engine needs the input (a track is armed or monitoring). The host opens the input soon after `true` and
    /// closes it after `false`, unless recording or the level meter still needs it.
    fn set_wanted(&mut self, wanted: bool);
    /// Input sample rate; `None` while the input is closed.
    fn sample_rate(&self) -> Option<u32>;
    /// How many stereo frames are waiting to be read.
    fn available(&self) -> usize;
    /// Reads up to `out.len() / 2` stereo frames into `out` (left and right interleaved); returns how many were read.
    fn read(&mut self, out: &mut [f32]) -> usize;
    /// Drops up to `frames` oldest frames, so the engine keeps monitoring latency low.
    fn skip(&mut self, frames: usize);
}
