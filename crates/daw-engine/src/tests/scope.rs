//! The master window for the visualizer (`EngineStats::master_window`): captured from the first read for as long as
//! reads come, the latest master frames in mono, the music before the clamp and the metronome. The node windows for the
//! effect pictures (`NodeScope::node_windows`): the nodes read, each with its input before the node changes it and its
//! output as it is sent on, paired frame by frame, and only the new frames for a reader's cursor. The live meter's
//! tap (`MasterTap`): every stereo frame of the exported mix, numbered from the opening, with the restart mark of playback
//! from the song start. The mono check (`MonoCheck`): only the device output is summed.

use daw_contracts::{
    BlockTiming, CAPTURE_LEASE_SECONDS, MASTER_TAP_FRAMES, MASTER_WINDOW_FRAMES, MasterTap, MasterWindow, MonoCheck, NODE_WINDOW_FRAMES,
    NodeCursor, NodeScope, NodeWindow,
};

use super::*;

/// A post-processor like the metronome: adds a constant click on top of the finished sound.
struct Click;

impl AudioProcessor for Click {
    fn process(&mut self, block: &mut AudioBlock, _: &BlockTiming) {
        for frame in 0..block.frames() {
            block.add_to_frame(frame, 0.125);
        }
    }
}

/// Frames of a read's lease at the rig's rate.
fn lease() -> usize {
    (CAPTURE_LEASE_SECONDS * SR as f32) as usize
}

#[test]
fn the_first_read_of_the_window_starts_the_capture_and_carries_the_prepared_rate() {
    let mut rig = Rig::new();
    rig.renderer.prepare(StreamConfig { sample_rate: 44_100, channels: 2 });
    rig.add_track(10, MASTER);
    rig.publish();
    rig.link.note_on(10, 60, 1.0);
    rig.render(1_000);
    assert_eq!(rig.link.master_window(), MasterWindow { samples: Vec::new(), sample_rate: 44_100 });
}

#[test]
fn the_window_read_ends_with_the_rendered_master_in_mono_until_reads_stop() {
    let mut rig = Rig::new();
    let node = rig.add_track(10, MASTER);
    // Panned left: the channels differ, so the window must hold their middle, not one of them.
    rig.set_strip(node, 1.0, -1.0, false, false);
    rig.publish();
    rig.link.master_window();
    rig.link.note_on(10, 60, 1.0);
    let out = rig.render(1_000);

    let window = rig.link.master_window();
    assert_eq!(window.samples.len(), MASTER_WINDOW_FRAMES);
    assert_eq!(window.sample_rate, SR);
    let (before, rendered) = window.samples.split_at(MASTER_WINDOW_FRAMES - 1_000);
    assert!(before.iter().all(|v| *v == 0.0), "nothing was rendered before the first read");
    for (frame, sample) in rendered.iter().enumerate() {
        let mono = f32::midpoint(Rig::left(&out, frame), Rig::right(&out, frame));
        assert!(near(*sample, mono), "frame {frame}: {sample} instead of {mono}");
    }
    assert!(near(rendered[999], 0.25), "the left channel 0.5 and a silent right one");

    rig.render(lease());
    assert!(rig.link.master_window().samples.is_empty(), "nobody read for a whole lease: the capture stopped");
}

#[test]
fn the_window_shows_the_music_before_the_clamp_and_the_metronome() {
    let mut rig = Rig::with_post_processors(vec![Box::new(Click)]);
    for track in 10..13 {
        rig.add_track(track, MASTER);
    }
    rig.publish();
    rig.link.master_window();
    for track in 10..13 {
        rig.link.note_on(track, 60, 1.0);
    }
    let out = rig.render(1_000);
    assert!(near(Rig::left(&out, 999), 1.125), "the device gets the clamped master plus the click");
    let newest = *rig.link.master_window().samples.last().unwrap();
    assert!(near(newest, 1.5), "three notes of 0.5, not clamped and without the click: {newest}");
}

/// The window of `node` (a whole window: no cursor).
fn window_of(rig: &Rig, node: NodeId) -> NodeWindow {
    let window = rig.link.node_windows(&[node], &[]).remove(0);
    assert_eq!((window.input.len(), window.output.len()), (NODE_WINDOW_FRAMES, NODE_WINDOW_FRAMES));
    window
}

/// The newest input and output samples of `node`'s window.
fn newest_of(rig: &Rig, node: NodeId) -> (f32, f32) {
    let window = window_of(rig, node);
    (*window.input.last().unwrap(), *window.output.last().unwrap())
}

fn near_pair((input, output): (f32, f32), expected: (f32, f32)) -> bool {
    near(input, expected.0) && near(output, expected.1)
}

#[test]
fn a_node_is_captured_from_its_first_read_until_reads_stop() {
    let mut rig = Rig::new();
    let fx = rig.add_fx(false, Box::new(Double));
    rig.connect(fx, MASTER, 1.0, false);
    rig.add_track(10, fx);
    rig.publish();
    rig.link.note_on(10, 60, 1.0);
    rig.render(1_000);
    assert!(rig.link.node_windows(&[], &[]).is_empty(), "no node asked for");
    assert_eq!(window_of(&rig, fx).end, 0, "nothing was copied before the first read");
    rig.render(1_000);
    assert_eq!(window_of(&rig, fx).end, 1_000);

    rig.render(lease());
    assert_eq!(window_of(&rig, fx).end, 0, "nobody read it for a whole lease: a new watch from silence");
}

#[test]
fn the_watched_effect_window_is_its_input_and_output_and_the_input_twice_while_bypassed() {
    let mut rig = Rig::new();
    let fx = rig.add_fx(false, Box::new(Double));
    rig.connect(fx, MASTER, 1.0, false);
    let track = rig.add_track(10, fx);
    // Panned left: the channels differ, so the window must hold their middle, not one of them.
    rig.set_strip(track, 1.0, -1.0, false, false);
    rig.publish();
    window_of(&rig, fx);
    rig.link.note_on(10, 60, 1.0);
    rig.render(1_000);

    let window = window_of(&rig, fx);
    assert_eq!((window.end, window.sample_rate), (1_000, SR));
    for (samples, value) in [(&window.input, 0.25), (&window.output, 0.5)] {
        let (before, rendered) = samples.split_at(NODE_WINDOW_FRAMES - 1_000);
        assert!(before.iter().all(|v| *v == 0.0), "nothing was rendered before the first read");
        assert!(rendered.iter().all(|v| near(*v, value)), "the left channel 0.5 doubled by the effect, a silent right one");
    }

    if let NodeKind::Effect { bypass, .. } = rig.node_kind(fx) {
        *bypass = true;
    }
    rig.publish();
    rig.render(1_000);
    assert!(near_pair(newest_of(&rig, fx), (0.25, 0.25)), "bypassed: the effect input passes on");
}

#[test]
fn a_whole_path_is_read_at_once_and_a_cursor_gets_only_the_new_frames() {
    let mut rig = Rig::new();
    let bus = rig.add_bus(0.5);
    let track = rig.add_track(10, bus);
    rig.publish();
    rig.link.note_on(10, 60, 1.0);

    let path = [track, bus];
    rig.link.node_windows(&path, &[]);
    rig.render(1_000);
    assert!(near_pair(newest_of(&rig, track), (0.5, 0.5)));
    assert!(near_pair(newest_of(&rig, bus), (0.5, 0.25)), "the bus input before its fader, the output after: 0.5 · 0.5");
    assert!(rig.link.master_window().samples.is_empty(), "the master visualizer has its own capture");

    let cursor = |w: &NodeWindow| NodeCursor { node: w.node, watch: w.watch, end: w.end };
    let seen: Vec<NodeCursor> = rig.link.node_windows(&path, &[]).iter().map(cursor).collect();
    rig.render(300);
    let fresh = rig.link.node_windows(&path, &seen);
    assert_eq!(fresh.iter().map(|w| w.node).collect::<Vec<_>>(), path, "in the order asked");
    for window in &fresh {
        assert_eq!((window.end, window.input.len(), window.output.len()), (1_300, 300, 300), "node {}: the new frames only", window.node);
    }
}

/// A track panned hard left playing a note: the left channel gets 0.5, the right one nothing.
fn left_only(rig: &mut Rig) {
    let node = rig.add_track(10, MASTER);
    rig.set_strip(node, 1.0, -1.0, false, false);
    rig.publish();
    rig.link.note_on(10, 60, 1.0);
}

#[test]
fn the_meter_tap_is_closed_until_opened_then_holds_every_stereo_frame() {
    let mut rig = Rig::new();
    left_only(&mut rig);
    rig.render(100);
    let closed = rig.link.frames_after(0);
    assert!(closed.samples.is_empty() && closed.sample_rate == SR, "{closed:?}");

    rig.link.open_tap(true);
    let out = rig.render(1_000);
    let frames = rig.link.frames_after(0);
    assert_eq!((frames.start, frames.samples.len(), frames.restart), (0, 2_000, None), "numbered from the opening");
    assert_eq!(frames.samples, out, "both channels, as the device got them");
    assert!(near(frames.samples[1_998], 0.5) && near(frames.samples[1_999], 0.0));

    assert!(rig.link.frames_after(1_000).samples.is_empty(), "nothing new");
    rig.render(300);
    let next = rig.link.frames_after(1_000);
    assert_eq!((next.start, next.samples.len()), (1_000, 600), "only the frames after the reader's end");

    rig.link.open_tap(false);
    assert!(rig.link.frames_after(0).samples.is_empty());
    rig.render(100);
    rig.link.open_tap(true);
    assert!(rig.link.frames_after(0).samples.is_empty(), "reopened: the count starts from 0 again");
}

#[test]
fn the_meter_tap_has_the_mix_before_the_clamp_the_metronome_and_the_mono_check() {
    let mut rig = Rig::with_post_processors(vec![Box::new(Click)]);
    for track in 10..13 {
        rig.add_track(track, MASTER);
    }
    rig.publish();
    rig.link.open_tap(true);
    rig.link.set_mono(true);
    for track in 10..13 {
        rig.link.note_on(track, 60, 1.0);
    }
    rig.render(1_000);
    let frames = rig.link.frames_after(0);
    let newest = &frames.samples[frames.samples.len() - 2..];
    assert!(near(newest[0], 1.5) && near(newest[1], 1.5), "three notes of 0.5, not clamped and without the click: {newest:?}");
}

#[test]
fn a_reader_far_behind_gets_the_newest_frames_and_sees_the_gap() {
    let mut rig = Rig::new();
    left_only(&mut rig);
    rig.link.open_tap(true);
    let total = MASTER_TAP_FRAMES + 5_000;
    rig.render(total);
    let frames = rig.link.frames_after(0);
    let count = (frames.samples.len() / 2) as u64;
    assert!(frames.start > 0, "the oldest frames were overwritten: the reader can tell");
    assert_eq!(frames.start + count, total as u64, "up to the newest frame");
    assert!(count <= MASTER_TAP_FRAMES as u64);
}

#[test]
fn playback_from_the_song_start_marks_the_restart_frame() {
    let mut rig = Rig::new();
    left_only(&mut rig);
    rig.link.open_tap(true);
    rig.render(100);
    rig.transport.play();
    rig.render(100);
    assert_eq!(rig.link.frames_after(0).restart, Some(100), "where playback from tick 0 started");

    // The engine sees a start only after a segment rendered while stopped.
    rig.transport.stop();
    rig.render(100);
    rig.transport.seek(3_840);
    rig.transport.play();
    rig.render(100);
    assert_eq!(rig.link.frames_after(0).restart, Some(100), "playback from the middle is not a restart");

    rig.transport.stop();
    rig.render(100);
    rig.transport.play();
    rig.render(100);
    assert_eq!(rig.link.frames_after(0).restart, Some(500), "stop went back to the start: a restart again");

    rig.link.open_tap(false);
    rig.link.open_tap(true);
    assert_eq!(rig.link.frames_after(0).restart, None, "a new opening forgets the mark");
}

#[test]
fn the_mono_check_sums_only_what_the_device_gets() {
    let mut rig = Rig::new();
    left_only(&mut rig);
    rig.link.master_window();
    rig.link.open_tap(true);
    assert!(!rig.link.is_mono(), "stereo at creation");
    rig.link.set_mono(true);
    assert!(rig.link.is_mono());
    let out = rig.render(1_000);
    assert!(near(Rig::left(&out, 999), 0.25) && near(Rig::right(&out, 999), 0.25), "the middle of 0.5 and 0 in both");
    let frames = rig.link.frames_after(0);
    assert!(near(frames.samples[1_998], 0.5) && near(frames.samples[1_999], 0.0), "the meter still measures the stereo mix");
    assert!(near(*rig.link.master_window().samples.last().unwrap(), 0.25), "the visualizer's mono window is unchanged");

    rig.link.set_mono(false);
    let out = rig.render(10);
    assert!(near(Rig::left(&out, 9), 0.5) && near(Rig::right(&out, 9), 0.0), "stereo again");
}
