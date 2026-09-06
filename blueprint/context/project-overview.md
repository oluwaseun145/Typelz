# Typelz - Project Overview

<!-- blueprint:source-hash c62ff40fafcb256aa9d5c8898a3a212e93425efe16917c9eccc78f43b9b3e774 -->

> System-wide AI voice dictation for Windows: local Parakeet STT + BYOK LLM cleanup,
> inserting clean text into the active application.

## Problem

Incumbent voice keyboards (e.g. Typeless) are subscription-locked ($12-30/mo,
8,000-word weekly cap) and tie the AI to their provider. Typelz replaces that
with local speech recognition plus a user-owned LLM key: a heavy daily
dictation habit at token cost, audio that stays on-device, and no provider
lock-in.

## Users

- Fast system-wide voice typists (any application, no separate editor)
- Developers/technical users — code, technical terms, URLs, paths, numbers
- Professionals dictating emails, documents, notes, messages
- BYOK users who refuse provider lock-in
- Privacy-conscious users (audio stays on-device)
- Power users — configurable models, prompts, vocabulary, styles, per-app behavior
- Cost-conscious users who manage their own API key instead of a subscription

## Features

v1, in build-plan order:

1. **Desktop application shell** - Vue + TypeScript app with Tauri integration: tray icon, settings window, typed command/event channel.
2. **Microphone capture** - reliable mic audio with start, stop, and device selection.
3. **Parakeet transcription** - local Parakeet TDT v3 INT8 via ONNX Runtime on CPU: VAD-gated utterances, lazy model load, one-time model download.
4. **BYOK provider configuration** - add/validate/edit/remove LLM provider credentials in the OS keychain.
5. **LLM provider abstraction** - common model interface; OpenAI-compatible chat completion first.
6. **LLM response normalization** - text, reasoning, tool calls, usage, completion state; reasoning and text in separate channels from day one.
7. **Dictation cleanup** - remove fillers, repetitions, false starts, self-corrections; fix punctuation, capitalization, grammar without changing meaning.
8. **Text formatting** - spoken structure into paragraphs, bullet/numbered lists, headings, checklists.
9. **System-wide text insertion** - insert final text into the active application (Windows SendInput). ← headline feature
10. **Global dictation controls** - toggle-to-talk (default) and push-to-talk hotkeys, plus stop/cancel controls.
11. **Dictation settings** - microphone, cleanup behavior, style, language, default model, hotkeys, interaction sounds.
12. **Voice bar** - overlay with recording state, live audio level, processing state, and interaction sound.
13. **Error handling and recovery** - actionable handling for microphone, Parakeet, provider, authentication, network, timeout, and model errors.
14. **Windows release** - package and validate the complete v1.

v1.1 (moved, not dropped): 15 personal dictionary · 16 application profiles ·
17 dictation history (opt-in store/search/copy/delete) · 18 privacy controls ·
19 provider/model routing · 20 performance and streaming pass · 21
thinking-model support (builds on item 6's channel split) · 22 Linux release.

Post-MVP (23-38): voice text editing, rewrite/tone controls, voice formatting
commands, translation, Ask Anything, selected-text intelligence, custom
commands, advanced personalization and vocabulary, multilingual and Nigerian
language support, model fallback rules, local LLM provider, advanced audio and
history controls.

Expansion (39-45): Android app, cross-device settings sync, team/workspace
features, workflow automation, plugin/integration system, advanced AI agent
capabilities, macOS.

## Data model

v1 is local-only: OS keychain for secrets, plain local application data for
settings. No database, no server. (PostgreSQL/Redis only if a future backend
ships — see Tech stack.)

### ProviderConfig

- `id` (string) - stable local id
- `name` (string) - user-facing label
- `adapter` (enum) - `openai-compatible` in v1; native `openai`/`anthropic`/`google` adapters later
- `baseUrl` (string) - provider endpoint
- `modelId` (string) - selected model
- `capabilities` (flags) - provider-specific capability bits
- `credentialRef` (string) - reference into the OS keychain; never the key itself

One-to-one with a keychain entry per provider. Used by items 4-6, 19, 21, 35.

### Credential (OS keychain)

- `keychainId` (string) - entry id referenced by `ProviderConfig.credentialRef`
- `secret` (string, encrypted at rest) - the API key. Raw keys never live in ordinary app data.

### DictationSettings (user preferences)

- `microphoneDeviceId` (string) - selected capture device (item 2)
- `talkMode` (enum) - `toggle` (default) | `push-to-talk` (item 10)
- `hotkey` (string) - global shortcut (item 10)
- `cleanupStyle` (string/enum) - dictation style and formatting preferences (items 7-8, 11)
- `language` (string) - `en` in v1 (Parakeet v3 ships a 25-language model)
- `defaultProviderId` (string) - default model selection (items 4, 11)
- `interactionSound` (bool/choice) - voice bar start/stop sound (items 11, 12)

One per user profile. Application-specific overrides arrive with `AppProfile`
in v1.1 (item 16).

### PersonalDictionary (v1.1, item 15)

- `entries[]` - `term`, `category` (name / technical term / acronym / product), `preferredSpelling`

Injected into the cleanup prompt.

### AppProfile (v1.1, item 16)

- `matcher` (string) - target application
- `instructions` (text), `style` (string), `vocabulary` (refs to PersonalDictionary), `formatting` (string), `providerId` (string)

Overrides `DictationSettings` for one application.

### DictationEntry (v1.1, item 17 — opt-in only)

- `id` (string), `timestamp` (datetime)
- `finalText` (text) - cleaned output
- `rawTranscript` (text?) - only when the user opts in
- `providerId` (string), `modelId` (string), `durationSec` (number), `appName` (string?)

Never stored by default; retention controlled by `PrivacySettings` (item 18).

### PrivacySettings (v1.1, item 18)

- `historyEnabled` (bool), `storeRawTranscript` (bool), `retention` (enum/duration)

### ModelCache

- `model` (const) - `parakeet-tdt-0.6b-v3` INT8 ONNX, ~670 MB
- `path` (local model cache dir) - downloaded once on first run; not application data; must not re-download on every start.

> Locked shapes later features depend on:
> 1. API keys live in the OS keychain only (items 4, 13, 18 depend on this).
> 2. The normalized LLM response carries `reasoning` and `text` as separate channels (item 6 locks the shape; item 21 builds on it).
> 3. No microphone audio is persisted by default, and transcripts go only to the user-selected provider (items 13, 18, 36 depend on this).

## Tech stack

- **Vue 3 + TypeScript (webview)** - UI only: settings window, voice bar overlay. Talks to the Rust core through typed Tauri commands/events; never touches audio or the pipeline.
- **Tauri + Rust** - core: microphone capture, Silero VAD, ONNX Runtime inference, global hotkeys (`tauri-plugin-global-shortcut`), text insertion (Windows SendInput), OS keychain access.
- **Parakeet `parakeet-tdt-0.6b-v3`** - local STT: 600M parameters, 25 languages, native punctuation and capitalization, CC-BY-4.0 (attribution required in the app), INT8 ONNX ~670 MB, CPU-only.
- **ONNX Runtime** - Parakeet inference on CPU (no FP16, no CUDA).
- **Silero VAD** - speech start/end detection; transcribe-on-utterance-end (no live word-by-word partials in v1).
- **BYOK LLM engine** - provider-agnostic; v1 = plain OpenAI-compatible chat completion; normalized response with `reasoning` + `text` channels; native OpenAI/Anthropic/Google adapters planned.
- **OS credential store** - Windows keychain / Linux keyring for API keys.
- **whisper.cpp (small/base, INT8)** - documented fallback STT engine behind the same Rust interface if Parakeet disappoints on target hardware.
- **PostgreSQL + Redis** - future-only, if/when an optional backend ships; the offline dictation path never depends on them.

Target hardware: AMD Ryzen 5 4650U laptop (6 cores / 12 threads, no discrete
GPU) — all design choices follow from CPU-only operation (expect roughly
10-20x real-time transcription).

## Monetization

Not in v1 — the BYOK path stays free and uncapped; that is the core promise.
Immediate driver is personal: a fully functional Typeless replacement at token
cost instead of $12-30/mo. Future candidates: paid premium features, optional
hosted services, team/workspace features, convenience features — none may
require storing or selling user audio or transcripts.

## UI/UX

Fast, lightweight, modern, unobtrusive; no unnecessary animation or delay. The
core flow is always visible: Microphone → Parakeet → raw transcript → selected
LLM → cleaned/formatted text → active application. The user always sees which
provider/model is in use; errors are actionable and understandable.

- **Settings window** - sections: microphone & dictation controls, LLM providers & API keys, model selection & task routing, cleanup/style, personal dictionary, application profiles, history & privacy, keyboard shortcuts.
- **Voice bar overlay** (v1, item 12) - small overlay near the screen edge on hotkey press; recording state + live audio level (no live text); turns "processing" while transcription/cleanup runs; dismisses when text inserts; interaction sound on start/stop.
- **Tray icon** - system presence and entry to settings (item 1).

## Deployment

- **Target** - native desktop packaged through Tauri; **v1 = Windows package only**. Linux moves to v1.1 (item 22); Android is expansion (item 39).
- **No hosted backend** - core dictation is offline-first; provider API requests go directly from the user's app. Any future optional backend (accounts, sync, teams, billing) is architected separately.
- **Model distribution** - first run offers a one-time ~670 MB INT8 Parakeet download into the local model cache.
- **Storage** - OS keychain + local app data only; no database in v1; no cloud audio processing.
- **Commands** - `npm run dev` (dev server), `npm run build` (vue-tsc typecheck + Vite production build), `npm run preview`. Tauri packaging/bundle commands land with item 14.
- **Env vars** - none in v1; provider base URL, key, and model are user-configured (key in keychain).

Latency budget (v1 targets on the 4650U):

- Hotkey press → "recording" state: under 200 ms.
- Utterance end → inserted text: transcribe time (a few seconds) + one LLM round-trip (1-3 s); a 30 s utterance stays under roughly 10 s total.
- First-ever dictation includes the one-time model download; dictation after load is warm (no model-load time).

## Open questions

- project-plan.md §3 lists "basic dictation history and settings" as MVP, but the v1/v1.1 split and build plan put history in v1.1 (item 17) while settings stay in v1 (item 11). Build plan wins; align the §3 wording.
- project-plan.md §3 lists "streaming and responsive transcription where practical" as MVP, but the performance/streaming pass is v1.1 (item 20) and v1 explicitly excludes live partials. v1 gets responsiveness via the voice bar (item 12); align the §3 wording.
- project-plan.md §5 names PostgreSQL + Redis as "planned application infrastructure where a backend is required," but no build-plan item ships a backend until expansion (items 40-41). Confirm they are post-MVP-only.
- The dictation cleanup prompt is a first-class artifact "validated by testing against real recordings" (item 7), but no recording corpus exists yet. > TODO - source real recordings before spec'ing item 7.
