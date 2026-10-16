//! Contract tests of the MIDI effects: arpeggiator steps on the grid, order, octaves, length, swing, free clock;
//! chords by type, inversion, scale; chains in different orders; the catalog.

use daw_contracts::{BlockTiming, Bpm, NoteBuffer, NoteEffect, NoteEffectCatalog, NoteEvent, Params, TimeSignature};

use crate::NoteEffects;

const SEGMENT: usize = 512;
/// 120 BPM at 48 kHz: 0.04 ticks per frame, so a sixteenth (240 ticks) lasts 6000 frames.
const TPF: f64 = 0.04;
const SIXTEENTH: usize = 6000;

fn effect(kind: &str, params: &[(&str, f32)]) -> Box<dyn NoteEffect> {
    let params: Params = params.iter().map(|(name, value)| ((*name).to_string(), *value)).collect();
    NoteEffects.create(kind, &params).unwrap()
}

fn timing(playing: bool, start_tick: f64) -> BlockTiming {
    BlockTiming {
        frames: SEGMENT,
        playing,
        start_tick,
        ticks_per_frame: TPF,
        tempo: Bpm::new(120.0).unwrap(),
        time_signature: TimeSignature::FOUR_FOUR,
        signature_start: 0.0,
        loop_region: None,
    }
}

/// An input note at an absolute frame: `(frame, pitch, velocity)`; velocity 0 is a release.
type Input = (usize, u8, f32);
/// An output event at an absolute frame: `(frame, pitch, is on)`.
type Output = (usize, u8, bool);

/// Runs the effect chain segment by segment up to frame `total`. The transport plays from tick 0 (`playing`) or stands at 0.
fn run(chain: &mut [Box<dyn NoteEffect>], playing: bool, inputs: &[Input], total: usize) -> Vec<Output> {
    let mut result = Vec::new();
    let mut start = 0;
    while start < total {
        let timing = timing(playing, if playing { start as f64 * TPF } else { 0.0 });
        let mut buffer = NoteBuffer::new();
        for &(frame, pitch, velocity) in inputs.iter().filter(|(frame, ..)| (start..start + SEGMENT).contains(frame)) {
            buffer.push(NoteEvent { frame: frame - start, pitch, velocity });
        }
        for effect in chain.iter_mut() {
            let mut out = NoteBuffer::new();
            effect.process(buffer.events(), &timing, &mut out);
            assert!(out.events().windows(2).all(|w| w[0].frame <= w[1].frame), "output is sorted by frame");
            buffer = out;
        }
        result.extend(buffer.events().iter().map(|e| (start + e.frame, e.pitch, e.is_on())));
        start += SEGMENT;
    }
    result
}

fn ons(events: &[Output]) -> Vec<(usize, u8)> {
    events.iter().filter(|e| e.2).map(|e| (e.0, e.1)).collect()
}

fn pitches(events: &[Output]) -> Vec<u8> {
    ons(events).into_iter().map(|e| e.1).collect()
}

/// Keys held from frame 0 (pressed in the order `order`).
fn held(order: &[u8]) -> Vec<Input> {
    order.iter().map(|&pitch| (0, pitch, 0.8)).collect()
}

#[test]
fn sixteenth_steps_land_on_the_grid_at_120_bpm_with_half_step_notes() {
    let mut chain = [effect("arp", &[])];
    let events = run(&mut chain, true, &held(&[60, 64, 67]), 4 * SIXTEENTH + 1);
    assert_eq!(ons(&events), [(0, 60), (SIXTEENTH, 64), (2 * SIXTEENTH, 67), (3 * SIXTEENTH, 60), (4 * SIXTEENTH, 64)]);
    let offs: Vec<_> = events.iter().filter(|e| !e.2).map(|e| e.0).collect();
    assert_eq!(offs[..4], [SIXTEENTH / 2, SIXTEENTH * 3 / 2, SIXTEENTH * 5 / 2, SIXTEENTH * 7 / 2], "length is 50 % of the step");
    assert!(events.iter().all(|e| !e.2 || (e.1 == 60 || e.1 == 64 || e.1 == 67)));
}

#[test]
fn rate_choices_set_the_step_length() {
    for (rate, frames) in [(0.0, 4 * SIXTEENTH), (1.0, 2 * SIXTEENTH), (2.0, 8000), (4.0, 4000), (5.0, SIXTEENTH / 2)] {
        let mut chain = [effect("arp", &[("rate", rate)])];
        let events = run(&mut chain, true, &held(&[60]), frames + 1);
        assert_eq!(ons(&events), [(0, 60), (frames, 60)], "rate {rate}");
    }
}

#[test]
fn modes_order_the_held_notes() {
    let cases: [(f32, &[u8], &[u8]); 4] = [
        (1.0, &[60, 64, 67], &[67, 64, 60, 67]),
        (2.0, &[60, 64, 67], &[60, 64, 67, 64, 60, 64]),
        (4.0, &[67, 60, 64], &[67, 60, 64, 67]),
        (0.0, &[67, 60, 64], &[60, 64, 67, 60]),
    ];
    for (mode, order, expected) in cases {
        let mut chain = [effect("arp", &[("mode", mode)])];
        let events = run(&mut chain, true, &held(order), (expected.len() - 1) * SIXTEENTH + 1);
        assert_eq!(pitches(&events), expected, "mode {mode}");
    }
    let mut chain = [effect("arp", &[("mode", 3.0)])];
    let random = pitches(&run(&mut chain, true, &held(&[60, 64, 67]), 19 * SIXTEENTH + 1));
    assert!(random.iter().all(|p| [60, 64, 67].contains(p)) && random.len() == 20);
    let mut again = [effect("arp", &[("mode", 3.0)])];
    assert_eq!(pitches(&run(&mut again, true, &held(&[60, 64, 67]), 19 * SIXTEENTH + 1)), random, "the seed is the same");
}

#[test]
fn octaves_repeat_the_pattern_higher_and_stay_in_midi_range() {
    let mut chain = [effect("arp", &[("octaves", 2.0)])];
    let events = run(&mut chain, true, &held(&[60, 64, 67]), 6 * SIXTEENTH + 1);
    assert_eq!(pitches(&events), [60, 64, 67, 72, 76, 79, 60]);
    let mut chain = [effect("arp", &[("octaves", 4.0)])];
    let events = run(&mut chain, true, &held(&[120]), 3 * SIXTEENTH + 1);
    assert_eq!(pitches(&events), [120, 120, 120, 120], "above 127 goes an octave down");
}

#[test]
fn gate_sets_the_note_length_and_swing_delays_every_second_step() {
    let mut chain = [effect("arp", &[("gate", 25.0)])];
    let events = run(&mut chain, true, &held(&[60]), SIXTEENTH / 2);
    assert_eq!(events, [(0, 60, true), (SIXTEENTH / 4, 60, false)]);

    let mut chain = [effect("arp", &[("swing", 50.0), ("gate", 100.0)])];
    let events = run(&mut chain, true, &held(&[60]), 2 * SIXTEENTH + 1);
    let swung = SIXTEENTH + SIXTEENTH / 2;
    assert_eq!(ons(&events), [(0, 60), (swung, 60), (2 * SIXTEENTH, 60)]);
    assert!(events.contains(&(2 * SIXTEENTH, 60, false)), "a long note is released on the next step");
}

#[test]
fn a_key_pressed_between_grid_lines_waits_for_the_next_step_while_playing() {
    let mut chain = [effect("arp", &[])];
    let events = run(&mut chain, true, &[(100, 60, 1.0)], SIXTEENTH + 1);
    assert_eq!(ons(&events), [(SIXTEENTH, 60)]);
}

#[test]
fn stopped_transport_runs_a_free_clock_started_by_the_first_key() {
    let mut chain = [effect("arp", &[])];
    let events = run(&mut chain, false, &[(1000, 60, 1.0), (1000, 64, 1.0)], 1000 + 2 * SIXTEENTH + 1);
    assert_eq!(ons(&events), [(1000, 60), (1000 + SIXTEENTH, 64), (1000 + 2 * SIXTEENTH, 60)]);
}

#[test]
fn releasing_the_keys_lets_the_last_note_finish_and_stops_the_steps() {
    let mut chain = [effect("arp", &[])];
    let events = run(&mut chain, true, &[(0, 60, 1.0), (100, 60, 0.0)], 3 * SIXTEENTH);
    assert_eq!(events, [(0, 60, true), (SIXTEENTH / 2, 60, false)]);
}

#[test]
fn reset_forgets_held_keys() {
    let mut arp = effect("arp", &[]);
    let mut out = NoteBuffer::new();
    arp.process(&[NoteEvent::on(0, 60, 1.0)], &timing(true, 0.0), &mut out);
    assert_eq!(out.events().len(), 1);
    arp.reset();
    let mut out = NoteBuffer::new();
    arp.process(&[], &timing(true, 240.0), &mut out);
    assert!(out.events().is_empty(), "no steps after a reset: {:?}", out.events());
}

fn chord(params: &[(&str, f32)], pitch: u8) -> Vec<u8> {
    let mut chain = [effect("chord", params)];
    pitches(&run(&mut chain, true, &[(0, pitch, 1.0)], 1))
}

#[test]
fn chord_types_give_their_intervals() {
    let expected: [&[u8]; 9] = [
        &[60, 64, 67],
        &[60, 63, 67],
        &[60, 62, 67],
        &[60, 65, 67],
        &[60, 64, 67, 71],
        &[60, 63, 67, 70],
        &[60, 64, 67, 70],
        &[60, 67],
        &[60, 72],
    ];
    for (kind, notes) in expected.iter().enumerate() {
        assert_eq!(chord(&[("type", kind as f32)], 60), *notes, "type {kind}");
    }
}

#[test]
fn inversion_and_spread_move_lower_notes_up_an_octave() {
    assert_eq!(chord(&[("inversion", 1.0)], 60), [64, 67, 72]);
    assert_eq!(chord(&[("inversion", 2.0)], 60), [67, 72, 76]);
    assert_eq!(chord(&[("inversion", 3.0)], 60), [67, 72, 76], "a triad has at most two inversions");
    assert_eq!(chord(&[("type", 4.0), ("inversion", 3.0)], 60), [71, 72, 76, 79]);
    assert_eq!(chord(&[("spread", 1.0)], 60), [60, 67, 76]);
    assert_eq!(chord(&[("type", 8.0)], 120), [120], "a note above 127 is dropped");
}

#[test]
fn in_scale_chords_stack_every_other_degree() {
    // C major: on E an E minor, on D a D minor seventh; C sharp falls to C.
    assert_eq!(chord(&[("type", 9.0)], 64), [64, 67, 71]);
    assert_eq!(chord(&[("type", 10.0)], 62), [62, 65, 69, 72]);
    assert_eq!(chord(&[("type", 9.0)], 61), [60, 64, 67]);
    // A minor (root A = 9): on C a C major.
    assert_eq!(chord(&[("type", 9.0), ("scale", 1.0), ("root", 9.0)], 60), [60, 64, 67]);
}

#[test]
fn chord_release_lets_go_of_what_was_played_even_after_a_parameter_change() {
    let mut chord = effect("chord", &[]);
    let mut out = NoteBuffer::new();
    chord.process(&[NoteEvent::on(0, 60, 1.0), NoteEvent::on(0, 64, 1.0)], &timing(true, 0.0), &mut out);
    // E is shared by both chords (C major and E major): it is pressed once.
    assert_eq!(out.events().iter().filter(|e| e.pitch == 64 && e.is_on()).count(), 1);
    chord.set_param("type", 1.0);
    let mut out = NoteBuffer::new();
    chord.process(&[NoteEvent::off(5, 60)], &timing(true, 20.0), &mut out);
    let released: Vec<u8> = out.events().iter().map(|e| e.pitch).collect();
    assert_eq!(released, [60, 67], "E is held by the second chord");
    let mut out = NoteBuffer::new();
    chord.process(&[NoteEvent::off(5, 64)], &timing(true, 40.0), &mut out);
    let released: Vec<u8> = out.events().iter().map(|e| e.pitch).collect();
    assert_eq!(released, [64, 68, 71]);
}

#[test]
fn chain_order_matters_arp_then_chord_versus_chord_then_arp() {
    let mut arp_then_chord = [effect("arp", &[]), effect("chord", &[])];
    let events = run(&mut arp_then_chord, true, &held(&[60, 62]), SIXTEENTH + 1);
    assert_eq!(ons(&events), [(0, 60), (0, 64), (0, 67), (SIXTEENTH, 62), (SIXTEENTH, 66), (SIXTEENTH, 69)], "every step is a chord");

    let mut chord_then_arp = [effect("chord", &[]), effect("arp", &[])];
    let events = run(&mut chord_then_arp, true, &held(&[60]), 3 * SIXTEENTH + 1);
    assert_eq!(pitches(&events), [60, 64, 67, 60], "arpeggio over the chord notes");
}

#[test]
fn catalog_lists_kinds_and_rejects_unknown_ones() {
    let kinds: Vec<_> = NoteEffects.kinds().into_iter().map(|k| k.kind).collect();
    assert_eq!(kinds, ["arp", "chord"]);
    assert_eq!(NoteEffects.defaults("arp").unwrap()["rate"], 3.0);
    assert!(NoteEffects.create("nope", &Params::new()).is_err());
    assert!(NoteEffects.defaults("nope").is_err());
}
