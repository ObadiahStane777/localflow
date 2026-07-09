//! Active-app detection for Styles (plan 2.6) and selected-text capture for
//! Command Mode (plan 2.8). Deliberately NO screenshots — app identity gives
//! most of the context value with none of the privacy blowback (RESEARCH §4.5).
//!
//! Both functions must run on the MAIN thread (NSWorkspace / AX APIs);
//! pipeline calls them through `snapshot_on_main`.

use anyhow::Result;
use tauri::AppHandle;

/// What the pipeline needs to know about the world at record time.
#[derive(Clone, Debug, Default)]
pub struct AppContext {
    pub bundle_id: String,
    /// Selected text in the focused element, when readable via AX.
    pub selection: Option<String>,
}

/// Captures frontmost bundle id + current selection from the main thread.
pub fn snapshot_on_main(app: &AppHandle) -> AppContext {
    let (tx, rx) = std::sync::mpsc::channel();
    let ok = app.run_on_main_thread(move || {
        let ctx = AppContext {
            bundle_id: frontmost_bundle_id().unwrap_or_default(),
            selection: read_ax_selection().ok().flatten().filter(|s| !s.is_empty()),
        };
        let _ = tx.send(ctx);
    });
    if ok.is_err() {
        return AppContext::default();
    }
    rx.recv_timeout(std::time::Duration::from_millis(500))
        .unwrap_or_default()
}

#[cfg(target_os = "macos")]
fn frontmost_bundle_id() -> Option<String> {
    use objc2_app_kit::NSWorkspace;
    unsafe {
        let ws = NSWorkspace::sharedWorkspace();
        let app = ws.frontmostApplication()?;
        app.bundleIdentifier().map(|s| s.to_string())
    }
}

#[cfg(not(target_os = "macos"))]
fn frontmost_bundle_id() -> Option<String> {
    None // GetForegroundWindow + process name lands with the Windows port.
}

// ---- AX selected text (macOS) ----

#[cfg(target_os = "macos")]
mod ax {
    use core_foundation::base::{CFRelease, CFTypeRef, TCFType};
    use core_foundation::string::{CFString, CFStringRef};
    use std::ffi::c_void;

    #[repr(C)]
    pub struct __AXUIElement(c_void);
    pub type AXUIElementRef = *const __AXUIElement;

    #[link(name = "ApplicationServices", kind = "framework")]
    extern "C" {
        pub fn AXUIElementCreateSystemWide() -> AXUIElementRef;
        pub fn AXUIElementCopyAttributeValue(
            element: AXUIElementRef,
            attribute: CFStringRef,
            value: *mut CFTypeRef,
        ) -> i32; // AXError; 0 = success
    }

    /// Reads the focused element's AXSelectedText. Errors are expected in
    /// non-AX apps (many Electron apps, terminals) — callers treat any failure
    /// as "no selection".
    pub fn selected_text() -> Option<String> {
        unsafe {
            let system = AXUIElementCreateSystemWide();
            if system.is_null() {
                return None;
            }
            let focused_attr = CFString::from_static_string("AXFocusedUIElement");
            let mut focused: CFTypeRef = std::ptr::null();
            let err = AXUIElementCopyAttributeValue(
                system,
                focused_attr.as_concrete_TypeRef(),
                &mut focused,
            );
            CFRelease(system as CFTypeRef);
            if err != 0 || focused.is_null() {
                return None;
            }
            let selected_attr = CFString::from_static_string("AXSelectedText");
            let mut value: CFTypeRef = std::ptr::null();
            let err = AXUIElementCopyAttributeValue(
                focused as AXUIElementRef,
                selected_attr.as_concrete_TypeRef(),
                &mut value,
            );
            CFRelease(focused);
            if err != 0 || value.is_null() {
                return None;
            }
            let s = CFString::wrap_under_create_rule(value as CFStringRef);
            Some(s.to_string())
        }
    }
}

#[cfg(target_os = "macos")]
fn read_ax_selection() -> Result<Option<String>> {
    Ok(ax::selected_text())
}

#[cfg(not(target_os = "macos"))]
fn read_ax_selection() -> Result<Option<String>> {
    Ok(None)
}

/// Imperative-vs-dictation heuristic (plan 2.8): with a selection present, an
/// utterance opening with a rewrite verb is a command.
pub fn looks_like_command(utterance: &str) -> bool {
    const VERBS: &[&str] = &[
        "make", "rewrite", "reword", "rephrase", "paraphrase", "summarize", "summarise",
        "shorten", "condense", "expand", "elaborate", "fix", "correct", "improve", "polish",
        "translate", "convert", "turn", "format", "simplify", "formalize", "formalise",
        "capitalize", "capitalise", "lowercase", "uppercase", "bulletize", "bullet",
    ];
    let first = utterance
        .trim()
        .split_whitespace()
        .next()
        .unwrap_or("")
        .to_lowercase();
    let first = first.trim_matches(|c: char| !c.is_alphanumeric());
    VERBS.contains(&first)
}

#[cfg(test)]
mod tests {
    use super::looks_like_command;

    #[test]
    fn command_heuristic() {
        assert!(looks_like_command("make this more formal"));
        assert!(looks_like_command("Summarize the key points."));
        assert!(looks_like_command("rewrite as a bulleted list"));
        assert!(!looks_like_command("the meeting is at three"));
        assert!(!looks_like_command("I need to make dinner later"));
        // "I need to make..." starts with "i", not a verb — correct.
    }
}
