//! `ProjectBounce` (`TrackBounce`): the copy below the track, its output and volume, one undo step, freeze and unfreeze.
//! A `TrackRender` double provides the sound; `daw-bootstrap/tests/render_bounce.rs` checks that it matches the track.

use std::sync::{Arc, Mutex};

use super::*;
use crate::testing::{FakeAssets, FakeEffects};
use crate::{BounceParts, ProjectBounce, ProjectClips, ProjectGraph, ProjectTracks};

const FILE: &str = "/m/P/P.dawproj";

/// A render double: one second of stereo from the start of the track's first clip; remembers requests; `fail` refuses.
#[derive(Default)]
struct FakeRender {
    calls: Mutex<Vec<(TrackId, f32)>>,
    fail: Mutex<bool>,
}

impl TrackRender for FakeRender {
    fn render_track(&self, project: &Project, track: TrackId, tail_seconds: f32) -> Result<TrackSound, ContractError> {
        self.calls.lock().unwrap().push((track, tail_seconds));
        if *self.fail.lock().unwrap() {
            return Err(codes::plugins::LOAD_FAILED.with("details", "broken"));
        }
        let start = project
            .clips
            .iter()
            .filter(|c| c.track == track)
            .map(|c| c.start_tick)
            .min()
            .ok_or_else(|| codes::export::TRACK_HAS_NO_CLIPS.with("id", track))?;
        Ok(TrackSound { start_tick: start, data: SampleData { sample_rate: 48_000, channels: 2, frames: vec![0.25; 96_000] } })
    }
}

struct Setup {
    rig: Rig,
    render: Arc<FakeRender>,
    bounce: ProjectBounce,
}

fn setup() -> Setup {
    let rig = Rig::new();
    let render = Arc::new(FakeRender::default());
    let parts = BounceParts {
        tracks: Arc::new(ProjectTracks::new(rig.service.clone(), Arc::new(FakeInstruments))),
        clips: Arc::new(ProjectClips::new(rig.service.clone(), rig.samples.clone())),
        graph: Arc::new(ProjectGraph::new(rig.service.clone(), Arc::new(FakeEffects))),
        samples: rig.catalog.clone(),
    };
    let bounce = ProjectBounce::new(rig.service.clone(), render.clone(), rig.writer.clone(), Arc::new(FakeAssets), parts);
    Setup { rig, render, bounce }
}

/// A project with a file: "Bass" (a clip from bar 2) → EQ → bus "Group" (level 0.6), with "Pad" below.
fn song(s: &Setup) -> (TrackId, TrackId, NodeId, NodeId) {
    let rig = &s.rig;
    rig.files.create(Path::new(FILE)).unwrap();
    let bass = rig.tracks.add_track("Bass", Some("synth"), 40).unwrap();
    let pad = rig.tracks.add_track("Pad", Some("synth"), 60).unwrap();
    rig.clips.add_note_clip(NewNoteClip::empty(bass, 3840, None)).unwrap();
    rig.clips.add_note_clip(NewNoteClip::empty(bass, 7680, None)).unwrap();
    let bus = rig.graph.add_bus("Group", 0.0, 0.0).unwrap();
    let eq = rig.graph.insert_effect(source(rig, bass), "eq", &Params::new(), 0.0, 0.0).unwrap();
    rig.graph.set_output(eq, bus).unwrap();
    rig.graph.connect(eq, bus, 0.6, false).unwrap();
    rig.service.mark_baseline();
    (bass, pad, eq, bus)
}

fn strip(rig: &Rig, node: NodeId) -> Strip {
    *rig.graph.graph().node(node).unwrap().kind.strip().unwrap()
}

#[test]
fn bounce_puts_an_audio_copy_right_below_and_mutes_the_original() {
    let s = setup();
    let (bass, pad, _, bus) = song(&s);
    let copy = s.bounce.bounce(bass, 1.5).unwrap();
    let rig = &s.rig;

    assert_eq!(*s.render.calls.lock().unwrap(), [(bass, 1.5)]);
    let order: Vec<TrackId> = rig.tracks.tracks().iter().map(|t| t.id).collect();
    assert_eq!(order, [bass, copy, pad], "the copy is right below the track");
    let copy_track = rig.tracks.tracks().into_iter().find(|t| t.id == copy).unwrap();
    assert!(copy_track.is_audio());
    assert_eq!(copy_track.name, "Bass (audio)");

    let clips: Vec<Clip> = rig.clips.clips().into_iter().filter(|c| c.track == copy).collect();
    assert_eq!(clips.len(), 1);
    assert_eq!(clips[0].start_tick, 3840, "the clip is at the place of the track's first clip");
    let sample = clips[0].content.sample().unwrap();
    let info = rig.catalog.samples().into_iter().find(|i| i.id == sample).unwrap();
    assert_eq!(info.path, Path::new("/m/P/samples/Bass (audio).wav"));
    assert_eq!(
        rig.writer.0.lock().unwrap().as_slice(),
        [(info.path.clone(), 48_000)],
        "the WAV is written to the project sounds folder: one second of stereo"
    );

    let graph = rig.graph.graph();
    let copy_node = source(rig, copy);
    assert_eq!(graph.chain(copy_node).output, Some(bus), "the output is where the track chain goes");
    assert_eq!(graph.edges.iter().find(|e| e.from == copy_node && e.to == bus).unwrap().level, 0.6);
    assert_eq!(strip(rig, copy_node).volume, 1.0, "the track volume is already in the sound");
    assert!(strip(rig, source(rig, bass)).mute, "the original track is muted");
    assert_eq!(rig.clips.clips().iter().filter(|c| c.track == bass).count(), 2, "the original track's notes are in place");
}

#[test]
fn the_copy_keeps_only_the_lead_of_the_channel_utility() {
    let s = setup();
    let (bass, ..) = song(&s);
    let rig = &s.rig;
    let node = source(rig, bass);
    rig.graph.set_polarity(node, Polarity { invert_left: true, invert_right: true }).unwrap();
    rig.graph.set_width(node, 0.5).unwrap();
    rig.graph.set_delay(node, -1.5).unwrap();
    let copy = strip(rig, source(rig, s.bounce.bounce(bass, 0.0).unwrap()));
    assert_eq!((copy.polarity, copy.width), (Polarity::default(), 1.0), "polarity and width are in the sound");
    assert_eq!(copy.delay_ms, -1.5, "a render cannot play earlier: the copy leads the mix itself");

    let s = setup();
    let (bass, ..) = song(&s);
    s.rig.graph.set_delay(source(&s.rig, bass), 2.0).unwrap();
    let copy = strip(&s.rig, source(&s.rig, s.bounce.bounce(bass, 0.0).unwrap()));
    assert_eq!(copy.delay_ms, 0.0, "a later alignment is in the sound already");
}

#[test]
fn bounce_is_one_undo_step() {
    let s = setup();
    let (bass, ..) = song(&s);
    let before = s.rig.service.snapshot();
    s.bounce.bounce(bass, 0.0).unwrap();
    assert_eq!(s.rig.service.undo_label().as_deref(), Some("Bounce to audio «Bass»"));
    assert!(s.rig.service.undo());
    assert_eq!(Project { next_id: before.next_id, ..s.rig.service.snapshot() }, before);
    assert!(!s.rig.service.can_undo(), "exactly one step");
}

#[test]
fn failures_leave_the_project_as_it_was() {
    let s = setup();
    let rig = &s.rig;
    let lonely = rig.tracks.add_track("No file", Some("synth"), 60).unwrap();
    rig.clips.add_note_clip(NewNoteClip::empty(lonely, 0, None)).unwrap();
    assert!(matches!(s.bounce.bounce(lonely, 0.0), Err(e) if e.is(codes::export::BOUNCE_NEEDS_SAVED_PROJECT)));

    let (bass, ..) = song(&s);
    let before = rig.service.snapshot();
    *s.render.fail.lock().unwrap() = true;
    assert!(matches!(s.bounce.bounce(bass, 0.0), Err(e) if e.is(codes::plugins::LOAD_FAILED)));
    *s.render.fail.lock().unwrap() = false;
    assert!(matches!(s.bounce.bounce(999, 0.0), Err(e) if e.is(codes::tracks::NOT_FOUND)));
    assert_eq!(rig.service.snapshot(), before);
    assert!(!rig.service.can_undo());
    assert!(rig.writer.0.lock().unwrap().is_empty());
}

#[test]
fn freeze_silences_the_instrument_and_unfreeze_brings_it_back() {
    let s = setup();
    let (bass, pad, ..) = song(&s);
    let rig = &s.rig;
    rig.dsp.0.lock().unwrap().clear();

    let copy = s.bounce.freeze(bass, 2.0).unwrap();
    let spec = rig.tracks.tracks().into_iter().find(|t| t.id == bass).unwrap().instrument.unwrap();
    assert_eq!(spec.frozen, Some(copy));
    assert!(!strip(rig, source(rig, bass)).mute, "freezing does not mute, it removes the instrument");
    assert!(rig.dsp_log().contains(&format!("remove_instrument:{bass}")), "the engine released the instrument");
    assert_eq!(rig.tracks.tracks().iter().find(|t| t.id == copy).unwrap().name, "Bass (frozen)");
    assert_eq!(rig.service.undo_label().as_deref(), Some("Freeze «Bass»"));

    assert!(matches!(s.bounce.freeze(bass, 0.0), Err(e) if e.is(codes::tracks::FROZEN)), "already frozen");
    assert!(matches!(s.bounce.bounce(bass, 0.0), Err(e) if e.is(codes::tracks::FROZEN)));
    let changed = rig.tracks.set_instrument(bass, "sampler", Params::new(), None);
    assert!(matches!(changed, Err(e) if e.is(codes::tracks::FROZEN)));
    assert!(matches!(s.bounce.unfreeze(pad), Err(e) if e.is(codes::export::NOT_FROZEN)), "not frozen");

    let frozen = rig.service.snapshot();
    rig.dsp.0.lock().unwrap().clear();
    s.bounce.unfreeze(bass).unwrap();
    let project = rig.service.snapshot();
    assert!(project.track(copy).is_none(), "the copy is removed");
    assert!(project.samples.is_empty(), "its sample played nowhere else, so it is removed");
    assert_eq!(project.track(bass).unwrap().instrument.as_ref().unwrap().frozen, None);
    assert!(rig.dsp_log().contains(&format!("add_instrument:{bass}")), "the instrument sounds again");
    assert_eq!(rig.service.undo_label().as_deref(), Some("Unfreeze «Bass»"));
    assert!(rig.service.undo());
    assert_eq!(Project { next_id: frozen.next_id, ..rig.service.snapshot() }, frozen, "unfreezing is one undo step");
}

#[test]
fn freeze_needs_an_instrument_and_unfreeze_survives_a_deleted_copy() {
    let s = setup();
    let (bass, ..) = song(&s);
    let rig = &s.rig;
    let audio = rig.tracks.add_track("Voice", None, 60).unwrap();
    assert!(matches!(s.bounce.freeze(audio, 0.0), Err(e) if e.is(codes::export::FREEZE_NEEDS_INSTRUMENT)));

    let copy = s.bounce.freeze(bass, 0.0).unwrap();
    rig.tracks.remove_track(copy).unwrap();
    s.bounce.unfreeze(bass).unwrap();
    assert_eq!(rig.service.snapshot().track(bass).unwrap().instrument.as_ref().unwrap().frozen, None);
}
