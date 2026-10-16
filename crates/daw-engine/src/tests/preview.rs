//! The preview voice: plays past the transport, in a loop, together with the timeline; starting the transport stops it.

use super::*;

#[test]
fn preview_plays_over_a_stopped_transport_and_reports_its_position() {
    let mut rig = Rig::new();
    rig.publish();
    assert_eq!(rig.link.preview_position(), None);
    rig.link.send(DspCommand::PreviewPlay { data: ramp_sample(10_000), from: 2_000, to: 6_000, looping: false }).unwrap();
    let out = rig.render(1_000);
    assert!(Rig::left(&out, 10) > 0.15, "audible: {}", Rig::left(&out, 10));
    let position = rig.link.preview_position().expect("playing");
    assert!((2_900..3_100).contains(&position), "position in file frames: {position}");

    // Play the region to the end: the voice goes silent and the position disappears.
    let out = rig.render(8_000);
    assert_eq!(rig.link.preview_position(), None);
    assert_eq!(Rig::left(&out, 7_900), 0.0);
}

#[test]
fn preview_loops_until_stopped_and_stop_is_immediate() {
    let mut rig = Rig::new();
    rig.publish();
    rig.link.send(DspCommand::PreviewPlay { data: ramp_sample(10_000), from: 1_000, to: 2_000, looping: true }).unwrap();
    let out = rig.render(20_000);
    assert!(Rig::left(&out, 19_900) > 0.05, "still playing");
    assert!(rig.link.preview_position().is_some());

    rig.link.send(DspCommand::PreviewStop).unwrap();
    let out = rig.render(512);
    assert!(out.iter().all(|v| *v == 0.0));
    assert_eq!(rig.link.preview_position(), None);
}

#[test]
fn preview_is_mixed_with_the_timeline_and_stays_within_limits() {
    let mut rig = Rig::new();
    rig.add_track(10, MASTER);
    rig.note(10, 60, 0, 960);
    rig.publish();
    rig.transport.play();
    rig.render(512);
    rig.link
        .send(DspCommand::PreviewPlay {
            data: Arc::new(SampleData { sample_rate: SR, channels: 1, frames: vec![1.0; 5_000] }),
            from: 0,
            to: 5_000,
            looping: false,
        })
        .unwrap();
    let out = rig.render(2_000);
    assert!(out.iter().all(|v| v.is_finite() && v.abs() <= 1.0), "the master is protected from overload");
    assert!(Rig::left(&out, 1_000) > 0.9, "the timeline and the preview add up");
}

#[test]
fn starting_playback_silences_a_running_preview_but_a_later_preview_still_plays() {
    let mut rig = Rig::new();
    rig.add_track(10, MASTER);
    rig.publish();
    let long = Arc::new(SampleData { sample_rate: SR, channels: 1, frames: vec![0.5; 200_000] });
    rig.link.send(DspCommand::PreviewPlay { data: long.clone(), from: 0, to: 200_000, looping: true }).unwrap();
    rig.render(1_024);
    assert!(rig.link.preview_position().is_some(), "the preview is playing");

    rig.transport.play();
    let out = rig.render(1_024);
    assert_eq!(rig.link.preview_position(), None, "starting the transport turned the preview off");
    assert!(out.iter().all(|v| *v == 0.0), "nothing sounds: the timeline has no clips");

    // Previewing during playback is allowed: only the moment of starting stops it.
    rig.link.send(DspCommand::PreviewPlay { data: long, from: 0, to: 200_000, looping: false }).unwrap();
    rig.render(512);
    assert!(rig.link.preview_position().is_some());
}
