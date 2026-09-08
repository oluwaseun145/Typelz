# Typelz - Project Overview

<!-- blueprint:source-hash c88e8751bc59a0fbfdd5b6819a10c50c3de8012ee58b044606aadbc6d7ffd717 -->

> System-wide AI voice dictation for Windows: local Parakeet STT + BYOK LLM
> cleanup, inserted into the active application via global hotkey.

## Problem

Existing voice typing products (Typeless at $12-30/mo) lock users into a single
AI provider and charge recurring subscriptions. Typelz lets users bring their own
API key: speech recognition runs locally (Parakeet), and an OpenAI-compatible LLM
of the user's choice cleans and formats the transcript — for roughly cents to a
dollar per day instead of a monthly subscription.

## Users

- **Everyday typists** — fast system-wide voice typing without app boundaries.
- **Developers / technical users** — accurate code, URLs, paths, numbers via a
  personal dictionary and per-app profiles.
- **Professionals** — dictate emails, documents, notes with cleanup and formatting.
- **Privacy-conscious users** — audio stays on-device; only transcripts go to the
  chosen provider.
- **Cost-conscious users** — refuse $12-30/mo subscriptions, willing to manage
  their own API key.

## Features

Build-plan order (v1 items 1-14, then v1.1 items 15-22):

1. **Desktop application shell** ✓ - Vue + TypeScript app with Tauri integration
   (tray icon, settings window, typed command/event channel).
2. **Microphone capture** ✓ - Start, stop, device selection; frame streaming to
   Rust core.
3. **Parakeet transcription** ✓ - Local STT via ONNX Runtime on CPU: VAD-gated
   utterance detection, lazy model load, one-time ~670 MB download.
4. **BYOK provider configuration** ✓ - Add, validate, edit, remove LLM provider
   credentials stored in the OS keychain.
5. **LLM provider abstraction** - Common interface for models; OpenAI-compatible
   chat completion as the first adapter.
6. **LLM response normalization** - Normalize text, reasoning, tool calls, usage,
   and completion state; reasoning and text travel in separate channels.
7. **Dictation cleanup** - Remove fillers, repetitions, false starts,
   self-corrections; add punctuation, capitalization, grammar — without changing
   meaning.
8. **Text formatting** - Convert spoken structure into paragraphs, bullet lists,
   numbered lists, headings, checklists.
9. **System-wide text insertion** - Insert final text at cursor in the active
   application (Windows SendInput).
10. **Global dictation controls** - Toggle-to-talk (default) and push-to-talk
    hotkeys, plus stop/cancel.
11. **Dictation settings** - Microphone, cleanup behavior, style, language,
    default model, hotkeys, interaction sounds.
12. **Voice bar** - Overlay showing recording state, audio level, processing
    state, with interaction sound.
13. **Error handling and recovery** - Actionable errors for microphone, Parakeet,
    provider, auth, network, timeout, model failures.
14. **Windows release** - Package and validate complete v1 on Windows.
15. **Personal dictionary** - Define names, technical terms, acronyms, preferred
    spellings.
16. **Application profiles** - Different instructions, styles, vocabulary, models
    per application.
17. **Dictation history** - Optional store, search, copy, delete previous
    dictations.
18. **Privacy controls** - Show what stays local vs. sent; controls for history
    and data retention.
19. **Provider/model routing** - Different models for different AI tasks.
20. **Performance and streaming** - Optimize pipeline for low latency on target
    CPU.
21. **Thinking-model support** - Full support for reasoning-output providers;
    builds on item 6's channel separation.
22. **Linux release** - Package and validate on Linux.

Post-MVP items 23-45 (voice editing, rewrite/tone controls, translation, Ask
Anything, multilingual, local LLM, advanced history, Android, etc.) are planned
but not in scope for v1/v1.1.

## Data model

### ProviderConfig

- `id` (string/UUID) - unique identifier
- `name` (string) - human-readable label (e.g. "OpenAI", "Custom")
- `type` (enum: openai | anthropic | google | custom) - provider kind
- `base_url` (string, optional) - API endpoint URL
- `default_model` (string) - model identifier
- `capabilities` (object) - supported features (streaming, reasoning, etc.)
- **Stored in OS keychain** — raw API key never in app data files.

### UserPreferences

- `dictation_style` (enum: casual | professional | formal | friendly | concise
  | custom)
- `default_language` (string, default "en")
- `interaction_sound_enabled` (boolean)
- `start_stop_sounds` (object: start, stop, error sound paths or IDs)

### DictationSettings

- `cleanup_enabled` (boolean)
- `filler_removal` (boolean)
- `repetition_removal` (boolean)
- `self_correction_handling` (boolean)
- `punctuation_enabled` (boolean)
- `capitalization_enabled` (boolean)
- `formatting_rules` (object) - paragraph, list, heading rules

### PersonalDictionaryEntry (v1.1, feature 15)

- `id` (string/UUID)
- `word` (string) - the term to recognize
- `correction` (string) - how it should be spelled/capitalized
- `category` (enum: name | technical | product | custom)
- `application_scope` (enum: global | app-specific)
- `app_filter` (string, optional) - target application executable name

### ApplicationProfile (v1.1, feature 16)

- `id` (string/UUID)
- `executable_name` (string) - e.g. "code.exe", "chrome.exe"
- `instructions_override` (string, optional) - custom cleanup prompt for this app
- `style_override` (enum or custom text)
- `model_override` (string, optional) - different model for this app
- `vocabulary_ids` (string[]) - linked personal dictionary entries

### DictationHistoryEntry (v1.1, feature 17)

- `id` (string/UUID)
- `raw_transcript` (string) - original Parakeet output
- `cleaned_text` (string) - LLM-processed result
- `timestamp` (datetime)
- `application` (string, optional) - where it was inserted
- `provider_used` (string) - which provider processed it

### AppDiagnosticState

- `parakeet_model_status` (enum: unconfigured | downloading | ready | error)
- `last_error` (string, nullable)
- `feature_flags` (object) - internal toggle state

> **Lock note:** ProviderConfig and DictationSettings are consumed by features
> 4-8. PersonalDictionaryEntry and ApplicationProfile are consumed by features
> 15-16. DictationHistoryEntry is consumed by feature 17 only when the user
> enables history.

## Tech stack

- **Vue 3 + TypeScript** - Settings window, voice bar overlay, provider config UI
  (runs in Tauri webview).
- **Tauri (Rust)** - Microphone capture, Silero VAD, Parakeet inference via ONNX
  Runtime, global hotkeys (tauri-plugin-global-shortcut), text insertion via
  Windows SendInput, OS keychain access.
- **Parakeet TDT v3 INT8** - 600M parameter model, ~670 MB, CC-BY-4.0 license.
  ONNX Runtime on CPU; ~10-20x real-time on Ryzen 5 4650U.
- **Silero VAD** - Voice activity detection for utterance boundary detection.
- **ONNX Runtime** - CPU-only inference for Parakeet (no FP16, no CUDA).
- **OpenAI-compatible API** - First LLM adapter; normalized request/response with
  separate reasoning/text channels from day one.
- **OS credential store** - Secure local storage for API keys (Windows Credential
  Manager on Windows, keyring on Linux).

## Monetization

Not in v1. The immediate driver is personal: a Typeless replacement at token cost
instead of $12-30/mo. Future monetization must keep the BYOK path free and
uncapped — no requirement to use a hosted service for core dictation. Potential
streams: premium features, optional hosted services, team/workspace features.

## UI/UX

**Feel:** fast, lightweight, modern, unobtrusive. No unnecessary animations or
delays — dictation is interaction where responsiveness matters.

**Main flow:** Microphone → Parakeet → Raw transcript → LLM → Cleaned text →
Active app.

**Routes/screens:**

- **Settings window** - Provider config, model selection, cleanup/style, personal
  dictionary, application profiles, history/privacy, keyboard shortcuts.
- **Voice bar overlay** - Appears on hotkey press; shows recording state, audio
  level; turns to "processing" during transcription/cleanup; dismisses after text
  insertion.
- **System tray icon** - Quick access to settings and dictation toggle.

The user should always see which provider/model is active. Errors must be
actionable and understandable.

## Deployment

- **Target:** Native desktop app via Tauri packaging.
- **Platforms:** Windows (v1), Linux (v1.1). Future: Android.
- **No hosted backend** required for core dictation path. Provider API calls go
  directly from the user's app.
- **Model distribution:** One-time ~670 MB Parakeet INT8 download on first run,
  cached locally. Not part of the app bundle.
- **Secure credential storage:** OS keychain — no raw keys in app data.
- **License compliance:** Parakeet is CC-BY-4.0; attribution required in-app.

> TODO: CI/CD pipeline for Windows/Linux builds, auto-updater strategy.

## Open questions

> **PostgreSQL / Redis** — project-plan.md mentions these as "planned application
> infrastructure where a backend is required" but no concrete migration plan or
> timeline exists. Not needed for v1 (offline-first).
>
> **Monetization model** — vague in project-plan.md. No immediate model defined;
> future streams listed but not scoped.
>
> **Auto-updater** — Tauri has built-in update support but no strategy or server
> is planned yet.
>
> **Fallback STT engine** — whisper.cpp documented as a second engine behind the
> same Rust interface if Parakeet underperforms on target hardware. Engine swap,
> not a redesign — no implementation work needed now.
