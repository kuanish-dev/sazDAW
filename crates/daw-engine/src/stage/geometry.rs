//! The stage geometry: the room's measures, where a point of the stage is relative to the listener's head (the listener
//! stands in the middle of the room, ears halfway up, facing ahead), and what the listener hears from it — the direct
//! sound's distance, direction and level, and the first reflections off the walls (the method of images, first order).
//! Pure math, no state. The stage view draws the room and its distance readout with the same numbers
//! (`app/src/stage/stageRules.ts`: `roomShape`, `position`, `directGain`): change both together.

use daw_contracts::StagePlace;

/// Speed of sound in air, m/s.
pub const SPEED_OF_SOUND: f32 = 343.0;
/// The radius of an average head, m: it sets how much later the far ear hears a sound from the side.
pub const HEAD_RADIUS: f32 = 0.0875;
/// The direct sound keeps its own level up to this distance and falls 6 dB per doubling beyond it (the inverse
/// distance law), m. Nothing gets louder than its own level: a source at the listener's nose would otherwise clip.
pub const REFERENCE_DISTANCE: f32 = 2.0;
/// Air and distance take the highs: the air lowpass is at 20 kHz up close and halves every this many metres beyond the
/// reference distance (real air takes less; the exaggeration stands in for a real hall's diffusion of a far source).
const AIR_DISTANCE: f32 = 8.0;
const AIR_HZ: f32 = 20_000.0;
/// The closest a source counts as to the listener's head, m: nearer, it keeps the level it has here, and its direction
/// fades into the middle of the head, so a source moved through the listener passes smoothly from ahead to behind.
const NEAREST: f32 = 0.5;
/// The closest a source comes to a wall, the floor or the ceiling, m.
const WALL_GAP: f32 = 0.5;
/// First reflections per source: the left and right walls, the walls ahead of and behind the listener, the floor and
/// the ceiling. A source behind the listener is heard mostly in reflections from behind.
pub const REFLECTIONS: usize = 6;

/// A point of the stage in the project's units, each 0 at the listener's head: `pan` −1…1 (wall to wall), `depth` −1…1
/// (the wall behind the listener to the wall ahead), `height` −1…1 (floor to ceiling).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Spot {
    pub pan: f32,
    pub depth: f32,
    pub height: f32,
}

impl Spot {
    pub fn new(pan: f32, place: StagePlace) -> Self {
        Self { pan, depth: place.depth, height: place.height }
    }

    /// `share` (0…1) of the way from here to `target`: a dragged point glides through the room instead of jumping.
    pub fn approach(self, target: Spot, share: f32) -> Spot {
        let step = |from: f32, to: f32| from + (to - from) * share;
        Spot { pan: step(self.pan, target.pan), depth: step(self.depth, target.depth), height: step(self.height, target.height) }
    }
}

/// A point relative to the listener's head, m: `x` to the right, `y` forward, `z` up.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Position {
    pub x: f32,
    pub y: f32,
    pub z: f32,
}

impl Position {
    pub fn distance(self) -> f32 {
        (self.x * self.x + self.y * self.y + self.z * self.z).sqrt()
    }
}

/// Where a sound comes from, seen from the listener's head: the direction's components along the three axes (a unit
/// vector; shorter for a source inside the head, so its cues fade to none in the middle of it).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Direction {
    /// Along the axis between the ears: −1 fully left … 1 fully right (the sine of the angle off the middle).
    pub lateral: f32,
    /// Along the nose: −1 straight behind … 1 straight ahead.
    pub front: f32,
    /// −1 straight down … 1 straight up (the sine of the angle above ear height).
    pub elevation: f32,
}

impl Direction {
    /// The direction of `position` at `distance` (at least the position's own length).
    fn of(position: Position, distance: f32) -> Self {
        Self { lateral: position.x / distance, front: position.y / distance, elevation: position.z / distance }
    }

    /// How far the sound comes from behind the line between the ears: 0 at the side or ahead … 1 straight behind.
    pub fn rear(self) -> f32 {
        (-self.front).max(0.0)
    }
}

/// One reflection off a wall: how long after the direct sound it comes, how loud it is and from where.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Reflection {
    /// Seconds after the direct sound.
    pub delay: f32,
    pub gain: f32,
    pub direction: Direction,
}

/// What the listener hears from a point of the stage.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Cues {
    /// From the head to the source, m.
    pub distance: f32,
    /// The direct sound's level (see [`direct_gain`]).
    pub gain: f32,
    pub direction: Direction,
    /// The corner of the air absorption lowpass, Hz.
    pub air_hz: f32,
    pub reflections: [Reflection; REFLECTIONS],
}

/// The room's measures for a `Room::size` (0…1). A room grows by ratios, so the measures are geometric between a small
/// studio room and a concert hall.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct RoomShape {
    pub width: f32,
    /// From the wall behind the listener to the wall ahead (the listener stands halfway).
    pub depth: f32,
    pub height: f32,
    /// How long the diffuse tail takes to fall by 60 dB, s.
    pub ring_seconds: f32,
    /// The walls absorb highs: the corner of the reflections' and the tail's lowpass, Hz (darker in a big hall).
    pub wall_hz: f32,
}

fn grow(small: f32, large: f32, size: f32) -> f32 {
    small * (large / small).powf(size.clamp(0.0, 1.0))
}

/// The direct sound's level at `distance`: 1 up to the reference distance, then the inverse distance law.
pub fn direct_gain(distance: f32) -> f32 {
    (REFERENCE_DISTANCE / distance).min(1.0)
}

/// How much later the far ear hears a sound whose direction has `lateral` (the sine of its angle off the middle), s:
/// Woodworth's spherical head, (a / c)(θ + sin θ), at most ≈ 0.66 ms for a sound from the side.
pub fn interaural_delay(lateral: f32) -> f32 {
    let side = lateral.abs().min(1.0);
    HEAD_RADIUS / SPEED_OF_SOUND * (side.asin() + side)
}

impl RoomShape {
    pub fn of(size: f32) -> Self {
        Self {
            width: grow(6.0, 30.0, size),
            depth: grow(5.0, 24.0, size),
            height: grow(3.0, 10.0, size),
            ring_seconds: grow(0.35, 2.8, size),
            wall_hz: grow(9_000.0, 4_500.0, size),
        }
    }

    /// Where a stage point is relative to the listener's head, who stands in the middle of the room, ears halfway up.
    pub fn position(&self, spot: Spot) -> Position {
        Position {
            x: spot.pan.clamp(-1.0, 1.0) * (self.width / 2.0 - WALL_GAP),
            y: spot.depth.clamp(-1.0, 1.0) * (self.depth / 2.0 - WALL_GAP),
            z: spot.height.clamp(-1.0, 1.0) * (self.height / 2.0 - WALL_GAP),
        }
    }

    /// What the listener hears from `spot`; `reflectivity` (0…1) is how much of the sound a wall sends back.
    pub fn cues(&self, spot: Spot, reflectivity: f32) -> Cues {
        let source = self.position(spot);
        let distance = source.distance().max(NEAREST);
        let images = [
            Position { x: -self.width - source.x, ..source },
            Position { x: self.width - source.x, ..source },
            Position { y: self.depth - source.y, ..source },
            Position { y: -self.depth - source.y, ..source },
            Position { z: -self.height - source.z, ..source },
            Position { z: self.height - source.z, ..source },
        ];
        let reflections = images.map(|image| {
            let path = image.distance();
            Reflection {
                delay: (path - distance).max(0.0) / SPEED_OF_SOUND,
                gain: reflectivity * direct_gain(path),
                direction: Direction::of(image, path),
            }
        });
        Cues {
            distance,
            gain: direct_gain(distance),
            direction: Direction::of(source, distance),
            air_hz: AIR_HZ / (1.0 + (distance - REFERENCE_DISTANCE).max(0.0) / AIR_DISTANCE),
            reflections,
        }
    }
}
