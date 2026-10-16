//! Take comps: a part plays its take from the same clip position, the seam is a short equal-power crossfade without a
//! click, and a comp edit during playback is picked up.

use daw_contracts::CompSegment;

use super::*;

/// An audio track and a clip 0..1920 ticks with takes 300 (a constant 0.5 level) and 301 (a rising sound) and a comp.
fn comped_rig(comp: Vec<CompSegment>) -> (Rig, ClipId, Arc<SampleData>) {
    let mut rig = Rig::new();
    rig.add_audio_track(20, MASTER);
    rig.add_sample(300, constant_sample(96_000, 0.5));
    let ramp = ramp_sample(96_000);
    rig.add_sample(301, Arc::clone(&ramp));
    let clip = rig.add_clip(20, 0, 1920, ClipContent::Audio { sample: 300, takes: vec![300, 301], comp });
    rig.publish();
    rig.transport.play();
    (rig, clip, ramp)
}

fn segment(start_ticks: u32, take: SampleId) -> CompSegment {
    CompSegment { start_ticks, take }
}

/// The largest jump between adjacent left channel frames in `[from, to)`.
fn max_step(out: &[f32], from: usize, to: usize) -> f32 {
    (from + 1..to).map(|i| (Rig::left(out, i) - Rig::left(out, i - 1)).abs()).fold(0.0, f32::max)
}

#[test]
fn comp_switches_takes_at_the_boundary_with_a_short_crossfade() {
    let (mut rig, _, ramp) = comped_rig(vec![segment(0, 300), segment(960, 301), segment(1440, 300)]);
    let out = rig.render(1920 * FRAMES_PER_TICK);
    let boundary = 960 * FRAMES_PER_TICK;
    // 10 ms at 48 kHz is 480 frames, 240 on each side of the seam.
    assert!(near(Rig::left(&out, boundary - 300), 0.5), "before the transition, the first take whole");
    let at = boundary + 1_000;
    assert!(near(Rig::left(&out, at), ramp.frames[at]), "the second take plays from the same clip position, not from its start");
    let middle = Rig::left(&out, boundary);
    let expected = (0.5 + ramp.frames[boundary]) * std::f32::consts::FRAC_1_SQRT_2;
    assert!((middle - expected).abs() < 2e-3, "the seam middle: both at sin(π/4): {middle} instead of {expected}");
    assert!(near(Rig::left(&out, 1440 * FRAMES_PER_TICK + 300), 0.5), "the third part is the first take again");
    // Without a crossfade the jump at the seam would be 0.5 − 0.25 = 0.25 per frame.
    let step = max_step(&out, 1, 1920 * FRAMES_PER_TICK);
    assert!(step < 5e-3, "the seam does not click: the largest jump is {step}");
}

#[test]
fn comp_edits_while_playing_are_followed() {
    let (mut rig, clip, ramp) = comped_rig(vec![segment(0, 300), segment(960, 301)]);
    rig.render(200 * FRAMES_PER_TICK);
    // The comp is removed: the whole clip is take 300, the second part's voice must not stay.
    let ClipContent::Audio { comp, .. } = &mut rig.clip_mut(clip).content else { unreachable!("an audio clip") };
    comp.clear();
    rig.publish();
    let out = rig.render(1_000 * FRAMES_PER_TICK);
    let after = (1_000 - 200) * FRAMES_PER_TICK;
    assert!(near(Rig::left(&out, after), 0.5), "after the former seam, the same take, without the second");
    // And back: a comp during playback, the new part plays from its own position.
    let ClipContent::Audio { comp, .. } = &mut rig.clip_mut(clip).content else { unreachable!("an audio clip") };
    *comp = vec![segment(0, 300), segment(1500, 301)];
    rig.publish();
    let out = rig.render(600 * FRAMES_PER_TICK);
    let frame = 1_600 * FRAMES_PER_TICK;
    assert!(near(Rig::left(&out, frame - 1_200 * FRAMES_PER_TICK), ramp.frames[frame]));
}

#[test]
fn a_comped_processed_clip_plays_each_take_from_the_snapshot() {
    let (mut rig, clip, _) = comped_rig(vec![segment(0, 300), segment(960, 301)]);
    rig.transport.stop();
    rig.render(64);
    rig.clip_mut(clip).pitch_semitones = 1.0;
    rig.clip_audio.insert((clip, 300), ClipAudio { data: constant_sample(96_000, 0.25), start_frame: 0.0 });
    rig.publish();
    rig.transport.play();
    let out = rig.render(1920 * FRAMES_PER_TICK);
    assert!(near(Rig::left(&out, 500), 0.25), "the first part is the processed sound of its take");
    assert_eq!(Rig::left(&out, 1_500 * FRAMES_PER_TICK), 0.0, "the second take's sound is not computed yet: its part is silent");
}
