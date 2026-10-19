//! Engine telemetry: what the audio thread reports outside through atomics — the stream format, load, preview position,
//! level meters, the master window for the visualizer, the captured nodes' windows for the effect pictures, the master's
//! every frame for the live meter, the total compensation latency, loss counters, the frame clock of the capture leases.
//! The audio thread only writes, the control side only reads (and renews leases).

use std::sync::Arc;
use std::sync::atomic::{AtomicU32, AtomicU64, Ordering};

use daw_contracts::{EngineLatency, EngineLosses, StreamConfig};

use crate::meters::Meters;
use crate::scope::{FrameClock, MasterScope, MeterTap, NodeTap};

/// What the audio thread lost so as not to wait or allocate (`EngineStats::losses`).
#[derive(Clone, Copy)]
pub enum Loss {
    LiveNote,
    RecordedEvent,
    LeakedGarbage,
    LatencyClamped,
    MonitorUnderrun,
}

pub struct Telemetry {
    config: AtomicU64,
    cpu_load: AtomicU32,
    /// The preview position: frame + 1 (0 means not playing).
    preview_position: AtomicU64,
    pub meters: Meters,
    /// The engine's frame count, shared with the scope and the node tap for their leases.
    pub clock: Arc<FrameClock>,
    pub scope: MasterScope,
    pub node_tap: NodeTap,
    pub meter_tap: MeterTap,
    /// The total compensation latency (PDC) in stream frames.
    latency: AtomicU32,
    /// Loss counters in `Loss` order.
    losses: [AtomicU64; 5],
}

fn pack(config: StreamConfig) -> u64 {
    u64::from(config.sample_rate) << 16 | u64::from(config.channels)
}

impl Telemetry {
    pub fn new() -> Self {
        let clock = Arc::new(FrameClock::new());
        Self {
            config: AtomicU64::new(pack(StreamConfig::DEFAULT)),
            cpu_load: AtomicU32::new(0),
            preview_position: AtomicU64::new(0),
            meters: Meters::new(),
            scope: MasterScope::new(Arc::clone(&clock)),
            node_tap: NodeTap::new(Arc::clone(&clock)),
            clock,
            meter_tap: MeterTap::new(),
            latency: AtomicU32::new(0),
            losses: Default::default(),
        }
    }

    pub fn stream_config(&self) -> StreamConfig {
        let bits = self.config.load(Ordering::Acquire);
        StreamConfig { sample_rate: (bits >> 16) as u32, channels: (bits & 0xffff) as u16 }
    }

    pub fn set_stream_config(&self, config: StreamConfig) {
        self.config.store(pack(config), Ordering::Release);
    }

    pub fn cpu_load(&self) -> f32 {
        f32::from_bits(self.cpu_load.load(Ordering::Relaxed))
    }

    pub fn set_cpu_load(&self, load: f32) {
        self.cpu_load.store(load.to_bits(), Ordering::Relaxed);
    }

    pub fn preview_position(&self) -> Option<usize> {
        match self.preview_position.load(Ordering::Relaxed) {
            0 => None,
            value => Some((value - 1) as usize),
        }
    }

    pub fn set_preview_position(&self, position: Option<usize>) {
        let value = position.map_or(0, |p| p as u64 + 1);
        self.preview_position.store(value, Ordering::Relaxed);
    }

    pub fn count(&self, loss: Loss, count: u64) {
        if count > 0 {
            self.losses[loss as usize].fetch_add(count, Ordering::Relaxed);
        }
    }

    pub fn losses(&self) -> EngineLosses {
        let get = |loss: Loss| self.losses[loss as usize].load(Ordering::Relaxed);
        EngineLosses {
            live_notes: get(Loss::LiveNote),
            recorded_events: get(Loss::RecordedEvent),
            leaked_garbage: get(Loss::LeakedGarbage),
            latency_clamped: get(Loss::LatencyClamped),
            monitor_underruns: get(Loss::MonitorUnderrun),
        }
    }

    pub fn set_latency(&self, frames: u32) {
        self.latency.store(frames, Ordering::Relaxed);
    }

    pub fn latency(&self) -> EngineLatency {
        EngineLatency { frames: self.latency.load(Ordering::Relaxed), sample_rate: self.stream_config().sample_rate }
    }
}
