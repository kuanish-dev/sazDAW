//! Live notes and recording: notes go to the instrument of their track and, while recording, back to the recorder.

use super::*;

#[test]
fn removed_instruments_stop_sounding_and_live_notes_work_while_stopped() {
    let mut rig = Rig::new();
    rig.add_track(10, MASTER);
    rig.publish();
    rig.link.note_on(10, 64, 1.0);
    let out = rig.render(512);
    assert!(near(Rig::left(&out, 100), 0.5), "a live note is audible with the transport stopped");
    rig.link.note_off(10, 64);
    rig.render(512);
    assert!(near(Rig::left(&rig.render(512), 100), 0.0));

    rig.link.note_on(10, 64, 1.0);
    rig.link.send(DspCommand::RemoveInstrument { track: 10 }).unwrap();
    assert!(near(Rig::left(&rig.render(512), 100), 0.0), "the instrument is removed");
}

#[test]
fn live_notes_go_to_the_instrument_of_their_track() {
    let mut rig = Rig::new();
    rig.add_track(10, MASTER);
    let quiet = rig.add_track(11, MASTER);
    rig.set_strip(quiet, 0.5, 0.0, false, false);
    rig.publish();
    rig.link.note_on(11, 64, 1.0);
    let out = rig.render(512);
    assert!(near(Rig::left(&out, 100), 0.25), "only track 11 sounds (0.5 · fader 0.5)");
    assert_eq!(rig.count("on:64"), 1);

    rig.link.note_off(11, 64);
    rig.link.note_on(99, 64, 1.0);
    rig.render(512);
    assert!(near(Rig::left(&rig.render(512), 100), 0.0));
}

#[test]
fn recorded_events_carry_the_track() {
    let mut rig = Rig::new();
    rig.add_track(10, MASTER);
    rig.add_track(11, MASTER);
    rig.publish();
    rig.link.note_on(11, 64, 0.8);
    rig.render(512);
    assert_eq!(rig.link.drain(), [] as [daw_contracts::RecordedEvent; 0]);

    rig.link.set_armed(true);
    rig.transport.play();
    rig.render(512);
    rig.link.note_on(11, 64, 0.8);
    rig.link.note_off(11, 64);
    rig.link.note_on(10, 60, 1.0);
    rig.render(512);
    let events = rig.link.drain();
    let tracks: Vec<_> = events
        .iter()
        .map(|e| match e.message {
            LiveMessage::Note { pitch, velocity } => (e.track, pitch, velocity > 0.0),
            LiveMessage::Controller { .. } => unreachable!("no controllers were played"),
        })
        .collect();
    assert_eq!(tracks, [(11, 64, true), (11, 64, false), (10, 60, true)]);
    assert!(events.iter().all(|e| e.tick >= 0.0));
}

#[test]
fn live_notes_that_do_not_fit_the_queue_are_counted_as_losses() {
    let mut rig = Rig::new();
    rig.add_track(10, MASTER);
    rig.publish();
    assert_eq!(rig.link.losses(), EngineLosses::default());
    for _ in 0..600 {
        rig.link.note_on(10, 60, 1.0);
    }
    assert_eq!(rig.link.losses().live_notes, 88);
    rig.render(512);
    rig.link.note_off(10, 60);
    assert_eq!(rig.link.losses().live_notes, 88, "after the queue is drained, notes pass again");
}

#[test]
fn recorded_events_that_do_not_fit_the_queue_are_counted_as_losses() {
    let mut rig = Rig::new();
    rig.add_track(10, MASTER);
    rig.publish();
    rig.link.set_armed(true);
    rig.transport.play();
    for _ in 0..9 {
        for _ in 0..512 {
            rig.link.note_on(10, 60, 1.0);
        }
        rig.render(64);
    }
    assert_eq!(rig.link.losses().recorded_events, 512);
    assert_eq!(rig.link.drain().len(), 4096);
}

#[test]
fn live_controllers_reach_the_instrument_clamped_and_are_recorded() {
    let mut rig = Rig::new();
    rig.add_track(10, MASTER);
    rig.publish();
    rig.link.controller(10, ControllerKind::PitchBend, -3.0);
    rig.link.controller(10, ControllerKind::Cc { number: 64 }, 1.0);
    rig.render(512);
    assert!(rig.log().contains(&"bend=-1@0".to_string()), "{:?}", rig.log());
    assert!(rig.log().contains(&"cc64=1@0".to_string()));

    rig.link.set_armed(true);
    rig.transport.play();
    rig.render(512);
    rig.link.controller(10, ControllerKind::Aftertouch, 0.5);
    rig.render(512);
    let recorded: Vec<_> = rig.link.drain().into_iter().map(|e| (e.track, e.message)).collect();
    assert_eq!(recorded, [(10, LiveMessage::Controller { kind: ControllerKind::Aftertouch, value: 0.5 })]);
}
