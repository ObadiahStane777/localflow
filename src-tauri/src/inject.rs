//! Text injection (plan 0.6): save pasteboard → set transcript → synthetic ⌘V
//! → ~100 ms → restore pasteboard. Skips entirely (with a signal to the UI)
//! when a secure input field (password box) holds the keyboard.
//!
//! Phase 0 restores plain-text clipboard contents only; rich/image clipboard
//! restore and the AX-insertion + keystroke-synthesis fallbacks are later
//! phases.

use anyhow::{bail, Context, Result};
use enigo::{Direction, Enigo, Key, Keyboard, Settings};
use std::thread::sleep;
use std::time::Duration;

#[cfg(target_os = "macos")]
extern "C" {
    // Carbon; linked in build.rs.
    fn IsSecureEventInputEnabled() -> bool;
}

pub fn secure_input_active() -> bool {
    #[cfg(target_os = "macos")]
    unsafe {
        IsSecureEventInputEnabled()
    }
    #[cfg(not(target_os = "macos"))]
    false
}

/// Pastes `text` into the frontmost app's focused field.
///
/// Must run on the MAIN thread: NSPasteboard (arboard) and any keyboard
/// APIs enigo touches are main-thread-only on macOS 15 — the process gets
/// SIGTRAPped otherwise. Use `pipeline::inject_on_main`, not this directly.
pub fn inject(text: &str) -> Result<()> {
    if text.is_empty() {
        return Ok(());
    }
    if secure_input_active() {
        bail!("a secure input field is active — dictation into password fields is blocked");
    }

    let mut clipboard = arboard::Clipboard::new().context("opening clipboard")?;
    let saved = clipboard.get_text().ok();

    clipboard
        .set_text(text.to_string())
        .context("writing transcript to clipboard")?;

    // Let the pasteboard write become visible cross-process BEFORE ⌘V.
    // With no gap the synthetic paste races the write and the target app
    // reads the *previous* clipboard — the "pastes old copied text" bug.
    // ponytail: fixed settle; make it a setting only if some app still races.
    sleep(Duration::from_millis(40));

    // Key::Other(9) = kVK_ANSI_V (raw virtual keycode). Key::Unicode('v')
    // would trigger enigo's layout lookup via TSMGetInputSourceProperty,
    // which macOS 15 SIGTRAPs off the main thread (same class of crash as
    // the rdev one — see hotkey.rs). Raw keycodes skip layout APIs entirely.
    #[cfg(target_os = "macos")]
    const KEY_V: Key = Key::Other(9);
    #[cfg(not(target_os = "macos"))]
    const KEY_V: Key = Key::Unicode('v');

    let mut enigo =
        Enigo::new(&Settings::default()).context("initializing synthetic input (Accessibility granted?)")?;
    enigo.key(Key::Meta, Direction::Press).context("⌘ down")?;
    enigo.key(KEY_V, Direction::Click).context("V press")?;
    enigo.key(Key::Meta, Direction::Release).context("⌘ up")?;

    // Too early races the paste; too late loses the user's clipboard feel.
    // Rich web editors (ChatGPT's ProseMirror in Safari) read the clipboard
    // ASYNCHRONOUSLY after the paste event — 150 ms restored the old clipboard
    // before their JS read it, so they pasted stale text. 500 ms clears that.
    // ponytail: fixed delay; poll NSPasteboard changeCount only if some app
    // still reads later than this.
    sleep(Duration::from_millis(500));
    if let Some(prev) = saved {
        let _ = clipboard.set_text(prev);
    }
    Ok(())
}
