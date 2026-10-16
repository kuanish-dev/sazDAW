//! Track MIDI effects: clip notes and live notes go through the chain, arpeggiator steps land on the exact frame, chain
//! changes, bypass, stop and removal leave no hanging notes, and what is played gets recorded, not the chain output.

use daw_contracts::{NoteEffectCatalog, NoteEffectSpec};
use daw_note_effects::NoteEffects;

use super::*;

/// A sixteenth (240 ticks) at 120 BPM and 48 kHz.
const SIXTEENTH: usize = 240 * FRAMES_PER_TICK;

/// A MIDI effect for the bench: `(id, kind, params, bypass)`.
type Unit<'a> = (u32, &'a str, &'a [(&'a str, f32)], bool);

impl Rig {
    /// The track MIDI effect chain: in the project (for the snapshot) and in the engine. `(id, kind, params, bypass)`.
    fn set_note_effects(&mut self, track: TrackId, chain: &[Unit]) {
        let specs: Vec<NoteEffectSpec> = chain
            .iter()
            .map(|(id, kind, params, bypass)| NoteEffectSpec {
                id: *id,
                kind: (*kind).to_string(),
                params: params.iter().map(|(name, value)| ((*name).to_string(), *value)).collect(),
                bypass: *bypass,
            })
            .collect();
        let units = specs.iter().map(|spec| (spec.id, NoteEffects.create(&spec.kind, &spec.params).unwrap())).collect();
        self.project.tracks.iter_mut().find(|t| t.id == track).unwrap().note_effects = specs;
        self.link.send(DspCommand::SetNoteEffects { track, chain: units }).unwrap();
    }

    /// Bypass of effect `id` on the track in the project (no command needed: the bypass travels with the snapshot).
    fn set_note_bypass(&mut self, track: TrackId, id: u32, bypass: bool) {
        let track = self.project.tracks.iter_mut().find(|t| t.id == track).unwrap();
        track.note_effects.iter_mut().find(|e| e.id == id).unwrap().bypass = bypass;
    }

    /// The pitches that the instrument log shows as pressed and not released.
    fn sounding(&self) -> Vec<u8> {
        let mut on = [false; 128];
        for entry in self.log() {
            if entry == "all_off" {
                on = [false; 128];
            } else if let Some(rest) = entry.strip_prefix("on:").or_else(|| entry.strip_prefix("off:")) {
                let pitch: usize = rest.split('@').next().unwrap().parse().unwrap();
                on[pitch] = entry.starts_with("on:");
            }
        }
        (0..128u8).filter(|p| on[usize::from(*p)]).collect()
    }
}

fn ons(rig: &Rig) -> Vec<String> {
    rig.log().into_iter().filter(|e| e.starts_with("on:")).collect()
}

#[test]
fn live_keyboard_goes_through_the_chain_to_the_instrument() {
    let mut rig = Rig::new();
    rig.add_track(10, MASTER);
    rig.set_note_effects(10, &[(1, "chord", &[], false)]);
    rig.publish();
    rig.link.note_on(10, 60, 1.0);
    rig.render(512);
    assert_eq!(ons(&rig), ["on:60@0", "on:64@0", "on:67@0"]);
    rig.link.note_off(10, 60);
    rig.render(512);
    assert_eq!(rig.sounding(), [] as [u8; 0]);
}

#[test]
fn clip_notes_are_arpeggiated_on_the_exact_frames_of_the_grid() {
    let mut rig = Rig::new();
    rig.add_track(10, MASTER);
    rig.set_note_effects(10, &[(1, "arp", &[], false)]);
    rig.note(10, 60, 0, 960);
    rig.note(10, 64, 0, 960);
    rig.publish();
    rig.transport.play();
    rig.render(4 * SIXTEENTH + 100);
    assert_eq!(
        ons(&rig),
        ["on:60@0".to_string(), format!("on:64@{SIXTEENTH}"), format!("on:60@{}", 2 * SIXTEENTH), format!("on:64@{}", 3 * SIXTEENTH)]
    );
    assert!(rig.log().contains(&format!("off:60@{}", SIXTEENTH / 2)), "50 % length: {:?}", rig.log());
    let after = rig.log().len();
    rig.render(4 * SIXTEENTH);
    assert_eq!(rig.log().len(), after, "the keys were released at tick 960 (at a step frame): no more steps");
    assert_eq!(rig.sounding(), [] as [u8; 0]);
}

#[test]
fn parameters_and_bypass_come_from_the_snapshot_without_stuck_notes() {
    let mut rig = Rig::new();
    rig.add_track(10, MASTER);
    rig.set_note_effects(10, &[(1, "chord", &[("type", 1.0)], false)]);
    rig.publish();
    rig.link.note_on(10, 60, 1.0);
    rig.render(512);
    assert_eq!(rig.sounding(), [60, 63, 67], "minor from the snapshot params");

    rig.set_note_bypass(10, 1, true);
    rig.publish();
    rig.render(512);
    assert_eq!(rig.sounding(), [60], "bypass: the chord is released, the held key sounds by itself");
    rig.link.note_off(10, 60);
    rig.render(512);
    assert_eq!(rig.sounding(), [] as [u8; 0]);
}

#[test]
fn replacing_or_removing_the_chain_releases_what_it_played() {
    let mut rig = Rig::new();
    rig.add_track(10, MASTER);
    rig.set_note_effects(10, &[(1, "chord", &[], false)]);
    rig.publish();
    rig.link.note_on(10, 60, 1.0);
    rig.render(512);

    rig.set_note_effects(10, &[(1, "chord", &[], false), (2, "chord", &[("type", 8.0)], false)]);
    rig.publish();
    rig.render(512);
    assert_eq!(rig.sounding(), [60, 64, 67, 72, 76, 79], "a new chain: a chord and octaves");

    rig.set_note_effects(10, &[]);
    rig.publish();
    rig.render(512);
    assert_eq!(rig.sounding(), [60], "the chain was removed: only the held key sounds");
    rig.link.note_off(10, 60);
    rig.render(512);
    assert_eq!(rig.sounding(), [] as [u8; 0]);
}

#[test]
fn stopping_silences_the_arpeggiator_and_the_next_play_starts_clean() {
    let mut rig = Rig::new();
    rig.add_track(10, MASTER);
    rig.set_note_effects(10, &[(1, "arp", &[("gate", 100.0)], false)]);
    rig.note(10, 60, 0, 3840);
    rig.publish();
    rig.transport.play();
    rig.render(SIXTEENTH + 100);
    rig.transport.stop();
    rig.render(512);
    rig.clear_log();
    rig.render(3 * SIXTEENTH);
    assert_eq!(ons(&rig), [] as [String; 0], "no steps after the stop");
    assert_eq!(rig.sounding(), [] as [u8; 0]);
}

#[test]
fn stopped_transport_runs_live_arpeggios_on_a_free_clock() {
    let mut rig = Rig::new();
    rig.add_track(10, MASTER);
    rig.set_note_effects(10, &[(1, "arp", &[], false)]);
    rig.publish();
    rig.render(512);
    rig.link.note_on(10, 60, 1.0);
    rig.render(2 * SIXTEENTH + 100);
    assert_eq!(ons(&rig), ["on:60@512".to_string(), format!("on:60@{}", 512 + SIXTEENTH), format!("on:60@{}", 512 + 2 * SIXTEENTH)]);
}

#[test]
fn recording_keeps_what_was_played_not_the_chain_output() {
    let mut rig = Rig::new();
    rig.add_track(10, MASTER);
    rig.set_note_effects(10, &[(1, "chord", &[], false)]);
    rig.publish();
    rig.link.set_armed(true);
    rig.transport.play();
    rig.render(512);
    rig.link.note_on(10, 60, 1.0);
    rig.render(512);
    let recorded: Vec<_> = rig.link.drain().into_iter().map(|e| e.message).collect();
    assert_eq!(recorded, [LiveMessage::Note { pitch: 60, velocity: 1.0 }]);
    assert_eq!(rig.count("on:"), 3, "while a chord is heard");
}
