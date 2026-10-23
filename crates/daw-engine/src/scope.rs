//! Live windows of sound for the interface: the master window for the visualizer (`EngineStats::master_window`), the
//! windows of the captured graph nodes for their pictures (`NodeScope`, the live visuals of the effects, one editor or a
//! whole signal path at once), both kept in `Lane`s, and every frame of the stereo master for the live meter
//! (`MasterTap`, `MeterTap`). Atomics only on the audio thread: it writes into preallocated rings, the control side
//! copies them out. The windows have no switch: a read holds a lease on its capture (`CAPTURE_LEASE_SECONDS`), counted
//! on the engine's `FrameClock`, and while no lease runs, the audio thread copies nothing.

use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

use daw_contracts::sync::lock;
use daw_contracts::{
    CAPTURE_LEASE_SECONDS, MASTER_TAP_FRAMES, MASTER_WINDOW_FRAMES, MAX_WATCHED_NODES, MasterFrames, NODE_WINDOW_FRAMES, NodeCursor,
    NodeId, NodeWindow,
};

use crate::SEGMENT_FRAMES;

/// Frames the engine has rendered since it was created: the clock of the capture leases. The audio thread moves it once
/// per segment, both sides read it. Why frames and not wall time: the audio thread checks a lease with one atomic load
/// and no clock call, and while no audio runs nothing is copied anyway, so a lease may as well stand still.
pub struct FrameClock(AtomicU64);

impl FrameClock {
    pub fn new() -> Self {
        Self(AtomicU64::new(0))
    }

    pub fn now(&self) -> u64 {
        self.0.load(Ordering::Acquire)
    }

    /// The audio thread rendered `frames` more (the only writer).
    pub fn advance(&self, frames: usize) {
        self.0.store(self.0.load(Ordering::Relaxed) + frames as u64, Ordering::Release);
    }
}

/// The frame up to which a capture read now is copied: `CAPTURE_LEASE_SECONDS` at `sample_rate` after `now`.
fn lease_end(now: u64, sample_rate: u32) -> u64 {
    now + (CAPTURE_LEASE_SECONDS * sample_rate as f32) as u64
}

/// A ring of mono frames. The position (where the next frame goes, which is also the oldest one) is kept by the owner:
/// a captured node moves two lanes with one frame count, so its input and output stay paired frame by frame.
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
    clock: Arc<FrameClock>,
    /// The lease: the audio thread copies while the clock is before this frame.
    until: AtomicU64,
    lane: Lane,
    position: AtomicUsize,
}

impl MasterScope {
    pub fn new(clock: Arc<FrameClock>) -> Self {
        Self { clock, until: AtomicU64::new(0), lane: Lane::new(MASTER_WINDOW_FRAMES), position: AtomicUsize::new(0) }
    }

    /// Adds a segment of the interleaved stereo master as mono (audio thread).
    pub fn write(&self, master: &[f32]) {
        if self.clock.now() < self.until.load(Ordering::Acquire) {
            let next = self.lane.write(self.position.load(Ordering::Relaxed), master);
            self.position.store(next, Ordering::Release);
        }
    }

    /// The window oldest first, and the lease renewed (control side, see `EngineStats::master_window`). A read that
    /// finds the lease run out clears the lane first, so a stale window from the last time is not shown; the audio
    /// thread is not copying then (at worst a segment that was finishing leaves a few samples, harmless for a picture).
    pub fn read(&self, sample_rate: u32) -> Vec<f32> {
        let now = self.clock.now();
        let running = now < self.until.load(Ordering::Relaxed);
        if !running {
            self.lane.clear();
        }
        self.until.store(lease_end(now, sample_rate), Ordering::Release);
        if running { self.lane.read(self.position.load(Ordering::Acquire), MASTER_WINDOW_FRAMES) } else { Vec::new() }
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

/// One slot of the tap: a captured node's input and output lanes, moved by one frame count.
struct Watch {
    /// The node; `NO_NODE` while the slot was never taken or is being handed to another node.
    node: AtomicU32,
    /// The lease: the audio thread copies the node while the clock is before this frame; once it is past, the slot is
    /// free.
    until: AtomicU64,
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

/// The tap on the captured graph nodes (`NodeScope`): the mixer offers every node's input before the node changes it
/// and its output after; a captured node's go into its slot's lanes. `MAX_WATCHED_NODES` slots are preallocated, so
/// capturing another node never allocates on the audio thread.
pub struct NodeTap {
    clock: Arc<FrameClock>,
    slots: Box<[Watch]>,
    /// The latest lease of any slot: once the clock is past it, a node costs the audio thread two atomic loads.
    any_until: AtomicU64,
    /// Control side only (`windows`, which may come from several threads at once); the audio thread never takes it.
    numbers: Mutex<Numbers>,
}

impl NodeTap {
    pub fn new(clock: Arc<FrameClock>) -> Self {
        let slot = || Watch {
            node: AtomicU32::new(NO_NODE),
            until: AtomicU64::new(0),
            input: Lane::new(NODE_WINDOW_FRAMES),
            output: Lane::new(NODE_WINDOW_FRAMES),
            end: AtomicU64::new(0),
        };
        Self {
            clock,
            slots: (0..MAX_WATCHED_NODES).map(|_| slot()).collect(),
            any_until: AtomicU64::new(0),
            numbers: Mutex::new(Numbers { slots: [0; MAX_WATCHED_NODES], next: 0 }),
        }
    }

    /// The slot capturing `node` (audio thread: atomic loads only).
    fn slot_of(&self, node: NodeId) -> Option<&Watch> {
        let now = self.clock.now();
        if node == NO_NODE || now >= self.any_until.load(Ordering::Acquire) {
            return None;
        }
        self.slots.iter().find(|slot| slot.node.load(Ordering::Acquire) == node && now < slot.until.load(Ordering::Acquire))
    }

    /// Node `node`'s input segment (interleaved stereo) as mono if it is captured, before the node processes it (audio
    /// thread). It takes the frames its output will take and does not move the frame count.
    pub fn input(&self, node: NodeId, input: &[f32]) {
        if let Some(slot) = self.slot_of(node) {
            slot.input.write(position(slot.end.load(Ordering::Relaxed)), input);
        }
    }

    /// Node `node`'s output segment as mono if it is captured, after the node processed it (audio thread); the frame
    /// count moves past the segment.
    pub fn output(&self, node: NodeId, output: &[f32]) {
        if let Some(slot) = self.slot_of(node) {
            let end = slot.end.load(Ordering::Relaxed);
            slot.output.write(position(end), output);
            slot.end.store(end + (output.len() / 2) as u64, Ordering::Release);
        }
    }

    /// One window per captured node of `nodes`, in their order, with their leases renewed (control side, see
    /// `NodeScope::node_windows`). A read between a segment's input and output sees that newest input in place of the
    /// oldest frame of a whole window: harmless for a picture.
    pub fn windows(&self, nodes: &[NodeId], seen: &[NodeCursor], sample_rate: u32) -> Vec<NodeWindow> {
        let mut numbers = lock(&self.numbers);
        let now = self.clock.now();
        let until = lease_end(now, sample_rate);
        let mut read = [false; MAX_WATCHED_NODES];
        let mut windows = Vec::with_capacity(nodes.len().min(MAX_WATCHED_NODES));
        for (i, &node) in nodes.iter().enumerate() {
            if node == NO_NODE || nodes[..i].contains(&node) {
                continue;
            }
            let running =
                self.slots.iter().position(|slot| slot.node.load(Ordering::Relaxed) == node && now < slot.until.load(Ordering::Relaxed));
            let Some(index) = running.or_else(|| self.start(node, &read, &mut numbers)) else {
                continue;
            };
            read[index] = true;
            let slot = &self.slots[index];
            slot.until.store(until, Ordering::Release);
            windows.push(Self::window(slot, node, numbers.slots[index], seen, sample_rate));
        }
        if !windows.is_empty() {
            self.any_until.store(until, Ordering::Release);
        }
        windows
    }

    /// Hands `node` a slot with a new watch: a free one (never taken, or its lease ran out), else the one read longest
    /// ago, but none read in this call; `None` when all were. The lanes are cleared while the audio thread does not
    /// write them (at worst a segment that was finishing leaves a few samples, harmless for a picture).
    fn start(&self, node: NodeId, read: &[bool; MAX_WATCHED_NODES], numbers: &mut Numbers) -> Option<usize> {
        let (index, slot) =
            self.slots.iter().enumerate().filter(|(index, _)| !read[*index]).min_by_key(|(_, slot)| slot.until.load(Ordering::Relaxed))?;
        slot.node.store(NO_NODE, Ordering::Release);
        slot.input.clear();
        slot.output.clear();
        slot.end.store(0, Ordering::Relaxed);
        numbers.slots[index] = numbers.next;
        numbers.next = numbers.next.wrapping_add(1);
        slot.node.store(node, Ordering::Release);
        Some(index)
    }

    /// The window of `node` in `slot` for a reader that has the cursors `seen`.
    fn window(slot: &Watch, node: NodeId, watch: u32, seen: &[NodeCursor], sample_rate: u32) -> NodeWindow {
        let end = slot.end.load(Ordering::Acquire);
        let whole = NODE_WINDOW_FRAMES as u64;
        let count = seen
            .iter()
            .find(|cursor| cursor.node == node && cursor.watch == watch && cursor.end <= end)
            .map_or(whole, |cursor| (end - cursor.end).min(whole)) as usize;
        let at = position(end);
        NodeWindow { node, watch, end, input: slot.input.read(at, count), output: slot.output.read(at, count), sample_rate }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const RATE: u32 = 48_000;

    /// Frames a read's lease lasts at `RATE`.
    fn lease() -> usize {
        lease_end(0, RATE) as usize
    }

    /// Interleaved stereo whose middle of the channels is `values`.
    fn stereo(values: impl Iterator<Item = f32>) -> Vec<f32> {
        values.flat_map(|v| [v - 1.0, v + 1.0]).collect()
    }

    fn master_scope() -> (Arc<FrameClock>, MasterScope) {
        let clock = Arc::new(FrameClock::new());
        (Arc::clone(&clock), MasterScope::new(clock))
    }

    #[test]
    fn nothing_is_copied_until_read_and_the_first_read_starts_the_capture() {
        let (_, scope) = master_scope();
        scope.write(&stereo((1..=10).map(|v| v as f32)));
        assert!(scope.read(RATE).is_empty(), "the first read starts the capture and has nothing yet");
        assert!(scope.read(RATE).iter().all(|v| *v == 0.0), "the write before it left nothing");
    }

    #[test]
    fn the_window_is_the_latest_frames_oldest_first_across_the_wrap() {
        let (_, scope) = master_scope();
        scope.read(RATE);
        scope.write(&stereo((1..=100).map(|v| v as f32)));
        let window = scope.read(RATE);
        assert_eq!(window.len(), MASTER_WINDOW_FRAMES);
        let (silence, played) = window.split_at(MASTER_WINDOW_FRAMES - 100);
        assert!(silence.iter().all(|v| *v == 0.0));
        assert_eq!(played, (1..=100).map(|v| v as f32).collect::<Vec<_>>());

        let total = MASTER_WINDOW_FRAMES + 10;
        scope.write(&stereo((101..=total + 100).map(|v| v as f32)));
        let expected: Vec<f32> = (total + 100 - MASTER_WINDOW_FRAMES + 1..=total + 100).map(|v| v as f32).collect();
        assert_eq!(scope.read(RATE), expected);
    }

    #[test]
    fn reads_keep_the_capture_going_and_without_them_it_stops_and_starts_over() {
        let (clock, scope) = master_scope();
        scope.read(RATE);
        scope.write(&stereo((1..=10).map(|v| v as f32)));
        clock.advance(lease() - 1);
        assert_eq!(scope.read(RATE).last(), Some(&10.0), "read within the lease: the picture stays");
        clock.advance(lease() - 1);
        scope.write(&stereo((11..=20).map(|v| v as f32)));
        assert_eq!(scope.read(RATE).last(), Some(&20.0), "the read renewed the lease");

        clock.advance(lease());
        scope.write(&stereo((21..=30).map(|v| v as f32)));
        assert!(scope.read(RATE).is_empty(), "nobody read for a whole lease: nothing copied, the read starts it again");
        assert!(scope.read(RATE).iter().all(|v| *v == 0.0), "the old window is gone");
    }

    fn node_tap() -> (Arc<FrameClock>, NodeTap) {
        let clock = Arc::new(FrameClock::new());
        (Arc::clone(&clock), NodeTap::new(clock))
    }

    /// Writes `values` as one segment of node `node`: its input, then its output ten times louder.
    fn segment(tap: &NodeTap, node: NodeId, values: impl Iterator<Item = f32> + Clone) {
        tap.input(node, &stereo(values.clone()));
        tap.output(node, &stereo(values.map(|v| 10.0 * v)));
    }

    fn cursor(window: &NodeWindow) -> NodeCursor {
        NodeCursor { node: window.node, watch: window.watch, end: window.end }
    }

    /// The window of the one node `node` (no cursor).
    fn window_of(tap: &NodeTap, node: NodeId) -> NodeWindow {
        tap.windows(&[node], &[], RATE).remove(0)
    }

    #[test]
    fn only_read_nodes_are_copied_with_input_and_output_paired() {
        let (_, tap) = node_tap();
        segment(&tap, 7, (1..=10).map(|v| v as f32));
        assert!(tap.windows(&[], &[], RATE).is_empty(), "no node asked for");
        let started = tap.windows(&[9, 7], &[], RATE);
        assert_eq!(started.iter().map(|w| w.node).collect::<Vec<_>>(), [9, 7], "in the order asked");
        assert!(started.iter().all(|w| w.end == 0 && w.input.iter().all(|v| *v == 0.0)), "the segment before the read is not in it");

        for node in [7, 8, 9] {
            segment(&tap, node, (1..=10).map(|v| (node * 100 + v) as f32));
        }
        for window in tap.windows(&[7, 9], &[], RATE) {
            assert_eq!((window.end, window.sample_rate), (10, RATE));
            assert_eq!((window.input.len(), window.output.len()), (NODE_WINDOW_FRAMES, NODE_WINDOW_FRAMES), "a whole window");
            let expected: Vec<f32> = (1..=10).map(|v| (window.node * 100 + v) as f32).collect();
            assert_eq!(window.input[NODE_WINDOW_FRAMES - 10..], expected);
            assert_eq!(window.output[NODE_WINDOW_FRAMES - 10..], expected.iter().map(|v| 10.0 * v).collect::<Vec<_>>());
        }
        let unread = window_of(&tap, 8);
        assert!(unread.end == 0 && unread.input.iter().all(|v| *v == 0.0), "node 8 was not copied before its first read");
    }

    #[test]
    fn a_cursor_of_the_same_watch_gets_only_the_new_frames() {
        let (_, tap) = node_tap();
        window_of(&tap, 7);
        segment(&tap, 7, (1..=10).map(|v| v as f32));
        let first = window_of(&tap, 7);

        segment(&tap, 7, (11..=15).map(|v| v as f32));
        let next = tap.windows(&[7], &[cursor(&first)], RATE).remove(0);
        assert_eq!((next.watch, next.end), (first.watch, 15));
        assert_eq!(next.input, [11.0, 12.0, 13.0, 14.0, 15.0]);
        assert_eq!(next.output, [110.0, 120.0, 130.0, 140.0, 150.0]);
        let nothing = tap.windows(&[7], &[cursor(&next)], RATE).remove(0);
        assert!(nothing.input.is_empty() && nothing.output.is_empty(), "nothing new");

        let other_watch = NodeCursor { watch: first.watch.wrapping_add(1), ..cursor(&next) };
        let ahead = NodeCursor { end: 99, ..cursor(&next) };
        for stale in [other_watch, ahead] {
            assert_eq!(tap.windows(&[7], &[stale], RATE)[0].input.len(), NODE_WINDOW_FRAMES, "{stale:?}: a whole window");
        }
        segment(&tap, 7, (0..NODE_WINDOW_FRAMES + 5).map(|v| v as f32));
        let behind = tap.windows(&[7], &[cursor(&next)], RATE).remove(0);
        assert_eq!(behind.input.len(), NODE_WINDOW_FRAMES, "fell behind by more than a window: the last whole window");
        assert_eq!(behind.input.last(), Some(&((NODE_WINDOW_FRAMES + 4) as f32)));
    }

    #[test]
    fn a_node_read_again_keeps_its_window_and_one_unread_for_a_lease_starts_over() {
        let (clock, tap) = node_tap();
        window_of(&tap, 7);
        segment(&tap, 7, (1..=10).map(|v| v as f32));
        let before = window_of(&tap, 7);

        let windows = tap.windows(&[8, 7, 7], &[], RATE);
        assert_eq!(windows.len(), 2, "a repeated id counts once");
        let kept = windows.iter().find(|w| w.node == 7).unwrap();
        assert_eq!((kept.watch, kept.end, kept.input.last()), (before.watch, 10, Some(&10.0)), "node 7 stays as it was");
        let joined = windows.iter().find(|w| w.node == 8).unwrap();
        assert_ne!(joined.watch, before.watch);
        assert_eq!(joined.end, 0);
        assert!(joined.input.iter().chain(&joined.output).all(|v| *v == 0.0), "node 8 starts from silence");

        clock.advance(lease() / 2);
        window_of(&tap, 8);
        clock.advance(lease() / 2);
        segment(&tap, 7, (11..=20).map(|v| v as f32));
        segment(&tap, 8, (11..=20).map(|v| v as f32));
        assert_eq!(window_of(&tap, 8).end, 10, "node 8 kept its lease");
        let again = window_of(&tap, 7);
        assert!(again.watch != before.watch && again.end == 0, "node 7's lease ran out: a new watch from silence");
        assert!(again.input.iter().all(|v| *v == 0.0));
    }

    #[test]
    fn a_full_tap_hands_the_slot_read_longest_ago_to_a_new_node() {
        let (clock, tap) = node_tap();
        let nodes: Vec<NodeId> = (0..MAX_WATCHED_NODES as NodeId + 4).collect();
        let first = tap.windows(&nodes, &[], RATE);
        assert_eq!(
            first.iter().map(|w| w.node).collect::<Vec<_>>(),
            nodes[..MAX_WATCHED_NODES],
            "the rest found every slot read in this call"
        );

        clock.advance(1);
        let others = &nodes[1..MAX_WATCHED_NODES];
        tap.windows(others, &[], RATE);
        clock.advance(1);
        assert_eq!(window_of(&tap, 100).node, 100, "node 0, read longest ago, gave up its slot");
        let watches = |windows: &[NodeWindow]| windows.iter().map(|w| (w.node, w.watch)).collect::<Vec<_>>();
        assert_eq!(watches(&tap.windows(others, &[], RATE)), watches(&first[1..]), "the others keep their watches");
    }

    #[test]
    fn the_input_waits_at_the_frames_of_its_output() {
        let (clock, tap) = node_tap();
        let nodes: Vec<NodeId> = (0..MAX_WATCHED_NODES as NodeId).collect();
        tap.windows(&nodes, &[], RATE);
        clock.advance(1);
        tap.windows(&nodes.iter().copied().filter(|node| *node != 7).collect::<Vec<_>>(), &[], RATE);
        tap.input(7, &stereo([1.0, 2.0].into_iter()));
        let staged = tap.windows(&[7], &[], RATE).remove(0);
        assert_eq!(staged.end, 0, "a staged input is not counted before its output");
        assert!(staged.output.iter().all(|v| *v == 0.0));

        clock.advance(1);
        tap.windows(&nodes.iter().copied().filter(|node| *node != 7).collect::<Vec<_>>(), &[], RATE);
        window_of(&tap, 100);
        tap.output(7, &stereo([3.0, 4.0].into_iter()));
        segment(&tap, 100, [5.0].into_iter());
        let window = window_of(&tap, 100);
        assert_eq!(window.end, 1);
        assert_eq!((window.input.last(), window.output.last()), (Some(&5.0), Some(&50.0)));
        assert!(window.input[..NODE_WINDOW_FRAMES - 1].iter().chain(&window.output[..NODE_WINDOW_FRAMES - 1]).all(|v| *v == 0.0));
    }
}
