//! What the audio thread shows the window: the oscilloscope and held notes.
//! Atomics only: the audio thread writes without locks or allocations, the window reads whenever it likes.
//!
//! The window and the sound may live in DIFFERENT plugin instances: the DAW host opens the window on a separate instance, while the sound goes through
//! the main one. So sounding instances are registered in a process-wide list (`register`), and the window picks from it
//! the one that sounds (`choose`). With a single instance (a regular host) the window shows that same one.

use std::sync::atomic::{AtomicU32, AtomicU64, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, OnceLock, PoisonError, Weak};
use std::time::Instant;

pub const SCOPE_LEN: usize = 512;
/// Every N-th frame goes into the oscilloscope: the window covers ~85 ms at 48 kHz.
pub const SCOPE_DECIMATION: usize = 8;

/// How many milliseconds the window holds on to the chosen instance after its last sound before looking for another.
const HOLD_MS: u64 = 500;

pub struct Visuals {
    /// When the instance last sounded: ms since process start plus 1 (0 means it has not sounded yet).
    last_sound: AtomicU64,
    scope: [AtomicU32; SCOPE_LEN],
    write: AtomicUsize,
    notes_low: AtomicU64,
    notes_high: AtomicU64,
}

impl Default for Visuals {
    fn default() -> Self {
        Self {
            last_sound: AtomicU64::new(0),
            scope: std::array::from_fn(|_| AtomicU32::new(0)),
            write: AtomicUsize::new(0),
            notes_low: AtomicU64::new(0),
            notes_high: AtomicU64::new(0),
        }
    }
}

fn now_ms() -> u64 {
    static START: OnceLock<Instant> = OnceLock::new();
    START.get_or_init(Instant::now).elapsed().as_millis() as u64 + 1
}

/// All plugin instances in the process that have sound (registered in `initialize`).
fn registry() -> &'static Mutex<Vec<Weak<Visuals>>> {
    static REGISTRY: OnceLock<Mutex<Vec<Weak<Visuals>>>> = OnceLock::new();
    REGISTRY.get_or_init(|| Mutex::new(Vec::new()))
}

/// Remembers the instance in the process list; called off the audio thread.
pub fn register(visuals: &Arc<Visuals>) {
    let mut list = registry().lock().unwrap_or_else(PoisonError::into_inner);
    list.retain(|weak| weak.strong_count() > 0);
    if !list.iter().any(|weak| weak.as_ptr() == Arc::as_ptr(visuals)) {
        list.push(Arc::downgrade(visuals));
    }
}

/// Which instance to show in window `own`: the previous one while it sounds; otherwise the one that sounded last; otherwise its own.
pub fn choose(current: Option<&Arc<Visuals>>, candidates: &[Arc<Visuals>], own: &Arc<Visuals>, now: u64) -> Arc<Visuals> {
    if let Some(current) = current {
        let last = current.last_sound.load(Ordering::Relaxed);
        if last > 0 && now.saturating_sub(last) <= HOLD_MS {
            return Arc::clone(current);
        }
    }
    candidates
        .iter()
        .chain(std::iter::once(own))
        .filter(|v| v.last_sound.load(Ordering::Relaxed) > 0)
        .max_by_key(|v| v.last_sound.load(Ordering::Relaxed))
        .map_or_else(|| Arc::clone(own), Arc::clone)
}

/// The window calls this every frame: returns the instance to show now and remembers the choice in `current`.
pub fn follow(own: &Arc<Visuals>, current: &mut Option<Arc<Visuals>>) -> Arc<Visuals> {
    let candidates: Vec<Arc<Visuals>> =
        registry().lock().unwrap_or_else(PoisonError::into_inner).iter().filter_map(Weak::upgrade).collect();
    let chosen = choose(current.as_ref(), &candidates, own, now_ms());
    *current = Some(Arc::clone(&chosen));
    chosen
}

impl Visuals {
    /// Marks that the instance is sounding right now (atomics only — fine from the audio thread).
    pub fn mark_sound(&self) {
        self.last_sound.store(now_ms(), Ordering::Relaxed);
    }

    /// Whether the instance is sounding: there was sound no longer than `HOLD_MS` ago.
    pub fn is_sounding(&self) -> bool {
        let last = self.last_sound.load(Ordering::Relaxed);
        last > 0 && now_ms().saturating_sub(last) <= HOLD_MS
    }

    pub fn push_sample(&self, value: f32) {
        let at = self.write.load(Ordering::Relaxed);
        self.scope[at % SCOPE_LEN].store(value.to_bits(), Ordering::Relaxed);
        self.write.store(at.wrapping_add(1), Ordering::Release);
    }

    pub fn set_notes(&self, mask: u128) {
        self.notes_low.store(mask as u64, Ordering::Relaxed);
        self.notes_high.store((mask >> 64) as u64, Ordering::Relaxed);
    }

    pub fn notes(&self) -> u128 {
        u128::from(self.notes_low.load(Ordering::Relaxed)) | (u128::from(self.notes_high.load(Ordering::Relaxed)) << 64)
    }

    /// The oscilloscope from the oldest sample to the newest.
    pub fn scope(&self) -> Vec<f32> {
        let end = self.write.load(Ordering::Acquire);
        (end..end + SCOPE_LEN).map(|i| f32::from_bits(self.scope[i % SCOPE_LEN].load(Ordering::Relaxed))).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sounded_at(ms: u64) -> Arc<Visuals> {
        let v = Arc::new(Visuals::default());
        v.last_sound.store(ms, Ordering::Relaxed);
        v
    }

    #[test]
    fn the_window_keeps_its_instance_while_it_sounds_and_switches_when_it_stops() {
        let (own, a, b) = (sounded_at(0), sounded_at(1_000), sounded_at(1_200));
        let both = [a.clone(), b.clone()];
        assert!(Arc::ptr_eq(&choose(Some(&a), &both, &own, 1_300), &a), "a still sounds — do not jump to b");
        assert!(Arc::ptr_eq(&choose(Some(&a), &both, &own, 2_000), &b), "a went silent — take the one that sounded last");
        assert!(Arc::ptr_eq(&choose(None, &both, &own, 1_300), &b));
    }

    #[test]
    fn without_any_sound_the_window_shows_its_own_instance() {
        let (own, silent) = (sounded_at(0), sounded_at(0));
        assert!(Arc::ptr_eq(&choose(None, &[silent], &own, 5_000), &own));
    }

    #[test]
    fn a_single_instance_host_shows_itself_when_it_sounds() {
        let own = sounded_at(900);
        assert!(Arc::ptr_eq(&choose(None, &[], &own, 1_000), &own));
    }

    #[test]
    fn registered_instances_are_listed_once_and_dropped_with_their_owner() {
        let v = Arc::new(Visuals::default());
        register(&v);
        register(&v);
        let count = |p: *const Visuals| registry().lock().unwrap().iter().filter(|w| w.as_ptr() == p).count();
        assert_eq!(count(Arc::as_ptr(&v)), 1);
        let ptr = Arc::as_ptr(&v);
        drop(v);
        register(&Arc::new(Visuals::default()));
        assert_eq!(count(ptr), 0, "a dead instance is removed from the list");
    }

    #[test]
    fn a_window_on_another_instance_follows_the_registered_instance_that_sounds() {
        let (playing, window) = (Arc::new(Visuals::default()), Arc::new(Visuals::default()));
        register(&playing);
        playing.mark_sound();
        let mut current = None;
        let shown = follow(&window, &mut current);
        assert!(Arc::ptr_eq(&shown, &playing), "the window shows the sounding instance, not its own silent one");
        assert!(shown.is_sounding());
        assert!(!window.is_sounding());
    }

    #[test]
    fn scope_is_ordered_oldest_first_and_wraps() {
        let v = Visuals::default();
        for i in 0..SCOPE_LEN + 10 {
            v.push_sample(i as f32);
        }
        let scope = v.scope();
        assert_eq!(scope[0], 10.0);
        assert_eq!(scope[SCOPE_LEN - 1], (SCOPE_LEN + 9) as f32);
    }

    #[test]
    fn notes_roundtrip_across_both_halves() {
        let v = Visuals::default();
        let mask = (1u128 << 3) | (1u128 << 100);
        v.set_notes(mask);
        assert_eq!(v.notes(), mask);
    }
}
