# Cloning Wispr Flow: Product Teardown & Full Build Plan
### A dictation app with pluggable local models and OpenAI GPT API support

**Date:** 3 July 2026 · **Mode:** Standard deep-research · **Author:** Claude (deep-research pipeline)

---

## Executive Summary

Wispr Flow is a system-wide AI dictation tool for Mac, Windows, iOS and Android. The user holds a hotkey, speaks, and polished text is injected into whatever app has focus. Its differentiation is not raw speech-to-text — it is the AI *formatting* layer: filler-word removal, mid-speech self-correction ("backtracking"), auto-punctuation, per-app tone matching, a personal dictionary, snippets, and a voice-driven Command Mode for editing selected text [1][2][5]. It is cloud-only at every tier (subprocessors include Baseten, OpenAI, Anthropic, Cerebras and AWS), costs $15/month for unlimited use, and suffered a public privacy backlash over its screenshot-based context-awareness feature [8][9][22]. That combination — subscription pricing plus mandatory cloud processing — is precisely the gap a clone with **local models and bring-your-own-key GPT API support** can exploit, and a wave of open-source projects (Handy ~23k GitHub stars, OpenWhispr, VoiceInk, Murmur) validates both the demand and the architecture [10][11][12][21].

The recommended build is a **Tauri 2 desktop app (Rust core + React/TypeScript settings UI)** implementing a five-stage pipeline: global hotkey → audio capture with voice-activity detection → pluggable transcription provider → pluggable formatting provider → text injection. Transcription providers: **whisper.cpp** (GGUF models, Metal/CUDA/Vulkan) and **NVIDIA Parakeet** locally, plus **OpenAI `gpt-4o-transcribe` / `whisper-1`** via API ($0.006/min, or $0.003/min for the mini variant) [13][14][16][17]. Formatting providers: a local 3–4B model via **Ollama** (Qwen 3.5 3B / Llama 3.2 3B class, ~20+ tok/s on Apple Silicon) or **GPT via the Chat Completions API** [20][23]. Text injection uses the industry-standard cascade: clipboard + synthetic paste first, Accessibility API second, keystroke simulation last [18][19].

The plan below decomposes every observable Wispr Flow feature, maps each to a concrete implementation, and lays out a four-phase roadmap: a working push-to-talk dictation core in ~1 week, an MVP with provider switching and dictionary in ~4 weeks, feature parity on the AI layer (styles, Command Mode, snippets) by ~8 weeks, and polish/distribution by ~12 weeks. End-to-end latency target is under ~1.5 seconds from key-release to injected text with local models on Apple Silicon — competitive with Wispr Flow's cloud round-trip, with zero per-word cost and full offline privacy.

---

## 1. Introduction

### 1.1 Scope

This report answers: *what exactly does Wispr Flow do, how does it work, and how do we build a functional clone that supports both local models and the OpenAI GPT API?* It covers the desktop product (macOS first, Windows second); mobile is treated as a later phase. "Clone" here means **functional parity** — replicating features and UX, not copying Wispr's code, branding, or name (Wispr Flow is a trademark; the clone needs its own identity).

### 1.2 Methodology

Evidence was gathered from Wispr Flow's official site (features, pricing, data-controls pages), independent 2026 reviews (Zapier, tl;dv, Willow, Efficient App, eesel, Spokenly), the source code and documentation of open-source equivalents (Handy, OpenWhispr, Murmur, Handsfree), STT benchmark publications, and OpenAI's published API pricing. Claims are cited inline as [N] against the Bibliography; the machine-readable evidence trail is in `sources.jsonl`, `evidence.jsonl`, and `claims.jsonl` alongside this report.

### 1.3 Assumptions (surfaced, not silent)

- **Target user:** you, a solo builder, shipping first for macOS on Apple Silicon; Windows support planned, Linux opportunistic.
- **"API from GPT"** is read as OpenAI's APIs: `gpt-4o-transcribe`/`whisper-1` for speech-to-text and a GPT chat model for the formatting layer, with the user supplying their own API key (BYOK).
- **"Local model"** is read as fully offline operation: local STT (Whisper-family or Parakeet) and optionally a local LLM (via Ollama) for cleanup.
- Mobile apps, team dashboards, and enterprise compliance (SOC 2) are out of MVP scope but included in the roadmap's final phase.

---

## 2. Product Teardown: What Wispr Flow Actually Is

### 2.1 The core loop

Wispr Flow's entire product hangs on one interaction: **hold a hotkey, speak, release, and clean text appears at the cursor in any app** [1][5]. It operates at the OS level, so it works in every text field — Gmail, Notion, Slack, WhatsApp, Cursor, VS Code, terminal [2][5]. Audio is streamed to cloud AI: one layer transcribes; further layers strip fillers ("um", "uh"), fix sentence structure, apply punctuation, and format for the app context [3][5]. Independent testers measure effective output of 150–184 words per minute versus 60–80 WPM typing — the basis of the "4× faster than your keyboard" claim [4].

### 2.2 Complete feature inventory

**Input & transcription** [2]
- Works in any app that accepts keyboard input (system-wide injection).
- 100+ languages, with mid-task language switching.
- **Whisper mode** — usable at very low speaking volume in shared spaces.
- **Name spelling** — uses context to spell uncommon proper nouns.

**Auto-editing (the "magic" layer)** [2][5][6]
- Filler-word and pause removal.
- **Backtracking** — "meet at 2… actually 3" produces "meet at 3".
- Auto-punctuation from pauses and prosody, plus verbal punctuation commands.
- Spoken lists become formatted numbered/bulleted lists.
- **Syntax awareness** — recognizes code contexts: CLI commands, camelCase, snake_case.

**Personalization** [2][6]
- **Personal dictionary** — learns from user corrections; manual entries for jargon.
- **Snippets** — voice shortcuts expanding to phrases, links, templates.
- **Tone / Styles** — detects the active app and adapts register: formal in Docs, casual in Slack, structured in email (English, desktop only).

**Command Mode (Pro)** [1][6][7]
- Highlight text, hold hotkey, speak an instruction — "make this more formal", "rewrite as a bulleted list", "summarize" — and the selection is rewritten in place.

**Team / Business tier** [2]
- Shared dictionary and shared snippets; usage/adoption dashboards.

**Platform & compliance** [1][4]
- Mac, Windows, iOS, Android (the only major player on all four as of April 2026).
- Native integration positioning for Cursor, Windsurf, VS Code.
- HIPAA-ready (BAA) on all plans; SOC 2 Type II on Enterprise.

### 2.3 Pricing and the privacy opening

| Tier | Price | Limits |
|---|---|---|
| Basic (free) | $0 | 2,000 words/week desktop, 1,000/week iPhone; dictionary, 100+ languages [8][22] |
| Pro | $15/mo ($12/mo annual) | Unlimited dictation + Command Mode [1][8] |
| Team/Enterprise | ~$10–12/user/mo | Shared assets, dashboards, SOC 2 [2] |

Wispr Flow is **cloud-only at every tier — there is no on-device mode** [9][22]. Audio is processed by subprocessors including Baseten, OpenAI, Anthropic, Cerebras, and AWS [9]. Its context-awareness feature drew viral criticism for capturing periodic screenshots of the active window and sending them to cloud infrastructure; after the backlash, the CTO apologized, training-data usage became opt-in, and an opt-in zero-retention "Privacy Mode" shipped on Pro [8][22]. Reliability complaints persist (Trustpilot 2.7/5, Windows performance issues) [8].

**Implication for the clone:** local-first processing is not a compromise — it is the headline feature. Every open-source competitor (Handy, OpenWhispr, VoiceInk, VoiceTypr) leads with "offline, private, no subscription" [10][11][21], and the fastest local setups now beat the cloud round-trip on latency [15].

---

## 3. Reference Architectures: What Existing Clones Prove

Three open-source projects establish the proven pattern:

- **Handy** (~23k stars, MIT, Tauri): React/TS settings UI over a Rust core; `cpal` for cross-platform audio I/O, `vad-rs` (Silero) for voice-activity detection, whisper.cpp bindings for GGML/GGUF Whisper models, `transcribe-rs` for CPU-optimized Parakeet, `rdev` for global hotkeys, `rubato` for resampling to 16 kHz mono [10].
- **Murmur** (Tauri + React + whisper.cpp): documents the canonical text path — capture → resample to 16 kHz mono → in-memory Whisper transcription → cleanup → **clipboard write + synthetic paste** into the focused field, gated behind macOS Accessibility permission; also supports a remote OpenAI-compatible Whisper endpoint, proving the local/API dual-provider pattern [12].
- **OpenWhispr** (Electron): local Whisper + Parakeet **and** BYOK cloud models, plus an LLM cleanup stage (local or cloud) for grammar, filler removal and formatting — the closest existing analogue to this build's goals [11][21].

Text-injection practice across the category converges on a cascade: **clipboard + simulated ⌘V (saving/restoring the prior pasteboard) as primary; Accessibility API (`AXUIElement`) insertion; simulated keystrokes as last resort** — because each method fails in some apps (secure fields, some Electron apps, terminals) [12][18][19].

---

## 4. Target Architecture

### 4.1 Stack decision

**Tauri 2: Rust core + React/TypeScript UI.** Rationale: it is the demonstrated-at-scale choice (Handy, Murmur); Rust gives first-class bindings for whisper.cpp, ONNX (Parakeet), cpal, and OS APIs; binaries are ~10–20 MB vs Electron's ~150 MB+; and one codebase covers macOS/Windows/Linux. Alternatives considered: native Swift (best mac polish, but locks out Windows — VoiceInk's position) and Electron (fastest web-dev iteration, heaviest footprint — OpenWhispr's position) [10][11][12].

### 4.2 Pipeline (the whole product in one diagram)

```
 ┌──────────┐   ┌───────────────┐   ┌─────────────┐   ┌──────────────┐   ┌────────────┐
 │  Global   │   │ Audio capture │   │ Transcribe  │   │   Format     │   │   Inject   │
 │  hotkey   │──▶│ cpal 16kHz    │──▶│  provider   │──▶│  provider    │──▶│ clipboard+ │
 │ (rdev)    │   │ mono + VAD    │   │ (trait)     │   │  (trait)     │   │ paste / AX │
 └──────────┘   └───────────────┘   └─────────────┘   └──────────────┘   └────────────┘
   push-to-talk    Silero VAD trims   local: whisper.cpp   local: Ollama      history +
   or toggle;      silence; guards    (GGUF, Metal/CUDA)   (Qwen/Llama 3B)    dictionary
   ~0ms            Whisper against    or Parakeet          cloud: GPT chat    learning
                   hallucination      cloud: OpenAI STT    API (BYOK)         (SQLite)
```

### 4.3 Provider abstraction (the local + GPT requirement)

Two Rust traits are the heart of the design:

```rust
#[async_trait]
pub trait TranscriptionProvider: Send + Sync {
    async fn transcribe(&self, audio: &AudioBuffer, ctx: &SessionCtx) -> Result<Transcript>;
    fn is_local(&self) -> bool;
}

#[async_trait]
pub trait FormattingProvider: Send + Sync {
    async fn format(&self, raw: &str, ctx: &FormatCtx) -> Result<String>;
    // FormatCtx: active app, style profile, dictionary terms, snippet table, language
}
```

**Transcription implementations**
1. `WhisperCpp` — whisper.cpp via `whisper-rs`; GGUF models from tiny → large-v3-turbo, Metal on macOS, CUDA/Vulkan on Windows/Linux. whisper.cpp is the strongest real-time choice: streaming examples + VAD support, ~10× real-time for large-v3 on recent Apple Silicon with Metal [13].
2. `Parakeet` — NVIDIA Parakeet TDT via `transcribe-rs`/ONNX; exceptional on CPU and Apple Silicon (~27 ms encoder inference per 10 s of audio for the 110M model on Apple GPU), English-focused — ideal default for low-end hardware [10][15].
3. `OpenAiStt` — `POST /v1/audio/transcriptions` with `gpt-4o-transcribe` ($0.006/min), `gpt-4o-mini-transcribe` ($0.003/min), or `whisper-1`; BYOK, with the Realtime API (~$0.017/min) reserved for a future live-streaming mode [16][17].

**Formatting implementations**
1. `OllamaFormatter` — hits the local Ollama server (`/api/chat`) with a 3–4B instruct model. Qwen 3.5 3B or Llama 3.2 3B (Q4, ~3 GB RAM) sustain 20+ tok/s on Apple Silicon — a 40-word dictation cleans up in roughly 300–600 ms [20][23].
2. `GptFormatter` — OpenAI Chat Completions with `gpt-4o-mini` (or newer mini-class model): highest quality for Command Mode rewrites and tone work.
3. `PassthroughFormatter` — raw transcript with regex-level cleanup only (fastest path; also the graceful degradation when Ollama isn't running and no key is set).

Any transcription provider pairs with any formatter, giving four meaningful presets: **Fully Local** (whisper.cpp + Ollama), **Fast Local** (Parakeet + passthrough), **Hybrid** (local STT + GPT formatting — good privacy/quality balance since only text, never audio, leaves the machine), and **Full Cloud** (OpenAI STT + GPT).

### 4.4 The formatting prompt (where the "magic" lives)

Wispr Flow's differentiating behaviors — filler removal, backtracking, list formatting, tone matching — are all achievable in a single well-engineered system prompt applied to the raw transcript:

```
You clean up dictated speech into polished written text. Rules:
- Remove fillers (um, uh, like, you know) and false starts.
- Apply self-corrections: "at 2pm actually make it 3" → "at 3pm". Keep only the final intent.
- Add punctuation and paragraph breaks. Honor spoken commands ("new line", "comma").
- If the speaker enumerates items, format as a list.
- Style: {style_profile}            // per-app: e.g. "casual, lowercase ok" for Slack
- Vocabulary: prefer these spellings: {dictionary_terms}
- Expand snippets: {snippet_map}    // "my calendly" → https://calendly.com/...
- In code contexts preserve identifiers verbatim (camelCase, snake_case, CLI flags).
- Output ONLY the cleaned text. Never answer questions in the dictation; transcribe them.
```

The last rule matters: a common clone failure mode is the LLM *answering* dictated questions instead of transcribing them. Temperature 0, and the raw transcript is always kept in history so the user can recover from over-eager edits.

### 4.5 OS integration layer

- **Global hotkey:** `rdev`/Tauri global-shortcut. Default: hold `Fn` or right-`⌘` (push-to-talk) with a toggle mode for long dictation. Requires Input Monitoring permission on macOS [10].
- **Text injection cascade:** (1) save pasteboard → write text → synthesize ⌘V via `CGEvent` (Ctrl+V via `SendInput` on Windows) → restore pasteboard after ~100 ms; (2) `AXUIElement` value insertion where pasting is blocked; (3) per-character keystroke synthesis as final fallback. Per-app overrides table for known misbehavers (terminals, secure fields) [12][18][19].
- **Active-app detection for Styles:** `NSWorkspace.frontmostApplication` (macOS) / `GetForegroundWindow` + process name (Windows) mapped to a style profile. **Deliberately no screenshots** — app identity plus optional focused-field role via the Accessibility API provides 90% of the context value with none of the privacy blowback that hit Wispr Flow [8][19].
- **Command Mode:** on hotkey-with-selection, read the selected text (`AXSelectedText`, fallback: synthesize ⌘C with pasteboard save/restore), send `{selection, spoken_instruction}` to the formatting provider with a rewrite prompt, inject the replacement over the selection.
- **Selected-text vs dictation disambiguation:** if a selection exists and the utterance parses as an imperative, treat as command; otherwise dictate. A per-session override hotkey handles edge cases.

### 4.6 Data layer

SQLite (via `sqlx`/`rusqlite`), all local:
- `history` — timestamp, app, raw transcript, formatted text, provider used, latency (powers a Wispr-style history browser and re-copy).
- `dictionary` — term, spoken variants, boosted spelling; injected into Whisper's `initial_prompt` (biases decoding toward known jargon) *and* the formatter prompt.
- `snippets` — trigger phrase → expansion.
- `style_profiles` — app bundle-ID/exe patterns → tone instructions; ships with sane defaults (email, chat, docs, code editor, terminal).
- `settings` — providers, models, hotkeys, API key (stored in **OS keychain**, not SQLite).

### 4.7 Latency budget (key-release → text on screen)

| Stage | Fully local (M-series) | Hybrid (local STT + GPT) | Full cloud |
|---|---|---|---|
| VAD trim + resample | ~10 ms | ~10 ms | ~10 ms |
| STT (10 s utterance) | 150–500 ms (Parakeet fastest; whisper.cpp Metal ~10× RT) [13][15] | 150–500 ms | 400–900 ms + upload |
| LLM formatting | 300–600 ms (3B @ 20+ tok/s) [20] | 400–800 ms (network) | 400–800 ms |
| Injection | ~50 ms | ~50 ms | ~50 ms |
| **Total** | **~0.5–1.2 s** | **~0.6–1.4 s** | **~0.9–1.8 s** |

Optimizations: start STT *during* recording on rolling chunks (whisper.cpp streaming) so most transcription is done at key-release; skip the LLM entirely for utterances under ~5 words (regex cleanup suffices); keep the Ollama model warm with a keep-alive ping.

---

## 5. Build Plan: Phased Roadmap

### Phase 0 — Walking skeleton (Week 1)
**Goal: dictate into any app, locally, end to end.**
- Scaffold Tauri 2 + React/TS; menu-bar (tray) app, no dock icon.
- `rdev` global hotkey (push-to-talk); `cpal` capture → `rubato` resample to 16 kHz mono; Silero VAD via `vad-rs`.
- whisper.cpp (`whisper-rs`) with `base`/`small` GGUF bundled-on-first-run; Metal enabled.
- Clipboard + synthetic-paste injection with pasteboard restore.
- macOS permissions flow: Microphone, Accessibility, Input Monitoring — with a guided onboarding screen (the #1 support burden in this category; do it right on day one).
- **Exit criteria:** hold key → speak → release → raw transcript appears in Notes/Slack/VS Code in <2 s.

### Phase 1 — MVP: providers, models, memory (Weeks 2–4)
**Goal: the local + GPT requirement, fully delivered.**
- `TranscriptionProvider` trait; add **Parakeet** (transcribe-rs) and **OpenAI STT** (`gpt-4o-transcribe` / `gpt-4o-mini-transcribe` / `whisper-1`, BYOK, key in keychain) [16][17].
- Model manager UI: download/verify/delete GGUF & Parakeet models with size/speed guidance per hardware.
- SQLite history browser; personal dictionary v1 (manual entries → Whisper `initial_prompt` + find/replace pass).
- Settings UI: hotkey remap, provider/preset picker (Fully Local / Fast Local / Hybrid / Full Cloud), input device, launch-at-login.
- Windows port compiles and passes the Phase 0 exit test (`SendInput`, `GetForegroundWindow`).
- **Exit criteria:** switch between whisper.cpp, Parakeet, and OpenAI STT from settings with no restart; dictionary terms transcribe correctly.

### Phase 2 — The AI layer: parity with Wispr's magic (Weeks 5–8)
**Goal: it doesn't just transcribe — it writes.**
- `FormattingProvider` trait; **Ollama** integration (auto-detect running server, recommend Qwen 3.5 3B / Llama 3.2 3B, one-click `ollama pull`) and **GPT formatter** (Chat Completions, BYOK) [20][23].
- The formatting system prompt (§4.4): fillers, backtracking, punctuation, lists, verbal commands, code-context preservation; snippet expansion; dictionary enforcement.
- **Styles:** active-app detection → style profiles with shipped defaults + user editing.
- **Command Mode:** selection capture → spoken instruction → in-place rewrite (undo restores original from history).
- Multi-language: Whisper language auto-detect; formatter instructed to respond in source language.
- Latency work: streaming transcription during recording; short-utterance LLM bypass; model keep-alive.
- **Exit criteria:** "um so the meeting is at 2… actually 3, and tell him uh thanks" → "The meeting is at 3. Thanks!" formatted casually in Slack and formally in Gmail; highlight + "make this more concise" works.

### Phase 3 — Polish & distribution (Weeks 9–12)
- Whisper mode: input auto-gain + a low-volume-tuned VAD threshold profile.
- Snippets UI; dictionary auto-learning (detect user's post-injection corrections via clipboard/history diffing — Wispr's "learns from corrections" [2]).
- Onboarding, first-run model download UX, error surfaces (mic in use, Ollama down → graceful fallback to passthrough with a toast).
- Auto-update (Tauri updater), code-signing + notarization (macOS), MSI/NSIS (Windows).
- Telemetry: **none by default**; opt-in anonymous latency metrics only. This is the brand.
- **Exit criteria:** a stranger can install, onboard, and dictate within 3 minutes with zero docs.

### Phase 4 — Beyond parity (backlog)
- Realtime API streaming mode for live-caption-style dictation ($0.017/min) [17].
- Additional cloud providers behind the same trait (Groq for near-instant Whisper, Deepgram, Anthropic for formatting).
- iOS/Android keyboard extension (the hard 20%: mobile keyboards are a separate product).
- Team sync (shared dictionary/snippets via end-to-end-encrypted sync) — Wispr's Business tier, without the server seeing content [2].

### Effort summary

| Phase | Duration | Output |
|---|---|---|
| 0 | 1 week | Working local dictation skeleton |
| 1 | 3 weeks | MVP: 3 STT providers, models, history, dictionary |
| 2 | 4 weeks | AI formatting, Styles, Command Mode, snippets |
| 3 | 4 weeks | Ship-quality signed builds, mac + Windows |

~12 weeks solo full-time to a shippable v1.0; a usable personal daily-driver exists from week 1.

---

## 6. Synthesis & Insights

1. **The moat is the formatting layer, not the STT.** Whisper-class transcription is commoditized; every review of Wispr Flow praises the *editing* (fillers, backtracking, tone) [3][5][6]. Budget prompt-engineering and evaluation time accordingly — build a regression suite of ~100 dictation→expected-output pairs early.
2. **Local-first flips Wispr's weakness into your headline.** Cloud-only processing plus the screenshot controversy is Wispr's softest flank [8][9][22]; the hybrid preset (local audio, cloud text) is a genuinely novel middle ground most competitors don't articulate.
3. **The architecture is de-risked.** Every component has a proven open-source reference: Handy for the Rust/Tauri pipeline, Murmur for injection, OpenWhispr for the dual local/cloud LLM cleanup [10][11][12]. Nothing here is research; it is assembly plus product taste.
4. **Latency parity is achievable, cost superiority is automatic.** Local pipeline lands at ~0.5–1.2 s [13][15][20]; heavy users pay $0 versus $180/yr Pro, and even full-cloud BYOK costs a heavy user (~300 min/mo) about $1–2/month in API fees [16][17].
5. **Permissions UX is the hidden boss fight.** Mic + Accessibility + Input Monitoring on macOS, secure-input fields, per-app paste quirks — this is where clones lose users, not in the ML [12][18][19].

## 7. Limitations & Caveats

- Wispr Flow's exact model stack and prompts are proprietary; the formatting behaviors were reconstructed from official marketing and third-party reviews, so parity claims are behavioral, not implementation-level [2][5][6].
- Latency figures are drawn from published benchmarks on differing hardware (M-series Macs, various GPUs) [13][15]; validate on target machines in Phase 0.
- OpenAI pricing and model names are as of mid-2026 and change frequently [16][17]; the provider trait isolates this risk.
- Reviews of Wispr Flow (word limits, Trustpilot score, backlash timeline) come from third-party blogs, some with competing products — cross-checked where possible but with residual bias risk [4][8][22].
- Mobile parity (iOS/Android keyboards) was scoped out of the 12-week plan; it is a substantially different engineering effort.

## 8. Recommendations

1. **Build Phase 0 this week** — the walking skeleton is ~3 days of work with `whisper-rs` + `cpal` + clipboard-paste, and daily-driving it will teach more than any further research.
2. **Default preset: Parakeet + Ollama Qwen 3.5 3B** on Apple Silicon (fastest fully-local), with one-click switch to Hybrid (local STT + `gpt-4o-mini` formatting) for quality-critical writing.
3. **Store the OpenAI key in the OS keychain, ship zero telemetry by default,** and say both loudly — privacy is the positioning.
4. **Build the dictation regression suite in Phase 2, not later:** raw-transcript → expected-clean-text pairs covering fillers, backtracking, lists, code, and the "don't answer questions" trap.
5. **Study Handy's repo before writing code** (MIT-licensed) for the audio/VAD/hotkey plumbing; write your own formatting layer — that is the product.

---

## Bibliography

1. Wispr Flow — official site. https://wisprflow.ai/
2. Wispr Flow — Features page. https://wisprflow.ai/features
3. Zapier — "Why is Wispr Flow different from other dictation apps?" https://zapier.com/blog/wispr-flow/
4. tl;dv — "Wispr Flow Review 2026." https://tldv.io/blog/wisprflow/
5. Willow Voice — "Wispr Flow Review: AI Voice Dictation Tool, January 2026." https://willowvoice.com/blog/wispr-flow-review-voice-dictation
6. Efficient App — "Wispr Flow Review 2026: Pros, Cons, Pricing & Verdict." https://efficient.app/apps/wispr-flow
7. Letterly — "Wispr Flow Review: Features, Pricing, Privacy Concerns, and Alternatives." https://letterly.app/blog/wispr-flow-review/
8. Spokenly — "Wispr Flow Review 2026: Is It Worth $15/mo?" https://spokenly.app/blog/wispr-flow-review
9. Wispr Flow — Data Controls. https://wisprflow.ai/data-controls
10. GitHub — cjpais/Handy (open-source offline STT app). https://github.com/cjpais/Handy
11. GitHub — OpenWhispr/openwhispr (local + BYOK cloud dictation). https://github.com/OpenWhispr/openwhispr
12. GitHub — kurenn/murmur (Tauri + whisper.cpp dictation, clipboard-paste injection). https://github.com/kurenn/murmur
13. PromptQuorum — "Whisper.cpp vs faster-whisper 2026: Local STT Benchmarks." https://www.promptquorum.com/power-local-llm/local-whisper-stt-comparison-2026
14. Northflank — "Best open source speech-to-text model in 2026 (with benchmarks)." https://northflank.com/blog/best-open-source-speech-to-text-stt-model-in-2026-benchmarks
15. Dictato — "Parakeet vs Whisper on Mac: 80ms Local AI Dictation." https://dicta.to/blog/whisper-vs-parakeet-vs-apple-speech-engine/
16. CostGoat — "OpenAI Transcribe & Whisper API Pricing (Jul 2026)." https://costgoat.com/pricing/openai-transcription
17. OpenAI — API Pricing. https://platform.openai.com/docs/pricing/
18. TypeVox — "Dictation on Mac: The Complete Guide (2026)." https://typevox.app/blog/dictation-on-mac/
19. EdgeWhisper — macOS dictation (AXUIElement + NSPasteboard/CGEvent injection). https://edgewhisper.com/
20. Local AI Master — "Best Ollama Models 2026." https://localaimaster.com/blog/best-ollama-models
21. Voibe — "Best Open Source Wispr Flow Alternatives (2026)." https://www.getvoibe.com/resources/best-open-source-wispr-flow-alternatives/
22. eesel AI — "A deep dive Wispr Flow review: Is it safe to use in 2026?" https://www.eesel.ai/blog/wispr-flow-review
23. ModelPiper — "Voice Chat With Ollama on Mac: Add STT and TTS to Any Local Model." https://modelpiper.com/blog/ollama-voice-chat-mac

---

## Methodology Appendix

**Pipeline:** SCOPE → PLAN → RETRIEVE (6 web-search passes + 1 targeted fetch of wisprflow.ai/features) → TRIANGULATE (product claims cross-checked across official pages, ≥2 independent reviews, and open-source implementations) → SYNTHESIZE → PACKAGE.

**Source classes:** (a) vendor-official (wisprflow.ai, platform.openai.com) for feature and pricing ground truth; (b) independent 2026 reviews for behavior, limits, and criticism; (c) open-source repositories for implementation-level architecture evidence; (d) benchmark publications for latency/throughput figures.

**Triangulation examples:** the cloud-only claim appears on Wispr's own Data Controls page and in two independent reviews [8][9][22]; the clipboard-paste injection cascade appears independently in Murmur's docs, TypeVox's guide, and EdgeWhisper's implementation notes [12][18][19].

**Known bias:** several review sources (Willow, Spokenly, Voibe, eesel, Letterly, Dictato) are published by competing dictation products; they were used for factual claims (prices, limits, dates) that could be cross-verified, not for qualitative judgments.

**Machine-readable trail:** `sources.jsonl` (canonical source registry), `evidence.jsonl` (quotes/paraphrases with locators), `claims.jsonl` (atomic claims → supporting evidence), `run_manifest.json` (query, mode, assumptions).
