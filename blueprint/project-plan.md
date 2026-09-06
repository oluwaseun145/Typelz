# Project Plan

## 1. Problem - What problem are we solving?

Build a system-wide AI voice dictation application that turns natural, messy speech into clean, properly formatted text and inserts it into the active application. The app should combine local Parakeet speech recognition with a flexible BYOK LLM layer so users can choose their own AI provider/model for transcript cleanup and future AI features. The product should start as a Typeless-style dictation tool and expand into a broader voice-driven AI productivity assistant.

Core principle: speech recognition happens locally, while the user controls which LLM processes the transcript through their own API key or a future local model.

## 2. Users - Who is this for?

- People who want fast system-wide voice typing.
- Developers and technical users who need accurate handling of code, technical terms, URLs, paths, numbers, and commands.
- Professionals who dictate emails, documents, notes, and messages.
- Users who want AI-assisted rewriting and formatting without being locked to one AI provider.
- Privacy-conscious users who want audio to remain on-device.
- Power users who want configurable models, prompts, vocabulary, styles, and application-specific behavior.

## 3. Features - What does the MVP need?

- Local Parakeet speech-to-text.
- BYOK LLM provider configuration.
- Provider/model abstraction supporting OpenAI-style and other provider APIs.
- Secure local API-key storage.
- Dictation cleanup for fillers, repetition, false starts, punctuation, capitalization, and grammar.
- Self-correction handling so the final intended statement is retained.
- Conservative transcript cleanup that does not answer questions or invent content.
- System-wide text insertion into the active application.
- Global hotkey and push-to-talk/toggle-to-talk controls.
- Microphone selection and basic audio controls.
- Configurable dictation style and formatting.
- Streaming and responsive transcription/cleanup where practical.
- Basic dictation history and settings.

## 4. Data - What are we storing?

- Provider configurations and model settings.
- API credentials stored securely in the local OS credential store; raw keys should not be stored in ordinary application data.
- User preferences.
- Dictation settings.
- Custom vocabulary/personal dictionary.
- Application-specific profiles and instructions.
- Dictation history, when enabled by the user.
- Model/task routing preferences.
- Local application state and diagnostics needed for troubleshooting.
- No microphone audio should be persistently stored by default.
- No transcript should be sent anywhere except the provider selected by the user for the relevant task.

## 5. Tech - What stack are we using?

- Desktop application: Vue + TypeScript.
- Local/native desktop layer: Tauri with Rust.
- Local speech recognition: Parakeet.
- LLM integration: provider-agnostic BYOK LLM engine.
- Native provider adapters for providers such as OpenAI, Anthropic, and Google.
- OpenAI-compatible/custom endpoint adapter for compatible services.
- Normalized internal LLM request/response format.
- Separate reasoning/thinking representation so thinking output never becomes dictated text.
- Local secure credential storage using the operating system's credential/keychain facilities.
- Local application data storage appropriate to the desktop platform.
- PostgreSQL and Redis are planned application infrastructure where a backend is required; the core offline dictation path should not depend on a remote server.
- Initial target: Windows and Linux.
- Future target: Android.
- English first, with French, Spanish, German, and Nigerian languages planned for later expansion.

## 6. Monetize - How will this make money?

The core product should support BYOK so users can bring their own AI provider credentials rather than paying for our model usage. Potential future monetization can include a free/core version, paid premium features, optional hosted services, team/workspace features, and convenience features that do not require us to control the user's AI provider. There should be no requirement to monetize by storing or selling user audio or transcripts.

## 7. UI/UX - How should this look and feel?

The application should feel fast, lightweight, modern, and unobtrusive. Dictation should be accessible from anywhere through a global hotkey or push-to-talk control without forcing the user into a separate editor.

The main UI should make the core flow obvious:

Microphone → Parakeet → Raw transcript → selected LLM → cleaned/formatted text → active application.

The settings area should provide clear sections for:
- Microphone and dictation controls.
- LLM providers and API keys.
- Model selection and task routing.
- Dictation cleanup/style.
- Personal dictionary.
- Application-specific profiles.
- History and privacy.
- Keyboard shortcuts.

The user should always be able to see which provider/model is being used. Errors should be actionable and understandable. The interface should avoid unnecessary animations or delays because dictation is an interaction where responsiveness matters.

## 8. Deployment - Where and how will this ship?

The primary deliverable is a native desktop application for Windows and Linux, packaged through Tauri. The application should run the Parakeet inference locally and communicate directly with the user-selected LLM provider when BYOK cloud inference is enabled.

There should be no mandatory hosted backend for the core dictation path. A future optional backend may support accounts, synchronization, teams, billing, or other online features.

Initial deployment requirements:
- Windows desktop package.
- Linux desktop package.
- Local Parakeet model distribution/download flow.
- Secure local credential storage.
- No cloud audio processing.
- Provider API requests made directly from the user's application where practical.
- Clear configuration for provider base URL, API key, model, and provider-specific capabilities.
- Future Android build with an appropriate native/local inference architecture.
- Future optional hosted services must be architected separately from the offline-first core.
