//! `Vst3Effect`: a VST3 plugin as an engine effect (`daw_contracts::Effect`).
//!
//! Plugin parameters are named `p<id>` and come in real units. `set_param` only converts the value to a
//! normalized one using a prebuilt table (`ParamScales`) and writes it to a fixed-capacity queue;
//! it reaches the plugin at the start of the next `process` (`IParameterChanges`). The plugin gets the transport time
//! (`set_timing`) in the `ProcessContext` of each call (`PluginClock`).
//! Failures on the audio thread (plugin refusal, queue overflow) are counted by `BlockFaults` and logged off the audio thread.

use std::sync::Arc;

use daw_contracts::{AppLog, BlockTiming, Effect, Params, StreamConfig};

use crate::context::PluginClock;
use crate::faults::BlockFaults;
use crate::instance::PluginInstance;
use crate::pending::PendingChanges;
use crate::runner::ProcessRunner;
use crate::scale::ParamScales;

pub struct Vst3Effect {
    instance: PluginInstance,
    runner: ProcessRunner,
    pending: PendingChanges,
    /// Converts the project's real values to the plugin's normalized values (built here, on the control side).
    scales: ParamScales,
    faults: BlockFaults,
    clock: PluginClock,
    /// `true` if `prepare` succeeded; otherwise `process` passes the sound through unchanged.
    ready: bool,
    latency: u32,
}

// SAFETY: the COM pointers inside the effect are not thread-safe by themselves, but the effect is used by exactly one
// thread at a time (it is handed to the audio thread whole, like any `Box<dyn Effect>`), and the plugin gets no
// references to the host that it could use from other threads: the host objects belong to this same effect.
unsafe impl Send for Vst3Effect {}

impl Vst3Effect {
    /// An effect with initial parameter values from the project (sent to the plugin in the first block).
    pub fn new(mut instance: PluginInstance, name: String, log: Arc<dyn AppLog>, params: &Params) -> Result<Self, String> {
        instance.configure_effect()?;
        let runner = ProcessRunner::new(instance.in_buses(), instance.out_buses());
        let scales = instance.parameters().scales;
        let mut effect = Self {
            instance,
            runner,
            pending: PendingChanges::default(),
            scales,
            faults: BlockFaults::new(name, log),
            clock: PluginClock::default(),
            ready: false,
            latency: 0,
        };
        for (name, value) in params {
            effect.set_param(name, *value);
        }
        Ok(effect)
    }
}

impl Effect for Vst3Effect {
    fn prepare(&mut self, config: StreamConfig) {
        self.ready = self.instance.guarded(|instance| self.faults.start(instance, config.sample_rate));
        self.clock.start(f64::from(config.sample_rate));
        if self.ready {
            self.latency = self.instance.latency_samples();
        }
    }

    fn set_param(&mut self, name: &str, value: f32) {
        let queued = self.pending.set_named(name, value, &self.scales);
        self.faults.param(queued);
    }

    fn reset(&mut self) {
        self.instance.reset();
    }

    fn process(&mut self, interleaved: &mut [f32], _sidechain: Option<&[f32]>) {
        if self.ready {
            let ok =
                self.instance.guarded(|instance| self.runner.run(instance.processor(), &mut self.pending, &mut self.clock, interleaved));
            self.faults.block(ok);
        }
    }

    fn latency_samples(&self) -> u32 {
        self.latency
    }

    fn set_timing(&mut self, timing: &BlockTiming) {
        self.clock.set_timing(timing);
    }
}
