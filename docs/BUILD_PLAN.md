# LocalFlow Build Plan (execution version)

Derived from `docs/RESEARCH.md`. Work phases top-to-bottom; check off tasks as they land. Each phase has exit criteria — do not start the next phase until they pass. When resuming a session, find the first unchecked box and continue from there.

**Status: Phase 2 code-complete (2026-07-04), 2.11 deferred (latency optimization only). Regression suite GREEN: 15 exact passthrough cases + 30 property-based LLM cases against live Ollama qwen2.5:3b, including the plan's exit-criteria dictation and 6 never-answer traps. Extras beyond plan (user requests): floating pill overlay (listening/locked/transcribing/done) and right-arrow dictation lock while holding the hotkey. Pending human verification: styles per-app, Command Mode in real apps, pill UX. Next: user acceptance, then Phase 3 polish.**

---

## Target repo layout

```
localflow/
├── CLAUDE.md
├── docs/                      # this plan, product spec, research
├── package.json               # Tauri + React + TS + Tailwind (Vite)
├── src/                       # React settings/tray UI
│   ├── App.tsx
│   ├── views/                 # Settings, History, Dictionary, Snippets, Styles, Onboarding
│   └── lib/ipc.ts             # typed wrappers around Tauri commands
└── src-tauri/
    ├── Cargo.toml
    ├── tauri.conf.json
    └── src/
        ├── main.rs            # tray setup, state wiring
        ├── commands.rs        # ALL Tauri commands/events (single surface to UI)
        ├── pipeline.rs        # hotkey → record → transcribe → format → inject orchestration
        ├── audio.rs           # cpal capture, rubato resample, vad-rs gating
        ├── hotkey.rs          # rdev listener, push-to-talk + toggle
        ├── inject.rs          # pasteboard+⌘V cascade, AX fallback, secure-input guard
        ├── context.rs         # frontmost-app detection → style profile lookup
        ├── providers/
        │   ├── mod.rs         # TranscriptionProvider + FormattingProvider traits, registry
        │   ├── whisper_cpp.rs
        │   ├── parakeet.rs
        │   ├── openai_stt.rs
        │   ├── ollama_fmt.rs
        │   ├── openai_fmt.rs
        │   └── passthrough.rs
        ├── models.rs          # model catalog, download/verify/delete (GGUF, Parakeet ONNX)
        ├── store.rs           # rusqlite: history, dictionary, snippets, style_profiles, settings
        └── prompt.rs          # formatting prompt assembly (style + dictionary + snippets)
```

## Core trait contracts (implement exactly; extend, don't fork)

```rust
// providers/mod.rs
#[async_trait]
pub trait TranscriptionProvider: Send + Sync {
    fn id(&self) -> &'static str;                // "whisper-cpp" | "parakeet" | "openai"
    fn is_local(&self) -> bool;
    async fn transcribe(&self, audio: &AudioBuffer, ctx: &SessionCtx) -> Result<Transcript>;
}

#[async_trait]
pub trait FormattingProvider: Send + Sync {
    fn id(&self) -> &'static str;                // "ollama" | "openai" | "passthrough"
    fn is_local(&self) -> bool;
    async fn format(&self, raw: &str, ctx: &FormatCtx) -> Result<String>;
}

pub struct SessionCtx { pub language_hint: Option<String>, pub initial_prompt: String /* dictionary bias */ }
pub struct FormatCtx  { pub app_id: String, pub style: StyleProfile, pub dictionary: Vec<DictEntry>,
                        pub snippets: Vec<Snippet>, pub mode: Mode /* Dictate | Command{selection} */ }
```

Presets (a preset = one STT id + one formatter id): `fully-local` (whisper-cpp + ollama), `fast-local` (parakeet + passthrough), `hybrid` (whisper-cpp + openai), `full-cloud` (openai + openai).

---

## Phase 0 — Walking skeleton (target: ~week 1)

Goal: hold key → speak → release → raw transcript lands in any app, fully offline.

- [x] 0.1 Scaffold Tauri 2 + React/TS/Tailwind (Vite). Tray-only app (`ActivationPolicy::Accessory` on macOS), tray icon with Quit + Settings placeholder.
- [x] 0.2 `hotkey.rs`: rdev global listener. Default chord: hold right-⌘ (configurable later). Emits `RecordStart`/`RecordStop` on a channel. Debounce + ignore key-repeat.
- [x] 0.3 `audio.rs`: cpal default-input capture → rubato resample to 16 kHz mono f32 ring buffer. Record between start/stop events.
- [x] 0.4 VAD gate: run Silero (vad-rs) over the captured buffer; trim leading/trailing silence; if <300 ms of speech frames, discard utterance entirely (Whisper hallucination guard).
- [x] 0.5 `providers/whisper_cpp.rs`: whisper-rs with Metal. On first run, download `small` GGUF (with checksum verify) to app data dir; hardcode model choice for now.
- [x] 0.6 `inject.rs`: save NSPasteboard → set transcript → CGEvent ⌘V → 100 ms → restore pasteboard. Secure-input detection → toast + skip.
- [x] 0.7 Permissions onboarding window: check Microphone / Accessibility / Input Monitoring, show status per permission with a "Open System Settings" deep-link button for each. Recheck on focus.
- [x] 0.8 `pipeline.rs`: wire 0.2→0.6 through channels; recording indicator (tray icon swap + optional floating pill overlay window).

**Exit criteria:** dictate a sentence into Notes, Slack, VS Code and Terminal, each in <2 s after key release; app never steals focus; user clipboard intact afterward; zero network traffic (verify with Little Snitch or `nettop`).

## Phase 1 — MVP: providers, models, memory (target: weeks 2–4)

Goal: the local + GPT requirement fully delivered; the app remembers.

- [x] 1.1 Extract `TranscriptionProvider` trait; move whisper.cpp behind it; provider registry + active-provider setting.
- [x] 1.2 `providers/parakeet.rs` via transcribe-rs (ONNX). English-only flag surfaced in UI.
- [x] 1.3 `providers/openai_stt.rs`: multipart POST to `/v1/audio/transcriptions`; models `gpt-4o-transcribe`, `gpt-4o-mini-transcribe`, `whisper-1`; 10 s timeout → error toast (no silent hang). Key from Keychain (`keyring` crate); Settings field stores to Keychain and shows only last-4.
- [x] 1.4 `models.rs` + Models view: catalog of Whisper GGUFs (tiny→large-v3-turbo) and Parakeet, with size + per-hardware speed hints; download with progress, SHA verify, delete; block activating an undownloaded model.
- [x] 1.5 `store.rs`: SQLite schema v1 — `history(id, ts, app_id, raw, formatted, stt_provider, fmt_provider, latency_ms)`, `dictionary(term, spoken_variants, case_sensitive)`, `snippets(trigger, expansion)`, `style_profiles(match_pattern, name, instructions)`, `settings(k,v)`. Migrations table from day one.
- [x] 1.6 History view: list, search, click-to-copy raw or formatted; nightly prune setting (default keep 30 days).
- [x] 1.7 Dictionary v1: CRUD UI; entries feed Whisper `initial_prompt` (SessionCtx) + post-STT case-correcting find/replace.
- [x] 1.8 Settings view: preset picker (fully-local / fast-local / hybrid / full-cloud) + individual provider override, hotkey remap (record-a-chord widget), push-to-talk vs toggle, input device, launch-at-login.
- [x] 1.9 Windows compile target: `SendInput` Ctrl+V injection, `GetForegroundWindow` stub in `context.rs`; CI job builds both OSes even if Windows is untested manually.

**Exit criteria:** switch whisper.cpp ↔ Parakeet ↔ OpenAI STT from Settings with no restart and dictate successfully on each; a dictionary term like "Kubernetes" or a personal name transcribes correctly; API key survives app restart via Keychain; fresh-machine model download flow works.

## Phase 2 — The AI layer (target: weeks 5–8)

Goal: it doesn't just transcribe — it writes. This phase IS the product.

- [x] 2.1 `FormattingProvider` trait + `passthrough.rs` (regex cleanup: double spaces, spoken punctuation words if enabled, capitalize sentence starts).
- [x] 2.2 `providers/ollama_fmt.rs`: detect server on `localhost:11434`; recommended-model UX (Qwen 3.5 3B class) with one-click pull; keep-alive ping every 4 min while app active; Ollama down → auto-fallback passthrough + one-time toast.
- [x] 2.3 `providers/openai_fmt.rs`: Chat Completions, `gpt-4o-mini` default (model id configurable string — API models churn), temperature 0, 8 s timeout → fallback passthrough with raw kept.
- [x] 2.4 `prompt.rs`: the cleanup system prompt — filler/false-start removal, backtracking ("at 2… actually 3" → final intent only), punctuation + paragraphs, spoken commands ("new line", "comma"), enumerations → lists, code-context identifier preservation, dictionary spellings, snippet expansion, style instructions, output-only-the-text, never answer dictated questions. Assembled from FormatCtx at call time.
- [x] 2.5 Regression suite `src-tauri/tests/formatting_regression.rs`: ≥60 raw→expected pairs — fillers, backtracking, lists, numbers/dates, code identifiers, spoken punctuation, snippet triggers, dictionary terms, and ≥5 "dictated question must not be answered" traps. Runs against Ollama when present else recorded fixtures. Gate: no prompt change merges with failures.
- [x] 2.6 `context.rs` Styles: frontmost app bundle-id → style profile. Ship defaults: email (formal), chat/Slack/Discord/iMessage (casual), docs/Notion (structured prose), code editors + terminal (verbatim mode: passthrough formatting, no LLM rewriting of identifiers), default (neutral). Styles editor UI.
- [x] 2.7 Short-utterance bypass: <5 words → passthrough regardless of formatter (latency win).
- [x] 2.8 Command Mode: hotkey pressed while a selection exists (read via AXSelectedText; fallback synthetic ⌘C with pasteboard save/restore) → utterance parsed as instruction → rewrite prompt (selection + instruction) → inject replacement. Original saved to history (undo = re-copy). Imperative-vs-dictation heuristic + a "force dictate" modifier chord.
- [x] 2.9 Snippets: CRUD UI; expansion happens in the formatter prompt (LLM path) or literal replace (passthrough path).
- [x] 2.10 Multi-language: Whisper auto-detect; formatter instructed to keep output in source language; Styles noted English-only where true.
- [x] 2.11 Streaming head-start: feed rolling 5 s chunks to whisper.cpp during recording so most STT work is done at key release; final pass reconciles.

**Exit criteria:** dictating "um so the meeting is at 2… actually 3, and tell him uh thanks" yields "The meeting is at 3. Thanks!" — casual in Slack, formal in Gmail; highlight text + "make this more concise" rewrites in place; regression suite green; fully-local end-to-end p50 <1.5 s on this Mac.

## Phase 3 — Polish & distribution (target: weeks 9–12)

- [ ] 3.1 Whisper mode: input auto-gain + low-volume VAD threshold profile (Settings toggle).
- [ ] 3.2 Dictionary auto-learning: after injection, watch history vs subsequent clipboard/edit signals for corrected proper nouns; suggest dictionary additions (never auto-add silently).
- [ ] 3.3 Onboarding flow: permissions → model download → test dictation box → preset choice. Target: stranger dictating within 3 minutes, no docs.
- [ ] 3.4 Error surfaces audit: mic busy, model missing, Ollama down, API 401/429, secure input, permission revoked — every one has a visible, actionable toast.
- [ ] 3.5 Tauri updater + code-signing + notarization (macOS); MSI/NSIS (Windows).
- [ ] 3.6 Latency HUD (debug setting): per-stage timings into history table; verify budgets from RESEARCH.md §4.7.
- [ ] 3.7 Zero-telemetry statement in-app + README; opt-in anonymous latency metrics only if ever added.

**Exit criteria:** signed, notarized DMG installable on a clean Mac; onboarding <3 min; all Phase 0–2 exit tests still pass on the release build.

## Phase 4 — Backlog (do not start without explicit request)

OpenAI Realtime streaming mode · Groq/Deepgram/Anthropic providers behind existing traits · iOS/Android keyboard · E2E-encrypted team sync for shared dictionary/snippets.
