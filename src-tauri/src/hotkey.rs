//! Global push-to-talk hotkey (plan 0.2, remappable per plan 1.8).
//!
//! Implementation note: this was originally rdev, but rdev 0.5's event
//! callback calls TSM keyboard-layout APIs (`TSMGetInputSourceProperty`) off
//! the main thread, which macOS 15 kills with a dispatch-queue assertion
//! (SIGTRAP). A raw CGEventTap reading only keycode + flag bits never touches
//! layout APIs, so it is safe on a background thread. Requires Input
//! Monitoring.
//!
//! The active keycode lives in an AtomicI64 so Settings can remap without
//! reinstalling the tap. Modifier keys arrive as FlagsChanged (down/up derived
//! from the flag bit); plain keys (F13…) arrive as KeyDown/KeyUp.

use crossbeam_channel::Sender;
use std::sync::atomic::{AtomicI64, Ordering};
use std::sync::Arc;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HotkeyEvent {
    RecordStart,
    RecordStop,
    /// Right-arrow tapped while the push-to-talk key is held: lock the
    /// recording on so the user can release the hotkey (user request).
    Lock,
}

/// macOS virtual keycode for the right-arrow "lock" key.
const LOCK_KEYCODE: i64 = 124;

/// Keycodes offered in Settings. (label, macOS virtual keycode, is_modifier)
pub const HOTKEY_CHOICES: &[(&str, i64, bool)] = &[
    ("Right ⌘", 54, true),
    ("Right ⌥", 61, true),
    ("Fn / Globe", 63, true),
    ("F13", 105, false),
    ("F16", 106, false),
    ("F17", 64, false),
    ("F18", 79, false),
    ("F19", 80, false),
];

#[cfg(target_os = "macos")]
fn modifier_mask(keycode: i64) -> Option<core_graphics::event::CGEventFlags> {
    use core_graphics::event::CGEventFlags;
    match keycode {
        54 | 55 => Some(CGEventFlags::CGEventFlagCommand),
        58 | 61 => Some(CGEventFlags::CGEventFlagAlternate),
        59 | 62 => Some(CGEventFlags::CGEventFlagControl),
        56 | 60 => Some(CGEventFlags::CGEventFlagShift),
        63 => Some(CGEventFlags::CGEventFlagSecondaryFn),
        _ => None,
    }
}

/// Spawns the event-tap listener on its own thread. Returns immediately.
/// `active_keycode` may be swapped at any time (Settings remap).
#[cfg(target_os = "macos")]
pub fn spawn(tx: Sender<HotkeyEvent>, active_keycode: Arc<AtomicI64>) {
    use core_foundation::runloop::CFRunLoop;
    use core_graphics::event::{
        CGEventTap, CGEventTapLocation, CGEventTapOptions, CGEventTapPlacement, CGEventType,
        CallbackResult, EventField,
    };
    use std::cell::Cell;

    std::thread::Builder::new()
        .name("hotkey-listener".into())
        .spawn(move || {
            let held = Cell::new(false);
            // CGEventTapOptions::Default (an active tap) so the lock keypress
            // can be swallowed with CallbackResult::Drop — otherwise the →
            // would also move the caret in the target app.
            let result = CGEventTap::with_enabled(
                CGEventTapLocation::HID,
                CGEventTapPlacement::HeadInsertEventTap,
                CGEventTapOptions::Default,
                vec![
                    CGEventType::FlagsChanged,
                    CGEventType::KeyDown,
                    CGEventType::KeyUp,
                ],
                |_proxy, etype, event| {
                    let target = active_keycode.load(Ordering::Relaxed);
                    let keycode =
                        event.get_integer_value_field(EventField::KEYBOARD_EVENT_KEYCODE);
                    // Lock chord: → while the hotkey is held (swallowed).
                    if keycode == LOCK_KEYCODE && held.get() {
                        if matches!(etype, CGEventType::KeyDown) {
                            let _ = tx.send(HotkeyEvent::Lock);
                            // Holding a modifier hotkey means the release
                            // FlagsChanged will still arrive; lock handling in
                            // the pipeline ignores it. Treat the chord as
                            // ending the "held" tracking so a later re-press
                            // stops the recording.
                            held.set(false);
                        }
                        return CallbackResult::Drop;
                    }
                    if keycode != target {
                        return CallbackResult::Keep;
                    }
                    let down = match etype {
                        CGEventType::FlagsChanged => match modifier_mask(target) {
                            Some(mask) => event.get_flags().contains(mask),
                            None => return CallbackResult::Keep,
                        },
                        CGEventType::KeyDown => true,
                        CGEventType::KeyUp => false,
                        _ => return CallbackResult::Keep,
                    };
                    // Cell-tracked state debounces key-repeat KeyDowns and
                    // repeated FlagsChanged deliveries.
                    if down && !held.get() {
                        held.set(true);
                        let _ = tx.send(HotkeyEvent::RecordStart);
                    } else if !down && held.get() {
                        held.set(false);
                        let _ = tx.send(HotkeyEvent::RecordStop);
                    }
                    CallbackResult::Keep
                },
                || CFRunLoop::run_current(),
            );
            if result.is_err() {
                tracing::error!(
                    "failed to install hotkey event tap — is Input Monitoring granted?"
                );
            } else {
                tracing::warn!("hotkey event tap run loop exited");
            }
        })
        .expect("failed to spawn hotkey thread");
}

#[cfg(not(target_os = "macos"))]
pub fn spawn(_tx: Sender<HotkeyEvent>, _active_keycode: Arc<AtomicI64>) {
    // Windows hotkey backend lands with the Phase 1.9 port.
    tracing::warn!("global hotkey not implemented on this platform yet");
}
