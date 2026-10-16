//! The song's tempo map: where each tempo applies (as a step or a smooth ramp) and where each time signature applies,
//! and the only time math based on it: ticks ↔ seconds and frames, ticks ↔ bars. Pure functions without
//! allocations, used by the audio thread (transport clock), export, clip rules and recording alike.
//! The frontend mirrors it in `app/src/domain/tempoMap.ts`.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::codes::tempo as codes;
use crate::{Bpm, ContractError, MAX_CLIP_TICK, PPQ, TimeSignature};

/// Maximum number of tempo points and time signature changes in a project.
pub const MAX_TEMPO_CHANGES: usize = 512;
pub const MAX_SIGNATURE_CHANGES: usize = 256;

/// A tempo point. `ramp` is a smooth transition: the tempo moves from the previous point to this one linearly in ticks
/// (beats) instead of stepping at this point. On the first point (tick 0) a ramp is meaningless and is reset.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct TempoChange {
    pub tick: u32,
    pub bpm: Bpm,
    #[serde(default)]
    pub ramp: bool,
}

/// A time signature change. It sits only at a bar start (of the previous signature), so bars are never broken.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct SignatureChange {
    pub tick: u32,
    pub signature: TimeSignature,
}

#[derive(Serialize, Deserialize, JsonSchema)]
struct RawTempoMap {
    tempos: Vec<TempoChange>,
    signatures: Vec<SignatureChange>,
}

/// Tempo map: tempo points and signature changes in ascending tick order, the first ones at tick 0 (the "project
/// tempo" and "project time signature"). An invalid map cannot be created: it is checked in [`TempoMap::new`] and
/// when read from JSON.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(try_from = "RawTempoMap", into = "RawTempoMap")]
pub struct TempoMap {
    tempos: Vec<TempoChange>,
    signatures: Vec<SignatureChange>,
    /// Seconds from the song start to each tempo point, computed once so conversion is a binary search.
    seconds: Vec<f64>,
}

impl TryFrom<RawTempoMap> for TempoMap {
    type Error = ContractError;

    fn try_from(raw: RawTempoMap) -> Result<Self, Self::Error> {
        Self::new(raw.tempos, raw.signatures)
    }
}

impl From<TempoMap> for RawTempoMap {
    fn from(map: TempoMap) -> Self {
        RawTempoMap { tempos: map.tempos, signatures: map.signatures }
    }
}

impl Default for TempoMap {
    fn default() -> Self {
        Self::constant(Bpm::default(), TimeSignature::default())
    }
}

fn seconds_per_tick(bpm: f64) -> f64 {
    60.0 / (f64::from(PPQ) * bpm)
}

impl TempoMap {
    /// Validates the map: the first points are at tick 0, ticks strictly increase and do not exceed `MAX_CLIP_TICK`,
    /// point counts are within the limits, a signature change is at a bar start of the previous signature (all
    /// `tempo::*` codes). A ramp on the first point is reset.
    pub fn new(mut tempos: Vec<TempoChange>, signatures: Vec<SignatureChange>) -> Result<Self, ContractError> {
        check_ticks("tempos", tempos.iter().map(|c| c.tick), MAX_TEMPO_CHANGES)?;
        check_ticks("signatures", signatures.iter().map(|c| c.tick), MAX_SIGNATURE_CHANGES)?;
        for pair in signatures.windows(2) {
            let bar = pair[0].signature.ticks_per_bar() as u32;
            if (pair[1].tick - pair[0].tick) % bar != 0 {
                return Err(codes::SIGNATURE_OFF_BAR
                    .with("tick", pair[1].tick)
                    .with("numerator", pair[0].signature.numerator())
                    .with("denominator", pair[0].signature.denominator())
                    .with("bar", bar)
                    .with("from", pair[0].tick));
            }
        }
        tempos[0].ramp = false;
        let mut seconds = Vec::with_capacity(tempos.len());
        seconds.push(0.0);
        for i in 1..tempos.len() {
            let span = f64::from(tempos[i].tick - tempos[i - 1].tick);
            seconds.push(seconds[i - 1] + segment_seconds(&tempos, i - 1, span));
        }
        Ok(Self { tempos, signatures, seconds })
    }

    /// A map without changes: one tempo and one time signature for the whole song.
    pub fn constant(tempo: Bpm, signature: TimeSignature) -> Self {
        Self {
            tempos: vec![TempoChange { tick: 0, bpm: tempo, ramp: false }],
            signatures: vec![SignatureChange { tick: 0, signature }],
            seconds: vec![0.0],
        }
    }

    pub fn tempos(&self) -> &[TempoChange] {
        &self.tempos
    }

    pub fn signatures(&self) -> &[SignatureChange] {
        &self.signatures
    }

    /// The same map with other tempo points (signatures unchanged).
    pub fn with_tempos(&self, tempos: Vec<TempoChange>) -> Result<Self, ContractError> {
        Self::new(tempos, self.signatures.clone())
    }

    /// The same map with other signature changes (tempo unchanged).
    pub fn with_signatures(&self, signatures: Vec<SignatureChange>) -> Result<Self, ContractError> {
        Self::new(self.tempos.clone(), signatures)
    }

    /// Tempo at the song start (tick 0).
    pub fn base_tempo(&self) -> Bpm {
        self.tempos[0].bpm
    }

    /// Time signature at the song start (tick 0).
    pub fn base_signature(&self) -> TimeSignature {
        self.signatures[0].signature
    }

    /// Whether there are tempo or signature changes after the start.
    pub fn has_changes(&self) -> bool {
        self.tempos.len() > 1 || self.signatures.len() > 1
    }

    /// Tempo at `tick` (an intermediate value inside a ramp). Before the song start, the tempo of tick 0.
    pub fn tempo_at(&self, tick: f64) -> Bpm {
        let i = self.tempo_index(tick);
        let from = self.tempos[i];
        match self.tempos.get(i + 1).filter(|next| next.ramp) {
            Some(to) => {
                let share = ((tick - f64::from(from.tick)) / f64::from(to.tick - from.tick)).clamp(0.0, 1.0);
                Bpm::new(from.bpm.get() + (to.bpm.get() - from.bpm.get()) * share).unwrap_or(from.bpm)
            }
            None => from.bpm,
        }
    }

    /// Whether a tempo ramp is in progress at `tick`.
    pub fn in_ramp(&self, tick: f64) -> bool {
        tick >= 0.0 && self.tempos.get(self.tempo_index(tick) + 1).is_some_and(|next| next.ramp)
    }

    /// The signature change in effect at `tick` (the first one before the song start).
    pub fn signature_at(&self, tick: f64) -> SignatureChange {
        self.signatures[self.signature_index(tick)]
    }

    /// The nearest tempo point or signature change strictly after `tick`: the audio segment timing must be split there.
    pub fn next_change(&self, tick: f64) -> Option<f64> {
        let tempo = self.tempos.get(self.tempos.partition_point(|c| f64::from(c.tick) <= tick)).map(|c| f64::from(c.tick));
        let signature = self.signatures.get(self.signatures.partition_point(|c| f64::from(c.tick) <= tick)).map(|c| f64::from(c.tick));
        match (tempo, signature) {
            (Some(a), Some(b)) => Some(a.min(b)),
            (a, b) => a.or(b),
        }
    }

    /// Seconds from the song start to `tick` (negative before the start, at the tempo of tick 0). Time inside a ramp is
    /// exact: the integral of 1/tempo over a linear tempo is a logarithm.
    pub fn seconds_at(&self, tick: f64) -> f64 {
        if tick <= 0.0 {
            return tick * seconds_per_tick(self.base_tempo().get());
        }
        let i = self.tempo_index(tick);
        self.seconds[i] + segment_seconds(&self.tempos, i, tick - f64::from(self.tempos[i].tick))
    }

    /// Inverse of [`TempoMap::seconds_at`]: the tick reached `seconds` seconds after the song start.
    pub fn tick_at_seconds(&self, seconds: f64) -> f64 {
        if seconds <= 0.0 {
            return seconds / seconds_per_tick(self.base_tempo().get());
        }
        let i = self.seconds.partition_point(|&s| s <= seconds).saturating_sub(1);
        let from = self.tempos[i];
        let elapsed = seconds - self.seconds[i];
        let ticks = match ramp_target(&self.tempos, i) {
            Some((to, span)) if to != from.bpm.get() => {
                // Tempo at the end of the elapsed piece: b0 · e^(t · k · PPQ / 60), where k is the tempo rise per tick.
                let slope = (to - from.bpm.get()) / span;
                let bpm = from.bpm.get() * (elapsed * slope * f64::from(PPQ) / 60.0).exp();
                (bpm - from.bpm.get()) / slope
            }
            _ => elapsed / seconds_per_tick(from.bpm.get()),
        };
        f64::from(from.tick) + ticks
    }

    /// How many frames at `sample_rate` play from `from` to `to` (ticks).
    pub fn frames_between(&self, from: f64, to: f64, sample_rate: u32) -> f64 {
        (self.seconds_at(to) - self.seconds_at(from)) * f64::from(sample_rate)
    }

    /// The tick the song reaches after `frames` frames at `sample_rate`, starting from `from`.
    pub fn tick_after_frames(&self, from: f64, frames: f64, sample_rate: u32) -> f64 {
        self.tick_at_seconds(self.seconds_at(from) + frames / f64::from(sample_rate.max(1)))
    }

    /// Start of the bar containing `tick` (bars are counted from the signature change in effect there).
    pub fn bar_start(&self, tick: f64) -> f64 {
        let change = self.signature_at(tick);
        let bar = change.signature.ticks_per_bar();
        f64::from(change.tick) + ((tick - f64::from(change.tick)) / bar).floor() * bar
    }

    /// Length in ticks of the bar containing `tick` (by the signature in effect there).
    pub fn bar_length_at(&self, tick: u32) -> u32 {
        self.signature_at(f64::from(tick)).signature.ticks_per_bar() as u32
    }

    /// Index (0-based) of the bar containing `tick`; negative before the song start.
    pub fn bar_index(&self, tick: f64) -> i64 {
        let j = self.signature_index(tick);
        let change = self.signatures[j];
        let inside = ((tick - f64::from(change.tick)) / change.signature.ticks_per_bar()).floor() as i64;
        self.bars_before(j) + inside
    }

    /// Start tick of bar number `bar` (0-based).
    pub fn bar_tick(&self, bar: u32) -> u64 {
        let mut first = 0_u64;
        for (j, change) in self.signatures.iter().enumerate() {
            let length = change.signature.ticks_per_bar() as u64;
            let bars = self.signatures.get(j + 1).map(|next| u64::from(next.tick - change.tick) / length);
            if bars.is_none_or(|bars| u64::from(bar) < first + bars) {
                return u64::from(change.tick) + (u64::from(bar) - first) * length;
            }
            first += bars.unwrap_or_default();
        }
        0
    }

    /// Number of whole bars before signature change `j`.
    fn bars_before(&self, j: usize) -> i64 {
        self.signatures[..=j]
            .windows(2)
            .map(|pair| i64::from((pair[1].tick - pair[0].tick) / pair[0].signature.ticks_per_bar() as u32))
            .sum()
    }

    /// The last tempo point not after `tick` (the first one before the song start).
    fn tempo_index(&self, tick: f64) -> usize {
        self.tempos.partition_point(|c| f64::from(c.tick) <= tick).saturating_sub(1)
    }

    fn signature_index(&self, tick: f64) -> usize {
        self.signatures.partition_point(|c| f64::from(c.tick) <= tick).saturating_sub(1)
    }
}

/// Ticks strictly increase, the first is 0, none exceeds `MAX_CLIP_TICK`, at most `max` of them.
fn check_ticks(list: &str, ticks: impl Iterator<Item = u32> + Clone, max: usize) -> Result<(), ContractError> {
    let count = ticks.clone().count();
    if count == 0 {
        return Err(codes::TEMPO_MAP_EMPTY.with("list", list));
    }
    if count > max {
        return Err(codes::TEMPO_MAP_TOO_MANY.with("list", list).with("max", max));
    }
    let mut previous: Option<u32> = None;
    for tick in ticks {
        match previous {
            None if tick != 0 => return Err(codes::TEMPO_MAP_FIRST_NOT_AT_ZERO.with("list", list).with("tick", tick)),
            Some(before) if tick <= before => {
                return Err(codes::TEMPO_MAP_UNORDERED.with("list", list).with("before", before).with("tick", tick));
            }
            _ if tick > MAX_CLIP_TICK => {
                return Err(codes::TICK_OUT_OF_RANGE.with("tick", tick).with("max", MAX_CLIP_TICK));
            }
            _ => {}
        }
        previous = Some(tick);
    }
    Ok(())
}

/// If a ramp follows point `i`: its end tempo and length in ticks.
fn ramp_target(tempos: &[TempoChange], i: usize) -> Option<(f64, f64)> {
    let next = tempos.get(i + 1).filter(|next| next.ramp)?;
    Some((next.bpm.get(), f64::from(next.tick - tempos[i].tick)))
}

/// Seconds for `ticks` ticks from point `i` (not beyond the next point).
fn segment_seconds(tempos: &[TempoChange], i: usize, ticks: f64) -> f64 {
    let from = tempos[i].bpm.get();
    match ramp_target(tempos, i) {
        Some((to, span)) if to != from => {
            let slope = (to - from) / span;
            60.0 / f64::from(PPQ) * ((from + slope * ticks) / from).ln() / slope
        }
        _ => ticks * seconds_per_tick(from),
    }
}

#[cfg(test)]
mod tests;
