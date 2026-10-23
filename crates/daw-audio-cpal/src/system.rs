//! Real audio through cpal. The only place where cpal types live.

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{BufferSize, Device, ErrorKind, SampleFormat, SupportedBufferSize};
use daw_contracts::codes::audio;
use daw_contracts::sync::lock;
use daw_contracts::{AudioDeviceInfo, ContractError, ErrorCode, L10n, StreamConfig};

use crate::backend::{Backend, OpenInput, OpenOutput, OutputInfo, SharedRenderer, render_block};
use crate::capture::{InputTap, input_queue};
use crate::choose::{buffer_request, index_of_name};
use crate::faults::{Fault, FaultSink};

pub(crate) struct SystemBackend;

/// Name shown for a device the system did not name.
const UNNAMED: L10n = L10n { en: "Unnamed device", ru: "Устройство без имени" };

fn name_of(device: &Device) -> Option<String> {
    device.description().ok().map(|d| d.name().to_string())
}

fn stream_error(error: impl std::fmt::Display) -> ContractError {
    audio::OUTPUT_FAILED.with("details", error)
}

/// The input code carries the microphone permission hint: on macOS a refused input is usually exactly that.
fn input_error(error: impl std::fmt::Display) -> ContractError {
    audio::INPUT_FAILED.with("details", error)
}

/// Device list with the channel count of the default format (`channels_of`; 0 when the system does not say).
fn listing(devices: Vec<Device>, default: Option<Device>, channels_of: impl Fn(&Device) -> u16) -> Vec<AudioDeviceInfo> {
    let default = default.as_ref().and_then(name_of);
    devices
        .iter()
        .filter_map(|device| Some((name_of(device)?, channels_of(device))))
        .map(|(name, channels)| AudioDeviceInfo { is_default: default.as_deref() == Some(name.as_str()), name, channels })
        .collect()
}

/// Device by name; `None` means the system default. `not_found` is the code for a named device that is gone,
/// `missing` the error when there is no default device.
fn pick(
    devices: Vec<Device>,
    default: Option<Device>,
    wanted: Option<&str>,
    not_found: ErrorCode,
    missing: ContractError,
) -> Result<Device, ContractError> {
    let Some(wanted) = wanted else { return default.ok_or(missing) };
    let names: Vec<_> = devices.iter().map(name_of).collect();
    let index = index_of_name(&names, wanted).ok_or_else(|| not_found.with("name", wanted))?;
    devices.into_iter().nth(index).ok_or_else(|| not_found.with("name", wanted))
}

fn output_device(name: Option<&str>) -> Result<Device, ContractError> {
    let host = cpal::default_host();
    let devices = host.output_devices().map_err(stream_error)?.collect();
    pick(devices, host.default_output_device(), name, audio::OUTPUT_DEVICE_NOT_FOUND, audio::NO_OUTPUT_DEVICE.err())
}

fn input_device(name: Option<&str>) -> Result<Device, ContractError> {
    let host = cpal::default_host();
    let devices = host.input_devices().map_err(input_error)?.collect();
    pick(devices, host.default_input_device(), name, audio::INPUT_DEVICE_NOT_FOUND, audio::NO_INPUT_DEVICE.err())
}

fn f32_only(format: SampleFormat) -> Result<(), ContractError> {
    if format == SampleFormat::F32 { Ok(()) } else { Err(audio::UNSUPPORTED_SAMPLE_FORMAT.with("format", format)) }
}

/// Stream error handler. cpal may call it from the audio thread (on macOS: overloads and render failures), so it only
/// bumps atomic counters (`StreamFaults`); the host thread does the logging and status. A route change is not a fault.
fn fault_handler(faults: FaultSink) -> impl FnMut(cpal::Error) + Send + 'static {
    move |error| {
        let fault = match error.kind() {
            ErrorKind::DeviceChanged => return,
            ErrorKind::Xrun => Fault::Xrun,
            ErrorKind::DeviceNotAvailable => Fault::DeviceGone,
            ErrorKind::StreamInvalidated => Fault::StreamInvalid,
            _ => Fault::Other,
        };
        faults.record(fault);
    }
}

fn buffer_range(size: &SupportedBufferSize) -> Option<(u32, u32)> {
    match size {
        SupportedBufferSize::Range { min, max } => Some((*min, *max)),
        SupportedBufferSize::Unknown => None,
    }
}

impl Backend for SystemBackend {
    fn outputs(&self) -> Vec<AudioDeviceInfo> {
        let host = cpal::default_host();
        let devices = host.output_devices().map(Iterator::collect).unwrap_or_default();
        listing(devices, host.default_output_device(), |d| d.default_output_config().map_or(0, |c| c.channels()))
    }

    fn inputs(&self) -> Vec<AudioDeviceInfo> {
        let host = cpal::default_host();
        let devices = host.input_devices().map(Iterator::collect).unwrap_or_default();
        listing(devices, host.default_input_device(), |d| d.default_input_config().map_or(0, |c| c.channels()))
    }

    fn probe_output(&self, device: Option<&str>) -> Result<OutputInfo, ContractError> {
        let device = output_device(device)?;
        let supported = device.default_output_config().map_err(stream_error)?;
        Ok(OutputInfo {
            name: name_of(&device).unwrap_or_else(|| UNNAMED.get().into()),
            buffer_range: buffer_range(supported.buffer_size()),
        })
    }

    fn open_output(
        &self,
        device: Option<&str>,
        buffer_frames: Option<u32>,
        renderer: SharedRenderer,
        faults: FaultSink,
    ) -> Result<OpenOutput, ContractError> {
        let device = output_device(device)?;
        let supported = device.default_output_config().map_err(stream_error)?;
        f32_only(supported.sample_format())?;
        let range = buffer_range(supported.buffer_size());

        let mut config = supported.config();
        if let Some(frames) = buffer_request(buffer_frames, range)? {
            config.buffer_size = BufferSize::Fixed(frames);
        }
        let stream_config = StreamConfig { sample_rate: config.sample_rate, channels: config.channels };
        lock(&renderer).prepare(stream_config);

        let stream = device
            .build_output_stream(config, move |data: &mut [f32], _| render_block(&renderer, data), fault_handler(faults), None)
            .map_err(stream_error)?;
        stream.play().map_err(stream_error)?;

        let name = name_of(&device).unwrap_or_else(|| UNNAMED.get().into());
        Ok(OpenOutput { stream: Box::new(stream), config: stream_config, info: OutputInfo { name, buffer_range: range } })
    }

    fn open_input(&self, device: Option<&str>, tap: InputTap, faults: FaultSink) -> Result<OpenInput, ContractError> {
        let device = input_device(device)?;
        let supported = device.default_input_config().map_err(input_error)?;
        f32_only(supported.sample_format())?;
        let config = supported.config();
        let (mut feed, source) = input_queue(tap, config.sample_rate, config.channels);
        let stream =
            device.build_input_stream(config, move |data: &[f32], _| feed.feed(data), fault_handler(faults), None).map_err(input_error)?;
        stream.play().map_err(input_error)?;
        Ok(OpenInput { stream: Box::new(stream), source })
    }
}
