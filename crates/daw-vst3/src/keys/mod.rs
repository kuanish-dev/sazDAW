//! Plugin window keys. Job: listen to key presses in all app windows and hand to `KeyPlayer` those that came
//! to a plugin window (macOS only for now; on other systems plugin windows do not intercept keys).
//!
//! A plugin window has no web page, so the main window's hotkeys do not work in it. The `NSEvent` monitor
//! is local: it sees only the app's own events and needs no input permissions. Eaten keys (notes, octaves)
//! are not passed further, so the window does not beep at "unwanted" keys; the rest goes as usual.
//! Everything runs on the main thread: that is where both the monitor and `watch`/`unwatch` from `Vst3Editors` are called.
//!
//! * `piano` — `KeyPlayer`, the pure "key -> note" logic with tests;
//! * `mac` — the `NSEvent` monitor and finding a window by its view (AppKit).

#[cfg(target_os = "macos")]
mod mac;
#[cfg_attr(
    not(target_os = "macos"),
    allow(dead_code, reason = "only the macOS key monitor presses keys; the layout and its tests stay on every system")
)]
mod piano;

use std::sync::Arc;
#[cfg(target_os = "macos")]
use std::sync::Mutex;

#[cfg(target_os = "macos")]
use daw_contracts::sync::lock;
use daw_contracts::{AppLog, LiveNoteSink, TrackId};

#[cfg(target_os = "macos")]
use crate::journal;
use piano::KeyPlayer;

/// Plugin window keys: a window is asked to play on a track (`watch`) and released on close (`unwatch`).
pub struct EditorKeys {
    player: Arc<KeyPlayer>,
    log: Arc<dyn AppLog>,
    #[cfg(target_os = "macos")]
    monitor: Mutex<Option<mac::Monitor>>,
}

impl EditorKeys {
    pub fn new(live: Arc<dyn LiveNoteSink>, log: Arc<dyn AppLog>) -> Self {
        Self {
            player: Arc::new(KeyPlayer::new(live)),
            log,
            #[cfg(target_os = "macos")]
            monitor: Mutex::new(None),
        }
    }

    /// The window holding view `view` (the native view address) plays on track `track`. Returns the window key for `unwatch`,
    /// or `None` if the view is not in a window (or the system is not supported). The monitor is enabled with the first window: this is the main thread,
    /// as AppKit requires.
    pub fn watch(&self, view: usize, track: TrackId) -> Option<usize> {
        #[cfg(target_os = "macos")]
        {
            let mut monitor = lock(&self.monitor);
            if monitor.is_none() {
                match mac::Monitor::new(Arc::clone(&self.player)) {
                    Some(created) => *monitor = Some(created),
                    None => journal::warn(&*self.log, "plugin window keys do not work: AppKit did not enable the key monitor".into()),
                }
            }
            drop(monitor);
            let Some(window) = mac::window_of(view) else {
                journal::warn(
                    &*self.log,
                    "plugin window: the view is not attached to a window yet, keys in this window play no notes".into(),
                );
                return None;
            };
            self.player.watch(window, track);
            Some(window)
        }
        #[cfg(not(target_os = "macos"))]
        {
            let _ = (view, track, &self.player, &self.log);
            None
        }
    }

    pub fn unwatch(&self, window: usize) {
        self.player.unwatch(window);
    }

    /// Whether there is something that may be removed only on the main thread (an enabled monitor).
    pub fn has_monitor(&self) -> bool {
        #[cfg(target_os = "macos")]
        {
            lock(&self.monitor).is_some()
        }
        #[cfg(not(target_os = "macos"))]
        {
            false
        }
    }

    /// Not the main thread: the monitor cannot be removed without breaking AppKit's requirements, so it is deliberately kept until exit.
    pub fn abandon_monitor(&self) {
        #[cfg(target_os = "macos")]
        std::mem::forget(lock(&self.monitor).take());
    }
}
