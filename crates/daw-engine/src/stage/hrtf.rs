//! The direction cue: what the listener's head and outer ears do to a sound arriving from a direction. On headphones it
//! is a compact parametric head-related transfer function (HRTF): a few biquads on the source and a fractional delay
//! and a shelf per ear, in place of a measured set. It needs no file, suits most heads about equally well, and
//! allocates and locks nothing on the audio thread. On speakers only what two speakers in front can carry: left/right
//! by level, and the height cue.
//!
//! Both parts are tuned against straight ahead: a sound from straight ahead at ear height passes unchanged, so
//! switching the listening mode never changes a centred sound's tone.
//! - **The head** ([`Head`], per ear). On headphones the far ear hears the sound later (the interaural time difference,
//!   Woodworth's spherical head, up to ≈ 0.66 ms), a little quieter, and duller (the head's shadow, Brown and Duda's
//!   first-order shelf), while the near ear gets a little more of the highs. All of it follows left/right alone, so it
//!   is the same ahead and behind, as with a real head; that is why the outer ear's cue is needed.
//! - **The outer ear** ([`Pinna`], on the source before it splits into the ears). These are the spectral cues that tell
//!   ahead from behind and up from down. From behind, the forward-facing pinna takes the presence and the air (a high
//!   shelf down), adds a little body around 1 kHz (Blauert's rear directional band) and cuts a notch. Above, the highs
//!   lift and the notch rises (from about 4.6 kHz straight below to 12 kHz straight above). On speakers only the height
//!   part plays, so a sound behind sounds like its mirror ahead.

use std::f32::consts::{FRAC_PI_4, SQRT_2};

use daw_contracts::Listening;

use super::dsp::Biquad;
use super::geometry::{Direction, interaural_delay};

/// The head on headphones: from the side, the far ear keeps this much less of the overall level (−1.9 dB) and of the
/// highs (−14 dB), and the near ear gets this much more of the highs (+3.5 dB). Straight ahead or behind, nothing
/// changes.
const FAR_EAR_LEVEL: f32 = 0.2;
const FAR_EAR_HIGHS: f32 = 0.8;
const NEAR_EAR_HIGHS: f32 = 0.5;
/// A reflection is one tap of the dry sound with no shadow filter of its own, so on headphones the head's shadow goes
/// into its level: from the side, the far ear keeps this much less of it (−6 dB, about the shadow across the band).
const REFLECTION_FAR_EAR: f32 = 0.5;
/// The outer ear from behind (headphones only): a shelf above `REAR_HZ`, `REAR_DB` down straight behind, and a bump of
/// `REAR_BAND_DB` around `REAR_BAND_HZ`. Both grow from nothing at the side to full straight behind.
const REAR_HZ: f32 = 3_000.0;
const REAR_DB: f32 = 8.0;
const REAR_BAND_HZ: f32 = 1_100.0;
const REAR_BAND_DB: f32 = 3.0;
const REAR_BAND_Q: f32 = 1.2;
/// The height cue (both modes): a shelf above `LIFT_HZ` (±`LIFT_DB` straight up or down) and a notch around `NOTCH_HZ`
/// that moves `NOTCH_OCTAVES` up straight above and down straight below, `NOTCH_DB` deep there. On headphones the
/// notch deepens behind as well (it is part of the rear spectrum), where at ear height it stays at `NOTCH_HZ`.
const LIFT_HZ: f32 = 7_000.0;
const LIFT_DB: f32 = 4.0;
const NOTCH_HZ: f32 = 7_500.0;
const NOTCH_OCTAVES: f32 = 0.7;
const NOTCH_DB: f32 = 9.0;
const NOTCH_Q: f32 = 2.0;

/// Equal-power gains for a direction (−1 left … 1 right), 1 each in the middle so a centred source keeps its level.
pub fn equal_power(lateral: f32) -> [f32; 2] {
    let angle = (lateral.clamp(-1.0, 1.0) + 1.0) * FRAC_PI_4;
    [angle.cos() * SQRT_2, angle.sin() * SQRT_2]
}

/// What the head does to a sound, per ear (left, right).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Head {
    pub gain: [f32; 2],
    /// How much later the sound reaches each ear, frames.
    pub delay: [f32; 2],
    /// The highs each ear keeps in the head's shadow (`dsp::HeadShadow`; 1 is flat).
    pub shadow: [f32; 2],
}

/// The ear turned away from a sound (0 left, 1 right) and how far to the side the sound is (0 middle … 1 side).
fn far_ear(direction: Direction) -> (usize, f32) {
    (usize::from(direction.lateral < 0.0), direction.lateral.abs().min(1.0))
}

impl Head {
    /// The head's part for the direct sound.
    pub fn direct(direction: Direction, listening: Listening, sample_rate: f32) -> Self {
        match listening {
            Listening::Speakers => Self { gain: equal_power(direction.lateral), delay: [0.0; 2], shadow: [1.0; 2] },
            Listening::Headphones => {
                let (far, side) = far_ear(direction);
                let mut head = Self { gain: [1.0; 2], delay: [0.0; 2], shadow: [1.0 + NEAR_EAR_HIGHS * side; 2] };
                head.gain[far] = 1.0 - FAR_EAR_LEVEL * side;
                head.delay[far] = interaural_delay(side) * sample_rate;
                head.shadow[far] = 1.0 - FAR_EAR_HIGHS * side;
                head
            }
        }
    }

    /// The head's part for a reflection: on headphones its shadow is folded into its level (`shadow` stays flat).
    pub fn reflection(direction: Direction, listening: Listening, sample_rate: f32) -> Self {
        match listening {
            Listening::Speakers => Self::direct(direction, listening, sample_rate),
            Listening::Headphones => {
                let (far, side) = far_ear(direction);
                let mut head = Self { gain: [1.0; 2], delay: [0.0; 2], shadow: [1.0; 2] };
                head.gain[far] = 1.0 - REFLECTION_FAR_EAR * side;
                head.delay[far] = interaural_delay(side) * sample_rate;
                head
            }
        }
    }
}

/// The outer ear's cue for a direction, in the values its filters take. The voice ramps it like its other values.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct PinnaCue {
    /// 0 at the side or ahead … 1 straight behind (always 0 on speakers).
    pub rear: f32,
    /// −1 straight down … 1 straight up.
    pub elevation: f32,
}

impl PinnaCue {
    pub fn of(direction: Direction, listening: Listening) -> Self {
        let rear = match listening {
            Listening::Speakers => 0.0,
            Listening::Headphones => direction.rear(),
        };
        Self { rear, elevation: direction.elevation }
    }

    /// Towards `target`, each value moving at most `most`.
    pub fn toward(self, target: PinnaCue, most: f32) -> PinnaCue {
        let step = |from: f32, to: f32| from + (to - from).clamp(-most, most);
        PinnaCue { rear: step(self.rear, target.rear), elevation: step(self.elevation, target.elevation) }
    }
}

/// The outer ear's filters for one voice: four biquads in a row on the source.
#[derive(Clone, Copy)]
pub struct Pinna {
    rear_shelf: Biquad,
    rear_band: Biquad,
    lift: Biquad,
    notch: Biquad,
    /// The cue the filters were last set for (`None`: not set yet).
    cue: Option<PinnaCue>,
}

impl Pinna {
    /// Flat, with empty filters, until the first cue.
    pub const FLAT: Pinna = Pinna { rear_shelf: Biquad::PASS, rear_band: Biquad::PASS, lift: Biquad::PASS, notch: Biquad::PASS, cue: None };

    /// Sets the filters for `cue`, unless they are set for it already.
    pub fn follow(&mut self, cue: PinnaCue, sample_rate: f32) {
        if self.cue == Some(cue) {
            return;
        }
        self.cue = Some(cue);
        let PinnaCue { rear, elevation } = cue;
        self.rear_shelf.set_high_shelf(sample_rate, REAR_HZ, -REAR_DB * rear);
        self.rear_band.set_peaking(sample_rate, REAR_BAND_HZ, REAR_BAND_DB * rear, REAR_BAND_Q);
        self.lift.set_high_shelf(sample_rate, LIFT_HZ, LIFT_DB * elevation);
        let notch_hz = NOTCH_HZ * 2f32.powf(NOTCH_OCTAVES * elevation);
        self.notch.set_peaking(sample_rate, notch_hz, -NOTCH_DB * elevation.abs().max(rear), NOTCH_Q);
    }

    pub fn process(&mut self, input: f32) -> f32 {
        self.notch.process(self.lift.process(self.rear_band.process(self.rear_shelf.process(input))))
    }
}

/// The outer ear's shade on the reflections that come from behind: the rear shelf alone (the walls have taken the
/// finer detail already). The voice keeps one per ear, set once per sample rate.
pub fn rear_shade(sample_rate: f32) -> Biquad {
    let mut shelf = Biquad::PASS;
    shelf.set_high_shelf(sample_rate, REAR_HZ, -REAR_DB);
    shelf
}
