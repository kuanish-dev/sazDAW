//! Live windows of sound for the interface: the master window for the visualizer (`EngineStats::master_window`), the
//! windows of the watched graph nodes for their pictures (`NodeScope`, the live visuals of the effects, one editor or a
//! whole signal path at once), both kept in `Lane`s, and every frame of the stereo master for the live meter
//! (`MasterTap`, `MeterTap`). Atomics only on the audio thread: it writes into preallocated rings, the control side
//! copies them out; while nobody watches, the audio thread copies nothing.

use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, AtomicUsize, Ordering};

use daw_contracts::sync::lock;
use daw_contracts::{
    MASTER_TAP_FRAMES, MASTER_WINDOW_FRAMES, MAX_WATCHED_NODES, MasterFrames, NODE_WINDOW_FRAMES, NodeCursor, NodeId, NodeWindow,
};

use crate::SEGMENT_FRAMES;

/// A ring of mono frames. The position (where the next frame goes, which is also the oldest one) is kept by the owner:
/// a watched node moves two lanes with one frame count, so its input and output stay paired frame by frame.
struct Lane {
    /// Mono samples as f32 bits.
    samples: Box<[AtomicU32]>,
}

impl Lane {
    fn new(frames: usize) -> Self {
        Self { samples: (0..frames).map(|_| AtomicU32::new(0)).collect() }
    }

    fn clear(&self) {
        for sample in &self.samples {
            sample.store(0, Ordering::Relaxed);
        }
    }

    /// Writes interleaved stereo as mono from `position` on, wrapping (audio thread): no allocations, no locks.
    /// Returns the position after the last frame.
    fn write(&self, position: usize, stereo: &[f32]) -> usize {
        let mut position = position;
        for frame in stereo.chunks_exact(2) {
            self.samples[position].store(f32::midpoint(frame[0], frame[1]).to_bits(), Ordering::Relaxed);
            position = (position + 1) % self.samples.len();
        }
        position
    }

    /// The `count` frames (at most the whole lane) before position `end`, oldest first (control side). A read racing a
    /// write may mix samples of two consecutive segments: harmless for a picture, and it keeps the audio thread free of
    /// locks.
    fn read(&self, end: usize, count: usize) -> Vec<f32> {
        let len = self.samples.len();
        let count = count.min(len);
        let start = (end + len - count) % len;
        (0..count).map(|i| f32::from_bits(self.samples[(start + i) % len].load(Ordering::Relaxed))).collect()
    }
}

pub struct MasterScope {
    on: AtomicBool,
    lane: Lane,
    position: AtomicUsize,
}

impl MasterScope {
    pub fn new() -> Self {
        Self { on: AtomicBool::new(false), lane: Lane::new(MASTER_WINDOW_FRAMES), position: AtomicUsize::new(0) }
    }

    /// Turning on clears the lane, so a stale window from the last time it was watched is not shown. It is cleared
    /// before switching on, while the audio thread is not copying (at worst a segment that was finishing leaves a few
    /// samples, harmless for a picture), and a repeated "on" keeps the live picture.
    pub fn watch(&self, on: bool) {
        if on && !self.on.load(Ordering::Relaxed) {
            self.lane.clear();
        }
        // Release: the audio thread that sees "on" also sees the cleared lane and does not write under the clearing.
        self.on.store(on, Ordering::Release);
    }

    /// Adds a segment of the interleaved stereo master as mono (audio thread).
    pub fn write(&self, master: &[f32]) {
        if self.on.load(Ordering::Acquire) {
            let next = self.lane.write(self.position.load(Ordering::Relaxed), master);
            self.position.store(next, Ordering::Release);
        }
    }

    /// The window oldest first (control side); empty while off.
    pub fn read(&self) -> Vec<f32> {
        if self.on.load(Ordering::Relaxed) {
            self.lane.read(self.position.load(Ordering::Acquire), MASTER_WINDOW_FRAMES)
        } else {
            Vec::new()
        }
    }
}

/// The master for the live meter (`MasterTap`): every frame of the stereo master in a preallocated ring of atomics,
/// numbered, so the one reader can tell what it has not read yet and what the ring overwrote before it got there. Unlike
/// the visualizer's window it keeps both channels (loudness, phase) and loses nothing while the reader keeps up.
pub struct MeterTap {
    on: AtomicBool,
    /// Left and right as f32 bits, interleaved: frame `n` (counted by `written`) sits at `n % MASTER_TAP_FRAMES`.
    samples: Box<[AtomicU32]>,
    /// Frames written since the engine started (only the audio thread moves it, and only forward): the reader's frame
    /// numbers count from `opened`, so reopening never races the audio thread over this count.
    written: AtomicU64,
    /// `written` when the tap was last opened.
    opened: AtomicU64,
    /// The frame (counted like `written`) where playback last started from the song start, plus one; 0 means none.
    restart: AtomicU64,
}

impl MeterTap {
    pub fn new() -> Self {
        Self {
            on: AtomicBool::new(false),
            samples: (0..2 * MASTER_TAP_FRAMES).map(|_| AtomicU32::new(0)).collect(),
            written: AtomicU64::new(0),
            opened: AtomicU64::new(0),
            restart: AtomicU64::new(0),
        }
    }

    /// Opening numbers the frames from 0 and forgets the restart mark (control side).
    pub fn open(&self, on: bool) {
        if on && !self.on.load(Ordering::Relaxed) {
            self.opened.store(self.written.load(Ordering::Acquire), Ordering::Relaxed);
            self.restart.store(0, Ordering::Relaxed);
        }
        // Release: the reader that sees "on" also sees the new numbering.
        self.on.store(on, Ordering::Release);
    }

    /// Adds a segment (at most `SEGMENT_FRAMES`) of the interleaved stereo master (audio thread): no allocations, no
    /// locks.
    pub fn write(&self, master: &[f32]) {
        if !self.on.load(Ordering::Acquire) {
            return;
        }
        let written = self.written.load(Ordering::Relaxed);
        let mut at = 2 * (written % MASTER_TAP_FRAMES as u64) as usize;
        for sample in master {
            self.samples[at].store(sample.to_bits(), Ordering::Relaxed);
            at = (at + 1) % self.samples.len();
        }
        self.written.store(written + (master.len() / 2) as u64, Ordering::Release);
    }

    /// Playback starts from the song start in the segment about to be written; the song start reaches the master
    /// `delay` frames later (the compensation latency). Audio thread.
    pub fn mark_restart(&self, delay: u32) {
        if self.on.load(Ordering::Relaxed) {
            let at = self.written.load(Ordering::Relaxed) + u64::from(delay);
            self.restart.store(at + 1, Ordering::Release);
        }
    }

    /// The frames after frame `end`, at most what the ring still holds (control side, see `MasterTap::frames_after`).
    pub fn frames_after(&self, end: u64, sample_rate: u32) -> MasterFrames {
        if !self.on.load(Ordering::Acquire) {
            return MasterFrames { start: end, samples: Vec::new(), restart: None, sample_rate };
        }
        let opened = self.opened.load(Ordering::Relaxed);
        // The oldest segment's worth may be under a write that has not moved `written` yet: it does not count as held.
        let held = (MASTER_TAP_FRAMES - SEGMENT_FRAMES) as u64;
        let written = self.written.load(Ordering::Acquire).saturating_sub(opened);
        let mut start = end.max(written.saturating_sub(held)).min(written);
        let mut samples: Vec<f32> = (start..written)
            .flat_map(|frame| {
                let at = 2 * ((opened + frame) % MASTER_TAP_FRAMES as u64) as usize;
                [self.samples[at].load(Ordering::Relaxed), self.samples[at + 1].load(Ordering::Relaxed)]
            })
            .map(f32::from_bits)
            .collect();
        // A reader far behind: frames the audio thread overwrote while they were copied are dropped from the front.
        let overwritten = self.written.load(Ordering::Acquire).saturating_sub(opened).saturating_sub(held).min(written);
        if overwritten > start {
            samples.drain(..2 * (overwritten - start) as usize);
            start = overwritten;
        }
        let restart = match self.restart.load(Ordering::Acquire) {
            0 => None,
            mark => (mark - 1).checked_sub(opened),
        };
        MasterFrames { start, samples, restart, sample_rate }
    }
}

const NO_NODE: NodeId = NodeId::MAX;

/// Where frame `end` of a watch goes in its lanes.
fn position(end: u64) -> usize {
    (end % NODE_WINDOW_FRAMES as u64) as usize
}

/// One slot of the tap: a watched node's input and output lanes, moved by one frame count.
struct Watch {
    /// The node; `NO_NODE` while the slot is free.
    node: AtomicU32,
    input: Lane,
    output: Lane,
    /// Frames captured since the watch began. Only the output moves it: a segment's input is written first at the same
    /// frames, so the lanes stay paired frame by frame, also when the watch changes between the two writes.
    end: AtomicU64,
}

/// What only the control side changes: each slot's watch number and the next number to give out.
struct Numbers {
    slots: [u32; MAX_WATCHED_NODES],
    next: u32,
}

/// The tap on the watched graph nodes (`NodeScope`): the mixer offers every node's input before the node changes it and
/// its output after; a watched node's go into its slot's lanes. `MAX_WATCHED_NODES` slots are preallocated, so watching
/// another node never allocates on the audio thread.
pub struct NodeTap {
    slots: Box<[Watch]>,
    /// Whether any slot is taken: while none is, a node costs the audio thread one atomic load.
    any: AtomicBool,
    /// Control side only (`watch`, `windows`, which may come from several threads at once); the audio thread never
    /// takes it.
    numbers: Mutex<Numbers>,
}

impl NodeTap {
    pub fn new() -> Self {
        let slot = || Watch {
            node: AtomicU32::new(NO_NODE),
            input: Lane::new(NODE_WINDOW_FRAMES),
            output: Lane::new(NODE_WINDOW_FRAMES),
            end: AtomicU64::new(0),
        };
        Self {
            slots: (0..MAX_WATCHED_NODES).map(|_| slot()).collect(),
            any: AtomicBool::new(false),
            numbers: Mutex::new(Numbers { slots: [0; MAX_WATCHED_NODES], next: 0 }),
        }
    }

    /// Replaces the watched nodes (control side, see `NodeScope::watch_nodes`). A node leaving the set frees its slot at
    /// once; a joining one takes a free slot, cleared while the audio thread does not write it (at worst a segment that
    /// was finishing leaves a few samples, harmless for a picture), with a new watch number.
    pub fn watch(&self, nodes: &[NodeId]) {
        let mut numbers = lock(&self.numbers);
        let mut wanted: Vec<NodeId> = Vec::with_capacity(MAX_WATCHED_NODES);
        for &node in nodes {
            if node != NO_NODE && !wanted.contains(&node) && wanted.len() < MAX_WATCHED_NODES {
                wanted.push(node);
            }
        }
        for slot in &self.slots {
            if !wanted.contains(&slot.node.load(Ordering::Relaxed)) {
                slot.node.store(NO_NODE, Ordering::Release);
            }
        }
        for &node in &wanted {
            if self.slots.iter().any(|slot| slot.node.load(Ordering::Relaxed) == node) {
                continue;
            }
            // There is a free slot: at most `MAX_WATCHED_NODES` are wanted, and every slot not wanted was just freed.
            let Some((index, slot)) = self.slots.iter().enumerate().find(|(_, slot)| slot.node.load(Ordering::Relaxed) == NO_NODE) else {
                break;
            };
            slot.input.clear();
            slot.output.clear();
            slot.end.store(0, Ordering::Relaxed);
            numbers.slots[index] = numbers.next;
            numbers.next = numbers.next.wrapping_add(1);
            // Release: the audio thread that sees the node also sees the cleared lanes.
            slot.node.store(node, Ordering::Release);
        }
        self.any.store(!wanted.is_empty(), Ordering::Release);
    }

    /// The slot watching `node` (audio thread: atomic loads only).
    fn slot_of(&self, node: NodeId) -> Option<&Watch> {
        if node == NO_NODE || !self.any.load(Ordering::Acquire) {
            return None;
        }
        self.slots.iter().find(|slot| slot.node.load(Ordering::Acquire) == node)
    }

    /// Node `node`'s input segment (interleaved stereo) as mono if it is watched, before the node processes it (audio
    /// thread). It takes the frames its output will take and does not move the frame count.
    pub fn input(&self, node: NodeId, input: &[f32]) {
        if let Some(slot) = self.slot_of(node) {
            slot.input.write(position(slot.end.load(Ordering::Relaxed)), input);
        }
    }

    /// Node `node`'s output segment as mono if it is watched, after the node processed it (audio thread); the frame
    /// count moves past the segment.
    pub fn output(&self, node: NodeId, output: &[f32]) {
        if let Some(slot) = self.slot_of(node) {
            let end = slot.end.load(Ordering::Relaxed);
            slot.output.write(position(end), output);
            slot.end.store(end + (output.len() / 2) as u64, Ordering::Release);
        }
    }

    /// One window per watched node, in slot order (control side, see `NodeScope::node_windows`). A read between a
    /// segment's input and output sees that newest input in place of the oldest frame of a whole window: harmless for a
    /// picture.
    pub fn windows(&self, seen: &[NodeCursor], sample_rate: u32) -> Vec<NodeWindow> {
        let numbers = lock(&self.numbers);
        self.slots
            .iter()
            .zip(numbers.slots)
            .filter_map(|(slot, watch)| {
                let node = slot.node.load(Ordering::Acquire);
                if node == NO_NODE {
                    return None;
                }
                let end = slot.end.load(Ordering::Acquire);
                let whole = NODE_WINDOW_FRAMES as u64;
                // A cursor counts only within the same watch and not past what was captured; otherwise a whole window.
                let count = seen
                    .iter()
                    .find(|cursor| cursor.node == node && cursor.watch == watch && cursor.end <= end)
                    .map_or(whole, |cursor| (end - cursor.end).min(whole)) as usize;
                let at = position(end);
                Some(NodeWindow { node, watch, end, input: slot.input.read(at, count), output: slot.output.read(at, count), sample_rate })
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Interleaved stereo whose middle of the channels is `values`.
    fn stereo(values: impl Iterator<Item = f32>) -> Vec<f32> {
        values.flat_map(|v| [v - 1.0, v + 1.0]).collect()
    }

    #[test]
    fn nothing_is_copied_or_shown_while_off() {
        let scope = MasterScope::new();
        scope.write(&stereo((1..=10).map(|v| v as f32)));
        assert!(scope.read().is_empty());
        scope.watch(true);
        assert!(scope.read().iter().all(|v| *v == 0.0), "the write while off left nothing");
    }

    #[test]
    fn the_window_is_the_latest_frames_oldest_first_across_the_wrap() {
        let scope = MasterScope::new();
        scope.watch(true);
        scope.write(&stereo((1..=100).map(|v| v as f32)));
        let window = scope.read();
        assert_eq!(window.len(), MASTER_WINDOW_FRAMES);
        let (silence, played) = window.split_at(MASTER_WINDOW_FRAMES - 100);
        assert!(silence.iter().all(|v| *v == 0.0));
        assert_eq!(played, (1..=100).map(|v| v as f32).collect::<Vec<_>>());

        // More than a whole ring in total: only the newest MASTER_WINDOW_FRAMES remain, in order.
        let total = MASTER_WINDOW_FRAMES + 10;
        scope.write(&stereo((101..=total + 100).map(|v| v as f32)));
        let expected: Vec<f32> = (total + 100 - MASTER_WINDOW_FRAMES + 1..=total + 100).map(|v| v as f32).collect();
        assert_eq!(scope.read(), expected);
    }

    #[test]
    fn turning_on_again_clears_the_stale_window_but_a_repeated_on_does_not() {
        let scope = MasterScope::new();
        scope.watch(true);
        scope.write(&stereo((1..=10).map(|v| v as f32)));
        scope.watch(true);
        assert_eq!(scope.read().last(), Some(&10.0), "already on: the picture stays");
        scope.watch(false);
        assert!(scope.read().is_empty());
        scope.watch(true);
        assert!(scope.read().iter().all(|v| *v == 0.0), "the old window is gone");
    }

    const RATE: u32 = 48_000;

    /// Writes `values` as one segment of node `node`: its input, then its output ten times louder.
    fn segment(tap: &NodeTap, node: NodeId, values: impl Iterator<Item = f32> + Clone) {
        tap.input(node, &stereo(values.clone()));
        tap.output(node, &stereo(values.map(|v| 10.0 * v)));
    }

    fn cursor(window: &NodeWindow) -> NodeCursor {
        NodeCursor { node: window.node, watch: window.watch, end: window.end }
    }

    #[test]
    fn only_watched_nodes_are_copied_with_input_and_output_paired() {
        let tap = NodeTap::new();
        segment(&tap, 7, (1..=10).map(|v| v as f32));
        assert!(tap.windows(&[], RATE).is_empty(), "off at creation");

        tap.watch(&[7, 9]);
        for node in [7, 8, 9] {
            segment(&tap, node, (1..=10).map(|v| (node * 100 + v) as f32));
        }
        let windows = tap.windows(&[], RATE);
        assert_eq!(windows.iter().map(|w| w.node).collect::<Vec<_>>(), [7, 9], "node 8 is not copied");
        for window in &windows {
            assert_eq!((window.end, window.sample_rate), (10, RATE));
            assert_eq!((window.input.len(), window.output.len()), (NODE_WINDOW_FRAMES, NODE_WINDOW_FRAMES), "a whole window");
            let expected: Vec<f32> = (1..=10).map(|v| (window.node * 100 + v) as f32).collect();
            assert_eq!(window.input[NODE_WINDOW_FRAMES - 10..], expected);
            assert_eq!(window.output[NODE_WINDOW_FRAMES - 10..], expected.iter().map(|v| 10.0 * v).collect::<Vec<_>>());
        }
    }

    #[test]
    fn a_cursor_of_the_same_watch_gets_only_the_new_frames() {
        let tap = NodeTap::new();
        tap.watch(&[7]);
        segment(&tap, 7, (1..=10).map(|v| v as f32));
        let first = tap.windows(&[], RATE).remove(0);

        segment(&tap, 7, (11..=15).map(|v| v as f32));
        let next = tap.windows(&[cursor(&first)], RATE).remove(0);
        assert_eq!((next.watch, next.end), (first.watch, 15));
        assert_eq!(next.input, [11.0, 12.0, 13.0, 14.0, 15.0]);
        assert_eq!(next.output, [110.0, 120.0, 130.0, 140.0, 150.0]);
        let nothing = tap.windows(&[cursor(&next)], RATE).remove(0);
        assert!(nothing.input.is_empty() && nothing.output.is_empty(), "nothing new");

        let other_watch = NodeCursor { watch: first.watch.wrapping_add(1), ..cursor(&next) };
        let ahead = NodeCursor { end: 99, ..cursor(&next) };
        for stale in [other_watch, ahead] {
            assert_eq!(tap.windows(&[stale], RATE)[0].input.len(), NODE_WINDOW_FRAMES, "{stale:?}: a whole window");
        }
        segment(&tap, 7, (0..NODE_WINDOW_FRAMES + 5).map(|v| v as f32));
        let behind = tap.windows(&[cursor(&next)], RATE).remove(0);
        assert_eq!(behind.input.len(), NODE_WINDOW_FRAMES, "fell behind by more than a window: the last whole window");
        assert_eq!(behind.input.last(), Some(&((NODE_WINDOW_FRAMES + 4) as f32)));
    }

    #[test]
    fn a_staying_node_keeps_its_window_and_a_joining_one_starts_from_silence() {
        let tap = NodeTap::new();
        tap.watch(&[7]);
        segment(&tap, 7, (1..=10).map(|v| v as f32));
        let before = tap.windows(&[], RATE).remove(0);

        tap.watch(&[8, 7, 7]);
        let windows = tap.windows(&[], RATE);
        let kept = windows.iter().find(|w| w.node == 7).unwrap();
        assert_eq!((kept.watch, kept.end, kept.input.last()), (before.watch, 10, Some(&10.0)), "node 7 stays as it was");
        let joined = windows.iter().find(|w| w.node == 8).unwrap();
        assert_ne!(joined.watch, before.watch);
        assert_eq!(joined.end, 0);
        assert!(joined.input.iter().chain(&joined.output).all(|v| *v == 0.0), "node 8 starts from silence");
        assert_eq!(windows.len(), 2, "a repeated id counts once");

        tap.watch(&[8]);
        tap.watch(&[8, 7]);
        let again = tap.windows(&[], RATE).into_iter().find(|w| w.node == 7).unwrap();
        assert!(again.watch != before.watch && again.end == 0, "watched again: a new watch from silence");
        assert!(again.input.iter().all(|v| *v == 0.0));

        tap.watch(&[]);
        segment(&tap, 8, (1..=10).map(|v| v as f32));
        assert!(tap.windows(&[], RATE).is_empty(), "an empty set watches nothing");
    }

    #[test]
    fn at_most_max_watched_nodes_are_watched() {
        let tap = NodeTap::new();
        let nodes: Vec<NodeId> = (0..MAX_WATCHED_NODES as NodeId + 4).collect();
        tap.watch(&nodes);
        let watched: Vec<NodeId> = tap.windows(&[], RATE).iter().map(|w| w.node).collect();
        assert_eq!(watched, nodes[..MAX_WATCHED_NODES], "the first distinct ones");
    }

    #[test]
    fn the_input_waits_at_the_frames_of_its_output() {
        let tap = NodeTap::new();
        tap.watch(&[7]);
        tap.input(7, &stereo([1.0, 2.0].into_iter()));
        let staged = tap.windows(&[], RATE).remove(0);
        assert_eq!(staged.end, 0, "a staged input is not counted before its output");
        assert!(staged.output.iter().all(|v| *v == 0.0));

        // A switch between the two writes: the output of node 7 is not copied, and node 8's lanes stay paired.
        tap.watch(&[8]);
        tap.output(7, &stereo([3.0, 4.0].into_iter()));
        segment(&tap, 8, [5.0].into_iter());
        let window = tap.windows(&[], RATE).remove(0);
        assert_eq!((window.node, window.end), (8, 1));
        assert_eq!((window.input.last(), window.output.last()), (Some(&5.0), Some(&50.0)));
        assert!(window.input[..NODE_WINDOW_FRAMES - 1].iter().chain(&window.output[..NODE_WINDOW_FRAMES - 1]).all(|v| *v == 0.0));
    }
}
