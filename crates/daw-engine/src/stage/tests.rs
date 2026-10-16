//! The stage's parts on their own: the geometry, the direction cue (the head and the outer ear), a voice, the room tail
//! and the voice slots. The stage inside the mixer (where a voice runs, the room on the master) is tested in
//! `crate::tests::stage`.

use std::f32::consts::TAU;

use daw_contracts::{Listening, Room};

use super::geometry::{Direction, RoomShape, Spot, interaural_delay};
use super::hrtf::{Head, Pinna, PinnaCue};
use super::room::RoomTail;
use super::voice::{Acoustics, Voice};
use super::{Stage, acoustics};

const RATE: f32 = 48_000.0;
const SEGMENT: usize = 512;

fn spot(pan: f32, depth: f32, height: f32) -> Spot {
    Spot { pan, depth, height }
}

fn room(listening: Listening, reverb: f32) -> Acoustics {
    acoustics(Room { size: 0.5, reverb, listening }, RATE)
}

/// Runs `segments` segments of a steady mono level `value` through the voice; returns the last segment.
fn steady(voice: &mut Voice, target: Spot, hall: &Acoustics, value: f32, segments: usize) -> Vec<f32> {
    let mut buffer = Vec::new();
    for _ in 0..segments {
        buffer = vec![value; SEGMENT * 2];
        voice.process(&mut buffer, target, hall, None);
    }
    buffer
}

fn left(buffer: &[f32]) -> impl Iterator<Item = f32> + '_ {
    buffer.iter().step_by(2).copied()
}

fn right(buffer: &[f32]) -> impl Iterator<Item = f32> + '_ {
    buffer.iter().skip(1).step_by(2).copied()
}

fn first_loud(samples: impl Iterator<Item = f32>) -> Option<usize> {
    samples.enumerate().find(|(_, s)| s.abs() > 0.01).map(|(i, _)| i)
}

fn db(level: f32, reference: f32) -> f32 {
    20.0 * (level / reference).log10()
}

/// How much of a steady sine of `hz` the outer ear lets through for `cue` (1 is unchanged).
fn pinna_gain(cue: PinnaCue, hz: f32) -> f32 {
    let mut pinna = Pinna::FLAT;
    pinna.follow(cue, RATE);
    let (settle, frames) = (RATE as usize / 10, RATE as usize / 5);
    let (mut input, mut output) = (0.0f32, 0.0f32);
    for frame in 0..frames {
        let sample = (TAU * hz * frame as f32 / RATE).sin();
        let shaped = pinna.process(sample);
        if frame >= settle {
            input += sample * sample;
            output += shaped * shaped;
        }
    }
    (output / input).sqrt()
}

/// The left ear's RMS level of a steady sine of `hz` played by a voice at `target`, once it has settled.
fn sine_level(target: Spot, hall: &Acoustics, hz: f32) -> f32 {
    let mut voice = Voice::new(RATE);
    let mut energy = 0.0f32;
    for segment in 0..24 {
        let mut buffer: Vec<f32> = (0..SEGMENT)
            .flat_map(|frame| {
                let sample = (TAU * hz * (segment * SEGMENT + frame) as f32 / RATE).sin();
                [sample, sample]
            })
            .collect();
        voice.process(&mut buffer, target, hall, None);
        if segment >= 16 {
            energy += left(&buffer).map(|s| s * s).sum::<f32>();
        }
    }
    (energy / (8 * SEGMENT) as f32).sqrt()
}

#[test]
fn the_listener_stands_in_the_middle_and_hears_all_around() {
    let shape = RoomShape::of(0.5);
    let at_head = shape.cues(spot(0.0, 0.0, 0.0), 0.0);
    assert_eq!(at_head.direction, Direction::default(), "in the middle of the head a sound comes from nowhere in particular");
    assert_eq!(at_head.gain, 1.0);

    let ahead = shape.cues(spot(0.0, 0.5, 0.0), 0.0);
    assert!((ahead.direction.front - 1.0).abs() < 1e-5 && ahead.direction.lateral == 0.0 && ahead.direction.elevation == 0.0);
    assert!((ahead.distance - shape.position(spot(0.0, 0.5, 0.0)).y).abs() < 1e-5);
    assert_eq!(ahead.direction.rear(), 0.0);
    let behind = shape.cues(spot(0.0, -0.5, 0.0), 0.0);
    assert!(behind.direction.rear() > 0.999, "depth below 0 is behind the listener: {:?}", behind.direction);
    assert!((behind.distance - ahead.distance).abs() < 1e-5, "as far behind as ahead");

    let right = shape.cues(spot(1.0, 0.0, 0.0), 0.0).direction;
    assert!(right.lateral > 0.999 && right.rear() == 0.0, "the right wall level with the listener is fully right: {right:?}");
    assert!(shape.cues(spot(-1.0, 0.0, 0.0), 0.0).direction.lateral < -0.999);
    assert!(shape.cues(spot(0.0, 0.2, 1.0), 0.0).direction.elevation > 0.3, "above");
    assert!(shape.cues(spot(0.0, 0.2, -1.0), 0.0).direction.elevation < -0.3, "below");
}

#[test]
fn distance_lowers_the_level_and_takes_the_highs() {
    let shape = RoomShape::of(0.5);
    let near = shape.cues(spot(0.0, 0.1, 0.0), 0.0);
    let far = shape.cues(spot(0.0, 1.0, 0.0), 0.0);
    assert_eq!(near.gain, 1.0, "within the reference distance the direct sound keeps its level");
    assert!(far.gain < 0.45, "the wall ahead in a mid-size room, 5 m away, is about 8 dB down: {}", far.gain);
    assert!(far.air_hz < near.air_hz * 0.8);
    assert_eq!(shape.cues(spot(0.0, -1.0, 0.0), 0.0).gain, far.gain, "the wall behind is as far");
    let hall = RoomShape::of(1.0).cues(spot(0.0, 1.0, 0.0), 0.0);
    assert!(hall.gain < far.gain, "the far wall of a hall is farther still");
}

#[test]
fn reflections_come_after_the_direct_sound_and_later_in_a_bigger_room() {
    let mean_delay = |size: f32| {
        let cues = RoomShape::of(size).cues(spot(0.2, 0.3, 0.0), 0.8);
        assert!(cues.reflections.iter().all(|r| r.delay > 0.0 && r.gain > 0.0 && r.gain < cues.gain));
        cues.reflections.iter().map(|r| r.delay).sum::<f32>()
    };
    assert!(mean_delay(1.0) > 2.0 * mean_delay(0.0));
    let dry = RoomShape::of(0.5).cues(spot(0.0, 0.3, 0.0), 0.0);
    assert!(dry.reflections.iter().all(|r| r.gain == 0.0), "no reflectivity, no reflections");
}

#[test]
fn a_source_behind_is_heard_in_reflections_from_behind() {
    let from_behind = |depth: f32| {
        let cues = RoomShape::of(0.5).cues(spot(0.3, depth, 0.0), 0.8);
        cues.reflections.iter().filter(|r| r.direction.front < 0.0).count()
    };
    assert_eq!(from_behind(-0.6), 5, "the side walls, the floor, the ceiling and the wall behind: all but the wall ahead");
    assert_eq!(from_behind(0.6), 1, "ahead, only the wall behind the listener sends it back from behind");
    let cues = RoomShape::of(0.5).cues(spot(0.3, -0.6, 0.0), 0.8);
    let straightest = cues.reflections.iter().map(|r| r.direction.front).fold(1.0, f32::min);
    assert!(straightest < -0.9, "the wall behind sends it back from almost straight behind: {straightest}");
}

#[test]
fn the_head_hears_ahead_and_behind_alike_so_the_outer_ear_tells_them_apart() {
    let ahead = Direction { lateral: 0.6, front: 0.8, elevation: 0.0 };
    let behind = Direction { front: -0.8, ..ahead };
    let head = Head::direct(ahead, Listening::Headphones, RATE);
    assert_eq!(head, Head::direct(behind, Listening::Headphones, RATE), "the same delay, level and shadow (the cone of confusion)");
    assert!(head.delay[0] > 10.0 && head.delay[1] == 0.0, "the left ear, turned away, hears it later: {:?}", head.delay);
    assert!(head.gain[0] < head.gain[1] && head.shadow[0] < 1.0 && head.shadow[1] > 1.0, "and quieter and duller");

    assert_eq!(PinnaCue::of(ahead, Listening::Headphones).rear, 0.0);
    assert!((PinnaCue::of(behind, Listening::Headphones).rear - 0.8).abs() < 1e-6);
    assert_eq!(PinnaCue::of(behind, Listening::Speakers), PinnaCue::of(ahead, Listening::Speakers), "speakers have no rear cue");

    let reflection = Head::reflection(behind, Listening::Headphones, RATE);
    assert_eq!(reflection.delay, head.delay, "a reflection reaches the far ear as much later");
    assert!(reflection.gain[0] < head.gain[0] && reflection.shadow == [1.0; 2], "its shadow is in its level");
}

#[test]
fn the_outer_ear_is_flat_ahead_darker_behind_and_moves_its_notch_with_height() {
    let ahead = PinnaCue { rear: 0.0, elevation: 0.0 };
    for hz in [200.0, 1_100.0, 4_620.0, 7_500.0, 12_180.0] {
        assert!((pinna_gain(ahead, hz) - 1.0).abs() < 1e-3, "straight ahead is unchanged at {hz} Hz");
    }
    let behind = PinnaCue { rear: 1.0, elevation: 0.0 };
    assert!(db(pinna_gain(behind, 6_000.0), 1.0) < -8.0, "behind loses the presence and the air");
    assert!(db(pinna_gain(behind, 7_500.0), 1.0) < db(pinna_gain(behind, 12_180.0), 1.0) - 3.0, "with a notch at 7.5 kHz");
    assert!(db(pinna_gain(behind, 1_100.0), 1.0) > 1.5, "and gains a little body around 1 kHz");
    assert!(db(pinna_gain(behind, 200.0), 1.0).abs() < 0.5, "the lows stay");

    // The notch: about 12 kHz straight above, about 4.6 kHz straight below.
    let above = PinnaCue { rear: 0.0, elevation: 1.0 };
    let below = PinnaCue { rear: 0.0, elevation: -1.0 };
    assert!(pinna_gain(above, 12_180.0) < pinna_gain(above, 4_620.0) * 0.7);
    assert!(pinna_gain(below, 4_620.0) < pinna_gain(below, 12_180.0) * 0.7);
}

#[test]
fn headphones_tell_behind_from_ahead_and_speakers_cannot() {
    let (ahead, behind) = (spot(0.0, 0.5, 0.0), spot(0.0, -0.5, 0.0));
    let phones = room(Listening::Headphones, 0.0);
    let highs = db(sine_level(behind, &phones, 6_000.0), sine_level(ahead, &phones, 6_000.0));
    assert!(highs < -6.0, "on headphones behind is darker: {highs} dB at 6 kHz");
    let lows = db(sine_level(behind, &phones, 200.0), sine_level(ahead, &phones, 200.0));
    assert!(lows.abs() < 1.0, "and keeps its lows: {lows} dB at 200 Hz");

    let speakers = room(Listening::Speakers, 0.0);
    let same = db(sine_level(behind, &speakers, 6_000.0), sine_level(ahead, &speakers, 6_000.0));
    assert!(same.abs() < 0.1, "two speakers in front play behind like ahead: {same} dB");
}

#[test]
fn the_far_ear_hears_up_to_two_thirds_of_a_millisecond_later() {
    assert_eq!(interaural_delay(0.0), 0.0);
    let side = interaural_delay(1.0);
    assert!((side - 0.000_656).abs() < 0.000_01, "{side}");
    assert!(interaural_delay(0.5) < side && interaural_delay(-0.5) == interaural_delay(0.5));
}

#[test]
fn a_source_straight_ahead_at_ear_height_keeps_its_level() {
    for listening in [Listening::Speakers, Listening::Headphones] {
        let mut voice = Voice::new(RATE);
        let out = steady(&mut voice, spot(0.0, 0.2, 0.0), &room(listening, 0.0), 0.5, 20);
        assert!(left(&out).chain(right(&out)).all(|s| (s - 0.5).abs() < 1e-3), "{listening:?}: {:?}", &out[..4]);
    }
}

#[test]
fn a_source_on_the_right_sounds_on_the_right_and_far_is_quieter() {
    let hall = room(Listening::Speakers, 0.0);
    let mut voice = Voice::new(RATE);
    let out = steady(&mut voice, spot(1.0, 0.0, 0.0), &hall, 0.5, 20);
    let (l, r) = (out[SEGMENT * 2 - 2], out[SEGMENT * 2 - 1]);
    // The right wall of a mid-size room is about 6 m away: quieter, and all on the right.
    assert!(r > 0.1 && l < 0.01, "equal power, hard right: {l} {r}");

    let mut near = Voice::new(RATE);
    let mut far = Voice::new(RATE);
    let near = steady(&mut near, spot(0.0, 0.2, 0.0), &hall, 0.5, 20)[0];
    let far = steady(&mut far, spot(0.0, 1.0, 0.0), &hall, 0.5, 20)[0];
    assert!(far < near * 0.5, "{far} vs {near}");
}

#[test]
fn headphones_bring_a_sound_from_the_right_to_the_left_ear_later() {
    let mut voice = Voice::new(RATE);
    let hall = room(Listening::Headphones, 0.0);
    steady(&mut voice, spot(1.0, 0.0, 0.0), &hall, 0.0, 1);
    let mut click = [0.0; SEGMENT * 2];
    click[0] = 1.0;
    click[1] = 1.0;
    voice.process(&mut click, spot(1.0, 0.0, 0.0), &hall, None);
    let (l, r) = (first_loud(left(&click)).unwrap(), first_loud(right(&click)).unwrap());
    assert_eq!(r, 0, "the near ear hears it at once");
    assert!((25..=33).contains(&l), "the far ear about 0.6 ms later: {l} frames");
}

#[test]
fn a_dragged_source_glides_without_jumps() {
    let hall = room(Listening::Headphones, 0.6);
    let mut voice = Voice::new(RATE);
    steady(&mut voice, spot(-1.0, 0.0, 0.0), &hall, 0.5, 10);
    let mut previous: Option<[f32; 2]> = None;
    let mut biggest = 0.0f32;
    for segment in 0..40 {
        let mut buffer = [0.5; SEGMENT * 2];
        // The target jumps from corner to corner, ahead up high and then through the listener's head to behind down
        // low: what is heard must still move smoothly (the outer ear's tone turns from ahead to behind on the way).
        let target = if segment < 20 { spot(1.0, 1.0, 1.0) } else { spot(-1.0, -1.0, -1.0) };
        voice.process(&mut buffer, target, &hall, None);
        for frame in buffer.chunks_exact(2) {
            if let Some([l, r]) = previous {
                biggest = biggest.max((frame[0] - l).abs()).max((frame[1] - r).abs());
            }
            previous = Some([frame[0], frame[1]]);
        }
    }
    assert!(biggest < 0.02, "the biggest step between two samples: {biggest}");
}

#[test]
fn reverb_adds_reflections_and_feeds_the_room_from_the_first_one() {
    let hall = room(Listening::Speakers, 1.0);
    let mut voice = Voice::new(RATE);
    steady(&mut voice, spot(0.0, 0.3, 0.0), &hall, 0.0, 1);
    let mut segments = Vec::new();
    let mut input = Vec::new();
    for index in 0..20 {
        let mut buffer = [0.0; SEGMENT * 2];
        if index == 0 {
            buffer[0] = 1.0;
            buffer[1] = 1.0;
        }
        let mut room_input = [0.0; SEGMENT * 2];
        voice.process(&mut buffer, spot(0.0, 0.3, 0.0), &hall, Some(&mut room_input));
        segments.extend(buffer);
        input.extend(room_input);
    }
    let after_direct = segments[400..].iter().map(|s| s.abs()).fold(0.0, f32::max);
    assert!(after_direct > 0.01, "reflections come later: {after_direct}");
    let first_input = first_loud(input.iter().copied()).unwrap() / 2;
    let first_reflection = hall.shape.cues(spot(0.0, 0.3, 0.0), hall.reflectivity).reflections.iter().map(|r| r.delay).fold(1.0, f32::min);
    assert!(((first_input as f32 / RATE) - first_reflection).abs() < 0.001, "{first_input} frames vs {first_reflection} s");
}

#[test]
fn the_room_tail_rings_for_its_time_and_then_sleeps() {
    let shape = RoomShape::of(0.5);
    let mut tail = RoomTail::new(RATE, &shape);
    let mut silence = [0.0; SEGMENT * 2];
    tail.process(&[0.0; SEGMENT * 2], false, &mut silence);
    assert!(silence.iter().all(|s| *s == 0.0), "a tail nobody fed is silent");

    let mut out = Vec::new();
    let total = (shape.ring_seconds * 2.0 * RATE) as usize / SEGMENT;
    for index in 0..total {
        let mut input = [0.0; SEGMENT * 2];
        input[0] = f32::from(u8::from(index == 0));
        let mut buffer = [0.0; SEGMENT * 2];
        tail.process(&input, index == 0, &mut buffer);
        out.extend(buffer);
    }
    let energy = |from: f32, to: f32| {
        let (from, to) = ((from * RATE) as usize * 2, (to * RATE) as usize * 2);
        out[from..to].iter().map(|s| s * s).sum::<f32>()
    };
    let early = energy(0.05, 0.15);
    let late = energy(shape.ring_seconds, shape.ring_seconds + 0.1);
    assert!(early > 0.0);
    assert!(10.0 * (late / early).log10() < -40.0, "60 dB down over the ring time (the walls take the highs faster)");

    let mut after = [0.25; SEGMENT * 2];
    tail.process(&[0.0; SEGMENT * 2], false, &mut after);
    assert!(after.iter().all(|s| *s == 0.25), "a tail that has rung out sleeps and leaves the buffer alone");
}

#[test]
fn voices_stay_with_their_channels_and_pass_on_when_they_leave() {
    let mut stage = Stage::new(RATE);
    let first = stage.bind(1).unwrap();
    let second = stage.bind(2).unwrap();
    assert_ne!(first, second);
    assert_eq!(stage.bind(1), Some(first), "the same channel keeps its voice");

    // Channel 1 leaves and 3 comes in the same snapshot: 3 may take 1's voice, 2 keeps its own.
    stage.keep(2);
    stage.release_others();
    assert_eq!(stage.bind(2), Some(second));
    assert_eq!(stage.bind(3), Some(first));
}
