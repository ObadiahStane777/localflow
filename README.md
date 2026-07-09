# LocalFlow

System-wide, local-first AI dictation for macOS. Hold **right ⌘**, speak, release — the transcript is typed into whatever app has focus. Audio never leaves your Mac: whisper.cpp (Metal) + Silero VAD, fully offline.

## Why LocalFlow

- **🔒 Runs on local models** — whisper.cpp (Metal-accelerated) and Silero VAD run entirely on your Mac. No account, no cloud, no audio ever leaving the device.
- **⚡ Fast transcription** — Metal-backed inference plus VAD gating means you speak, release, and the text is already at your cursor.
- **🔑 Freedom to BYOK** — prefer the cloud? Bring your own key. Switch to OpenAI (`gpt-4o-transcribe`, GPT formatting) at runtime; the pipeline is provider-agnostic, so local and BYOK are a toggle apart.
- **🆓 Free to use** — no subscription, no per-word billing, no telemetry. The only network call is the one-time model download. Local mode costs nothing, forever.

> Phase 0 walking skeleton (see `docs/BUILD_PLAN.md`). Providers (Parakeet, OpenAI BYOK), AI formatting, styles, and Command Mode land in Phases 1–2.

## Build

Requires: Rust (rustup), Node 20+, cmake (`brew install cmake`), Xcode CLT.

```bash
npm install
npm run tauri build        # → src-tauri/target/release/bundle/macos/LocalFlow.app
# or for development:
npm run tauri dev
```

First launch downloads the Whisper `small` model (~466 MB, SHA-256 verified) to
`~/Library/Application Support/com.localflow.app/models/`.

## Permissions (macOS)

LocalFlow needs three grants, all surfaced in the settings window with fix-it buttons:

| Permission | Used for |
|---|---|
| Microphone | recording while the hotkey is held |
| Input Monitoring | detecting the global right-⌘ push-to-talk key |
| Accessibility | synthesizing ⌘V to paste the transcript |

After granting Input Monitoring or Accessibility, quit and relaunch the app (macOS applies them at process start). Dev builds lose Accessibility grants on each rebuild — re-grant to the new binary.

## Use

1. Launch LocalFlow — it lives in the menu bar (no dock icon).
2. Focus any text field (Notes, Slack, VS Code, Terminal…).
3. Hold right ⌘, speak, release. Text appears at the cursor; your clipboard is restored.

A red dot appears next to the menu-bar icon while recording. Utterances with under 300 ms of detected speech are discarded (hallucination guard). Dictation into password fields is blocked by design.

## Headless STT check (no permissions needed)

```bash
say -v Samantha -o /tmp/t.aiff "testing local flow" && afconvert /tmp/t.aiff /tmp/t.wav -f WAVE -d LEI16@44100 -c 1
cd src-tauri && cargo run --release --example transcribe_file -- \
  "$HOME/Library/Application Support/com.localflow.app/models/ggml-small.bin" /tmp/t.wav
```

## Privacy

No telemetry. The only network call is the one-time model download. See `docs/PRODUCT_SPEC.md`.
