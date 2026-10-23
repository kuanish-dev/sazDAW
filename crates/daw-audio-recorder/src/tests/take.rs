//! A take into the project: latency cut, WAV, sample and clip in one undo step; several tracks with their own channels;
//! recording over a clip makes its new take; an empty take and a busy spot; loop recording (every pass is a take).

use super::*;

#[test]
fn a_take_becomes_a_wav_a_sample_and_a_clip_at_the_start_tick_in_one_undo_step() {
    let w = world();
    w.record(AUDIO_TRACK);
    w.capture.push(&ramp(4_800));
    let clip = w.stop_one();

    let written = w.writer.written.lock().unwrap();
    let (path, data) = &written[0];
    assert_eq!(path, &PathBuf::from("/p/samples/Recording Vocals.wav"));
    assert_eq!((data.sample_rate, data.channels), (48_000, 2));
    assert_eq!(data.frame_count(), 4_800 - 480, "the 10 ms latency is cut from the start");
    assert_eq!(data.frames[0], 480.0, "the first frame is the 480th received");
    assert_eq!(w.samples.imported.lock().unwrap().as_slice(), std::slice::from_ref(path));
    assert_eq!((clip.track, clip.start_tick), (AUDIO_TRACK, 960), "the clip is at the take start tick");
    assert_eq!(clip.content, ClipContent::audio(7));
    assert_eq!(
        w.events(),
        [
            "capture.start",
            "transport.play",
            "capture.stop",
            "transport.stop",
            "group.begin Audio recording",
            "wav.write",
            "sample.import",
            "clip.add",
            "group.end"
        ],
        "file, import and clip inside one undo group"
    );
    assert!(!w.recorder.status().recording);
}

#[test]
fn estimated_latency_is_used_when_none_is_configured() {
    let w = world();
    *w.devices.settings.lock().unwrap() = AudioSettings { buffer_frames: Some(256), ..AudioSettings::default() };
    w.record(AUDIO_TRACK);
    w.capture.push(&ramp(2_000));
    w.recorder.stop().unwrap();
    let written = w.writer.written.lock().unwrap();
    assert_eq!(written[0].1.frame_count(), 2_000 - 512, "two buffers of 256 frames");
}

#[test]
fn plugin_delay_compensation_is_trimmed_on_top_of_the_record_latency() {
    let w = world();
    w.stats.latency.store(240, Ordering::Relaxed);
    w.record(AUDIO_TRACK);
    w.capture.push(&ramp(4_800));
    w.recorder.stop().unwrap();
    let written = w.writer.written.lock().unwrap();
    assert_eq!(written[0].1.frame_count(), 4_800 - 720);
    assert_eq!(written[0].1.frames[0], 720.0);
}

#[test]
fn an_empty_take_is_an_error_and_leaves_the_project_alone() {
    let w = world();
    w.record(AUDIO_TRACK);
    let error = w.recorder.stop().unwrap_err();
    assert!(error.is(audio::EMPTY_TAKE), "{error}");
    assert!(w.writer.written.lock().unwrap().is_empty());
    assert!(w.samples.imported.lock().unwrap().is_empty() && w.clips.clips().is_empty());
    assert!(!w.events().iter().any(|e| e.starts_with("group")));
    assert!(w.events().contains(&"transport.stop".to_string()), "transport and input are released anyway");
    assert!(w.events().contains(&"capture.stop".to_string()));
}

#[test]
fn a_take_shorter_than_the_latency_is_an_error_without_a_file() {
    let w = world();
    w.record(AUDIO_TRACK);
    w.capture.push(&ramp(100));
    assert!(matches!(w.recorder.stop(), Err(e) if e.is(audio::TAKE_SHORTER_THAN_LATENCY)));
    assert!(w.writer.written.lock().unwrap().is_empty());
}

#[test]
fn a_take_over_a_spot_that_is_busy_but_not_covered_keeps_the_sound_but_makes_no_clip() {
    let w = world();
    w.clips.clips.lock().unwrap().push(audio_clip(5, AUDIO_TRACK, 1_000, 100_000, 3));
    w.record(AUDIO_TRACK);
    w.capture.push(&ramp(48_000));
    let error = w.recorder.stop().unwrap_err();
    assert!(error.is(audio::TAKES_FAILED), "{error}");
    assert!(error.message().contains("\"Vocals\": The audio was saved to Samples, but the clip was not created"), "{error}");
    assert_eq!(w.writer.written.lock().unwrap().len(), 1, "the file is written");
    assert_eq!(w.samples.imported.lock().unwrap().len(), 1, "and imported");
    assert_eq!(w.clips.clips().len(), 1, "no new clip");
    assert_eq!(w.events().last().unwrap(), "group.end", "the group is closed on error too");
}

#[test]
fn recording_over_a_clip_makes_a_new_take_aligned_to_the_clip_start() {
    let w = world();
    let mut clip = audio_clip(5, AUDIO_TRACK, 1_000, 500, 3);
    clip.offset_frames = 200;
    w.clips.clips.lock().unwrap().push(clip);
    w.record(AUDIO_TRACK);
    w.capture.push(&ramp(48_000));
    let changed = w.stop_one();
    assert_eq!(changed.id, 5, "the same clip");
    assert_eq!((changed.start_tick, changed.length_ticks), (1_000, 500), "the clip neither moves nor grows");
    assert_eq!(
        changed.content,
        ClipContent::Audio { sample: 7, takes: vec![3, 7], comp: Vec::new() },
        "the new take is active, the previous one is kept"
    );
    let written = w.writer.written.lock().unwrap();
    assert_eq!(written[0].1.frames[200 * 2], 1_480.0);
    assert!(w.events().contains(&"take.add 5".to_string()) && !w.events().contains(&"clip.add".to_string()));
}

#[test]
fn a_take_that_starts_late_gets_silence_in_front() {
    let w = world();
    w.clips.clips.lock().unwrap().push(audio_clip(5, AUDIO_TRACK, 0, 3_000, 3));
    w.record(AUDIO_TRACK);
    w.capture.push(&ramp(48_000));
    w.stop_one();
    let written = w.writer.written.lock().unwrap();
    let frames = &written[0].1.frames;
    assert_eq!(frames[0], 0.0, "silence before the take start");
    assert_eq!(frames[24_000 * 2], 480.0, "960 ticks = 24,000 frames in front");
}

#[test]
fn several_armed_tracks_record_their_own_channels_in_one_undo_step() {
    let w = world();
    w.recorder.set_input(AUDIO_TRACK, InputChannels::Left).unwrap();
    w.recorder.set_input(BASS_TRACK, InputChannels::Right).unwrap();
    w.recorder.set_armed(AUDIO_TRACK, true).unwrap();
    w.recorder.set_armed(BASS_TRACK, true).unwrap();
    w.recorder.start().unwrap();
    let frames: Vec<f32> = (0..4_800).flat_map(|i| [i as f32, -(i as f32)]).collect();
    w.capture.push(&frames);
    let clips = w.recorder.stop().unwrap();

    assert_eq!(clips.iter().map(|c| (c.track, c.start_tick)).collect::<Vec<_>>(), [(AUDIO_TRACK, 960), (BASS_TRACK, 960)]);
    let written = w.writer.written.lock().unwrap();
    assert_eq!(written[0].0, PathBuf::from("/p/samples/Recording Vocals.wav"));
    assert_eq!((written[0].1.channels, written[0].1.frames[0]), (1, 480.0), "channel 1, latency cut");
    assert_eq!(written[1].0, PathBuf::from("/p/samples/Recording Bass.wav"));
    assert_eq!((written[1].1.channels, written[1].1.frames[0]), (1, -480.0), "channel 2");
    let groups = w.events().iter().filter(|e| e.starts_with("group.begin")).count();
    assert_eq!(groups, 1, "all tracks are one undo step");
}

#[test]
fn a_failing_track_does_not_lose_the_others() {
    let w = world();
    w.clips.clips.lock().unwrap().push(audio_clip(5, BASS_TRACK, 1_000, 100_000, 3));
    w.recorder.set_armed(AUDIO_TRACK, true).unwrap();
    w.recorder.set_armed(BASS_TRACK, true).unwrap();
    w.recorder.start().unwrap();
    w.capture.push(&ramp(4_800));
    let error = w.recorder.stop().unwrap_err();
    assert!(error.message().contains("\"Bass\"") && !error.message().contains("\"Vocals\""), "{error}");
    assert!(w.clips.clips().iter().any(|c| c.track == AUDIO_TRACK), "the Vocals clip is created");
}

#[test]
fn a_neighbouring_clip_that_only_touches_is_not_a_collision() {
    let w = world();
    w.clips.clips.lock().unwrap().push(audio_clip(5, AUDIO_TRACK, 0, 960, 3));
    w.record(AUDIO_TRACK);
    w.capture.push(&ramp(4_800));
    assert!(w.recorder.stop().is_ok());
}

#[test]
fn loop_passes_become_takes_of_one_new_clip_on_the_loop_region() {
    let w = world();
    w.transport.set_loop(Some((960, 1_920)));
    w.record(AUDIO_TRACK);
    w.capture.push(&ramp(480 + 24_000 * 2 + 12_000));
    let clip = w.stop_one();
    assert_eq!((clip.id, clip.track, clip.start_tick), (107, AUDIO_TRACK, 960), "one clip at the loop start, made from the first pass");
    assert_eq!(
        clip.content,
        ClipContent::Audio { sample: 8, takes: vec![7, 8, 9], comp: Vec::new() },
        "every pass is a take; the last whole pass plays"
    );
    let written = w.writer.written.lock().unwrap();
    let names: Vec<_> = written.iter().map(|(path, _)| path.file_name().unwrap().to_string_lossy().into_owned()).collect();
    assert_eq!(names, ["Recording Vocals (pass 1).wav", "Recording Vocals (pass 2).wav", "Recording Vocals (pass 3).wav"]);
    let cuts: Vec<_> = written.iter().map(|(_, data)| (data.frames[0], data.frame_count())).collect();
    assert_eq!(cuts, [(480.0, 24_000), (24_480.0, 24_000), (48_480.0, 12_000)], "cut where playback jumped back");
    assert_eq!(w.events().iter().filter(|e| *e == "clip.add").count(), 1);
    assert!(w.events().contains(&"take.active 107 8".to_string()), "the pass cut short by stop does not play");
    assert_eq!(w.events().iter().filter(|e| e.starts_with("group.begin")).count(), 1, "one undo step");
}

#[test]
fn loop_passes_go_into_the_clip_under_the_loop() {
    let w = world();
    w.clips.clips.lock().unwrap().push(audio_clip(5, AUDIO_TRACK, 960, 960, 3));
    w.transport.set_loop(Some((960, 1_920)));
    w.record(AUDIO_TRACK);
    w.capture.push(&ramp(480 + 24_000 * 2));
    let clip = w.stop_one();
    assert_eq!((clip.id, clip.start_tick, clip.length_ticks), (5, 960, 960), "the same clip, neither moved nor grown");
    assert_eq!(clip.content, ClipContent::Audio { sample: 8, takes: vec![3, 7, 8], comp: Vec::new() });
    assert!(!w.events().contains(&"clip.add".to_string()));
    assert!(!w.events().iter().any(|e| e.starts_with("take.active")), "the last pass is already the active take");
}

#[test]
fn a_pre_roll_before_the_loop_is_cut_from_the_first_pass() {
    let w = world();
    w.transport.set_loop(Some((1_920, 2_880)));
    w.record(AUDIO_TRACK);
    w.capture.push(&ramp(480 + 48_000 + 24_000));
    let clip = w.stop_one();
    assert_eq!(clip.start_tick, 1_920, "the clip is on the loop, not at the take start");
    let written = w.writer.written.lock().unwrap();
    assert_eq!((written[0].1.frames[0], written[0].1.frame_count()), (24_480.0, 24_000), "the pre-roll beat is cut");
    assert_eq!((written[1].1.frames[0], written[1].1.frame_count()), (48_480.0, 24_000));
}

#[test]
fn a_take_started_past_the_loop_end_is_one_ordinary_take() {
    let w = world();
    w.transport.set_loop(Some((0, 480)));
    w.record(AUDIO_TRACK);
    w.capture.push(&ramp(48_000));
    let clip = w.stop_one();
    assert_eq!(clip.start_tick, 960);
    assert_eq!(w.writer.written.lock().unwrap()[0].0, PathBuf::from("/p/samples/Recording Vocals.wav"));
}
