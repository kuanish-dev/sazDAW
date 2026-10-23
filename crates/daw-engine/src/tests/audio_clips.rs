//! Audio clips: voices through the `Audio` node, fades, entering mid-clip, the sound is freed outside the audio thread.

use super::*;
use daw_contracts::FadeShape;

#[test]
fn audio_clips_play_through_the_audio_node_with_fades() {
    let mut rig = Rig::new();
    rig.add_audio_track(20, MASTER);
    rig.add_sample(300, constant_sample(5_000, 0.5));
    let clip = rig.add_clip(20, 960, 960, ClipContent::audio(300));
    rig.clip_mut(clip).fade_in_ticks = 40;
    rig.publish();
    rig.transport.play();
    let start = 960 * FRAMES_PER_TICK;
    let out = rig.render(start + 6_000);

    assert_eq!(Rig::left(&out, start - 1), 0.0);
    assert!(Rig::left(&out, start + 500) > 0.2 && Rig::left(&out, start + 500) < 0.3, "the middle of the fade-in");
    assert!(near(Rig::left(&out, start + 2_000), 0.5), "full level");
    assert_eq!(Rig::left(&out, start + 5_500), 0.0, "the file has ended");
}

#[test]
fn fades_follow_their_shapes() {
    let mut rig = Rig::new();
    rig.add_audio_track(20, MASTER);
    rig.add_sample(300, constant_sample(48_000, 0.5));
    let clip = rig.add_clip(20, 960, 960, ClipContent::audio(300));
    let edit = rig.clip_mut(clip);
    edit.fade_in_ticks = 40;
    edit.fade_in_shape = FadeShape::EqualPower;
    edit.fade_out_ticks = 40;
    edit.fade_out_shape = FadeShape::Exponential;
    rig.publish();
    rig.transport.play();
    let start = 960 * FRAMES_PER_TICK;
    let end = start + 960 * FRAMES_PER_TICK;
    let out = rig.render(end + 100);

    let expected_in = 0.5 * FadeShape::EqualPower.gain(0.5) as f32;
    assert!((Rig::left(&out, start + 500) - expected_in).abs() < 0.01, "the middle of the equal-power fade-in is −3 dB");
    let expected_out = 0.5 * FadeShape::Exponential.gain(0.5) as f32;
    assert!((Rig::left(&out, end - 500) - expected_out).abs() < 0.01, "the middle of the exponential fade-out is low");
    assert!(near(Rig::left(&out, start + 12_000), 0.5), "full level between the fades");
}

/// The sound a voice played must not be freed in the audio thread even if the voice is the last owner (the sample may
/// have vanished from the project during playback): the reference goes to the control side through the garbage queue.
/// The reference count shows it: the voice's reference lives in the queue until the control side collects it.
#[test]
fn a_finished_audio_voice_hands_its_sample_to_the_control_side_instead_of_dropping_it_in_the_audio_thread() {
    let mut rig = Rig::new();
    rig.add_audio_track(20, MASTER);
    let sample = constant_sample(2_000, 0.5);
    let probe = Arc::clone(&sample);
    rig.add_sample(300, sample);
    rig.add_clip(20, 0, 960, ClipContent::audio(300));
    rig.publish();
    rig.transport.play();
    rig.render(100);
    let while_playing = Arc::strong_count(&probe);

    rig.render(5_000);
    assert_eq!(
        Arc::strong_count(&probe),
        while_playing,
        "the voice's reference waits in the garbage queue instead of being freed in place"
    );
    rig.link.send(DspCommand::PreviewStop).unwrap();
    assert_eq!(Arc::strong_count(&probe), while_playing - 1, "the control side freed it");
}

#[test]
fn stopping_the_transport_hands_playing_audio_voices_to_the_control_side_too() {
    let mut rig = Rig::new();
    rig.add_audio_track(20, MASTER);
    let sample = constant_sample(48_000, 0.5);
    let probe = Arc::clone(&sample);
    rig.add_sample(300, sample);
    rig.add_clip(20, 0, 1920, ClipContent::audio(300));
    rig.publish();
    rig.transport.play();
    rig.render(100);
    let while_playing = Arc::strong_count(&probe);

    rig.transport.stop();
    rig.render(100);
    assert_eq!(Arc::strong_count(&probe), while_playing, "cutting off a voice does not free the sound in the audio thread");
    rig.link.collect_garbage();
    assert_eq!(Arc::strong_count(&probe), while_playing - 1);
}

#[test]
fn an_audio_source_has_its_own_fader() {
    let mut rig = Rig::new();
    let node = rig.add_audio_track(20, MASTER);
    rig.samples.insert(300, constant_sample(20_000, 0.5));
    rig.add_clip(20, 0, 960, ClipContent::audio(300));
    rig.set_strip(node, 0.5, 0.0, false, false);
    rig.publish();
    rig.transport.play();
    assert!(near(Rig::left(&rig.render(2_000), 1_000), 0.25), "the Audio node volume");

    rig.set_strip(node, 1.0, 0.0, true, false);
    rig.publish();
    rig.render(100);
    assert!(near(Rig::left(&rig.render(2_000), 1_000), 0.0), "mute silences the audio track");
}

#[test]
fn entering_the_middle_of_an_audio_clip_resumes_at_the_right_place() {
    let mut rig = Rig::new();
    rig.add_audio_track(20, MASTER);
    let frames: Vec<f32> = (0..48_000).map(|i| i as f32 / 48_000.0).collect();
    rig.add_sample(300, Arc::new(SampleData { sample_rate: SR, channels: 1, frames }));
    rig.add_clip(20, 0, 1920, ClipContent::audio(300));
    rig.publish();
    rig.transport.seek(960);
    rig.transport.play();
    let out = rig.render(100);
    assert!((Rig::left(&out, 0) - 0.5).abs() < 0.01, "{}", Rig::left(&out, 0));
}

/// The release queue is full (the control side has not collected it for long): references stay in memory forever
/// instead of being freed in the audio thread, and the loss shows in the counter.
#[test]
fn garbage_that_does_not_fit_the_queue_is_leaked_and_counted() {
    let mut rig = Rig::new();
    rig.add_audio_track(20, MASTER);
    rig.add_sample(300, constant_sample(3, 0.5));
    for _ in 0..10 {
        rig.add_clip(20, 0, 960, ClipContent::audio(300));
    }
    rig.publish();
    rig.transport.play();
    for _ in 0..1024 {
        rig.link.send(DspCommand::PreviewPlay { data: constant_sample(10, 0.1), from: 0, to: 10, looping: false }).unwrap();
    }
    rig.render(512);
    assert!(rig.link.losses().leaked_garbage > 0, "{:?}", rig.link.losses());
}

#[test]
fn overlapping_audio_clips_crossfade_with_equal_power() {
    let mut rig = Rig::new();
    rig.add_audio_track(20, MASTER);
    rig.add_sample(300, constant_sample(48_000, 0.5));
    rig.add_clip(20, 0, 960, ClipContent::audio(300));
    rig.add_clip(20, 480, 960, ClipContent::audio(300));
    rig.publish();
    rig.transport.play();
    let out = rig.render(1_500 * FRAMES_PER_TICK);
    assert!(near(Rig::left(&out, 100), 0.5), "before the overlap, one clip");
    assert!(near(Rig::left(&out, 480 * FRAMES_PER_TICK + 1), 0.5), "the overlap start: the first one whole, the second from zero");
    let middle = Rig::left(&out, 720 * FRAMES_PER_TICK);
    assert!((middle - 0.5 * std::f32::consts::SQRT_2).abs() < 1e-3, "the middle: both at sin(π/4), {middle}");
    assert!(near(Rig::left(&out, 1_200 * FRAMES_PER_TICK), 0.5), "after the overlap, the second clip");
}

#[test]
fn a_processed_clip_plays_its_sound_from_the_snapshot_and_maps_the_offset() {
    let mut rig = Rig::new();
    rig.add_audio_track(20, MASTER);
    rig.add_sample(300, constant_sample(48_000, 0.5));
    let clip = rig.add_clip(20, 0, 960, ClipContent::audio(300));
    rig.clip_mut(clip).pitch_semitones = 2.0;
    rig.clip_mut(clip).offset_frames = 1_000;
    rig.publish();
    rig.transport.play();
    assert!(
        rig.render(2_000).iter().all(|v| *v == 0.0),
        "the processed sound is not ready yet: the clip is silent rather than playing the file"
    );

    rig.transport.stop();
    rig.render(64);
    let processed = ramp_sample(96_000);
    rig.clip_audio.insert((clip, 300), ClipAudio { data: Arc::clone(&processed), start_frame: 2_000.0 });
    rig.publish();
    rig.transport.play();
    let out = rig.render(100);
    assert!(
        near(Rig::left(&out, 0), processed.frames[2_000]),
        "the clip start is frame 2000 of the processed sound (offset 1000 × stretch 2)"
    );
}

#[test]
fn a_playing_voice_switches_to_the_new_processed_sound() {
    let mut rig = Rig::new();
    rig.add_audio_track(20, MASTER);
    rig.add_sample(300, constant_sample(48_000, 0.5));
    let clip = rig.add_clip(20, 0, 3_840, ClipContent::audio(300));
    rig.clip_mut(clip).pitch_semitones = -1.0;
    rig.clip_audio.insert((clip, 300), ClipAudio { data: constant_sample(96_000, 0.25), start_frame: 0.0 });
    rig.publish();
    rig.transport.play();
    assert!(near(Rig::left(&rig.render(1_000), 500), 0.25));
    rig.clip_audio.insert((clip, 300), ClipAudio { data: constant_sample(96_000, 0.75), start_frame: 0.0 });
    rig.publish();
    rig.render(64);
    assert!(near(Rig::left(&rig.render(1_000), 500), 0.75), "the tempo changed: the voice moved to the new sound at the same position");
}
