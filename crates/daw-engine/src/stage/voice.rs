//! One channel standing on the stage: turns its sound into what the listener hears from its place. The sound is taken
//! as one point source (the middle of its channels). The direct sound gets its level by distance, loses highs to the
//! air and takes its direction cue (`hrtf.rs`): the outer ear's tone on the source, then per ear the head's delay,
//! level and shadow (on speakers: equal-power panning and the height cue only). Six first reflections off the walls
//! come after it, each from its own direction; on headphones each also reaches the far ear later, and those from
//! behind come darker, so a source behind is heard behind in its echoes too. The sound feeds the shared room tail from
//! the moment of the first reflection.
//!
//! Nothing a move changes jumps: the place glides towards the new one (a dragged point travels through the room), and
//! every per-sample value ramps from one segment's to the next, so dragging never clicks; moving delays bend the pitch
//! a little, like a real moving source.

use daw_contracts::Listening;

use super::dsp::{Biquad, DelayLine, HeadShadow, OnePole, one_pole};
use super::geometry::{Cues, REFLECTIONS, RoomShape, Spot, interaural_delay};
use super::hrtf::{self, Head, Pinna, PinnaCue, equal_power};

/// How long the place takes to glide most of the way (63 %) to a new one, s.
const GLIDE_SECONDS: f32 = 0.04;
/// The direct line holds the longest interaural delay (≈ 0.66 ms) with room to spare at 192 kHz, frames.
const DIRECT_FRAMES: usize = 256;
/// Reflections later than this are left to the diffuse tail, s.
const LONGEST_REFLECTION: f32 = 0.12;
/// The outer ear's filters take new coefficients this often while the direction moves, frames (often enough not to
/// zipper).
const PINNA_FRAMES: usize = 32;
/// The outer ear's cue takes at least this long to go all the way from ahead to behind, s. A source passing right by
/// the head turns from ahead to behind within a few milliseconds; the filters' steps would click if it followed.
const PINNA_SLEW_SECONDS: f32 = 0.05;
/// How far the room's input leans to the source's side (0 not at all … 1 as far as the source).
const ROOM_LEAN: f32 = 0.5;

/// The room as every voice hears it this segment (from the project's `Room` and the sample rate).
pub struct Acoustics {
    pub shape: RoomShape,
    pub listening: Listening,
    /// How much of the sound each wall sends back (0…1).
    pub reflectivity: f32,
    /// The room tail's input level.
    pub send: f32,
    /// The walls' lowpass coefficient (the reflections lose highs).
    pub wall: f32,
    pub sample_rate: f32,
}

/// One reflection in samples, per ear (left, right): its delay after the direct sound (frames) and its level.
#[derive(Clone, Copy, Default)]
struct Tap {
    delay: [f32; 2],
    gain: [f32; 2],
    /// The share of it that comes from behind (headphones only): that share takes the outer ear's rear shade.
    rear: f32,
}

/// The per-sample values of a voice; they ramp from one segment's to the next.
#[derive(Clone, Copy, Default)]
struct Mix {
    /// The direct sound's level in each ear (left, right).
    gain: [f32; 2],
    /// How much later the direct sound reaches each ear, frames (headphones only).
    delay: [f32; 2],
    /// The highs each ear keeps in the head's shadow (headphones only; 1 is flat).
    shadow: [f32; 2],
    /// The air lowpass coefficient.
    air: f32,
    /// The outer ear's cue (its filters follow it).
    pinna: PinnaCue,
    taps: [Tap; REFLECTIONS],
    /// The room tail's input level per channel, from the first reflection's moment (frames after the direct sound).
    send: [f32; 2],
    send_delay: f32,
}

fn lerp(from: f32, to: f32, t: f32) -> f32 {
    from + (to - from) * t
}

fn lerp2(from: [f32; 2], to: [f32; 2], t: f32) -> [f32; 2] {
    [lerp(from[0], to[0], t), lerp(from[1], to[1], t)]
}

impl Mix {
    fn of(cues: &Cues, acoustics: &Acoustics) -> Self {
        let (rate, listening) = (acoustics.sample_rate, acoustics.listening);
        let head = Head::direct(cues.direction, listening, rate);
        let taps = cues.reflections.map(|r| {
            let head = Head::reflection(r.direction, listening, rate);
            let delay = r.delay.min(LONGEST_REFLECTION) * rate;
            Tap {
                delay: head.delay.map(|d| delay + d),
                gain: head.gain.map(|g| g * r.gain),
                rear: PinnaCue::of(r.direction, listening).rear,
            }
        });
        let first = cues.reflections.iter().map(|r| r.delay).fold(LONGEST_REFLECTION, f32::min);
        Self {
            gain: head.gain.map(|g| g * cues.gain),
            delay: head.delay,
            shadow: head.shadow,
            air: one_pole(cues.air_hz, rate),
            pinna: PinnaCue::of(cues.direction, listening),
            taps,
            send: equal_power(cues.direction.lateral * ROOM_LEAN).map(|g| g * acoustics.send),
            send_delay: first * rate,
        }
    }

    fn lerp(&self, to: &Mix, t: f32) -> Mix {
        Mix {
            gain: lerp2(self.gain, to.gain, t),
            delay: lerp2(self.delay, to.delay, t),
            shadow: lerp2(self.shadow, to.shadow, t),
            air: lerp(self.air, to.air, t),
            pinna: PinnaCue { rear: lerp(self.pinna.rear, to.pinna.rear, t), elevation: lerp(self.pinna.elevation, to.pinna.elevation, t) },
            taps: std::array::from_fn(|i| Tap {
                delay: lerp2(self.taps[i].delay, to.taps[i].delay, t),
                gain: lerp2(self.taps[i].gain, to.taps[i].gain, t),
                rear: lerp(self.taps[i].rear, to.taps[i].rear, t),
            }),
            send: lerp2(self.send, to.send, t),
            send_delay: lerp(self.send_delay, to.send_delay, t),
        }
    }
}

pub struct Voice {
    /// Where the voice is now (gliding towards the channel's place).
    spot: Spot,
    /// The per-sample values at the end of the previous segment.
    mix: Mix,
    /// Just bound to a channel: the next segment starts right at its place.
    fresh: bool,
    air: OnePole,
    pinna: Pinna,
    shadow: [HeadShadow; 2],
    /// The reflections from behind, per ear (headphones only).
    rear_shade: [Biquad; 2],
    walls: [OnePole; 2],
    /// The shaped direct sound (for the interaural delay) and the dry sound (for the reflections and the room).
    direct: DelayLine,
    early: DelayLine,
}

impl Voice {
    /// A voice with its delay lines for this sample rate (allocates: only outside the audio thread).
    pub fn new(sample_rate: f32) -> Self {
        Self {
            spot: Spot { pan: 0.0, depth: 0.0, height: 0.0 },
            mix: Mix::default(),
            fresh: true,
            air: OnePole::default(),
            pinna: Pinna::FLAT,
            shadow: [HeadShadow::new(sample_rate); 2],
            rear_shade: [hrtf::rear_shade(sample_rate); 2],
            walls: [OnePole::default(); 2],
            direct: DelayLine::new(DIRECT_FRAMES),
            early: DelayLine::new(((LONGEST_REFLECTION + interaural_delay(1.0)) * sample_rate).ceil() as usize),
        }
    }

    /// Forgets the previous channel: the next segment starts at the new channel's place with silent lines and filters.
    pub fn reset(&mut self, sample_rate: f32) {
        self.fresh = true;
        self.air = OnePole::default();
        self.pinna = Pinna::FLAT;
        self.shadow = [HeadShadow::new(sample_rate); 2];
        self.rear_shade = [hrtf::rear_shade(sample_rate); 2];
        self.walls = [OnePole::default(); 2];
        self.direct.clear();
        self.early.clear();
    }

    /// Replaces the stereo `buffer` with what the listener hears from `target`; the room tail's input (if the channel's
    /// sound reaches the master) gets this channel's share added.
    pub fn process(&mut self, buffer: &mut [f32], target: Spot, acoustics: &Acoustics, mut room: Option<&mut [f32]>) {
        let frames = buffer.len() / 2;
        if frames == 0 {
            return;
        }
        let rate = acoustics.sample_rate;
        self.spot = if self.fresh { target } else { self.spot.approach(target, 1.0 - (-(frames as f32) / (GLIDE_SECONDS * rate)).exp()) };
        let cues = acoustics.shape.cues(self.spot, acoustics.reflectivity);
        let mut next = Mix::of(&cues, acoustics);
        if std::mem::take(&mut self.fresh) {
            self.mix = next;
        }
        next.pinna = self.mix.pinna.toward(next.pinna, frames as f32 / (PINNA_SLEW_SECONDS * rate));
        let start = self.mix;
        for (index, frame) in buffer.chunks_exact_mut(2).enumerate() {
            let mix = start.lerp(&next, (index + 1) as f32 / frames as f32);
            if index % PINNA_FRAMES == 0 {
                self.pinna.follow(mix.pinna, rate);
            }
            let dry = f32::midpoint(frame[0], frame[1]);
            self.early.push(dry);
            let shaped = self.pinna.process(self.air.process(dry, mix.air));
            self.direct.push(shaped);
            let mut ahead = [0.0f32; 2];
            let mut behind = [0.0f32; 2];
            for tap in &mix.taps {
                let sound = [0, 1].map(|ear| self.early.read(tap.delay[ear]) * tap.gain[ear]);
                for (ear, sound) in sound.into_iter().enumerate() {
                    ahead[ear] += sound * (1.0 - tap.rear);
                    behind[ear] += sound * tap.rear;
                }
            }
            for (ear, out) in frame.iter_mut().enumerate() {
                let direct = self.shadow[ear].process(self.direct.read(mix.delay[ear]), mix.shadow[ear]) * mix.gain[ear];
                let reflected = ahead[ear] + self.rear_shade[ear].process(behind[ear]);
                *out = direct + self.walls[ear].process(reflected, acoustics.wall);
            }
            if let Some(room) = room.as_deref_mut() {
                let sound = self.early.read(mix.send_delay);
                room[index * 2] += sound * mix.send[0];
                room[index * 2 + 1] += sound * mix.send[1];
            }
        }
        self.mix = next;
    }
}
