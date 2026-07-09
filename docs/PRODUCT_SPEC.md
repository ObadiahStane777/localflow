# LocalFlow Product Spec

Behavioral definition of every feature. "Done" means the behavior below, not the code existing. Feature set is modeled on Wispr Flow's publicly documented behavior (see docs/RESEARCH.md §2 for sources); implementation is entirely our own.

## 1. Core dictation loop

- Hold the hotkey → recording starts within 50 ms (visible indicator: tray icon change + optional floating pill).
- Release → text appears at the cursor of the focused app. Local preset p50 <1.5 s, p95 <2.5 s for a 10 s utterance.
- Works in: native macOS apps, Electron apps (Slack, VS Code, Discord), browsers (Gmail, Notion, Google Docs), Terminal/iTerm.
- Toggle mode: tap to start, tap to stop, for long dictation.
- If no speech detected: nothing is injected, subtle "no speech" indicator. Never inject hallucinated text from silence.
- User's clipboard content is preserved across every injection.
- Focus is never stolen from the target app.

## 2. Auto-editing (formatter behavior contract)

Given raw transcript, the formatter must:
- Remove fillers (um, uh, like-as-filler, you know) and false starts.
- Apply self-corrections keeping only final intent: "meet at 2… actually make it 3" → "meet at 3".
- Add punctuation, capitalization, and paragraph breaks from sentence structure.
- Honor spoken commands: "new line", "new paragraph", "comma", "period", "question mark", "quote … end quote".
- Format spoken enumerations ("first… second… third…" or "one… two…") as numbered/bulleted lists.
- Preserve technical tokens verbatim: camelCase, snake_case, CLI flags, file paths, URLs spelled out.
- Prefer dictionary spellings for registered terms.
- Expand snippet triggers to their expansions.
- NEVER answer a dictated question — "what time is the meeting" is output as text, not answered.
- NEVER add content the user didn't say (no sign-offs, no "Sure!", no elaboration).
- Output is the cleaned text only — no quotes, no preamble.

## 3. Styles (per-app tone)

- App identity (bundle id / exe name) selects a style profile automatically; user can edit profiles and add app matches.
- Defaults: email → formal, full sentences; chat apps → casual, contractions ok; docs → structured prose; code editors & terminal → verbatim mode (no LLM rewriting); everything else → neutral cleanup.
- Context signal is app identity only. No screenshots, no window-content capture (explicit anti-goal; see RESEARCH.md §2.3 on the Wispr backlash).

## 4. Command Mode

- Trigger: hotkey while text is selected in the target app.
- Spoken utterance is treated as an instruction applied to the selection ("make this more concise", "rewrite as bullet points", "fix the grammar", "translate to Spanish").
- Replacement text is injected over the selection; the original is kept in History for recovery.
- Ambiguity rule: selection + imperative-sounding utterance → command; selection + clearly dictational utterance → dictation (with a force-dictate modifier chord as escape hatch).

## 5. Personalization

- **Dictionary:** user-added terms (names, jargon, products) transcribe with correct spelling/casing. Sources: manual CRUD + suggested additions from detected corrections (suggestions require user approval).
- **Snippets:** trigger phrase → expansion (text, links, templates). Spoken "insert my calendly" → the link.
- **History:** searchable log of every dictation (timestamp, app, raw, formatted, providers, latency). Copy either version. Default retention 30 days, configurable, local-only.

## 6. Providers & presets

- STT: whisper.cpp (any downloaded GGUF), Parakeet (English), OpenAI (`gpt-4o-transcribe`, `gpt-4o-mini-transcribe`, `whisper-1`).
- Formatting: Ollama (local model of user's choice, 3B recommended), OpenAI chat (default `gpt-4o-mini`, editable model string), Passthrough (regex only).
- Presets: fully-local / fast-local / hybrid (local STT + cloud formatting — audio never leaves the machine) / full-cloud.
- Switching providers or presets takes effect on the next dictation, no restart.
- Cloud failures degrade to passthrough with the raw transcript injected and a toast — a dictation is never lost.

## 7. Privacy posture (product identity — non-negotiable)

- Default preset is fully local; zero network traffic out of the box (model downloads excepted, user-initiated).
- API keys in OS keychain only. No telemetry. No account. All data in local SQLite under the user's app-data dir.
- The privacy story is a first-class marketing feature: state it in onboarding and README.

## 8. Non-goals (v1)

Mobile apps · meeting transcription/notes · TTS · team/shared features · Wispr-style screenshot context awareness (permanent anti-goal).
