//! The channel utility: polarity, width and mono, fine time alignment (later is a plain delay, earlier is reported to
//! delay compensation), width automation, and changes that glide instead of clicking.

use daw_contracts::{AutomationClip, MAX_CLIP_LOOP_TICKS, Polarity};

use super::latency::first_sound;
use super::*;

/// 1 ms at the test rate.
const MS: usize = 48;

fn utility(rig: &mut Rig, node: NodeId, change: impl FnOnce(&mut Strip)) {
    change(rig.node_kind(node).strip_mut().expect("a channel"));
}

/// The largest step between neighbouring frames of the left side: a click shows up as a jump.
fn largest_step(out: &[f32]) -> f32 {
    out.chunks_exact(2).zip(out.chunks_exact(2).skip(1)).map(|(a, b)| (b[0] - a[0]).abs()).fold(0.0, f32::max)
}

/// A track hard left into a bus: the bus hears 0.5 on the left and nothing on the right.
fn left_only_into_bus(rig: &mut Rig) -> NodeId {
    let bus = rig.add_bus(1.0);
    let track = rig.add_track(10, bus);
    rig.set_strip(track, 1.0, -1.0, false, false);
    rig.note(10, 60, 0, 960);
    bus
}

#[test]
fn inverting_a_side_turns_it_upside_down() {
    let mut rig = Rig::new();
    let node = rig.add_track(10, MASTER);
    rig.note(10, 60, 0, 960);
    utility(&mut rig, node, |strip| strip.polarity = Polarity { invert_left: true, invert_right: false });
    rig.publish();
    rig.transport.play();
    let out = rig.render(1_000);
    assert!(near(Rig::left(&out, 500), -0.5), "{}", Rig::left(&out, 500));
    assert!(near(Rig::right(&out, 500), 0.5), "the other side stays");
}

#[test]
fn width_scales_the_side_and_mono_sums_both_sides() {
    let mut rig = Rig::new();
    let bus = left_only_into_bus(&mut rig);
    utility(&mut rig, bus, |strip| strip.width = 2.0);
    rig.publish();
    rig.transport.play();
    let out = rig.render(1_000);
    assert!(near(Rig::left(&out, 500), 0.75) && near(Rig::right(&out, 500), -0.25), "{out:?}");

    utility(&mut rig, bus, |strip| strip.mono = true);
    rig.publish();
    let out = rig.render(2_000);
    assert!(near(Rig::left(&out, 1_500), 0.25) && near(Rig::right(&out, 1_500), 0.25), "mono whatever the width");

    utility(&mut rig, bus, |strip| {
        strip.mono = false;
        strip.width = 0.0;
    });
    rig.publish();
    let out = rig.render(2_000);
    assert!(near(Rig::left(&out, 1_500), 0.25) && near(Rig::right(&out, 1_500), 0.25), "width 0 is mono too");
}

#[test]
fn width_automation_overrides_the_channel_for_the_segment() {
    let mut rig = Rig::new();
    let bus = left_only_into_bus(&mut rig);
    rig.project.automation.push(AutomationLane {
        id: 900,
        target: ParamTarget::NodeWidth { node: bus },
        clips: vec![AutomationClip::once(1_000, 0, MAX_CLIP_LOOP_TICKS, vec![AutoPoint::new(0, 0.0)])],
    });
    rig.publish();
    rig.transport.play();
    let out = rig.render(2_000);
    assert!(near(Rig::left(&out, 1_500), 0.25) && near(Rig::right(&out, 1_500), 0.25), "the lane plays the bus mono");
}

#[test]
fn a_channel_set_later_is_a_plain_delay() {
    let mut rig = Rig::new();
    let node = rig.add_track(10, MASTER);
    rig.note(10, 60, 0, 960);
    utility(&mut rig, node, |strip| strip.delay_ms = 1.0);
    rig.publish();
    rig.transport.play();
    let out = rig.render(1_000);
    assert_eq!(first_sound(&out), Some(MS));
    assert!(near(Rig::left(&out, MS), 0.5));
    assert_eq!(rig.link.latency().frames, 0, "a delay holds nothing else back");
}

#[test]
fn a_channel_set_earlier_leads_the_mix_through_delay_compensation() {
    let mut rig = Rig::new();
    let early = rig.add_track(10, MASTER);
    rig.add_track(11, MASTER);
    rig.note(10, 60, 0, 960);
    rig.note(11, 62, 0, 960);
    utility(&mut rig, early, |strip| strip.delay_ms = -1.0);
    rig.publish();
    rig.transport.play();
    let out = rig.render(1_000);
    assert_eq!(rig.link.latency().frames, MS as u32, "the lead is the channel's latency");
    assert_eq!(first_sound(&out), Some(0));
    assert!(near(Rig::left(&out, MS - 1), 0.5), "only the earlier channel at first");
    assert!(near(Rig::left(&out, MS), 1.0), "the other one joins {MS} frames later");
}

#[test]
fn a_polarity_flip_fades_through_silence() {
    let mut rig = Rig::new();
    let node = rig.add_track(10, MASTER);
    rig.note(10, 60, 0, 960);
    rig.publish();
    rig.transport.play();
    rig.render(1_024);
    utility(&mut rig, node, |strip| strip.polarity = Polarity { invert_left: true, invert_right: true });
    rig.publish();
    let out = rig.render(2_048);
    assert!(largest_step(&out) < 0.01, "no click: {}", largest_step(&out));
    assert!(near(Rig::left(&out, 2_000), -0.5));
}

/// An audio track playing a rising ramp: any jump in time shows up as a step far larger than the ramp's own.
fn ramp_track(rig: &mut Rig) -> NodeId {
    let node = rig.add_audio_track(20, MASTER);
    rig.add_sample(300, ramp_sample(48_000));
    rig.add_clip(20, 0, 1_920, ClipContent::audio(300));
    node
}

#[test]
fn a_new_delay_crossfades_from_the_old_one() {
    let mut rig = Rig::new();
    let node = ramp_track(&mut rig);
    rig.publish();
    rig.transport.play();
    rig.render(4_096);
    utility(&mut rig, node, |strip| strip.delay_ms = 2.0);
    rig.publish();
    let out = rig.render(4_096);
    assert!(largest_step(&out) < 1e-4, "no jump: {}", largest_step(&out));
}

#[test]
fn nudging_a_channel_earlier_moves_the_rest_of_the_mix_without_a_jump() {
    let mut rig = Rig::new();
    let early = rig.add_track(10, MASTER);
    ramp_track(&mut rig);
    utility(&mut rig, early, |strip| strip.delay_ms = -1.0);
    rig.publish();
    rig.transport.play();
    rig.render(4_096);
    assert_eq!(rig.link.latency().frames, MS as u32);
    utility(&mut rig, early, |strip| strip.delay_ms = -2.0);
    rig.publish();
    let out = rig.render(4_096);
    assert_eq!(rig.link.latency().frames, 2 * MS as u32);
    assert!(largest_step(&out) < 1e-4, "the compensation line glides to its new delay: {}", largest_step(&out));
}
