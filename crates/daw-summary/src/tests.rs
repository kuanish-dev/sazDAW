//! The summary of synthetic projects: the arrangement (length, sections, spans, notes, sounds), the mixer (channels,
//! insert paths, buses, shared effects) and the JSON shape the AI reads.
#![allow(clippy::unwrap_used, reason = "tests: a failure with a clear message is the check itself")]

use std::path::PathBuf;

use daw_contracts::*;
use serde_json::json;

use crate::Summarizer;

const B: u32 = BAR_TICKS;

fn clip(id: ClipId, track: TrackId, start: u32, content: ClipContent) -> Clip {
    Clip {
        id,
        track,
        start_tick: start,
        length_ticks: B,
        content,
        gain: 1.0,
        fade_in_ticks: 0,
        fade_out_ticks: 0,
        fade_in_shape: Default::default(),
        fade_out_shape: Default::default(),
        offset_ticks: 0,
        offset_frames: 0,
        warp: None,
        pitch_semitones: 0.0,
    }
}

fn notes(pitches: &[u8]) -> ClipContent {
    let notes =
        (0u32..).zip(pitches).map(|(i, &pitch)| Note { id: i + 1, pitch, start_tick: i * 240, length_ticks: 240, velocity: 100 }).collect();
    ClipContent::Notes(NoteContent { notes, ..NoteContent::empty() })
}

fn node(id: NodeId, name: &str, kind: NodeKind) -> Node {
    Node { id, name: name.into(), kind, x: 0.0, y: 0.0 }
}

fn effect(id: NodeId, kind: &str, bypass: bool) -> Node {
    node(id, kind, NodeKind::Effect { effect: kind.into(), bypass, params: Params::new(), state: None })
}

fn edge(from: NodeId, to: NodeId) -> Edge {
    Edge { from, to, level: 1.0, sidechain: false, pre_fader: false }
}

/// Tracks: Bass (1, node 10), Lead (2, node 11, frozen), Kick (3, node 12, no clips), Vox (4, node 13, audio).
/// Mixer: Bass → eq 20 → compressor 21 (bypassed) → bus 30 → master; Lead → limiter 40 and a send to bus 30; Kick →
/// limiter 40 and a sidechain key into the compressor; limiter 40 → eq 41 → master; Vox → master.
/// Clips: Bass in bars 1–2 (two touching clips) and 5, Lead in bar 3, Vox in bars 1–3 (two sounds).
/// Markers: Intro (bar 1), Drop (bar 3), Outro (bar 7, after the end).
fn project() -> Project {
    let mut project = Project::empty();
    let synth = || Some(InstrumentSpec::new("synth", Params::new()));
    let mut bass = Track::new(1, "Bass", synth(), 36);
    bass.folder = Some(90);
    bass.note_effects = vec![NoteEffectSpec { id: 91, kind: "arpeggiator".into(), params: Params::new(), bypass: false }];
    let lead = Track::new(2, "Lead", Some(InstrumentSpec { frozen: Some(99), ..InstrumentSpec::new("synth", Params::new()) }), 60);
    project.tracks =
        vec![bass, lead, Track::new(3, "Kick", Some(InstrumentSpec::new("kick", Params::new())), 36), Track::new(4, "Vox", None, 60)];
    project.folders = vec![Folder { id: 90, name: "Low end".into(), collapsed: false, color: None }];

    let quiet = Strip { volume: 0.5, mute: true, ..Strip::default() };
    project.graph.nodes.extend([
        node(10, "Bass", NodeKind::Instrument { track: 1, strip: quiet }),
        node(11, "Lead", NodeKind::Instrument { track: 2, strip: Strip::default() }),
        node(12, "Kick", NodeKind::Instrument { track: 3, strip: Strip::default() }),
        node(13, "Vox", NodeKind::Audio { track: 4, strip: Strip::default(), monitor: MonitorMode::default() }),
        effect(20, "eq", false),
        effect(21, "compressor", true),
        node(30, "Low bus", NodeKind::Bus { strip: Strip { solo: true, ..Strip::default() } }),
        effect(40, "limiter", false),
        effect(41, "eq", false),
    ]);
    project.graph.edges = vec![
        edge(10, 20),
        edge(20, 21),
        edge(21, 30),
        edge(30, MASTER),
        edge(11, 40),
        edge(11, 30),
        edge(12, 40),
        Edge { sidechain: true, ..edge(12, 21) },
        edge(40, 41),
        edge(41, MASTER),
        edge(13, MASTER),
    ];

    project.samples = ["vox.wav", "vox take 2.wav"]
        .into_iter()
        .zip(7..)
        .map(|(name, id)| SampleRef { id, name: name.into(), path: PathBuf::from(name), markers: Vec::new() })
        .collect();
    project.clips = vec![
        clip(100, 1, 0, notes(&[36, 48])),
        clip(101, 1, B, notes(&[40])),
        clip(102, 1, 4 * B, notes(&[])),
        clip(103, 2, 2 * B, notes(&[60])),
        clip(104, 4, 0, ClipContent::audio(7)),
        clip(105, 4, B, ClipContent::audio(8)),
        clip(106, 4, 2 * B, ClipContent::audio(7)),
    ];
    project.markers = vec![
        Marker { id: 80, tick: 0, name: "Intro".into() },
        Marker { id: 81, tick: 2 * B, name: "Drop".into() },
        Marker { id: 82, tick: 6 * B, name: "Outro".into() },
    ];
    project.automation = vec![AutomationLane {
        id: 70,
        target: ParamTarget::NodeVolume { node: 30 },
        clips: vec![AutomationClip::once(71, 0, B, Vec::new()), AutomationClip::once(72, B, B, Vec::new())],
    }];
    project.loop_region = Some((0, B));
    project
}

fn track(summary: &Summary, id: TrackId) -> &TrackSummary {
    summary.tracks.iter().find(|track| track.id == id).unwrap()
}

fn ids(effects: &[EffectSummary]) -> Vec<NodeId> {
    effects.iter().map(|effect| effect.id).collect()
}

#[test]
fn an_empty_project_has_only_the_master() {
    let summary = Summarizer.summarize(&Project::empty());
    assert_eq!((summary.end_tick, summary.bars), (0, 0));
    assert_eq!(summary.bpm, TempoMap::default().base_tempo());
    assert!(!summary.tempo_changes);
    assert!(summary.sections.is_empty() && summary.tracks.is_empty() && summary.buses.is_empty());
    assert!(summary.shared_effects.is_empty() && summary.automation.is_empty());
    assert!((summary.master_volume - DEFAULT_VOLUME).abs() < 1e-6);
}

#[test]
fn the_song_length_and_sections_come_from_the_clips_and_markers() {
    let summary = Summarizer.summarize(&project());
    assert_eq!((summary.end_tick, summary.bars), (5 * B, 5), "the last clip ends with bar 5");
    assert_eq!(summary.loop_region, Some((0, B)));

    let sections: Vec<_> = summary.sections.iter().map(|s| (s.name.as_str(), s.from_tick, s.to_tick, s.bar, s.tracks.clone())).collect();
    assert_eq!(
        sections,
        [("Intro", 0, 2 * B, 1, vec![1, 4]), ("Drop", 2 * B, 6 * B, 3, vec![1, 2, 4]), ("Outro", 6 * B, 6 * B, 7, vec![]),],
        "a section runs to the next marker; the last one past the song end is empty"
    );
    assert_eq!(summary.sections[1].marker, 81);
}

#[test]
fn a_track_shows_its_instrument_channel_path_and_where_it_plays() {
    let summary = Summarizer.summarize(&project());
    let bass = track(&summary, 1);
    assert_eq!(bass.instrument.as_deref(), Some("synth"));
    assert!(!bass.frozen && track(&summary, 2).frozen);
    assert_eq!(bass.folder.as_deref(), Some("Low end"));
    assert_eq!(bass.note_effects, ["arpeggiator"]);
    assert_eq!(bass.node, Some(10));
    assert_eq!(bass.strip, Some(Strip { volume: 0.5, mute: true, ..Strip::default() }));

    assert_eq!(ids(&bass.effects), [20, 21], "inserts in order");
    assert!(!bass.effects[0].bypass && bass.effects[1].bypass);
    assert_eq!(bass.output, Some(30));

    assert_eq!(bass.clips, 3);
    assert_eq!(bass.spans, [(0, 2 * B), (4 * B, 5 * B)], "touching clips make one span");
    assert_eq!((bass.notes, bass.pitch_range), (3, Some((36, 48))));

    let kick = track(&summary, 3);
    assert_eq!((kick.clips, kick.notes, kick.pitch_range), (0, 0, None));
    assert!(kick.spans.is_empty());
}

#[test]
fn an_audio_track_names_its_sounds_once() {
    let summary = Summarizer.summarize(&project());
    let vox = track(&summary, 4);
    assert_eq!(vox.instrument, None);
    assert_eq!(vox.samples, ["vox.wav", "vox take 2.wav"]);
    assert_eq!(vox.spans, [(0, 3 * B)]);
    assert_eq!((vox.notes, vox.output), (0, Some(MASTER)));
}

#[test]
fn a_bus_gathers_the_tracks_that_reach_it_but_not_a_sidechain_key() {
    let summary = Summarizer.summarize(&project());
    assert_eq!(summary.buses.len(), 1);
    let bus = &summary.buses[0];
    assert_eq!((bus.id, bus.name.as_str()), (30, "Low bus"));
    assert!(bus.strip.solo);
    assert_eq!(bus.tracks, [1, 2], "Bass through its inserts and Lead through a send; the Kick only keys the compressor");
    assert!(bus.effects.is_empty());
    assert_eq!(bus.output, Some(MASTER));
}

#[test]
fn a_shared_effect_is_listed_once_with_the_effects_after_it() {
    let summary = Summarizer.summarize(&project());
    assert_eq!(track(&summary, 2).output, Some(40), "the Lead's path ends at the shared limiter");
    assert_eq!(summary.shared_effects.len(), 1, "the eq after the limiter is part of its path: {:?}", summary.shared_effects);
    let limiter = &summary.shared_effects[0];
    assert_eq!((limiter.effect.id, limiter.effect.kind.as_str()), (40, "limiter"));
    assert_eq!(ids(&limiter.effects), [41]);
    assert_eq!(limiter.output, Some(MASTER));
}

#[test]
fn automation_lanes_show_their_target_and_spans() {
    let summary = Summarizer.summarize(&project());
    assert_eq!(summary.automation.len(), 1);
    assert_eq!(summary.automation[0].target, ParamTarget::NodeVolume { node: 30 });
    assert_eq!(summary.automation[0].spans, [(0, 2 * B)]);
}

#[test]
fn the_json_is_compact_camel_case_with_the_channel_inline() {
    let value = serde_json::to_value(Summarizer.summarize(&project())).unwrap();
    assert_eq!(value["endTick"], 5 * B);
    assert_eq!(value["signature"], json!({ "numerator": 4, "denominator": 4 }));
    assert_eq!(value["sections"][1]["tracks"], json!([1, 2, 4]));

    let bass = &value["tracks"][0];
    assert_eq!((bass["volume"].clone(), bass["mute"].clone()), (json!(0.5), json!(true)), "the channel sits on the track: {bass}");
    assert_eq!(bass["spans"], json!([[0, 2 * B], [4 * B, 5 * B]]));
    assert_eq!(bass["pitchRange"], json!([36, 48]));
    assert!(bass.get("frozen").is_none() && bass.get("samples").is_none(), "empty extras are left out: {bass}");
    assert!(value["tracks"][2].get("pitchRange").is_none());
    assert_eq!(value["buses"][0]["solo"], true);
    assert_eq!(value["sharedEffects"][0]["kind"], "limiter");
    assert!(value["tracks"][0]["effects"][0].get("bypass").is_none() && value["tracks"][0]["effects"][1]["bypass"] == true);
}
