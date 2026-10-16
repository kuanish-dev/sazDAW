//! Plugin delay compensation (PDC): at every node all inputs (regular and sidechain) are aligned to the longest one, so
//! parallel paths meet in step, and all sound in the master lags the transport position by the same number of frames —
//! the total latency. Connections are delayed (not nodes): a connection bypassing a longer path gets a delay line from
//! the `DelayLines` store.
//!
//! Computed on the audio side on every snapshot apply (and after `prepare`): plugin latencies are read there too, and
//! the tables are preallocated. A bypassed effect counts as zero latency: it does not process or delay the sound, and
//! turning it on or off comes as a new snapshot anyway and rearranges the compensation. A channel whose time alignment
//! is set earlier (`Strip::delay_ms` below zero) counts its lead as latency: the rest of the mix is held back by it,
//! which plays the channel that much earlier (`utility.rs` only delays).

use daw_contracts::TrackId;

use super::delay::MAX_PDC_FRAMES;
use super::{GraphMixer, Role};

/// The result of the compensation layout.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Alignment {
    /// The master latency: all timeline sound plays this many frames after the transport position.
    pub total: u32,
    /// Part of the compensation did not fit (a delay over `MAX_PDC_FRAMES` or the lines ran out).
    pub clamped: bool,
}

impl GraphMixer {
    /// Recomputes the path latencies and hands delay lines to connections. `instrument_latency` is the latency of a track instrument.
    pub fn align_latency(&mut self, instrument_latency: impl Fn(TrackId) -> u32) -> Alignment {
        self.measure_arrivals(instrument_latency);
        let clamped = self.assign_delay_lines();
        let total = if self.master < self.nodes.len() { self.scratch.arrival[self.master] } else { 0 };
        Alignment { total, clamped }
    }

    /// The node's own latency: of an instrument, of an enabled effect and the lead of a channel set earlier; other
    /// nodes do not delay sound.
    fn own_latency(&self, index: usize, instrument_latency: &impl Fn(TrackId) -> u32) -> u32 {
        let node = &self.nodes[index];
        let lead = self.utilities.lead(node.delay_ms);
        match node.role {
            Role::Instrument { track, .. } => instrument_latency(track).saturating_add(lead),
            Role::Audio { .. } | Role::Bus { .. } => lead,
            Role::Effect { bypass: false } => node.effect.map_or(0, |position| self.effects.latency(position)),
            Role::Effect { bypass: true } | Role::Master => 0,
        }
    }

    /// `arrival[i]` is when (in delay frames) the sound arrives at the inputs of node `i` after alignment: the maximum
    /// over inputs. `output[i]` is the latency of its output. The snapshot order is topological, so a node's input is ready by its turn.
    fn measure_arrivals(&mut self, instrument_latency: impl Fn(TrackId) -> u32) {
        let count = self.nodes.len();
        self.scratch.arrival.clear();
        self.scratch.arrival.resize(count, 0);
        self.scratch.output.clear();
        self.scratch.output.resize(count, 0);
        for position in 0..self.order.len() {
            let index = self.order[position];
            let output = self.scratch.arrival[index].saturating_add(self.own_latency(index, &instrument_latency));
            self.scratch.output[index] = output;
            for edge in &self.edges[self.starts[index]..self.starts[index + 1]] {
                let arrival = &mut self.scratch.arrival[edge.to];
                *arrival = (*arrival).max(output);
            }
        }
    }

    /// Connections whose sound arrives earlier than the other inputs of the receiver get a delay line for the difference.
    /// First (`keep`) lines stay with their previous connections, so their sound is not cut off; then new ones are handed out.
    /// Returns `true` if some delay had to be cut.
    fn assign_delay_lines(&mut self) -> bool {
        let mut clamped = false;
        self.delay_lines.begin();
        for keep in [true, false] {
            for from in 0..self.nodes.len() {
                for position in self.starts[from]..self.starts[from + 1] {
                    let edge = self.edges[position];
                    if keep {
                        self.edges[position].line = None;
                    } else if edge.line.is_some() {
                        continue;
                    }
                    let wanted = self.scratch.arrival[edge.to].saturating_sub(self.scratch.output[from]) as usize;
                    if wanted == 0 {
                        continue;
                    }
                    let owner = (self.nodes[from].id, self.nodes[edge.to].id, edge.sidechain);
                    let line = if keep { self.delay_lines.keep(owner, wanted) } else { self.delay_lines.take(owner, wanted) };
                    clamped |= wanted > MAX_PDC_FRAMES || (!keep && line.is_none());
                    self.edges[position].line = line;
                }
            }
        }
        self.delay_lines.finish();
        clamped
    }
}
