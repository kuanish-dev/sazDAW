//! Keeping the kernel in step with the settings off the audio thread. Parameters reach an effect on the audio thread
//! (`Effect::set_param`), where designing a kernel (two long FFTs and thousands of biquad magnitudes) has no place, so
//! each linear-phase EQ has a designer thread of its own:
//! * the audio side asks for the settings it has (`Exchange::ask`: atomic stores and a request count);
//! * the designer polls the count, designs the kernel (`Designer`) and publishes it (`Exchange::publish`);
//! * the audio side copies a newly published kernel into its own buffer when it next processes (`Exchange::take`).
//!
//! The kernel crosses as a seqlock over atomics (no `unsafe`, no lock): the version is odd while the designer writes,
//! and a reader that saw it change underneath drops its copy and tries at the next block. The designer never waits for
//! the audio side and the audio side never waits at all. Dropping the handle stops the thread (`Worker`).

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering, fence};
use std::thread::{self, Thread};
use std::time::Duration;

use daw_contracts::{ContractError, codes};

use super::design::{Designer, MAX_KERNEL, kernel_len};
use crate::eq::VALUES;

/// How often an idle designer looks for a request: a knob turned now is heard within this plus a block.
const POLL: Duration = Duration::from_millis(20);

pub struct Exchange {
    /// The settings asked for (f32 bits) and how many times anything was asked.
    values: [AtomicU32; VALUES],
    sample_rate: AtomicU32,
    requests: AtomicU64,
    /// The published kernel: odd `version` while it is being written; `rate` it was designed for; `R(k)` as f32 bits.
    version: AtomicU64,
    rate: AtomicU32,
    response: Box<[AtomicU32]>,
    stop: AtomicBool,
}

impl Exchange {
    pub fn new() -> Arc<Self> {
        Arc::new(Self {
            values: std::array::from_fn(|_| AtomicU32::new(0)),
            sample_rate: AtomicU32::new(0),
            requests: AtomicU64::new(0),
            version: AtomicU64::new(0),
            rate: AtomicU32::new(0),
            response: (0..=MAX_KERNEL).map(|_| AtomicU32::new(0)).collect(),
            stop: AtomicBool::new(false),
        })
    }

    /// Audio side: asks for a kernel for `values` at `sample_rate`.
    pub fn ask(&self, values: &[f32; VALUES], sample_rate: u32) {
        for (slot, value) in self.values.iter().zip(values) {
            slot.store(value.to_bits(), Ordering::Relaxed);
        }
        self.sample_rate.store(sample_rate, Ordering::Relaxed);
        self.requests.fetch_add(1, Ordering::Release);
    }

    /// How many times a kernel was asked for: the designer starts from the count a synchronous design already covered.
    pub fn requests(&self) -> u64 {
        self.requests.load(Ordering::Acquire)
    }

    /// Audio side: when a kernel for `sample_rate` was published after version `seen` and is whole, copies it into
    /// `into` (`len + 1` values) and returns its version.
    pub fn take(&self, seen: u64, sample_rate: u32, into: &mut [f32]) -> Option<u64> {
        let version = self.version.load(Ordering::Acquire);
        if version == seen || version % 2 == 1 || self.rate.load(Ordering::Relaxed) != sample_rate {
            return None;
        }
        for (value, slot) in into.iter_mut().zip(self.response.iter()) {
            *value = f32::from_bits(slot.load(Ordering::Relaxed));
        }
        fence(Ordering::Acquire);
        (self.version.load(Ordering::Relaxed) == version).then_some(version)
    }

    /// Designer side: publishes a kernel designed for `sample_rate`.
    fn publish(&self, sample_rate: u32, response: &[f32]) {
        let version = self.version.load(Ordering::Relaxed);
        self.version.store(version + 1, Ordering::Relaxed);
        fence(Ordering::Release);
        self.rate.store(sample_rate, Ordering::Relaxed);
        for (slot, value) in self.response.iter().zip(response) {
            slot.store(value.to_bits(), Ordering::Relaxed);
        }
        self.version.store(version + 2, Ordering::Release);
    }

    /// Designer side: the settings asked for now.
    fn asked(&self) -> (u32, [f32; VALUES]) {
        (self.sample_rate.load(Ordering::Relaxed), self.values.each_ref().map(|slot| f32::from_bits(slot.load(Ordering::Relaxed))))
    }
}

/// The designer thread of one effect; dropping it stops the thread (without waiting for it: it exits at its next poll).
pub struct Worker {
    exchange: Arc<Exchange>,
    thread: Thread,
}

impl Worker {
    /// Starts the designer; requests up to `covered` are already designed.
    pub fn start(exchange: Arc<Exchange>, covered: u64) -> Result<Self, ContractError> {
        let shared = Arc::clone(&exchange);
        let handle = thread::Builder::new()
            .name("daw-linear-eq".into())
            .spawn(move || run(&shared, covered))
            .map_err(|error| codes::graph::EFFECT_WORKER_FAILED.with("kind", "linear_eq").with("details", error))?;
        Ok(Self { exchange, thread: handle.thread().clone() })
    }
}

impl Drop for Worker {
    fn drop(&mut self) {
        self.exchange.stop.store(true, Ordering::Release);
        self.thread.unpark();
    }
}

fn run(exchange: &Exchange, mut covered: u64) {
    let mut designer: Option<Designer> = None;
    while !exchange.stop.load(Ordering::Acquire) {
        let requests = exchange.requests();
        if requests == covered {
            thread::park_timeout(POLL);
            continue;
        }
        covered = requests;
        let (sample_rate, values) = exchange.asked();
        let len = kernel_len(sample_rate);
        if designer.as_ref().is_none_or(|d| d.len() != len) {
            designer = Some(Designer::new(len));
        }
        if let Some(designer) = designer.as_mut() {
            exchange.publish(sample_rate, designer.design(sample_rate as f32, &values));
        }
    }
}
