//! Live MIDI input: connecting to a device and passing key presses and controllers (pitch bend, CC, channel
//! pressure) to the chosen track. A CC is first offered to the parameter mappings (`MidiMappingControl`): a CC they take
//! does not reach the instrument.

use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};

use daw_contracts::codes::midi;
use daw_contracts::{
    AppLog, ContractError, ControllerKind, LiveNoteSink, LogEntry, MAX_CC_NUMBER, MidiInputControl, MidiMappingControl, TrackId,
    velocity_level,
};
use midir::{Ignore, MidiInput, MidiInputConnection};

use crate::values::{bend_from_14bit, from_7bit};

#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) enum InputMessage {
    On {
        pitch: u8,
        velocity: f32,
    },
    Off {
        pitch: u8,
    },
    /// A control change as is (channel 0..=15, number and value 0..=127): the parameter mappings see it first.
    Cc {
        channel: u8,
        number: u8,
        value: u8,
    },
    /// Pitch bend or channel pressure, already in controller values.
    Controller {
        kind: ControllerKind,
        value: f32,
    },
}

/// Parses a raw MIDI message; anything else (polyphonic aftertouch, program change, system messages) is `None`.
pub(crate) fn parse_message(bytes: &[u8]) -> Option<InputMessage> {
    let (&status, data) = bytes.split_first()?;
    if data.iter().any(|byte| *byte > 127) {
        return None;
    }
    let channel = status & 0x0F;
    match (status & 0xF0, data) {
        (0x90, &[pitch, velocity, ..]) if velocity > 0 => Some(InputMessage::On { pitch, velocity: velocity_level(velocity) }),
        (0x90 | 0x80, &[pitch, _, ..]) => Some(InputMessage::Off { pitch }),
        (0xB0, &[number, value, ..]) => Some(InputMessage::Cc { channel, number, value }),
        (0xD0, &[pressure, ..]) => Some(InputMessage::Controller { kind: ControllerKind::Aftertouch, value: from_7bit(pressure) }),
        (0xE0, &[lsb, msb, ..]) => {
            let raw = u16::from(msb) << 7 | u16::from(lsb);
            Some(InputMessage::Controller { kind: ControllerKind::PitchBend, value: bend_from_14bit(raw) })
        }
        _ => None,
    }
}

/// Where a parsed message goes: notes and controllers to the instrument of track `track`, a CC to the mappings first.
fn route(message: InputMessage, track: TrackId, sink: &dyn LiveNoteSink, mappings: &dyn MidiMappingControl) {
    match message {
        InputMessage::On { pitch, velocity } => sink.note_on(track, pitch, velocity),
        InputMessage::Off { pitch } => sink.note_off(track, pitch),
        InputMessage::Cc { channel, number, value } => {
            if !mappings.control_change(channel, number, value) && number <= MAX_CC_NUMBER {
                sink.controller(track, ControllerKind::Cc { number }, from_7bit(value));
            }
        }
        InputMessage::Controller { kind, value } => sink.controller(track, kind, value),
    }
}

struct Connected {
    device: String,
    _connection: MidiInputConnection<()>,
}

pub struct MidiInputService {
    sink: Arc<dyn LiveNoteSink>,
    mappings: Arc<dyn MidiMappingControl>,
    log: Arc<dyn AppLog>,
    target: Arc<AtomicU32>,
    connection: Mutex<Option<Connected>>,
}

impl MidiInputService {
    pub fn new(sink: Arc<dyn LiveNoteSink>, mappings: Arc<dyn MidiMappingControl>, log: Arc<dyn AppLog>) -> Self {
        Self { sink, mappings, log, target: Arc::new(AtomicU32::new(0)), connection: Mutex::new(None) }
    }

    fn lock(&self) -> MutexGuard<'_, Option<Connected>> {
        self.connection.lock().unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    fn client() -> Result<MidiInput, ContractError> {
        let mut input = MidiInput::new("daw").map_err(|e| midi::UNAVAILABLE.with("details", e))?;
        input.ignore(Ignore::SysexAndTime);
        Ok(input)
    }
}

impl MidiInputControl for MidiInputService {
    fn devices(&self) -> Vec<String> {
        let input = match Self::client() {
            Ok(input) => input,
            Err(error) => {
                self.log.record(LogEntry::warn("midi", format!("MIDI device list is unavailable: {error}")));
                return Vec::new();
            }
        };
        input.ports().iter().filter_map(|port| input.port_name(port).ok()).collect()
    }

    fn connect(&self, device: &str, target: TrackId) -> Result<(), ContractError> {
        let input = Self::client()?;
        let port = input
            .ports()
            .into_iter()
            .find(|port| input.port_name(port).is_ok_and(|name| name == device))
            .ok_or_else(|| midi::DEVICE_NOT_FOUND.with("name", device))?;

        self.target.store(target, Ordering::Release);
        let sink = Arc::clone(&self.sink);
        let mappings = Arc::clone(&self.mappings);
        let current = Arc::clone(&self.target);
        let connection = input
            .connect(
                &port,
                "daw-input",
                move |_, bytes, ()| {
                    if let Some(message) = parse_message(bytes) {
                        route(message, current.load(Ordering::Acquire), sink.as_ref(), mappings.as_ref());
                    }
                },
                (),
            )
            .map_err(|e| midi::DEVICE_NOT_CONNECTED.with("name", device).with("details", e))?;
        *self.lock() = Some(Connected { device: device.to_string(), _connection: connection });
        Ok(())
    }

    fn set_target(&self, target: TrackId) {
        self.target.store(target, Ordering::Release);
    }

    fn disconnect(&self) {
        *self.lock() = None;
    }

    fn connected_device(&self) -> Option<String> {
        self.lock().as_ref().map(|c| c.device.clone())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_note_on_off_and_velocity_zero() {
        assert_eq!(parse_message(&[0x90, 60, 127]), Some(InputMessage::On { pitch: 60, velocity: 1.0 }));
        assert_eq!(parse_message(&[0x93, 60, 0]), Some(InputMessage::Off { pitch: 60 }));
        assert_eq!(parse_message(&[0x80, 72, 64]), Some(InputMessage::Off { pitch: 72 }));
    }

    #[test]
    fn parses_control_change_pitch_bend_and_channel_pressure() {
        assert_eq!(parse_message(&[0xB3, 7, 100]), Some(InputMessage::Cc { channel: 3, number: 7, value: 100 }));
        let bend = |bytes: &[u8]| match parse_message(bytes) {
            Some(InputMessage::Controller { kind: ControllerKind::PitchBend, value }) => value,
            other => panic!("{other:?}"),
        };
        assert_eq!(bend(&[0xE0, 0, 64]), 0.0, "center");
        assert_eq!(bend(&[0xE0, 127, 127]), 1.0);
        assert_eq!(bend(&[0xE0, 0, 0]), -1.0);
        assert_eq!(parse_message(&[0xD0, 127]), Some(InputMessage::Controller { kind: ControllerKind::Aftertouch, value: 1.0 }));
    }

    #[test]
    fn ignores_other_messages_and_malformed_data() {
        assert_eq!(parse_message(&[0xA0, 60, 100]), None, "polyphonic pressure");
        assert_eq!(parse_message(&[0xC0, 5]), None, "program change");
        assert_eq!(parse_message(&[0x90, 60]), None, "truncated");
        assert_eq!(parse_message(&[0xB0, 7]), None, "truncated CC");
        assert_eq!(parse_message(&[]), None);
        assert_eq!(parse_message(&[0x90, 200, 100]), None, "pitch out of range");
    }

    #[derive(Default)]
    struct Played(Mutex<Vec<String>>);

    impl LiveNoteSink for Played {
        fn note_on(&self, track: TrackId, pitch: u8, _: f32) {
            self.0.lock().unwrap().push(format!("{track}:on{pitch}"));
        }
        fn note_off(&self, track: TrackId, pitch: u8) {
            self.0.lock().unwrap().push(format!("{track}:off{pitch}"));
        }
        fn controller(&self, track: TrackId, kind: ControllerKind, value: f32) {
            self.0.lock().unwrap().push(format!("{track}:{kind:?}={value}"));
        }
    }

    /// A mapping that takes CC 7 on any channel.
    struct Volume7;

    impl MidiMappingControl for Volume7 {
        fn mappings(&self) -> Vec<daw_contracts::MidiMapping> {
            Vec::new()
        }
        fn learn(&self, _: daw_contracts::ParamTarget) -> Result<(), ContractError> {
            Ok(())
        }
        fn cancel_learn(&self) {}
        fn learning(&self) -> Option<daw_contracts::ParamTarget> {
            None
        }
        fn remove_mapping(&self, _: &daw_contracts::ParamTarget) -> Result<(), ContractError> {
            Ok(())
        }
        fn control_change(&self, _: u8, cc: u8, _: u8) -> bool {
            cc == 7
        }
    }

    #[test]
    fn mapped_cc_is_taken_and_the_rest_reaches_the_instrument_of_the_track() {
        let played = Played::default();
        for bytes in [[0xB0, 7, 100], [0xB0, 1, 127], [0xB0, 123, 0], [0xE0, 0, 64], [0x90, 60, 100]] {
            route(parse_message(&bytes).unwrap(), 4, &played, &Volume7);
        }
        let played = played.0.into_inner().unwrap();
        assert_eq!(played, ["4:Cc { number: 1 }=1", "4:PitchBend=0", "4:on60"], "the mapping took CC 7, 123 is a channel mode message");
    }

    struct Silent;

    #[derive(Default)]
    struct MemoryLog(Mutex<Vec<LogEntry>>);

    impl AppLog for MemoryLog {
        fn record(&self, entry: LogEntry) {
            self.0.lock().unwrap().push(entry);
        }
        fn recent(&self, limit: usize) -> Vec<LogEntry> {
            let entries = self.0.lock().unwrap();
            entries[entries.len().saturating_sub(limit)..].to_vec()
        }
        fn file(&self) -> Option<std::path::PathBuf> {
            None
        }
    }

    impl LiveNoteSink for Silent {
        fn note_on(&self, _: TrackId, _: u8, _: f32) {}
        fn note_off(&self, _: TrackId, _: u8) {}
        fn controller(&self, _: TrackId, _: ControllerKind, _: f32) {}
    }

    #[test]
    fn connecting_to_an_unknown_device_fails_cleanly() {
        let service = MidiInputService::new(Arc::new(Silent), Arc::new(Volume7), Arc::new(MemoryLog::default()));
        assert!(service.connect("no such device", 1).is_err_and(|e| e.is(midi::DEVICE_NOT_FOUND) || e.is(midi::UNAVAILABLE)));
        assert_eq!(service.connected_device(), None);
        service.disconnect();
    }

    #[test]
    fn listing_devices_never_fails_silently() {
        let log = Arc::new(MemoryLog::default());
        let service = MidiInputService::new(Arc::new(Silent), Arc::new(Volume7), log.clone());
        if service.devices().is_empty() && MidiInputService::client().is_err() {
            assert_eq!(log.recent(10).len(), 1);
            assert_eq!(log.recent(10)[0].source, "midi");
        } else {
            assert!(log.recent(10).is_empty());
        }
    }
}
