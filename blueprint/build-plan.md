# Build Plan

## MVP

- [ ] 1. **Desktop application shell** - establish the Vue + TypeScript desktop application and native Tauri integration
- [ ] 2. **Microphone capture** - capture microphone audio reliably with start, stop, and device selection
- [ ] 3. **Parakeet transcription** - run Parakeet locally and return raw transcript text
- [ ] 4. **BYOK provider configuration** - let users add, validate, edit, and remove their own LLM provider credentials
- [ ] 5. **LLM provider abstraction** - provide a common interface for models while supporting native provider adapters and OpenAI-compatible endpoints
- [ ] 6. **LLM response normalization** - normalize text, reasoning, tool calls, usage, and completion state across providers
- [ ] 7. **Thinking/reasoning isolation** - prevent model reasoning output from leaking into the final dictated text
- [ ] 8. **Dictation cleanup** - clean fillers, repetitions, false starts, self-corrections, punctuation, capitalization, and grammar without changing meaning
- [ ] 9. **Text formatting** - convert spoken structure into paragraphs, bullet lists, numbered lists, headings, checklists, and other appropriate formatting
- [ ] 10. **System-wide text insertion** - insert final text into the currently active application
- [ ] 11. **Global dictation controls** - implement global hotkey, push-to-talk, toggle-to-talk, and stop/cancel controls
- [ ] 12. **Dictation settings** - configure microphone, cleanup behavior, style, language, and default model
- [ ] 13. **Personal dictionary** - let users define names, technical terms, acronyms, products, and preferred spellings
- [ ] 14. **Application profiles** - configure different instructions, styles, vocabulary, formatting, and models for different applications
- [ ] 15. **Dictation history** - optionally store, search, copy, and delete previous dictations
- [ ] 16. **Privacy controls** - clearly show what stays local, what is sent to the selected provider, and provide controls for history and data retention
- [ ] 17. **Provider/model routing** - allow different models to be selected for different AI tasks
- [ ] 18. **Error handling and recovery** - provide actionable handling for microphone, Parakeet, provider, authentication, network, timeout, and model errors
- [ ] 19. **Performance and streaming** - optimize the pipeline for low latency and responsive partial/final results
- [ ] 20. **Windows release** - package and validate the complete MVP on Windows
- [ ] 21. **Linux release** - package and validate the complete MVP on Linux

## Post-MVP: Typeless-style AI features

- [ ] 22. **Voice text editing** - edit selected text using spoken instructions
- [ ] 23. **Rewrite controls** - shorten, expand, polish, simplify, and rewrite text
- [ ] 24. **Tone controls** - change text between casual, professional, formal, friendly, concise, and custom styles
- [ ] 25. **Voice formatting commands** - turn existing text into numbered lists, bullet lists, headings, checklists, tables, and other structures
- [ ] 26. **Translation** - translate dictated or selected text while preserving intended meaning, tone, and structure
- [ ] 27. **Ask Anything** - provide an optional voice-driven general AI assistant mode
- [ ] 28. **Selected-text intelligence** - summarize, explain, analyze, extract action items, compare, and transform selected text
- [ ] 29. **Custom commands** - allow users to define reusable voice commands and workflows
- [ ] 30. **Advanced personalization** - learn and apply user-approved writing preferences
- [ ] 31. **Advanced vocabulary handling** - improve technical terms, names, URLs, paths, numbers, code identifiers, and other structured content
- [ ] 32. **Multilingual dictation** - add French, Spanish, German, automatic language detection, and mixed-language support
- [ ] 33. **Nigerian language support** - investigate and add Nigerian language recognition and cleanup support
- [ ] 34. **Model fallback and routing rules** - automatically select fallback models/providers according to user-defined rules
- [ ] 35. **Local LLM provider** - add local cleanup/AI models as an alternative to BYOK cloud providers
- [ ] 36. **Advanced audio controls** - add mute-other-audio behavior, interaction sounds, audio-device profiles, and related controls
- [ ] 37. **Advanced history** - add transcript search, filtering, favorites, export, and retention controls

## Expansion

- [ ] 38. **Android application** - bring the core dictation experience to Android with an appropriate native inference path
- [ ] 39. **Cross-device settings sync** - optionally synchronize user settings, vocabulary, and profiles
- [ ] 40. **Team/workspace features** - support shared profiles, vocabulary, policies, and configurations
- [ ] 41. **Workflow automation** - connect voice commands to application actions and multi-step workflows
- [ ] 42. **Plugin/integration system** - allow third-party extensions and custom integrations
- [ ] 43. **Advanced AI agent capabilities** - allow controlled voice-driven actions beyond text generation
- [ ] 44. **Additional desktop platforms** - evaluate and support macOS after the Windows/Linux experience is stable
