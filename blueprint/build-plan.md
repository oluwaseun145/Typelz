# Build Plan

## v1: Daily-use release (Windows)

- [x] 1. **Desktop application shell** - establish the Vue + TypeScript desktop application and Tauri integration (tray icon, settings window, typed command/event channel)
- [ ] 2. **Microphone capture** - capture microphone audio reliably with start, stop, and device selection
- [ ] 3. **Parakeet transcription** - run Parakeet TDT v3 INT8 locally via ONNX Runtime on CPU with VAD-gated utterance detection, lazy model load, and one-time model download
- [ ] 4. **BYOK provider configuration** - let users add, validate, edit, and remove their own LLM provider credentials in the OS keychain
- [ ] 5. **LLM provider abstraction** - provide a common interface for models, OpenAI-compatible chat completion first
- [ ] 6. **LLM response normalization** - normalize text, reasoning, tool calls, usage, and completion state; reasoning and text travel in separate channels from day one
- [ ] 7. **Dictation cleanup** - clean fillers, repetitions, false starts, self-corrections, punctuation, capitalization, and grammar without changing meaning
- [ ] 8. **Text formatting** - convert spoken structure into paragraphs, bullet lists, numbered lists, headings, checklists, and other appropriate formatting
- [ ] 9. **System-wide text insertion** - insert final text into the currently active application (Windows SendInput)
- [ ] 10. **Global dictation controls** - toggle-to-talk (default) and push-to-talk global hotkeys, plus stop/cancel controls
- [ ] 11. **Dictation settings** - configure microphone, cleanup behavior, style, language, default model, hotkeys, and interaction sounds
- [ ] 12. **Voice bar** - overlay showing recording state, audio level, and processing state, with interaction sound
- [ ] 13. **Error handling and recovery** - actionable handling for microphone, Parakeet, provider, authentication, network, timeout, and model errors
- [ ] 14. **Windows release** - package and validate the complete v1 on Windows

## v1.1: Moved v1-adjacent features and second platform

- [ ] 15. **Personal dictionary** - let users define names, technical terms, acronyms, products, and preferred spellings
- [ ] 16. **Application profiles** - configure different instructions, styles, vocabulary, formatting, and models for different applications
- [ ] 17. **Dictation history** - optionally store, search, copy, and delete previous dictations
- [ ] 18. **Privacy controls** - clearly show what stays local, what is sent to the selected provider, and provide controls for history and data retention
- [ ] 19. **Provider/model routing** - allow different models to be selected for different AI tasks
- [ ] 20. **Performance and streaming** - optimize the pipeline for low latency and responsive partial/final results, evaluated on the target CPU
- [ ] 21. **Thinking-model support** - full support for providers/models that emit reasoning output, validated against real thinking models; builds on item 6's channel separation
- [ ] 22. **Linux release** - package and validate the complete experience on Linux

## Post-MVP: Typeless-style AI features

- [ ] 23. **Voice text editing** - edit selected text using spoken instructions
- [ ] 24. **Rewrite controls** - shorten, expand, polish, simplify, and rewrite text
- [ ] 25. **Tone controls** - change text between casual, professional, formal, friendly, concise, and custom styles
- [ ] 26. **Voice formatting commands** - turn existing text into numbered lists, bullet lists, headings, checklists, tables, and other structures
- [ ] 27. **Translation** - translate dictated or selected text while preserving intended meaning, tone, and structure
- [ ] 28. **Ask Anything** - provide an optional voice-driven general AI assistant mode
- [ ] 29. **Selected-text intelligence** - summarize, explain, analyze, extract action items, compare, and transform selected text
- [ ] 30. **Custom commands** - allow users to define reusable voice commands and workflows
- [ ] 31. **Advanced personalization** - learn and apply user-approved writing preferences
- [ ] 32. **Advanced vocabulary handling** - improve technical terms, names, URLs, paths, numbers, code identifiers, and other structured content
- [ ] 33. **Multilingual dictation** - add French, Spanish, German, automatic language detection, and mixed-language support
- [ ] 34. **Nigerian language support** - investigate and add Nigerian language recognition and cleanup support
- [ ] 35. **Model fallback and routing rules** - automatically select fallback models/providers according to user-defined rules
- [ ] 36. **Local LLM provider** - add local cleanup/AI models as an alternative to BYOK cloud providers
- [ ] 37. **Advanced audio controls** - add mute-other-audio behavior, interaction sounds, audio-device profiles, and related controls
- [ ] 38. **Advanced history** - add transcript search, filtering, favorites, export, and retention controls

## Expansion

- [ ] 39. **Android application** - bring the core dictation experience to Android with an appropriate native inference path
- [ ] 40. **Cross-device settings sync** - optionally synchronize user settings, vocabulary, and profiles
- [ ] 41. **Team/workspace features** - support shared profiles, vocabulary, policies, and configurations
- [ ] 42. **Workflow automation** - connect voice commands to application actions and multi-step workflows
- [ ] 43. **Plugin/integration system** - allow third-party extensions and custom integrations
- [ ] 44. **Advanced AI agent capabilities** - allow controlled voice-driven actions beyond text generation
- [ ] 45. **Additional desktop platforms** - evaluate and support macOS after the Windows/Linux experience is stable
