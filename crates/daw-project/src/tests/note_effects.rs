//! A track's MIDI effects: the chain in the model (add, remove, reorder, param, bypass — one undo step each), errors
//! by the contract, and syncing the chain with the engine.

use super::*;

fn effects(rig: &Rig, track: TrackId) -> Vec<NoteEffectSpec> {
    rig.tracks.tracks().into_iter().find(|t| t.id == track).unwrap().note_effects
}

fn ids(rig: &Rig, track: TrackId) -> Vec<NoteEffectId> {
    effects(rig, track).iter().map(|e| e.id).collect()
}

#[test]
fn effects_are_added_with_defaults_moved_and_removed_one_undo_step_each() {
    let (rig, track, _) = rig_with_clip();
    let first = rig.note_effects.add(track, "arp", Params::new()).unwrap();
    let params: Params = [("gate".to_string(), 500.0)].into();
    let second = rig.note_effects.add(track, "arp", params).unwrap();
    let chain = effects(&rig, track);
    assert_eq!(chain[0].params["rate"], 2.0, "the default");
    assert_eq!(chain[1].params["gate"], 100.0, "within the parameter range");
    assert!(!chain[0].bypass);

    rig.note_effects.move_to(track, second, 0).unwrap();
    assert_eq!(ids(&rig, track), [second, first]);
    rig.note_effects.move_to(track, second, 99).unwrap();
    assert_eq!(ids(&rig, track), [first, second], "past the end goes to the end");
    rig.note_effects.remove(track, first).unwrap();
    assert_eq!(ids(&rig, track), [second]);

    assert!(rig.service.undo());
    assert_eq!(ids(&rig, track), [first, second]);
    assert!(rig.service.undo());
    assert_eq!(ids(&rig, track), [second, first]);
}

#[test]
fn params_are_fitted_and_coalesced_and_bypass_is_one_step() {
    let (rig, track, _) = rig_with_clip();
    let arp = rig.note_effects.add(track, "arp", Params::new()).unwrap();
    rig.note_effects.set_param(track, arp, "rate", 1.4).unwrap();
    rig.note_effects.set_param(track, arp, "rate", 7.0).unwrap();
    assert_eq!(effects(&rig, track)[0].params["rate"], 3.0, "a choice is a whole number within range");
    rig.note_effects.set_bypass(track, arp, true).unwrap();
    assert!(effects(&rig, track)[0].bypass);
    assert!(rig.service.undo());
    assert!(!effects(&rig, track)[0].bypass);
    assert!(rig.service.undo());
    assert_eq!(effects(&rig, track)[0].params["rate"], 2.0, "a knob move is one step");
}

#[test]
fn errors_follow_the_contract() {
    let (rig, track, _) = rig_with_clip();
    let audio = rig.tracks.add_track("Vocals", None, 60).unwrap();
    use codes::notes::{NOTE_EFFECT_NOT_FOUND, NOTE_EFFECT_PARAM_NOT_A_NUMBER, NOTE_EFFECTS_NEED_INSTRUMENT, UNKNOWN_NOTE_EFFECT_PARAM};
    assert!(matches!(rig.note_effects.add(audio, "arp", Params::new()), Err(e) if e.is(NOTE_EFFECTS_NEED_INSTRUMENT)));
    assert!(matches!(rig.note_effects.add(9999, "arp", Params::new()), Err(e) if e.is(codes::tracks::NOT_FOUND)));
    assert!(matches!(rig.note_effects.add(track, "nope", Params::new()), Err(e) if e.is(codes::notes::UNKNOWN_NOTE_EFFECT)));
    let params: Params = [("nope".to_string(), 1.0)].into();
    assert!(matches!(rig.note_effects.add(track, "arp", params), Err(e) if e.is(UNKNOWN_NOTE_EFFECT_PARAM)));
    let arp = rig.note_effects.add(track, "arp", Params::new()).unwrap();
    assert!(matches!(rig.note_effects.set_param(track, arp, "nope", 1.0), Err(e) if e.is(UNKNOWN_NOTE_EFFECT_PARAM)));
    assert!(matches!(rig.note_effects.set_param(track, arp, "gate", f32::NAN), Err(e) if e.is(NOTE_EFFECT_PARAM_NOT_A_NUMBER)));
    assert!(matches!(rig.note_effects.set_bypass(track, 9999, true), Err(e) if e.is(NOTE_EFFECT_NOT_FOUND)));
    assert!(matches!(rig.note_effects.remove(audio, arp), Err(e) if e.is(NOTE_EFFECTS_NEED_INSTRUMENT)));
    for _ in 1..MAX_NOTE_EFFECTS {
        rig.note_effects.add(track, "arp", Params::new()).unwrap();
    }
    let extra = rig.note_effects.add(track, "arp", Params::new());
    assert_eq!(extra, Err(codes::notes::TOO_MANY_NOTE_EFFECTS.with("max", MAX_NOTE_EFFECTS)));
}

#[test]
fn the_engine_gets_a_new_chain_only_when_its_makeup_changes() {
    let (rig, track, _) = rig_with_clip();
    let sent = |rig: &Rig| rig.dsp_log().into_iter().filter(|e| e.starts_with("note_effects")).collect::<Vec<_>>();
    let arp = rig.note_effects.add(track, "arp", Params::new()).unwrap();
    assert_eq!(sent(&rig), [format!("note_effects:{track}:[{arp}]")]);
    rig.note_effects.set_param(track, arp, "gate", 30.0).unwrap();
    rig.note_effects.set_bypass(track, arp, true).unwrap();
    assert_eq!(sent(&rig).len(), 1, "params and bypass travel with the snapshot");
    assert!(rig.last_snapshot().project.track(track).unwrap().note_effects[0].bypass);

    let second = rig.note_effects.add(track, "arp", Params::new()).unwrap();
    rig.note_effects.move_to(track, second, 0).unwrap();
    assert_eq!(sent(&rig)[2], format!("note_effects:{track}:[{second},{arp}]"), "the order changed, so the chain is rebuilt");

    rig.tracks.remove_track(track).unwrap();
    assert_eq!(sent(&rig).last().unwrap(), &format!("note_effects:{track}:[]"), "the track is gone, so the chain is removed");
    assert!(rig.service.undo());
    assert_eq!(sent(&rig).last().unwrap(), &format!("note_effects:{track}:[{second},{arp}]"));
}

#[test]
fn an_effect_that_fails_to_create_is_reported_and_the_rest_still_work() {
    let (rig, track, _) = rig_with_clip();
    let broken = rig.note_effects.add(track, "broken", Params::new()).unwrap();
    let arp = rig.note_effects.add(track, "arp", Params::new()).unwrap();
    assert_eq!(rig.dsp_log().last().unwrap(), &format!("note_effects:{track}:[{arp}]"));
    assert!(rig.service.take_warnings().iter().any(|w| w.is(codes::notes::NOTE_EFFECT_NOT_CREATED) && w.message().contains("broken")));
    assert!(effects(&rig, track).iter().any(|e| e.id == broken), "the effect stays in the project");
}
