# Project Plan

## 1. Problem - What problem are we solving?

Build a system-wide AI voice dictation application that turns natural, messy
speech into clean, properly formatted text and inserts it into the active
application. The app should combine local Parakeet speech recognition with a
flexible BYOK LLM layer so users can choose their own AI provider/model for
transcript cleanup and future AI features. The product should start as a
Typeless-style dictation tool and expand into a broader voice-driven AI
productivity assistant.

Core principle: speech recognition happens locally, while the user controls
which LLM processes the transcript through their own API key or a future local
model.

Reference product and why this exists:

The direct reference is Typeless (typeless.com): a voice keyboard that works
in every app. Press the shortcut (Right Alt on Windows), a voice bar appears,
speak, press the shortcut again, and cleaned text inserts at the cursor. Its
paid tier is $12/mo billed annually ($30/mo monthly); the free tier caps at
8,000 words/week, which a heavy user hits in 2-3 days. Typeless also
demonstrates the full feature direction: filler/repetition/self-correction
removal, auto-formatting, personal dictionary, per-app tone, speak-to-edit
selected text, speak-to-ask about selected text, and an "Ask Anything" voice
assistant that returns answers in a pop-up panel.

Typelz's differentiator is that the user owns the AI: local STT plus BYOK LLM.
A heavy daily dictation habit costs a fraction of the subscription (roughly
cents to a dollar per day in tokens, depending on provider and model). There
is open-source precedent for this model: OpenTypeless ships a free BYOK
desktop dictation app with the same architecture (local or cloud STT + user
keys, stored locally in an encrypted config).

## 2. Users - Who is this for?

- People who want fast system-wide voice typing.
- Developers and technical users who need accurate handling of code, technical
  terms, URLs, paths, numbers, and commands.
- Professionals who dictate emails, documents, notes, and messages.
- Users who want AI-assisted rewriting and formatting without being locked to
  one AI provider.
- Privacy-conscious users who want audio to remain on-device.
- Power users who want configurable models, prompts, vocabulary, styles, and
  application-specific behavior.
- Cost-conscious users who refuse the $12-30/mo subscription model of the
  incumbent products and will happily manage their own API key.

## 3. Features - What does the MVP need?

- Local Parakeet speech-to-text.
- BYOK LLM provider configuration.
- Provider/model abstraction supporting OpenAI-style and other provider APIs.
- Secure local API-key storage.
- Dictation cleanup for fillers, repetition, false starts, punctuation,
  capitalization, and grammar.
- Self-correction handling so the final intended statement is retained.
- Conservative transcript cleanup that does not answer questions or invent
  content.
- System-wide text insertion into the active application.
- Global hotkey and push-to-talk/toggle-to-talk controls.
- Microphone selection and basic audio controls.
- Configurable dictation style and formatting.
- Streaming and responsive transcription/cleanup where practical.
- Basic dictation history and settings.

v1 vs v1.1 split (all v1.1 features stay in the plan, just later):

v1 (the release you use daily):

- Local Parakeet STT, BYOK provider configuration, provider abstraction
  (OpenAI-compatible first), response normalization with separate
  reasoning/text channels.
- Dictation cleanup and text formatting.
- System-wide text insertion, global hotkey (toggle-to-talk default,
  push-to-talk configurable), microphone selection and settings.
- Voice bar / listening state UI with interaction sound.
- Error handling and recovery for microphone, Parakeet, provider,
  authentication, network, timeout, and model errors.
- Windows release.

v1.1 (moved, not dropped):

- Personal dictionary, application profiles, dictation history, privacy
  controls, provider/model routing, performance/streaming pass, Linux release.
- Thinking-model support: full provider support for models that emit
  reasoning output (tested against real thinking models when we get there),
  building on the reasoning/text channel separation that v1 ships.

Explicit v1 non-goals:

- No live word-by-word partial transcription (VAD-gated transcribe-on-utterance
  end; live partials are a post-MVP candidate).
- No thinking-model providers yet (OpenAI-compatible plain chat completion
  only; the response format still separates reasoning from text from day one).
- No multilingual or Nigerian language support (English-first with Parakeet
  v3's 25-language model).
- No cloud processing of audio, no hosted backend, no mobile.

Post-MVP (unchanged in intent): voice text editing, rewrite and tone controls,
voice formatting commands, translation, Ask Anything, selected-text
intelligence, custom commands, advanced personalization and vocabulary,
multilingual dictation including Nigerian languages, model fallback rules,
local LLM provider, advanced audio controls, advanced history.

## 4. Data - What are we storing?

- Provider configurations and model settings.
- API credentials stored securely in the local OS credential store; raw keys
  should not be stored in ordinary application data.
- User preferences.
- Dictation settings.
- Custom vocabulary/personal dictionary.
- Application-specific profiles and instructions.
- Dictation history, when enabled by the user.
- Model/task routing preferences.
- Local application state and diagnostics needed for troubleshooting.
- No microphone audio should be persistently stored by default.
- No transcript should be sent anywhere except the provider selected by the
  user for the relevant task.
- The Parakeet model weights (approximately 670 MB INT8) are downloaded once
  on first run and cached locally; the model is not part of the application
  data and must not be re-downloaded on every start.

## 5. Tech - What stack are we using?

- Desktop application: Vue + TypeScript.
- Local/native desktop layer: Tauri with Rust.
- Local speech recognition: Parakeet.
- LLM integration: provider-agnostic BYOK LLM engine.
- Native provider adapters for providers such as OpenAI, Anthropic, and Google.
- OpenAI-compatible/custom endpoint adapter for compatible services.
- Normalized internal LLM request/response format.
- Separate reasoning/thinking representation so thinking output never becomes
  dictated text.
- Local secure credential storage using the operating system's
  credential/keychain facilities.
- Local application data storage appropriate to the desktop platform.
- PostgreSQL and Redis are planned application infrastructure where a backend
  is required; the core offline dictation path should not depend on a remote
  server.
- Initial target: Windows and Linux.
- Future target: Android.
- English first, with French, Spanish, German, and Nigerian languages planned
  for later expansion.

Concrete architecture (confirmed against the target hardware):

Target hardware: the primary machine is an AMD Ryzen 5 4650U laptop (6 cores /
12 threads, no discrete GPU). All design choices below follow from CPU-only
operation.

- Rust core (Tauri) owns the entire pipeline: microphone capture, Silero VAD,
  ONNX Runtime inference for Parakeet, global hotkeys
  (tauri-plugin-global-shortcut), text insertion via the Windows SendInput
  API, and OS keychain access.
- Vue (webview) is the UI only: settings window, voice bar overlay, provider
  configuration, history. It talks to the Rust core through typed Tauri
  commands and events, and never touches audio or the pipeline directly.
- Parakeet model: `parakeet-tdt-0.6b-v3` (600M parameters, 25 languages,
  native punctuation and capitalization, CC-BY-4.0 license so attribution is
  required in the app), run as INT8 ONNX via ONNX Runtime on CPU (approximately
  670 MB). No FP16 (meaningless on x86 CPU), no CUDA. Expect roughly 10-20x
  real-time transcription on the 4650U; a 1 minute utterance should transcribe
  in a few seconds.
- Pipeline shape: toggle hotkey starts capture, Silero VAD detects
  speech start/end, the utterance accumulates in memory, the hotkey press (or
  VAD silence boundary) ends it, Parakeet transcribes, the LLM cleans and
  formats, the result inserts at the cursor. The voice bar shows recording
  state and audio level live, without live word-by-word text.
- Model lifecycle: load the Parakeet model lazily on first dictation, keep it
  warm briefly, release after inactivity to respect the laptop's memory.
- Fallback engine: if Parakeet quality or speed disappoints on this hardware,
  whisper.cpp (small or base, INT8) is a documented second engine behind the
  same Rust interface. This is an engine swap, not a redesign.
- LLM call: plain OpenAI-compatible chat completion for v1. The normalized
  response carries `reasoning` and `text` as separate channels from the first
  adapter, so thinking-model support later is a normalization-layer feature,
  not a rewrite.
- Cleanup quality is prompt-driven: the dictation cleanup prompt (filler,
  repetition, self-correction, formatting rules, conservatism constraints) is
  a first-class artifact validated by testing against real recordings, not
  specified up front.

Latency budget (measured during v1 build, targets to hold):

- Hotkey press to "recording" state: under 200 ms.
- Utterance end to inserted text: transcribe time (a few seconds on the
  4650U) plus one LLM round-trip (1-3 s typical). Total for a 30 s utterance
  should stay under roughly 10 s on the target hardware.
- First-ever dictation includes the one-time model download (about 670 MB);
  first dictation after load should not include model load time (warm state).

## 6. Monetize - How will this make money?

The core product should support BYOK so users can bring their own AI provider
credentials rather than paying for our model usage. Potential future
monetization can include a free/core version, paid premium features, optional
hosted services, team/workspace features, and convenience features that do not
require us to control the user's AI provider. There should be no requirement
to monetize by storing or selling user audio or transcripts.

The immediate driver is personal: a fully functional Typeless replacement at
token cost instead of a $12-30/mo subscription. Any future monetization must
keep the BYOK path free and uncapped, or the core promise breaks.

## 7. UI/UX - How should this look and feel?

The application should feel fast, lightweight, modern, and unobtrusive.
Dictation should be accessible from anywhere through a global hotkey or
push-to-talk control without forcing the user into a separate editor.

The main UI should make the core flow obvious:

Microphone -> Parakeet -> Raw transcript -> selected LLM -> cleaned/formatted
text -> active application.

The settings area should provide clear sections for:

- Microphone and dictation controls.
- LLM providers and API keys.
- Model selection and task routing.
- Dictation cleanup/style.
- Personal dictionary.
- Application-specific profiles.
- History and privacy.
- Keyboard shortcuts.

The user should always be able to see which provider/model is being used.
Errors should be actionable and understandable. The interface should avoid
unnecessary animations or delays because dictation is an interaction where
responsiveness matters.

The voice bar (Typeless's signature element) is in v1: a small overlay near
the screen edge that appears on hotkey press, shows recording state and audio
level, turns to "processing" while transcription/cleanup runs, and dismisses
when text is inserted. An interaction sound on start/stop is part of the
settings surface.

## 8. Deployment - Where and how will this ship?

The primary deliverable is a native desktop application for Windows and Linux,
packaged through Tauri. The application should run the Parakeet inference
locally and communicate directly with the user-selected LLM provider when BYOK
cloud inference is enabled.

There should be no mandatory hosted backend for the core dictation path. A
future optional backend may support accounts, synchronization, teams, billing,
or other online features.

Initial deployment requirements:

- Windows desktop package.
- Linux desktop package.
- Local Parakeet model distribution/download flow.
- Secure local credential storage.
- No cloud audio processing.
- Provider API requests made directly from the user's application where
  practical.
- Clear configuration for provider base URL, API key, model, and
  provider-specific capabilities.
- Future Android build with an appropriate native/local inference
  architecture.
- Future optional hosted services must be architected separately from the
  offline-first core.

Release order: v1 ships as the Windows package only (primary hardware is a
Windows laptop). The Linux package moves to v1.1. The model download flow
ships with v1: first run offers a one-time download of the approximately
670 MB INT8 Parakeet weights into the local model cache.
