//! Full session and samples: slicing by markers, previewing a region, saving the region and the markers.
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    reason = "integration tests: failing with a clear message is the check itself"
)]

use crate::common::*;

#[test]
fn slicing_by_hand_placed_markers_makes_one_track_per_piece() {
    let dir = temp_dir("slices");
    write_two_tones(&dir.join("tones.wav"));
    let mut session = start(false);
    let controls = session.controls.clone();
    let sample = controls.samples.import(&dir.join("tones.wav")).unwrap();

    controls.sample_editor.set_markers(sample.id, vec![22_050, 5, 22_050]).unwrap();
    let markers = controls.samples.samples()[0].markers.clone();
    assert_eq!(markers, [5, 22_050], "markers are sorted and without duplicates");

    // Spans between the markers and the file edges → tracks.
    let mut bounds = vec![0u64];
    bounds.extend(&markers);
    bounds.push(sample.frame_count as u64);
    let tracks: Vec<_> = bounds
        .windows(2)
        .enumerate()
        .map(|(i, w)| sampler_track(&format!("Slice {}", i + 1), sample.id, w[0] as f32 / 44_099.0, w[1] as f32 / 44_099.0))
        .collect();
    let ids = controls.tracks.add_tracks(tracks).unwrap();
    assert_eq!(ids.len(), 3);

    // The second slice (5…22050) holds the first tone, the third one the second.
    for id in &ids[1..] {
        let clip = note_clip(&controls, *id, 0, 1);
        controls.notes.add_note(clip.id, NewNote { pitch: 60, start_tick: 0, length_ticks: 960, velocity: 127 }).unwrap();
    }
    controls.transport.play();
    let both = render(&mut session, 12_000);
    assert!(rms(&both) > 0.05, "the slices sound");
    std::fs::remove_dir_all(dir).ok();
}

#[test]
fn preview_plays_a_region_and_reports_position_without_touching_the_transport() {
    let dir = temp_dir("preview");
    write_two_tones(&dir.join("tones.wav"));
    let mut session = start(false);
    let controls = session.controls.clone();
    let sample = controls.samples.import(&dir.join("tones.wav")).unwrap();

    controls.preview.play(sample.id, 22_050, 39_690, false).unwrap();
    let out = render(&mut session, 4_800);
    assert!(rms(&out) > 0.2, "the preview is audible");
    let position = controls.preview.position().expect("playing");
    assert!((22_050..39_690).contains(&position), "position in the file: {position}");
    assert_eq!(controls.transport.state(), TransportState::Stopped, "the transport is not touched");

    controls.preview.stop();
    render(&mut session, 512);
    assert_eq!(controls.preview.position(), None);
    assert!(peak(&render(&mut session, 512)) < 1e-6);

    // A looped preview plays longer than the region itself.
    controls.preview.play(sample.id, 22_050, 23_050, true).unwrap();
    render(&mut session, 48_000);
    assert!(controls.preview.position().is_some());
    controls.preview.stop();
    assert!(controls.preview.play(sample.id, 5, 5, false).is_err());
    assert!(controls.preview.play(999, 0, 10, false).is_err());
    std::fs::remove_dir_all(dir).ok();
}

#[test]
fn a_region_can_be_saved_as_a_wav_and_markers_survive_save_and_load() {
    let dir = temp_dir("region-save");
    write_two_tones(&dir.join("tones.wav"));
    let session = start(false);
    let controls = session.controls.clone();
    let sample = controls.samples.import(&dir.join("tones.wav")).unwrap();

    let out = dir.join("high.wav");
    controls.sample_editor.export_region(sample.id, 22_050, 39_690, &out).unwrap();
    let (spec, samples) = read_wav(&out);
    assert_eq!((spec.sample_rate, spec.channels, spec.bits_per_sample), (44_100, 1, 24));
    assert_eq!(samples.len(), 17_640);
    assert!(rms(&samples) > 0.3);

    controls.sample_editor.set_markers(sample.id, vec![10_000, 20_000]).unwrap();
    let project_path = dir.join("p").join("song.dawproj");
    controls.project.save(&project_path).unwrap();
    let other = start(false);
    other.controls.project.load(&project_path).unwrap();
    assert_eq!(other.controls.samples.samples()[0].markers, [10_000, 20_000]);
    std::fs::remove_dir_all(dir).ok();
}
