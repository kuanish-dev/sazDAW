//! `GuardMap`: "keep the crash marker in a memory-mapped file" — the byte layout, atomic access to it and
//! reading the marker left by the previous session.
//!
//! The file is memory-mapped (`mmap`, shared): writing to it is a plain memory write without system calls, and
//! what is written survives a process crash (the pages stay in the kernel cache and go to disk). So the audio thread can
//! mark "I am inside plugin N" with one atomic store.
//!
//! Layout (numbers are little-endian, like the processor):
//! * `0..8` — the signature `DAWGRD01` (another file or an old format means no marker);
//! * `8..12` — the session is running (1) or closed normally (0);
//! * `12..16` — who is processing: slot number + 1, 0 means nobody;
//! * then `SLOTS` slots of `SLOT_BYTES`: the catalog kind and the plugin name, each `TEXT_BYTES` bytes of UTF-8 padded with zeros.

use std::fs::OpenOptions;
use std::io;
use std::path::Path;
use std::sync::atomic::{AtomicU8, AtomicU32, Ordering};

use memmap2::{MmapMut, MmapOptions};

/// How many plugin instances are marked at once (the rest work without a marker, with a log entry).
pub const SLOTS: usize = 256;
/// Text limit in a slot; longer text is cut at a character boundary.
pub const TEXT_BYTES: usize = 64;

const MAGIC: [u8; 8] = *b"DAWGRD01";
const RUNNING_AT: usize = 8;
const CURRENT_AT: usize = 12;
const SLOTS_AT: usize = 16;
const SLOT_BYTES: usize = 2 * TEXT_BYTES;
pub const FILE_BYTES: usize = SLOTS_AT + SLOTS * SLOT_BYTES;

/// A marker left by the previous session: it broke off while a plugin was processing.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Mark {
    pub kind: String,
    pub name: String,
}

pub struct GuardMap {
    /// Keeps the mapping alive; the data itself is read and written only through `base` with atomics.
    _map: MmapMut,
    base: *mut u8,
}

// SAFETY: `base` points into the mapping owned by this same struct (it does not move when `MmapMut` is moved
// and lives as long as the struct); all reads and writes through it are atomic, so concurrent access from different
// threads (audio thread, command thread) is not a data race.
unsafe impl Send for GuardMap {}
// SAFETY: see above: shared access is atomic operations only.
unsafe impl Sync for GuardMap {}

impl GuardMap {
    /// Reads the previous session's marker from `path` and starts a new one: the file is zeroed and the session marked running.
    /// No file means no previous marker.
    pub fn open(path: &Path) -> io::Result<(Self, Option<Mark>)> {
        let previous = match std::fs::read(path) {
            Ok(bytes) => previous_mark(&bytes),
            Err(error) if error.kind() == io::ErrorKind::NotFound => None,
            Err(error) => return Err(error),
        };
        let file = OpenOptions::new().read(true).write(true).create(true).truncate(true).open(path)?;
        file.set_len(FILE_BYTES as u64)?;
        // SAFETY: the file is ours (the app state folder) and was just truncated to the right size; no other process writes to it
        // while this app runs (the limitation is described in `crates/daw-vst3/CLAUDE.md`: one app per state folder).
        let map = unsafe { MmapOptions::new().len(FILE_BYTES).map_mut(&file) }?;
        Ok((Self::start(map), previous))
    }

    /// A marker in memory only (tests, sessions without a state folder): works the same but does not survive the process.
    pub fn anonymous() -> io::Result<Self> {
        Ok(Self::start(MmapOptions::new().len(FILE_BYTES).map_anon()?))
    }

    fn start(mut map: MmapMut) -> Self {
        let base = map.as_mut_ptr();
        let guard = Self { _map: map, base };
        for (offset, byte) in MAGIC.iter().enumerate() {
            guard.byte(offset).store(*byte, Ordering::Relaxed);
        }
        guard.set_running(true);
        guard
    }

    fn byte(&self, offset: usize) -> &AtomicU8 {
        debug_assert!(offset < FILE_BYTES);
        let pointer = self.base.wrapping_add(offset);
        // SAFETY: `offset` is less than the mapping size (all offsets come from the layout constants), the memory lives as long as `self`,
        // and is accessed only atomically (see `unsafe impl Sync`).
        unsafe { AtomicU8::from_ptr(pointer) }
    }

    fn word(&self, offset: usize) -> &AtomicU32 {
        debug_assert!(offset + 4 <= FILE_BYTES && offset.is_multiple_of(4));
        #[allow(clippy::cast_ptr_alignment, reason = "the mapping is page-aligned and the offset is a multiple of 4 (see SAFETY below)")]
        let pointer = self.base.wrapping_add(offset).cast::<u32>();
        // SAFETY: the offset is a multiple of 4 inside the mapping, and the mapping is page-aligned, so the pointer is aligned for
        // `u32`; the memory lives as long as `self` and is accessed only atomically.
        unsafe { AtomicU32::from_ptr(pointer) }
    }

    pub fn set_running(&self, running: bool) {
        self.word(RUNNING_AT).store(u32::from(running), Ordering::Relaxed);
    }

    /// Slot `slot` is processing. Only an atomic store: may be called from the audio thread.
    pub fn enter(&self, slot: usize) {
        self.word(CURRENT_AT).store(slot as u32 + 1, Ordering::Relaxed);
    }

    /// Slot `slot` left processing. Another slot's mark (another thread, e.g. export, may have set it) is not erased.
    pub fn leave(&self, slot: usize) {
        let _ = self.word(CURRENT_AT).compare_exchange(slot as u32 + 1, 0, Ordering::Relaxed, Ordering::Relaxed);
    }

    /// Labels the slot with its plugin (written on the control side before the first processing).
    pub fn write_slot(&self, slot: usize, kind: &str, name: &str) {
        let at = SLOTS_AT + slot * SLOT_BYTES;
        self.write_text(at, kind);
        self.write_text(at + TEXT_BYTES, name);
    }

    fn write_text(&self, at: usize, text: &str) {
        let bytes = clipped(text).as_bytes();
        for index in 0..TEXT_BYTES {
            self.byte(at + index).store(bytes.get(index).copied().unwrap_or(0), Ordering::Relaxed);
        }
    }
}

/// The start of `text` that fits into a slot (with a trailing zero), without cutting a character.
fn clipped(text: &str) -> &str {
    let mut end = text.len().min(TEXT_BYTES - 1);
    while !text.is_char_boundary(end) {
        end -= 1;
    }
    &text[..end]
}

/// The marker from the previous session's file bytes: present only if the session did not close normally and the audio thread was in a plugin.
pub fn previous_mark(bytes: &[u8]) -> Option<Mark> {
    if bytes.len() < FILE_BYTES || bytes[..MAGIC.len()] != MAGIC {
        return None;
    }
    let word = |at: usize| u32::from_le_bytes([bytes[at], bytes[at + 1], bytes[at + 2], bytes[at + 3]]);
    let current = word(CURRENT_AT) as usize;
    if word(RUNNING_AT) == 0 || current == 0 || current > SLOTS {
        return None;
    }
    let at = SLOTS_AT + (current - 1) * SLOT_BYTES;
    let text = |from: usize| {
        let field = &bytes[from..from + TEXT_BYTES];
        let end = field.iter().position(|b| *b == 0).unwrap_or(TEXT_BYTES);
        String::from_utf8_lossy(&field[..end]).into_owned()
    };
    let mark = Mark { kind: text(at), name: text(at + TEXT_BYTES) };
    (!mark.kind.is_empty()).then_some(mark)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp(name: &str) -> std::path::PathBuf {
        std::env::temp_dir().join(format!("daw-guard-{name}-{}.bin", std::process::id()))
    }

    #[test]
    fn a_session_that_died_inside_a_plugin_leaves_its_mark_and_a_clean_one_does_not() {
        let path = temp("cycle");
        std::fs::remove_file(&path).ok();
        let (map, previous) = GuardMap::open(&path).unwrap();
        assert_eq!(previous, None, "first start");
        map.write_slot(3, "vst3:surge_xt", "Surge XT");
        map.enter(3);
        drop(map); // a "crash": no normal exit, the marker stays in the file

        let (map, previous) = GuardMap::open(&path).unwrap();
        assert_eq!(previous, Some(Mark { kind: "vst3:surge_xt".into(), name: "Surge XT".into() }));
        map.write_slot(0, "vst3:distortion", "Distortion");
        map.enter(0);
        map.set_running(false); // a normal exit in the middle of processing
        drop(map);
        let (map, previous) = GuardMap::open(&path).unwrap();
        assert_eq!(previous, None, "a normally closed session is not a crash");
        drop(map);
        assert_eq!(GuardMap::open(&path).unwrap().1, None, "a crash outside a plugin (nobody processing) is not a plugin crash");
        std::fs::remove_file(path).ok();
    }

    #[test]
    fn leaving_clears_only_its_own_mark() {
        let map = GuardMap::anonymous().unwrap();
        map.write_slot(1, "vst3:a", "A");
        map.enter(1);
        map.leave(2);
        assert_eq!(map.word(CURRENT_AT).load(Ordering::Relaxed), 2, "another slot's exit does not touch the mark");
        map.leave(1);
        assert_eq!(map.word(CURRENT_AT).load(Ordering::Relaxed), 0);
    }

    #[test]
    fn foreign_or_short_files_and_long_names_are_handled() {
        assert_eq!(previous_mark(b"not a guard"), None);
        assert_eq!(previous_mark(&vec![0; FILE_BYTES]), None, "no signature");
        let mut bytes = vec![0; FILE_BYTES];
        bytes[..8].copy_from_slice(&MAGIC);
        bytes[RUNNING_AT] = 1;
        bytes[CURRENT_AT] = (SLOTS + 1) as u8;
        bytes[CURRENT_AT + 1] = 1;
        assert_eq!(previous_mark(&bytes), None, "slot number outside the table");

        let long = "Ж".repeat(40);
        let clip = clipped(&long);
        assert!(clip.len() < TEXT_BYTES && long.starts_with(clip) && clip.chars().count() == 31);
    }
}
