//! AppKit for plugin window keys: the local `NSEvent` monitor and the window a view is attached to. Main thread only.

use std::ptr::NonNull;
use std::sync::Arc;

use block2::RcBlock;
use objc2::MainThreadMarker;
use objc2::rc::Retained;
use objc2::runtime::AnyObject;
use objc2_app_kit::{NSEvent, NSEventMask, NSEventModifierFlags, NSEventType, NSView};

use super::piano::{KeyPlayer, KeyPress};

/// An enabled key monitor; while it is alive, keys in plugin windows play notes.
pub struct Monitor {
    handle: Retained<AnyObject>,
}

// SAFETY: the monitor is created and removed only on the main thread (`watch` is called from the plugin window's `attach`, and
// `Vst3Editors` never removes the monitor off the main thread, it deliberately keeps it); the `Mutex` in `EditorKeys` only holds its place.
unsafe impl Send for Monitor {}

/// Key of the window `view` is attached to (the `NSView` address); `None` when the view is not in a window yet.
pub fn window_of(view: usize) -> Option<usize> {
    if view == 0 {
        return None;
    }
    // SAFETY: the address is a live `NSView` the app passed to `attach`; it is not released on this thread.
    let view = unsafe { &*(view as *const NSView) };
    view.window().map(|window| Retained::as_ptr(&window) as usize)
}

impl Monitor {
    /// `None` when AppKit did not enable the monitor (returned nil).
    pub fn new(player: Arc<KeyPlayer>) -> Option<Self> {
        let handler = RcBlock::new(move |event: NonNull<NSEvent>| -> *mut NSEvent {
            // The monitor is called on the main thread; if somehow not, the key is left alone.
            let Some(main) = MainThreadMarker::new() else { return event.as_ptr() };
            // SAFETY: AppKit passes a live event for the duration of the call.
            let event_ref = unsafe { event.as_ref() };
            let down = event_ref.r#type() == NSEventType::KeyDown;
            let window = event_ref.window(main).map_or(0, |w| Retained::as_ptr(&w) as usize);
            let chord = event_ref
                .modifierFlags()
                .intersects(NSEventModifierFlags::Command | NSEventModifierFlags::Control | NSEventModifierFlags::Option);
            // `isARepeat` must not be asked of a key release (AppKit throws an exception), it may be asked of a press.
            let repeat = down && event_ref.isARepeat();
            let key = KeyPress { code: event_ref.keyCode(), down, repeat, chord };
            if player.press(window, key) { std::ptr::null_mut() } else { event.as_ptr() }
        });
        // SAFETY: the block returns either the same event or null, as AppKit requires; the monitor is removed in `Drop`.
        let handle = unsafe { NSEvent::addLocalMonitorForEventsMatchingMask_handler(NSEventMask::KeyDown | NSEventMask::KeyUp, &handler) };
        handle.map(|handle| Self { handle })
    }
}

impl Drop for Monitor {
    fn drop(&mut self) {
        // SAFETY: `handle` came from `addLocalMonitor…` and is removed exactly once.
        unsafe { NSEvent::removeMonitor(&self.handle) };
    }
}
