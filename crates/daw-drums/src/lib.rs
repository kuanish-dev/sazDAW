//! Synthesized drums: each is an instrument with one voice.

mod voice;

use daw_contracts::{ContractError, Instrument, InstrumentFactory, L10n, ParamInfo, ParamKind, Params, PluginState};

use voice::Drum;

#[derive(Debug, Clone, Copy)]
enum DrumKind {
    Kick,
    Snare,
    Hat,
    Clap,
    OpenHat,
    TomHigh,
    TomMid,
    TomLow,
    Crash,
    Ride,
    Rim,
}

/// The parameters of every drum (the same for all kinds).
const PARAMS: [ParamInfo; 3] = [
    ParamInfo {
        name: "gain",
        label: L10n { en: "Volume", ru: "Громкость" },
        min: 0.0,
        max: 1.5,
        default: 1.0,
        kind: ParamKind::Number,
        choices: &[],
    },
    ParamInfo {
        name: "tune",
        label: L10n { en: "Pitch, semitones", ru: "Высота, полутона" },
        min: -12.0,
        max: 12.0,
        default: 0.0,
        kind: ParamKind::Number,
        choices: &[],
    },
    ParamInfo {
        name: "decay",
        label: L10n { en: "Decay, x", ru: "Затухание, x" },
        min: 0.3,
        max: 3.0,
        default: 1.0,
        kind: ParamKind::Number,
        choices: &[],
    },
];

struct DrumFactory {
    kind: DrumKind,
    id: &'static str,
    label: L10n,
}

impl InstrumentFactory for DrumFactory {
    fn kind(&self) -> &'static str {
        self.id
    }

    fn label(&self) -> L10n {
        self.label
    }

    fn describe(&self) -> Vec<ParamInfo> {
        PARAMS.to_vec()
    }

    // A built-in instrument has no state of its own: everything is in the parameters.
    fn create(&self, params: &Params, _state: Option<&PluginState>) -> Result<Box<dyn Instrument>, ContractError> {
        let mut drum = Drum::new(self.kind);
        for (name, value) in params {
            drum.set_param(name, *value);
        }
        Ok(Box::new(drum))
    }
}

/// All drums: the kind, the instrument kind id and the name, in catalog order.
const DRUMS: [(DrumKind, &str, L10n); 11] = [
    (DrumKind::Kick, "kick", L10n { en: "Kick", ru: "Кик" }),
    (DrumKind::Snare, "snare", L10n { en: "Snare", ru: "Снейр" }),
    (DrumKind::Hat, "hat", L10n { en: "Hi-hat", ru: "Хэт" }),
    (DrumKind::Clap, "clap", L10n { en: "Clap", ru: "Клэп" }),
    (DrumKind::OpenHat, "openhat", L10n { en: "Open hi-hat", ru: "Открытый хэт" }),
    (DrumKind::TomHigh, "tom_hi", L10n { en: "High tom", ru: "Том высокий" }),
    (DrumKind::TomMid, "tom_mid", L10n { en: "Mid tom", ru: "Том средний" }),
    (DrumKind::TomLow, "tom_lo", L10n { en: "Low tom", ru: "Том низкий" }),
    (DrumKind::Crash, "crash", L10n { en: "Crash", ru: "Крэш" }),
    (DrumKind::Ride, "ride", L10n { en: "Ride", ru: "Райд" }),
    (DrumKind::Rim, "rim", L10n { en: "Rimshot", ru: "Римшот" }),
];

/// Factories of all drums (see `DRUMS`).
pub fn factories() -> Vec<Box<dyn InstrumentFactory>> {
    DRUMS.into_iter().map(|(kind, id, label)| Box::new(DrumFactory { kind, id, label }) as Box<dyn InstrumentFactory>).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use daw_contracts::{AudioBlock, StreamConfig};

    /// One hit of `kind` with `params`: `frames` mono frames.
    fn hit_with(kind: DrumKind, params: &[(&str, f32)], frames: usize) -> Vec<f32> {
        let mut drum = Drum::new(kind);
        drum.prepare(StreamConfig { sample_rate: 48_000, channels: 1 });
        for (name, value) in params {
            drum.set_param(name, *value);
        }
        drum.note_on(36, 1.0);
        let mut buffer = vec![0.0; frames];
        drum.render(&mut AudioBlock::new(&mut buffer, 1));
        buffer
    }

    fn hit(kind: DrumKind, frames: usize) -> Vec<f32> {
        hit_with(kind, &[], frames)
    }

    #[test]
    fn every_drum_is_audible_finite_and_ends() {
        for (kind, _, _) in DRUMS {
            let out = hit(kind, 120_000);
            let peak = out.iter().fold(0.0f32, |a, b| a.max(b.abs()));
            assert!(peak > 0.05 && peak <= 1.0, "{kind:?} peak = {peak}");
            assert!(out.iter().all(|v| v.is_finite()));
            assert!(out[100_000..].iter().all(|v| *v == 0.0), "{kind:?} should end");
        }
    }

    #[test]
    fn tune_and_decay_change_the_sound() {
        let kick = |tune: f32, decay: f32| hit_with(DrumKind::Kick, &[("tune", tune), ("decay", decay)], 48_000);
        let normal = kick(0.0, 1.0);
        let last_audible = |b: &[f32]| b.iter().rposition(|v| *v != 0.0).unwrap_or(0);
        assert!(last_audible(&kick(0.0, 2.0)) > last_audible(&normal));
        assert_ne!(normal, kick(12.0, 1.0));
    }

    #[test]
    fn toms_are_ordered_by_pitch_and_cymbals_differ_in_length() {
        let zero_crossings = |kind| hit(kind, 4_800).windows(2).filter(|w| w[0] <= 0.0 && w[1] > 0.0).count();
        assert!(zero_crossings(DrumKind::TomHigh) > zero_crossings(DrumKind::TomMid));
        assert!(zero_crossings(DrumKind::TomMid) > zero_crossings(DrumKind::TomLow));
        let last_audible = |kind| hit(kind, 120_000).iter().rposition(|v| v.abs() > 1e-4).unwrap_or(0);
        assert!(last_audible(DrumKind::Hat) < last_audible(DrumKind::OpenHat));
        assert!(last_audible(DrumKind::OpenHat) < last_audible(DrumKind::Crash));
    }

    #[test]
    fn all_notes_off_fades_a_ringing_hit_out_quickly() {
        let mut drum = Drum::new(DrumKind::Crash);
        drum.prepare(StreamConfig { sample_rate: 48_000, channels: 1 });
        drum.note_on(49, 1.0);
        let mut before = vec![0.0; 4_800];
        drum.render(&mut AudioBlock::new(&mut before, 1));
        drum.all_notes_off();
        let mut after = vec![0.0; 4_800];
        drum.render(&mut AudioBlock::new(&mut after, 1));
        assert!(after[..240].iter().any(|v| *v != 0.0), "a fade, not a cut (a cut clicks)");
        assert!(after[2_400..].iter().all(|v| *v == 0.0), "silent after 50 ms instead of ringing on for over a second");
        drum.note_on(49, 1.0);
        let mut next = vec![0.0; 4_800];
        drum.render(&mut AudioBlock::new(&mut next, 1));
        assert!(next[2_400..].iter().any(|v| *v != 0.0), "the next hit plays in full");
    }

    #[test]
    fn silent_until_triggered() {
        let mut drum = Drum::new(DrumKind::Kick);
        drum.prepare(StreamConfig { sample_rate: 48_000, channels: 1 });
        let mut buffer = vec![0.0; 256];
        drum.render(&mut AudioBlock::new(&mut buffer, 1));
        assert!(buffer.iter().all(|v| *v == 0.0));
    }
}
