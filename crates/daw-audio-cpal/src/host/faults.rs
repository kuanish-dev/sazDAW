//! Moves stream faults to the log and status. The cpal error callback only counts them in atomics (`crate::faults`);
//! the host thread takes the counters every ~250 ms: a fault goes to the log (and for the output to the status, so the
//! interface shows why it is silent), overloads at most once per `XRUN_LOG_INTERVAL`, otherwise the log would flood.

use std::sync::Arc;
use std::time::{Duration, Instant};

use daw_contracts::LogLevel;

use super::Host;
use crate::faults::{Fault, FaultSink, StreamFaults};

/// Overloads (clicks under load) are logged at most once per this interval.
const XRUN_LOG_INTERVAL: Duration = Duration::from_secs(5);

/// Fault counters of one direction and when overloads were last logged.
pub(super) struct FaultWatch {
    what: &'static str,
    faults: FaultSink,
    last_xrun_log: Option<Instant>,
}

impl FaultWatch {
    pub fn new(what: &'static str) -> Self {
        Self { what, faults: Arc::new(StreamFaults::default()), last_xrun_log: None }
    }

    /// Where a new stream reports its faults.
    pub fn sink(&self) -> FaultSink {
        Arc::clone(&self.faults)
    }

    /// Overloads due for logging (`XRUN_LOG_INTERVAL` passed since the last entry); otherwise they keep accumulating.
    fn due_xruns(&mut self, now: Instant) -> u64 {
        if self.last_xrun_log.is_some_and(|last| now.duration_since(last) < XRUN_LOG_INTERVAL) {
            return 0;
        }
        let xruns = self.faults.take_xruns();
        if xruns > 0 {
            self.last_xrun_log = Some(now);
        }
        xruns
    }
}

impl Host {
    pub(crate) fn report_faults(&mut self) {
        let now = Instant::now();
        for output in [true, false] {
            let watch = if output { &mut self.output_faults } else { &mut self.input_faults };
            let what = watch.what;
            let xruns = watch.due_xruns(now);
            let errors = watch.faults.take_errors();
            if xruns > 0 {
                self.fault(output, what, Fault::Xrun, xruns);
            }
            if let Some((fault, count)) = errors {
                self.fault(output, what, fault, count);
            }
        }
    }

    fn fault(&self, output: bool, what: &str, fault: Fault, count: u64) {
        let times = if count > 1 { format!(" (×{count})") } else { String::new() };
        let error = fault.error();
        self.note(LogLevel::Error, format!("{what} stream fault: {error}{times}"));
        if output {
            self.shared.update_status(|status| {
                status.error = Some(error);
                if fault.is_fatal() {
                    status.running = false;
                }
            });
        }
    }
}
