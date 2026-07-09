# LocalFlow — Wispr Flow clone (local models + OpenAI GPT API)

System-wide AI dictation for macOS (Windows later): hold a hotkey, speak, release — polished text is injected into whatever app has focus. Fully-local by default (whisper.cpp / Parakeet + Ollama), with BYOK OpenAI providers (gpt-4o-transcribe STT, GPT chat formatting) switchable at runtime.

"LocalFlow" is a working title only — do not copy Wispr Flow branding, copy, or assets. Functional parity only.

## Where things live

- `docs/BUILD_PLAN.md` — **the execution plan.** Phased checklists with acceptance criteria. Work top-to-bottom; check items off as they land. Always state which phase/task you are working on.
- `docs/PRODUCT_SPEC.md` — feature behavior spec (what "done" looks like for each feature).
- `docs/RESEARCH.md` — full research report with citations (background only; the two files above are authoritative for the build).

## Stack (decided — do not relitigate)

- **Tauri 2**: Rust core + React/TypeScript/Tailwind settings UI. Menu-bar/tray app, no dock icon.
- Audio: `cpal` (capture) + `rubato` (resample to 16 kHz mono) + Silero VAD via `voice_activity_detector` (chosen over `vad-rs`: same Silero model but embedded in the crate — no external ONNX file to ship).
- Hotkeys: raw CGEventTap on `FlagsChanged` via `core-graphics` (push-to-talk on right ⌘). **Do NOT use rdev**: its callback calls TSM keyboard-layout APIs off the main thread and macOS 15 SIGTRAPs the process on the first keystroke (verified via crash report, 2026-07-03). Modifier-only taps read keycode+flags and are thread-safe.
- **macOS 15 hard rule: any TSM keyboard-layout API (`TSMGetInputSourceProperty` etc.) off the main thread = instant SIGTRAP.** This killed both rdev (hotkeys) AND enigo `Key::Unicode` (paste) — verified via crash reports 2026-07-03/04. Synthetic keys must use raw virtual keycodes (`Key::Other(9)` = kVK_ANSI_V), and all injection/pasteboard work runs on the main thread via `pipeline::inject_on_main`.
- STT: `whisper-rs` (whisper.cpp, GGUF, Metal) and `transcribe-rs` (Parakeet) locally; OpenAI `/v1/audio/transcriptions` remote.
- Formatting: Ollama HTTP API locally (Qwen 3.5 3B class); OpenAI Chat Completions remote; regex-only passthrough fallback.
- Storage: SQLite via `rusqlite` (history, dictionary, snippets, style profiles). **API keys go in the macOS Keychain (`keyring` crate), never SQLite or plaintext config.**
- Injection: pasteboard save → write text → synthetic ⌘V via CGEvent → restore pasteboard (~100 ms delay); AX insertion and keystroke synthesis as fallbacks.

## Architecture invariants

- Everything pluggable goes behind the two traits in `src-tauri/src/providers/mod.rs`: `TranscriptionProvider` and `FormattingProvider`. New engines = new impls; the pipeline never knows which is active.
- Pipeline stages communicate via channels; never block the hotkey/audio threads on network or inference.
- Raw transcript is ALWAYS persisted to history before formatting, so users can recover from bad LLM edits.
- Graceful degradation: Ollama down / no API key → PassthroughFormatter + toast, never a hard failure.
- Zero telemetry. No network calls except user-selected cloud providers and model downloads.

## Commands

- `npm run tauri dev` — run the app in dev mode
- `npm run tauri build` — release build
- `cargo test` (in `src-tauri/`) — Rust unit tests
- `cargo test --test formatting_regression` — the dictation cleanup regression suite (Phase 2+; must pass before any prompt change merges)

## Known gotchas (learned from prior art — respect these)

- macOS permissions (Microphone, Accessibility, Input Monitoring) fail SILENTLY when missing. Every OS-integration feature must check its permission and surface a fix-it prompt. Rebuilds change the ad-hoc code signature and INVALIDATE existing TCC grants while System Settings still shows them as ON — toggling is not enough. After every rebuild: `for svc in Accessibility ListenEvent Microphone; do tccutil reset $svc com.localflow.app; done`, relaunch, re-grant fresh. (Goes away with stable code-signing in Phase 3.5.)
- Whisper hallucinates text on silence/noise ("thank you for watching" etc.) — always VAD-gate before transcribing; drop utterances with no speech frames.
- Pasteboard restore too early races the paste; too late loses user clipboard. Keep the ~100 ms delay and test in Slack, Notes, VS Code, Terminal, Chrome.
- Secure input fields (password boxes) block synthetic events — detect via `IsSecureEventInputEnabled` and no-op with a toast.
- Overlay windows (the pill): a Tauri window created `visible: false` never runs its WKWebView layout pass — the surface composites at alpha 0 forever, even after `show()` (verified via `screencapture -l <windowID>`, 2026-07-04). Keep the window ALWAYS visible (transparent + click-through), drive visibility in the DOM, and force one resize nudge after the first show. Also: the Dock draws at window level 20 and covers floating windows (level 5) — position overlays inside `monitor.work_area()` and raise the NSWindow level to 21.
- The formatter LLM must never ANSWER dictated questions — it transcribes them. This is enforced in the prompt and covered by regression tests; never remove those cases.

## Conventions

- Rust: `anyhow` for app errors, `thiserror` for library-ish modules, `tracing` for logs. No `unwrap()` outside tests.
- Frontend talks to core only through Tauri commands/events defined in `src-tauri/src/commands.rs`; keep the UI dumb.
- Conventional commits (`feat:`, `fix:`, `chore:`); one phase task per commit where practical.
