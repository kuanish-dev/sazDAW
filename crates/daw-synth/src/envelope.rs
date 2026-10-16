//! Envelopes of a synth voice: the AHDSR volume envelope and the attack–decay envelope that opens the filter.

/// Times in seconds; sustain is a level `0..=1`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct Ahdsr {
    pub attack: f32,
    /// How many seconds the level holds at maximum after the attack.
    pub hold: f32,
    pub decay: f32,
    pub sustain: f32,
    pub release: f32,
}

impl Default for Ahdsr {
    fn default() -> Self {
        Self { attack: 0.005, hold: 0.0, decay: 0.15, sustain: 0.6, release: 0.12 }
    }
}

/// Linear envelope increment per frame for a given duration.
fn rate(seconds: f32, sample_rate: f32) -> f32 {
    1.0 / (seconds.max(0.0005) * sample_rate)
}

#[derive(Debug, Clone, Copy, PartialEq)]
enum Stage {
    Idle,
    Attack,
    Hold,
    Decay,
    Sustain,
    Release,
}

/// The volume envelope: linear segments; the release falls from the current level to zero in exactly `release`
/// seconds.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Envelope {
    stage: Stage,
    level: f32,
    /// How many frames to keep holding the maximum (the Hold stage).
    hold_left: f32,
    /// Level drop per frame in the Release stage.
    release_step: f32,
}

impl Envelope {
    pub(crate) const IDLE: Envelope = Envelope { stage: Stage::Idle, level: 0.0, hold_left: 0.0, release_step: 0.0 };

    /// Starts the attack from the current level: a retriggered voice rises from where it was, without a click.
    pub(crate) fn start(&mut self) {
        self.stage = Stage::Attack;
    }

    pub(crate) fn release(&mut self, seconds: f32, sample_rate: f32) {
        if !matches!(self.stage, Stage::Idle | Stage::Release) {
            self.stage = Stage::Release;
            self.release_step = self.level * rate(seconds, sample_rate);
        }
    }

    pub(crate) fn is_idle(&self) -> bool {
        self.stage == Stage::Idle
    }

    /// The key is still down: not finished and not releasing.
    pub(crate) fn is_held(&self) -> bool {
        !matches!(self.stage, Stage::Idle | Stage::Release)
    }

    /// One frame; returns the level `0..=1`.
    pub(crate) fn advance(&mut self, times: &Ahdsr, sample_rate: f32) -> f32 {
        match self.stage {
            Stage::Idle | Stage::Sustain => {}
            Stage::Attack => {
                self.level += rate(times.attack, sample_rate);
                if self.level >= 1.0 {
                    self.level = 1.0;
                    if times.hold > 0.0 {
                        self.hold_left = times.hold * sample_rate;
                        self.stage = Stage::Hold;
                    } else {
                        self.stage = Stage::Decay;
                    }
                }
            }
            Stage::Hold => {
                self.hold_left -= 1.0;
                if self.hold_left <= 0.0 {
                    self.stage = Stage::Decay;
                }
            }
            Stage::Decay => {
                self.level -= rate(times.decay, sample_rate) * (1.0 - times.sustain).max(0.0001);
                if self.level <= times.sustain {
                    self.level = times.sustain;
                    self.stage = Stage::Sustain;
                }
            }
            Stage::Release => {
                self.level -= self.release_step;
                if self.level <= 0.0 {
                    *self = Self::IDLE;
                }
            }
        }
        self.level
    }
}

/// The filter envelope: rises to 1 in `attack` seconds, then falls back to 0 in `decay` seconds whether the key is
/// held or not (a pluck, or a slow swell with a long attack).
#[derive(Debug, Clone, Copy)]
pub(crate) struct AttackDecay {
    level: f32,
    rising: bool,
}

impl AttackDecay {
    pub(crate) const REST: AttackDecay = AttackDecay { level: 0.0, rising: false };

    /// Starts rising from the current level.
    pub(crate) fn start(&mut self) {
        self.rising = true;
    }

    /// One frame; returns the level `0..=1`.
    pub(crate) fn advance(&mut self, attack: f32, decay: f32, sample_rate: f32) -> f32 {
        if self.rising {
            self.level += rate(attack, sample_rate);
            if self.level >= 1.0 {
                self.level = 1.0;
                self.rising = false;
            }
        } else if self.level > 0.0 {
            self.level = (self.level - rate(decay, sample_rate)).max(0.0);
        }
        self.level
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SR: f32 = 1_000.0;

    fn run(envelope: &mut Envelope, times: &Ahdsr, frames: usize) -> f32 {
        (0..frames).fold(0.0, |_, _| envelope.advance(times, SR))
    }

    #[test]
    fn rises_holds_decays_to_sustain_and_releases_to_idle() {
        let times = Ahdsr { attack: 0.01, hold: 0.05, decay: 0.01, sustain: 0.3, release: 0.02 };
        let mut envelope = Envelope::IDLE;
        envelope.start();
        assert!((run(&mut envelope, &times, 10) - 1.0).abs() < 1e-3, "the attack reaches the top");
        assert_eq!(run(&mut envelope, &times, 40), 1.0, "and holds it");
        assert!((run(&mut envelope, &times, 40) - 0.3).abs() < 1e-3, "the decay ends at the sustain");
        assert!(envelope.is_held());
        envelope.release(times.release, SR);
        assert!(!envelope.is_held());
        run(&mut envelope, &times, 21);
        assert!(envelope.is_idle(), "the release ends on time");
    }

    #[test]
    fn a_retrigger_rises_from_the_current_level() {
        let times = Ahdsr { attack: 0.1, sustain: 1.0, ..Ahdsr::default() };
        let mut envelope = Envelope::IDLE;
        envelope.start();
        let middle = run(&mut envelope, &times, 50);
        envelope.release(0.1, SR);
        let released = run(&mut envelope, &times, 10);
        envelope.start();
        let next = envelope.advance(&times, SR);
        assert!(released < middle && next > released && next - released < 0.02, "no jump: {released} → {next}");
    }

    #[test]
    fn the_filter_envelope_rises_then_falls_back_to_rest() {
        let mut envelope = AttackDecay::REST;
        envelope.start();
        let top = (0..10).fold(0.0, |_, _| envelope.advance(0.01, 0.1, SR));
        assert_eq!(top, 1.0);
        let half = (0..50).fold(0.0, |_, _| envelope.advance(0.01, 0.1, SR));
        assert!((half - 0.5).abs() < 0.02, "{half}");
        assert_eq!((0..60).fold(1.0, |_, _| envelope.advance(0.01, 0.1, SR)), 0.0);
    }
}
