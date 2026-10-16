//! Faults of open cpal streams. The cpal error callback may run on the audio thread (macOS reports overloads and
//! render failures from there), so it only bumps atomic counters: no strings, locks or logging.
//! The host thread takes them regularly and writes them to the log and status (`host/faults.rs`).

use daw_contracts::ContractError;
use daw_contracts::codes::audio;

use std::sync::Arc;
use std::sync::atomic::{AtomicU8, AtomicU64, Ordering};

/// What happened to the stream.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Fault {
    /// Overload: the stream missed a block (an audible click).
    Xrun,
    /// The device is gone (unplugged).
    DeviceGone,
    /// The stream no longer works (the system closed it).
    StreamInvalid,
    Other,
}

impl Fault {
    /// After this the stream no longer plays.
    pub fn is_fatal(self) -> bool {
        matches!(self, Fault::DeviceGone | Fault::StreamInvalid)
    }

    pub fn error(self) -> ContractError {
        match self {
            Fault::Xrun => audio::STREAM_OVERLOAD,
            Fault::DeviceGone => audio::DEVICE_UNAVAILABLE,
            Fault::StreamInvalid => audio::STREAM_CLOSED_BY_SYSTEM,
            Fault::Other => audio::STREAM_FAILED,
        }
        .err()
    }

    /// Value for the atomic; 0 means no fault.
    fn code(self) -> u8 {
        self as u8 + 1
    }

    fn from_code(code: u8) -> Option<Fault> {
        match code {
            1 => Some(Fault::Xrun),
            2 => Some(Fault::DeviceGone),
            3 => Some(Fault::StreamInvalid),
            4 => Some(Fault::Other),
            _ => None,
        }
    }
}

/// Fault counters of one direction (output or input).
#[derive(Default)]
pub(crate) struct StreamFaults {
    xruns: AtomicU64,
    errors: AtomicU64,
    /// Code of the last fault other than overloads; 0 means none.
    last_error: AtomicU8,
}

/// Where the error callback reports stream faults.
pub(crate) type FaultSink = Arc<StreamFaults>;

impl StreamFaults {
    /// From the error callback (possibly the audio thread): atomics only.
    pub fn record(&self, fault: Fault) {
        if fault == Fault::Xrun {
            self.xruns.fetch_add(1, Ordering::Relaxed);
        } else {
            self.last_error.store(fault.code(), Ordering::Relaxed);
            self.errors.fetch_add(1, Ordering::Relaxed);
        }
    }

    /// Overloads since the last call.
    pub fn take_xruns(&self) -> u64 {
        self.xruns.swap(0, Ordering::Relaxed)
    }

    /// Faults other than overloads since the last call: the last one and how many in total.
    pub fn take_errors(&self) -> Option<(Fault, u64)> {
        let count = self.errors.swap(0, Ordering::Relaxed);
        let last = Fault::from_code(self.last_error.swap(0, Ordering::Relaxed))?;
        (count > 0).then_some((last, count))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn faults_are_counted_and_taken_once() {
        let faults = StreamFaults::default();
        assert_eq!(faults.take_errors(), None);
        faults.record(Fault::Xrun);
        faults.record(Fault::Xrun);
        faults.record(Fault::Other);
        faults.record(Fault::DeviceGone);
        assert_eq!(faults.take_xruns(), 2);
        assert_eq!(faults.take_errors(), Some((Fault::DeviceGone, 2)), "the last fault and the count");
        assert_eq!((faults.take_xruns(), faults.take_errors()), (0, None));
        assert!(Fault::DeviceGone.is_fatal() && !Fault::Xrun.is_fatal());
    }
}
