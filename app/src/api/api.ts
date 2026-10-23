import type { BackendError } from "../i18n/errors";
import type { RpcClient } from "./rpc";
import { decodeMasterWindow, decodeNodeWindows } from "./wire";
import type {
  AudioDevices, AudioRecordStatus, AudioSettings, AudioStatus, AutoPoint, AutomationClip, AutomationClipEdit, AutomationWriteMode, Catalog, ChordPartReport, ChordPartRequest, Clip, ClipEdit, CompSegment, ControllerKind, ControllerPoint, DrumPartReport,
  DrumPartRequest, DrumPattern, EditorTarget, ExportOptions, ExportResult, FolderEdit, GrooveTemplate, HistorySteps, Id, InputChannels, InputLevel, LibraryEntry, LogEntry, LogRecord,
  MasterReading, MidiMappings, MonitorMode, NewNote, NewNoteClip, NewTrack, NodeCursor, Note, ParamTarget, Params, PluginCrash, PluginList, Polarity, Preferences, Project, RecentProject, Room, SampleInfo, ScanReport,
  Scale, ShapeRequest, SignatureChange, StagePlace, Status, StrumPattern, TempoChange, TrackColor, Variation, Waveform,
} from "./types";

export function createApi(rpc: RpcClient) {
  const call = rpc.call.bind(rpc);
  const callBinary = rpc.callBinary.bind(rpc);
  return {
    transport: {
      play: () => call("transport.play"),
      pause: () => call("transport.pause"),
      stop: () => call("transport.stop"),
      seek: (tick: number) => call("transport.seek", { tick }),
      setCue: (tick: number | null) => call("transport.setCue", { tick }),
      setSongLoop: (enabled: boolean) => call("transport.setSongLoop", { enabled }),
    },
    /** The song's tempo map: lists are replaced as a whole; each edit is one undo step. */
    tempo: {
      setChanges: (changes: TempoChange[]) => call("tempo.setChanges", { changes }),
      setSignatureChanges: (changes: SignatureChange[]) => call("tempo.setSignatureChanges", { changes }),
    },
    metronome: {
      setEnabled: (enabled: boolean) => call("metronome.setEnabled", { enabled }),
    },
    project: {
      get: () => call<Project>("project.get"),
      status: () => call<Status>("project.status"),
      fillDemo: () => call("project.fillDemo"),
      save: (path: string) => call("project.save", { path }),
      load: (path: string) => call("project.load", { path }),
      create: (path: string) => call("project.create", { path }),
      recent: () => call<RecentProject[]>("project.recent"),
      forgetRecent: (path: string) => call("project.forgetRecent", { path }),
      /** Path of an autosave left over from the previous run; `null` if there is none. */
      pendingAutosave: () => call<string | null>("project.autosave"),
      restoreAutosave: () => call("project.restoreAutosave"),
      discardAutosave: () => call("project.discardAutosave"),
      /** Problems of the last load (e.g. samples that did not load); each is returned once. */
      takeWarnings: () => call<BackendError[]>("project.takeWarnings"),
      exportAudio: (options: ExportOptions) => call<ExportResult>("project.export", options),
      /** New folder project from a template; opened immediately. */
      createFromTemplate: (path: string, template: string) => call("project.createFromTemplate", { path, template }),
    },
    catalog: {
      get: () => call<Catalog>("catalog.get"),
    },
    history: {
      undo: () => call<boolean>("history.undo"),
      redo: () => call<boolean>("history.redo"),
      /** The undo history: done steps oldest first, undone steps in redo order. Reading changes nothing. */
      list: () => call<HistorySteps>("history.list"),
      /** Marks the current point of the undo history, for `squash`. */
      checkpoint: () => call<number>("history.checkpoint"),
      /** Folds every undo step since `since` into one named `label` (a change made of many calls); false — nothing changed. */
      squash: (since: number, label: string) => call<boolean>("history.squash", { since, label }),
    },
    tracks: {
      add: (name: string, kind: string | null, defaultPitch: number) => call<Id>("tracks.add", { name, kind, defaultPitch }),
      addMany: (tracks: NewTrack[]) => call<Id[]>("tracks.addMany", { tracks }),
      remove: (id: Id) => call("tracks.remove", { id }),
      rename: (id: Id, name: string) => call("tracks.rename", { id, name }),
      /**
       * Changes the instrument kind: the new kind's default params plus `params`; notes stay, the track's parameter
       * automation is removed; `name` renames the track (and its node) in the same edit, otherwise the name is kept. One undo step.
       */
      setInstrument: (id: Id, kind: string, params: Params = {}, name?: string) => call("tracks.setInstrument", { id, kind, params, name }),
      setParam: (id: Id, name: string, value: number) => call("tracks.setParam", { id, name, value }),
      /** Moves the track to position `index` in the list (0 = topmost); the others shift. One undo step. */
      move: (id: Id, index: number) => call("tracks.move", { id, index }),
      /** Audio track input monitoring (off / on / auto = while recording is armed). One undo step. */
      setMonitor: (track: Id, mode: MonitorMode) => call("tracks.setMonitor", { track, mode }),
      /** Track color from the palette; `null` means color by row index. One undo step. */
      setColor: (id: Id, color: TrackColor | null) => call("tracks.setColor", { id, color }),
      /** `null` returns the track to the interface setting. */
      setHeight: (id: Id, height: number | null) => call("tracks.setHeight", { id, height }),
      /** Bounce to audio: renders the track to WAV, adds an audio track with the clip below it, mutes the original. Returns the copy's id. */
      bounce: (id: Id, tailSeconds?: number) => call<Id>("tracks.bounce", { id, tailSeconds }),
      /** Freeze: like `bounce`, but the instrument is not instantiated (less load) rather than muted. Returns the copy's id. */
      freeze: (id: Id, tailSeconds?: number) => call<Id>("tracks.freeze", { id, tailSeconds }),
      /** Unfreeze: the copy is removed and the instrument plays again. */
      unfreeze: (id: Id) => call("tracks.unfreeze", { id }),
    },
    templates: {
      list: () => call<string[]>("templates.list"),
      /** Saves the current project's structure (tracks, instruments, graph, tempo) as a template; the same name overwrites. */
      save: (name: string) => call("templates.save", { name }),
      remove: (name: string) => call("templates.remove", { name }),
    },
    presets: {
      list: (kind: string) => call<string[]>("presets.list", { kind }),
      /** Saves the target's parameters and plugin state as a preset of its kind; the same name overwrites. */
      save: (target: EditorTarget, name: string) => call("presets.save", { target, name }),
      /** Applies a preset of the target's kind to the target; one undo step. */
      apply: (target: EditorTarget, name: string) => call("presets.apply", { target, name }),
      remove: (kind: string, name: string) => call("presets.remove", { kind, name }),
    },
    markers: {
      /** Song section marker; at most one per tick. Returns the id. */
      add: (tick: number, name: string) => call<Id>("markers.add", { tick, name }),
      /** Moves and/or renames; an omitted field is unchanged. */
      edit: (id: Id, change: { tick?: number; name?: string }) => call("markers.edit", { id, ...change }),
      remove: (id: Id) => call("markers.remove", { id }),
    },
    /** A track's MIDI effects (the chain before the instrument); `effect` is an effect id from `Track.noteEffects`. */
    noteEffects: {
      /** Appends to the end of the chain; returns the effect id. */
      add: (track: Id, kind: string, params: Params = {}) => call<Id>("noteEffects.add", { track, kind, params }),
      remove: (track: Id, effect: Id) => call("noteEffects.remove", { track, effect }),
      move: (track: Id, effect: Id, index: number) => call("noteEffects.move", { track, effect, index }),
      setParam: (track: Id, effect: Id, name: string, value: number) => call("noteEffects.setParam", { track, effect, name, value }),
      setBypass: (track: Id, effect: Id, bypass: boolean) => call("noteEffects.setBypass", { track, effect, bypass }),
    },
    folders: {
      /** Folder from tracks: they are placed contiguously, in this order, at the position of the topmost one. Returns the id. */
      create: (name: string, tracks: Id[]) => call<Id>("folders.create", { name, tracks }),
      edit: (id: Id, edit: FolderEdit) => call("folders.edit", { id, ...edit }),
      /** Removes the folder; the tracks stay where they are. */
      remove: (id: Id) => call("folders.remove", { id }),
      /** Sets the folder's full membership and order (this is how a track is put into a folder at a given position). */
      setTracks: (id: Id, tracks: Id[]) => call("folders.setTracks", { id, tracks }),
      setMute: (id: Id, on: boolean) => call("folders.setMute", { id, on }),
      setSolo: (id: Id, on: boolean) => call("folders.setSolo", { id, on }),
    },
    clips: {
      /** Note clip with notes and content length in one undo step. */
      addNote: (clip: NewNoteClip) => call<Clip>("clips.addNote", clip),
      addAudio: (track: Id, startTick: number, sample: Id) => call<Clip>("clips.addAudio", { track, startTick, sample }),
      edit: (id: Id, edit: ClipEdit) => call<Clip>("clips.edit", { id, edit }),
      split: (id: Id, tick: number) => call<Clip>("clips.split", { id, tick }),
      /** Moves the left edge to `startTick`, keeping the end; the content stays in place (start and offset move together). */
      trimStart: (id: Id, startTick: number) => call<Clip>("clips.trimStart", { id, startTick }),
      /** Sets each audio clip's gain so its played peak reaches `peakDb` dBFS (default −0.3). One undo step. */
      normalize: (ids: Id[], peakDb?: number) => call<Clip[]>("clips.normalize", { ids, peakDb }),
      /** Plays the same piece backwards from a reversed copy of the sample (the project must be saved). One undo step. */
      reverse: (id: Id) => call<Clip>("clips.reverse", { id }),
      remove: (id: Id) => call("clips.remove", { id }),
      /** Edits several clips in one undo step; positions are checked against the final state (the group moves as a whole). */
      editMany: (changes: { id: Id; edit: ClipEdit }[]) => call<Clip[]>("clips.editMany", { changes }),
      /** Removes several clips in one undo step. */
      removeMany: (ids: Id[]) => call("clips.removeMany", { ids }),
      /** Copies clips onto the same tracks: the leftmost copy lands at `atTick`, the others keep their spacing. One undo step. */
      copy: (ids: Id[], atTick: number) => call<Clip[]>("clips.copy", { ids, atTick }),
      setLoopRegion: (region: [number, number] | null) => call("clips.setLoopRegion", { region }),
      /** The audio clip plays its take `take` (a sample from `content.takes`). One undo step. */
      setActiveTake: (id: Id, take: Id) => call<Clip>("clips.setActiveTake", { id, take }),
      /** Removes a take from the clip (the sample stays in the project); if it was active, the clip plays the last remaining one. */
      removeTake: (id: Id, take: Id) => call<Clip>("clips.removeTake", { id, take }),
      /** Sets the whole take comp (first segment at 0, ascending); empty means the whole clip plays the active take. One undo step. */
      setComp: (id: Id, segments: CompSegment[]) => call<Clip>("clips.setComp", { id, segments }),
      /** The range `[from, to)` (ticks from clip start) plays take `take`; the rest is unchanged. One undo step. */
      chooseTakeRange: (id: Id, from: number, to: number, take: Id) => call<Clip>("clips.chooseTakeRange", { id, from, to, take }),
    },
    notes: {
      add: (clip: Id, note: NewNote) => call<Note>("notes.add", { clip, note }),
      remove: (clip: Id, id: Id) => call("notes.remove", { clip, id }),
      clear: (clip: Id) => call("notes.clear", { clip }),
      addMany: (clip: Id, notes: NewNote[]) => call<Note[]>("notes.addMany", { clip, notes }),
      updateMany: (clip: Id, changes: { id: Id; note: NewNote }[]) => call<Note[]>("notes.updateMany", { clip, changes }),
      removeMany: (clip: Id, ids: Id[]) => call("notes.removeMany", { clip, ids }),
      setLoopTicks: (clip: Id, loopTicks: number) => call("notes.setLoopTicks", { clip, loopTicks }),
      /** Pulls starts (and, with `ends`, ends) toward a grid of `grid` ticks by `strength` 0…1. */
      quantize: (clip: Id, ids: Id[] | undefined, grid: number, strength: number, ends: boolean) =>
        call<Note[]>("notes.quantize", { clip, ids, grid, strength, ends }),
      /** Random offset of starts by ±`timingTicks` and velocity by ±`velocity`; the same `seed` gives the same result. */
      humanize: (clip: Id, ids: Id[] | undefined, timingTicks: number, velocity: number, seed?: number) =>
        call<Note[]>("notes.humanize", { clip, ids, timingTicks, velocity, seed }),
      /** Shifts by `semitones`; a note outside 0…127 is an error and nothing changes. */
      transpose: (clip: Id, ids: Id[] | undefined, semitones: number) => call<Note[]>("notes.transpose", { clip, ids, semitones }),
      /** Moves each note to the nearest scale degree from tonic `root` (0 = C); ties go down. */
      fitToScale: (clip: Id, ids: Id[] | undefined, root: number, scale: Scale) => call<Note[]>("notes.fitToScale", { clip, ids, root, scale }),
      /** Extends each note to the start of the next note in the group; the last ones are unchanged. */
      legato: (clip: Id, ids: Id[] | undefined) => call<Note[]>("notes.legato", { clip, ids }),
      /** Template groove: each note is shifted and re-weighted by its nearest sixteenth; `amount` 0…1 is how much. */
      groove: (clip: Id, ids: Id[] | undefined, template: GrooveTemplate, amount: number) =>
        call<Note[]>("notes.groove", { clip, ids, template, amount }),
      /** Groove extracted from note clip `sourceClip` (average offsets and velocities per sixteenth of a bar). */
      grooveFromClip: (clip: Id, ids: Id[] | undefined, sourceClip: Id, amount: number) =>
        call<Note[]>("notes.grooveFromClip", { clip, ids, sourceClip, amount }),
      /** Pattern variation; returns the group's remaining notes, then the added ones. */
      vary: (clip: Id, ids: Id[] | undefined, variation: Variation) => call<Note[]>("notes.vary", { clip, ids, ...variation }),
      /** Copies the clip right after itself with varied notes in one undo step; returns the new clip. */
      varyToNewClip: (clip: Id, ids: Id[] | undefined, variation: Variation) =>
        call<Clip>("notes.varyToNewClip", { clip, ids, ...variation }),
      /** Sets the clip's whole controller lane (points sorted by tick); an empty list removes the lane. One undo step. */
      setControllerLane: (clip: Id, kind: ControllerKind, points: ControllerPoint[]) => call("notes.setControllerLane", { clip, kind, points }),
    },
    graph: {
      addEffect: (effect: string, x: number, y: number) => call<Id>("graph.addEffect", { effect, x, y }),
      addBus: (name: string, x: number, y: number) => call<Id>("graph.addBus", { name, x, y }),
      removeNode: (id: Id) => call("graph.removeNode", { id }),
      renameNode: (id: Id, name: string) => call("graph.renameNode", { id, name }),
      moveNode: (id: Id, x: number, y: number) => call("graph.moveNode", { id, x, y }),
      connect: (from: Id, to: Id, level: number, sidechain: boolean) => call("graph.connect", { from, to, level, sidechain }),
      disconnect: (from: Id, to: Id, sidechain: boolean) => call("graph.disconnect", { from, to, sidechain }),
      setEdgePreFader: (from: Id, to: Id, preFader: boolean) => call("graph.setEdgePreFader", { from, to, preFader }),
      /** Sidechain key source of a compressor or gate (`null` removes it); switches its `sidechain` param on/off. One undo step. */
      setSidechain: (effect: Id, source: Id | null) => call("graph.setSidechain", { effect, source }),
      setVolume: (id: Id, volume: number) => call("graph.setVolume", { id, volume }),
      setPan: (id: Id, pan: number) => call("graph.setPan", { id, pan }),
      setMute: (id: Id, mute: boolean) => call("graph.setMute", { id, mute }),
      setSolo: (id: Id, solo: boolean) => call("graph.setSolo", { id, solo }),
      /** Channel utility of a source or bus: polarity per side, width 0…2, mono, fine time alignment in ms (±10). */
      setPolarity: (id: Id, polarity: Polarity) => call("graph.setPolarity", { id, ...polarity }),
      setWidth: (id: Id, width: number) => call("graph.setWidth", { id, width }),
      setMono: (id: Id, mono: boolean) => call("graph.setMono", { id, mono }),
      setDelay: (id: Id, ms: number) => call("graph.setDelay", { id, ms }),
      setEffectParam: (id: Id, name: string, value: number) => call("graph.setEffectParam", { id, name, value }),
      setEffectBypass: (id: Id, bypass: boolean) => call("graph.setEffectBypass", { id, bypass }),
      /** "+ Effect" on a track or bus: inserts an effect right after the node without breaking the signal path. */
      insertEffect: (after: Id, effect: string, x: number, y: number) => call<Id>("graph.insertEffect", { after, effect, x, y }),
      /** "+ Effect on master": inserts an effect right before the node (master, bus, effect); all its inputs go through the effect. */
      insertEffectBefore: (node: Id, effect: string, x: number, y: number) => call<Id>("graph.insertEffectBefore", { node, effect, x, y }),
      removeEffectKeepPath: (id: Id) => call("graph.removeEffectKeepPath", { id }),
      /** Routes the node's output to a single node, replacing previous regular connections (sidechains stay). */
      setOutput: (node: Id, target: Id) => call("graph.setOutput", { node, target }),
    },
    stage: {
      /** Puts a channel (track source or bus) on the virtual stage or moves it: `pan` is its left/right; the moves of one
       * channel merge into one undo step (a drag). */
      place: (node: Id, pan: number, place: StagePlace) => call("stage.place", { node, pan, ...place }),
      /** Off the stage: plain pan again; its depth and height lanes are removed. */
      takeOff: (node: Id) => call("stage.takeOff", { node }),
      room: () => call<Room>("stage.room"),
      /** The shared room; consecutive changes merge into one undo step (a slider). */
      setRoom: (room: Room) => call("stage.setRoom", { ...room }),
    },
    automation: {
      addLane: (target: ParamTarget) => call<Id>("automation.addLane", { target }),
      removeLane: (id: Id) => call("automation.removeLane", { id }),
      /** A clip starting as a flat line at the parameter's current value; loop = length. Returns the clip id. */
      addClip: (lane: Id, startTick: number, lengthTicks: number) => call<Id>("automation.addClip", { lane, startTick, lengthTicks }),
      editClip: (lane: Id, clip: Id, edit: AutomationClipEdit) => call<AutomationClip>("automation.editClip", { lane, clip, ...edit }),
      /** A copy right after the clip; returns its id. */
      duplicateClip: (lane: Id, clip: Id) => call<Id>("automation.duplicateClip", { lane, clip }),
      removeClip: (lane: Id, clip: Id) => call("automation.removeClip", { lane, clip }),
      /** Replaces the clip's pattern points (ticks from the clip start, `0…loopTicks`). */
      setPoints: (lane: Id, clip: Id, points: AutoPoint[]) => call("automation.setPoints", { lane, clip, points }),
      /** Replaces the pattern points in `fromTick…toTick` with a shape; one undo step. */
      drawShape: (lane: Id, clip: Id, shape: ShapeRequest) => call("automation.drawShape", { lane, clip, ...shape }),
      setWriteMode: (mode: AutomationWriteMode) => call("automation.setWriteMode", { mode }),
      /** A control is moving the parameter (sent right before setting it): written while playing with the write mode on. */
      touch: (target: ParamTarget, value: number) => call("automation.touch", { target, value }),
      /** Every control held in the interface was released. */
      releaseAll: () => call("automation.releaseAll"),
    },
    /** Project samples: files in `<project>/samples`. */
    samples: {
      list: () => call<SampleInfo[]>("samples.list"),
      /** Copies the file into the project folder (unless already there) and makes it a sample; repeating for the same file returns the same sample. */
      import: (path: string) => call<SampleInfo>("samples.import", { path }),
      waveform: (id: Id, from: number, to: number, buckets: number) => call<Waveform>("samples.waveform", { id, from, to, buckets }),
      setMarkers: (id: Id, markers: number[]) => call("samples.setMarkers", { id, markers }),
      exportRegion: (id: Id, from: number, to: number, path: string) => call("samples.exportRegion", { id, from, to, path }),
      /** Removes the sample from the project along with its audio clips; the file on disk stays. One undo step. */
      remove: (id: Id) => call("samples.remove", { id }),
    },
    library: {
      /** The user's home folder: where file pickers start. */
      home: () => call<string>("library.home"),
    },
    /** Sounds inside the project folder and linked folders: browsing is limited to them. */
    sounds: {
      list: (dir: string) => call<LibraryEntry[]>("sounds.list", { dir }),
      /** Linked folders: convenient places to take sounds from. Files are never copied or deleted. */
      sources: () => call<string[]>("sounds.sources"),
      addSource: (path: string) => call("sounds.addSource", { path }),
      removeSource: (path: string) => call("sounds.removeSource", { path }),
    },
    preview: {
      play: (id: Id, from: number, to: number, looping: boolean) => call("preview.play", { id, from, to, looping }),
      /** Previews a file from disk without adding it to the project. */
      playFile: (path: string) => call("preview.playFile", { path }),
      stop: () => call("preview.stop"),
    },
    midi: {
      exportClip: (clip: Id, path: string) => call("midi.exportClip", { clip, path }),
      importInto: (clip: Id, path: string) => call<number>("midi.importInto", { clip, path }),
      /** Replaces the song's tempo and time signatures with events from a MIDI file (one undo step). */
      importTempo: (path: string) => call("midi.importTempo", { path }),
      /** MIDI input device names; an empty list if MIDI is unavailable. */
      devices: () => call<string[]>("midi.devices"),
      /** Connects an input device; its notes go to track `target`. */
      connect: (device: string, target: Id) => call("midi.connect", { device, target }),
      /** The track the connected device plays into. */
      setTarget: (target: Id) => call("midi.setTarget", { target }),
      disconnect: () => call("midi.disconnect"),
      /** MIDI learn: the next turned knob (CC) will control parameter `target`. */
      learn: (target: ParamTarget) => call("midi.learn", { target }),
      cancelLearn: () => call("midi.cancelLearn"),
      mappings: () => call<MidiMappings>("midi.mappings"),
      removeMapping: (target: ParamTarget) => call("midi.removeMapping", { target }),
    },
    record: {
      setArmed: (armed: boolean) => call("record.setArmed", { armed }),
      setQuantize: (ticks: number) => call("record.setQuantize", { ticks }),
    },
    /** On-disk error log: writing from the UI and reading recent entries. */
    log: {
      write: (record: LogRecord) => call("log.write", { ...record, details: record.details ?? null }),
      recent: (limit: number) => call<LogEntry[]>("log.recent", { limit }),
      file: () => call<string | null>("log.file"),
    },
    /** Recording audio input onto an audio track: the take starts at the current position; stop creates a clip. */
    audioRecord: {
      /** Arms or disarms recording on an audio track (arming opens the input). */
      setArmed: (track: Id, armed: boolean) => call<null>("audioRecord.setArmed", { track, armed }),
      setInput: (track: Id, input: InputChannels) => call<null>("audioRecord.setInput", { track, input }),
      /** Starts a take on all armed audio tracks at once. */
      start: () => call<null>("audioRecord.start"),
      /** Clips (new or with a new take), one per track. */
      stop: () => call<Clip[]>("audioRecord.stop"),
      status: () => call<AudioRecordStatus>("audioRecord.status"),
    },
    audio: {
      devices: () => call<AudioDevices>("audio.devices"),
      settings: () => call<AudioSettings>("audio.settings"),
      status: () => call<AudioStatus>("audio.status"),
      apply: (settings: AudioSettings) => call<AudioStatus>("audio.apply", { ...settings }),
      watchInput: (on: boolean) => call<null>("audio.watchInput", { on }),
      inputLevel: () => call<InputLevel>("audio.inputLevel"),
      /** The latest 2048 frames of the master (mono, before the clamp and the metronome), as bytes. Reading keeps the
       *  capture going for `CAPTURE_LEASE_SECONDS`; the first read starts it and has no samples. */
      masterWindow: () => callBinary("audio.masterWindow").then(decodeMasterWindow),
      /** The live master meter's readings. Reading keeps it measuring for `CAPTURE_LEASE_SECONDS`; the first reading
       *  starts a fresh measurement. */
      masterMeter: () => call<MasterReading>("audio.masterMeter"),
      /** Starts the integrated loudness, the loudness range and the held true peak over. */
      resetMasterMeter: () => call<null>("audio.resetMasterMeter"),
      /** The mono check: sums the master to mono for the speakers (not saved, not exported). */
      setMono: (on: boolean) => call<null>("audio.setMono", { on }),
      /** The newest input and output of each node of `nodes` that is captured (at most `MAX_WATCHED_NODES`), as bytes: only
       *  the frames after a cursor in `seen` from the same watch, otherwise the latest `NODE_WINDOW_FRAMES`. Reading a node
       *  keeps it captured for `CAPTURE_LEASE_SECONDS`; one read for the first time starts from silence. */
      nodeWindows: (nodes: Id[], seen: NodeCursor[]) => callBinary("audio.nodeWindows", { nodes, seen }).then(decodeNodeWindows),
    },
    /** Third-party plugins (VST3): scanned only on demand, never loaded at app startup. */
    plugins: {
      scan: () => call<ScanReport>("plugins.scan"),
      /** Installed plugins (built-in and found by the last scan) and the scan folders. */
      list: () => call<PluginList>("plugins.list"),
      /** Disables a plugin (not offered; projects open without it) or re-enables it. */
      setDisabled: (kind: string, disabled: boolean) => call<null>("plugins.setDisabled", { kind, disabled }),
      /** The plugin the previous session crashed inside, or `null`. */
      lastCrash: () => call<PluginCrash | null>("plugins.lastCrash"),
      /** The crash report was shown: `lastCrash` returns `null` from now on. */
      dismissCrash: () => call<null>("plugins.dismissCrash"),
    },
    /** App settings (profile, autosave, appearance); stored by the backend. */
    preferences: {
      get: () => call<Preferences>("preferences.get"),
      /** Saves all settings; returns what was saved. Invalid values are an error with a readable message. */
      set: (preferences: Preferences) => call<Preferences>("preferences.set", { preferences }),
    },
    patterns: {
      list: () => call<DrumPattern[]>("patterns.list"),
    },
    /** Building a drum part from patterns (one undo step) and previewing it. */
    drums: {
      build: (request: DrumPartRequest) => call<DrumPartReport>("drums.build", request),
      /** Loops a preview without changing the project; plays until `stopPreview` is called. */
      preview: (request: DrumPartRequest) => call<null>("drums.preview", request),
      stopPreview: () => call<null>("drums.stopPreview"),
    },
    /** Strumming a chord progression on one track (one undo step) and previewing it. */
    chords: {
      strums: () => call<StrumPattern[]>("chords.strums"),
      build: (request: ChordPartRequest) => call<ChordPartReport>("chords.build", request),
      /** Loops a preview without changing the project; plays until `stopPreview` is called. */
      preview: (request: ChordPartRequest) => call<null>("chords.preview", request),
      stopPreview: () => call<null>("chords.stopPreview"),
    },
    /** Native VST3 plugin windows (opened by the app; closed by the window itself). */
    pluginEditor: {
      open: (target: EditorTarget) => call("pluginEditor.open", { target }),
    },
    live: {
      noteOn: (track: Id, pitch: number, velocity: number) => call("live.noteOn", { track, pitch, velocity }),
      noteOff: (track: Id, pitch: number) => call("live.noteOff", { track, pitch }),
    },
  };
}

export type DawApi = ReturnType<typeof createApi>;
