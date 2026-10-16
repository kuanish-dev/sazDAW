//! The virtual stage in the mixer: a channel on the stage plays from its place instead of its pan, its voice runs after
//! its inserts, the shared room rings on the master, a channel routed nowhere stays silent in the room, and automation
//! moves a channel through the room.

use daw_contracts::{AutomationClip, Effect, Listening, MAX_CLIP_LOOP_TICKS, Room, StagePlace};

use super::*;

/// An effect: silences the left channel.
struct MuteLeft;

impl Effect for MuteLeft {
    fn prepare(&mut self, _: StreamConfig) {}
    fn set_param(&mut self, _: &str, _: f32) {}
    fn reset(&mut self) {}
    fn process(&mut self, buffer: &mut [f32], _: Option<&[f32]>) {
        for frame in buffer.chunks_exact_mut(2) {
            frame[0] = 0.0;
        }
    }
}

fn place(rig: &mut Rig, node: NodeId, pan: f32, depth: f32, height: f32) {
    let strip = rig.node_kind(node).strip_mut().expect("a channel");
    strip.pan = pan;
    strip.stage = Some(StagePlace { depth, height });
}

/// A dry room (no reflections, no tail) heard on speakers, so the numbers are the direct sound alone.
fn dry_room(rig: &mut Rig) {
    rig.project.room = Room { size: 0.5, reverb: 0.0, listening: Listening::Speakers };
}

fn peak(buffer: &[f32], channel: usize, frames: std::ops::Range<usize>) -> f32 {
    frames.map(|frame| buffer[frame * 2 + channel].abs()).fold(0.0, f32::max)
}

#[test]
fn a_channel_on_the_stage_plays_from_its_place_and_off_it_pans_as_before() {
    let mut rig = Rig::new();
    let source = rig.add_track(10, MASTER);
    rig.note(10, 60, 0, 960);
    dry_room(&mut rig);
    place(&mut rig, source, -1.0, 0.0, 0.0);
    rig.publish();
    rig.transport.play();
    let out = rig.render(4_000);
    // The left wall of a mid-size room is about 6 m away: quieter than the track's own 0.5, and all on the left.
    assert!(Rig::left(&out, 3_000) > 0.15, "hard left, equal power: {}", Rig::left(&out, 3_000));
    assert!(Rig::right(&out, 3_000) < 0.01);

    // At the wall behind the listener, about 5 m away: two speakers play it in the middle, like the wall ahead.
    place(&mut rig, source, 0.0, -1.0, 0.0);
    rig.publish();
    rig.render(20_000);
    let far = rig.render(1_000);
    assert!(Rig::left(&far, 500) < 0.25 && near(Rig::left(&far, 500), Rig::right(&far, 500)), "far behind, in the middle");

    // Off the stage: the balance pan of the strip again.
    rig.node_kind(source).strip_mut().unwrap().stage = None;
    rig.node_kind(source).strip_mut().unwrap().pan = 1.0;
    rig.publish();
    rig.render(2_000);
    let out = rig.render(1_000);
    assert!(near(Rig::left(&out, 500), 0.0) && near(Rig::right(&out, 500), 0.5));
}

#[test]
fn the_stage_places_a_channel_after_its_inserts() {
    let mut rig = Rig::new();
    let source = rig.add_track(10, MASTER);
    rig.project.graph.edges.clear();
    let insert = rig.add_fx(false, Box::new(MuteLeft));
    rig.connect(source, insert, 1.0, false);
    rig.connect(insert, MASTER, 1.0, false);
    rig.note(10, 60, 0, 960);
    dry_room(&mut rig);
    // On the left: if the stage ran before the insert, the insert would silence it entirely.
    place(&mut rig, source, -1.0, 0.0, 0.0);
    rig.publish();
    rig.transport.play();
    let out = rig.render(4_000);
    assert!(Rig::left(&out, 3_000) > 0.05, "the insert's sound is placed on the left: {}", Rig::left(&out, 3_000));
}

#[test]
fn the_shared_room_rings_on_after_the_sound_stops() {
    let tail_after_the_note = |reverb: f32| {
        let mut rig = Rig::new();
        let source = rig.add_track(10, MASTER);
        rig.note(10, 60, 0, 960);
        rig.project.room = Room { size: 0.8, reverb, listening: Listening::Headphones };
        place(&mut rig, source, 0.3, 0.5, 0.2);
        rig.publish();
        rig.transport.play();
        // The note sounds for 960 ticks = 24 000 frames; 0.3 s after it ends:
        let out = rig.render(40_000);
        peak(&out, 0, 38_000..40_000)
    };
    assert!(tail_after_the_note(1.0) > 1e-3, "a live room rings on");
    assert!(tail_after_the_note(0.0) < 1e-4, "an open-air stage does not");
}

#[test]
fn a_channel_routed_nowhere_stays_silent_in_the_room() {
    let mut rig = Rig::new();
    let nowhere = rig.add_node(NodeKind::Bus { strip: Strip::default() });
    let source = rig.add_track(10, nowhere);
    rig.note(10, 60, 0, 960);
    rig.project.room = Room { size: 0.5, reverb: 1.0, listening: Listening::Speakers };
    place(&mut rig, source, 0.0, 0.5, 0.0);
    rig.publish();
    rig.transport.play();
    let out = rig.render(20_000);
    assert_eq!(peak(&out, 0, 0..20_000), 0.0);
}

#[test]
fn automation_moves_a_channel_through_the_room() {
    let level_at_depth = |depth: Option<f32>| {
        let mut rig = Rig::new();
        let source = rig.add_track(10, MASTER);
        rig.note(10, 60, 0, 960);
        dry_room(&mut rig);
        place(&mut rig, source, 0.0, 0.0, 0.0);
        if let Some(depth) = depth {
            rig.project.automation.push(AutomationLane {
                id: 900,
                target: ParamTarget::StageDepth { node: source },
                clips: vec![AutomationClip::once(1_000, 0, MAX_CLIP_LOOP_TICKS, vec![AutoPoint::new(0, depth)])],
            });
        }
        rig.publish();
        rig.transport.play();
        let out = rig.render(20_000);
        Rig::left(&out, 19_000)
    };
    let at_head = level_at_depth(None);
    let back = level_at_depth(Some(-1.0));
    assert!(near(at_head, 0.5), "{at_head}");
    assert!(back < at_head * 0.5, "the lane takes it to the wall behind: {back}");
}
