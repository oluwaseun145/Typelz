# Feature: Dictation cleanup

**From build-plan:** feature 7
**Status:** verified
**Branch:** `feature/dictation-cleanup`

## Goal

Build-plan item 7: given the raw Parakeet transcript, produce cleaned text via the user's selected BYOK LLM provider, removing fillers, repetitions, false starts, and self-corrections and adding punctuation, capitalization, and grammar fixes **without changing meaning**. The stage must never lose the dictation: any cleanup failure falls back to the raw transcript with a visible indication.

## In scope

- A cleanup pipeline stage in the frontend webview that sends only the raw
  transcript text to a stored provider through the existing
  `send_chat_completion` Tauri command (Rust layer unchanged).
- `DictationSettings` type matching the overview data model, plus temporary
  defaults used until feature 11 persists user choices. Nothing is stored.
- A pure, deterministic prompt builder whose system message encodes the
  cleanup contract and whose per-flag clauses toggle with the settings booleans.
- A pure result evaluator that decides cleaned vs raw fallback with a stable
  `CleanupResult` shape, including failure reasons.
- A `useDictationCleanup` composable wiring the builder, the evaluator, and
  `useChatCompletion` with an explicit state machine.
- A "Dictation Cleanup" section in `App.vue` (same visual pattern as the
  Transcribe section) so the running Tauri app can exercise the stage end to
  end: provider select, Clean button, cleaned output, fallback indication,
  error display.
- Raw-transcript fallback on every failure path: empty response, truncated
  response (`finish_reason` `length`), content-filtered response, provider/
  transport errors (the actionable message `llm.rs` already produces), and
  missing provider. Dictation text is never dropped.

## Out of scope

- Text formatting conversion into lists, headings, checklists, etc. (item 8,
  uses `formatting_rules`).
- Text insertion into the active application (item 9), global hotkeys (item 10),
  dictation settings persistence and UI (item 11), voice bar (item 12), the
  global error-handling system (item 13).
- Streaming, chunking of long transcripts, or latency work (item 20).
- Personal dictionary and application profiles (items 15-16).
- Any Rust/Tauri backend change, any new dependency, any persistence.
- Automatically triggering cleanup after `transcribe_stop`; the future
  orchestrator (insertion/hotkey features) owns that trigger.

## Build loop

Build one small step at a time. Follow `workflow.stepReview` in
`blueprint/config.json`: `feature` produces one review packet after all steps.
`checkpointCommits` is `disabled`, so no checkpoint commits between steps.
`/complete` makes the final feature commit. Every step must leave
`npm run build` green.

Config: `stepReview: "feature"`, `checkpointCommits: "disabled"`. One
feature-level review packet after all steps pass; `/complete` creates the
single feature commit.

## Build steps

Small, reviewable units. Each ends with something working. `/implement` checks
these off as it finishes them, so progress survives a context clear: a fresh
session reads which boxes are ticked and resumes from the first unchecked step.

- [x] **Step 1 - Settings types and defaults** - Add `src/types/dictation.ts` with `DictationSettings` (`cleanup_enabled`, `filler_removal`, `repetition_removal`, `self_correction_handling`, `punctuation_enabled`, `capitalization_enabled`, `formatting_rules: Record<string, unknown>`), `CleanupState`, and `CleanupResult` (see Data / contracts), and export `DEFAULT_DICTATION_SETTINGS` (all booleans `true`, `formatting_rules: {}`) from `src/lib/buildCleanupPrompt.ts`. *Done when:* `npm run build` passes and `DictationSettings` matches the overview data model field for field, including `formatting_rules`.

- [x] **Step 2 - Cleanup prompt builder** - Add `buildCleanupMessages(rawTranscript: string, settings: DictationSettings): ChatCompletionMessage[]` to `src/lib/buildCleanupPrompt.ts`, returning exactly `[system, user]` with `user.content` = trimmed transcript. System message carries the always-on clauses (role: transcription cleanup engine, not a chat assistant; output: only cleaned text, no preamble/quotes/markdown/explanations; meaning: never add, remove, answer, translate, or change language, keep names and terms as spoken; grammar: fix grammar and spelling without changing meaning; structure: keep paragraph structure, add no lists/headings/numbering, item 8 owns formatting) plus the five conditional clauses present if and only if the matching flag is `true` (filler removal with example list such as um, uh, you know, like; repetition removal; false-start/self-correction handling: drop the abandoned attempt, keep the corrected form; standard punctuation; sentence capitalization). Pure and deterministic: no timestamps, no randomness. *Done when:* `npm run build` passes; with every conditional flag `false` none of the five conditional clauses appear while the always-on clauses do; identical input yields identical messages.

- [x] **Step 3 - Result evaluator** - Add `evaluateCleanupResult(response: NormalizedLlmResponse, rawTranscript: string): CleanupResult` to a new `src/lib/evaluateCleanupResponse.ts` implementing the decision table in Data / contracts: cleaned only for non-empty trimmed text with `finish_reason` not `length` or `content_filter`; raw fallback with reasons `empty-response`, `truncated`, `content-filter`; the `reasoning` channel never appears in the returned `text`. *Done when:* `npm run build` passes and the table holds for non-empty text with `finish_reason` `stop`, empty text, `length`, and `content_filter`.

- [x] **Step 4 - Cleanup composable** - Add `src/composables/useDictationCleanup.ts` exposing `state` (`'idle' | 'cleaning' | 'done'`), `rawTranscript`, `cleanedText`, `isFallback`, `fallbackReason`, `error`, plus `cleanTranscript(providerId: string, rawTranscript: string, settings?: DictationSettings)` and `reset()`. No invoke for `cleanup_enabled` false, whitespace-only input, or missing provider id (finishes with `no-provider`); requests use `temperature: 0` and omit `max_tokens`; provider failure sets `error` to the invoke message and finishes with `provider-error`; `cleanedText` always holds the final text (cleaned or raw) when `state` is `'done'`. *Done when:* `npm run build` passes and no invoke path exists for the three skip cases.

- [x] **Step 5 - Cleanup section in the main window** - Add a "Dictation Cleanup" section to `src/App.vue` following the existing section pattern (max-width 480px block, same button/indicator/error CSS classes): provider `<select>` populated from `useProviders()` (refreshed on mount), a Clean button disabled while `state === 'cleaning'`, when there is no non-empty `lastTranscript`, or when no provider is selected, a cleaned-text output box, a visible indication that the raw transcript is shown because cleanup failed when `isFallback` is true, and the error display. Render transcript and cleaned text only through Vue `{{ }}` text interpolation, never `v-html`. *Done when:* `npm run build` passes, and in the running Tauri app (`npx tauri dev`, `@tauri-apps/cli` is already a devDependency): after recording a transcript with the existing Transcribe controls, selecting a stored provider and clicking Clean shows cleaned text; a failing provider (for example one whose base URL is unreachable) shows the raw transcript with the fallback indication and the actionable error message; no console errors appear in any of these paths.

## Files / areas

New:
- `src/types/dictation.ts` - `DictationSettings`, `CleanupState`,
  `CleanupResult`.
- `src/lib/buildCleanupPrompt.ts` - `DEFAULT_DICTATION_SETTINGS`,
  `buildCleanupMessages`.
- `src/lib/evaluateCleanupResponse.ts` - `evaluateCleanupResult`.
- `src/composables/useDictationCleanup.ts` - state machine over
  `useChatCompletion`.

Modified:
- `src/App.vue` - new Dictation Cleanup section beside the Transcribe section.

Consumed, unchanged (evidence from repository inspection):
- `src-tauri/src/llm.rs` - `send_chat_completion` command: resolves the API key
  from the OS credential store, forces `stream: false`, 30 s timeout, and
  returns `NormalizedResponse`; `classify_chat_status` produces actionable
  auth/rate-limit/timeout/HTTP errors.
- `src/composables/useChatCompletion.ts` - `sendCompletion(providerId,
  messages, options)` returns `NormalizedLlmResponse | null` and sets its own
  `error`.
- `src/composables/useTranscription.ts` - source of `lastTranscript`.
- `src/composables/useProviders.ts` + `src/types/provider.ts` - provider list
  for the select.
- `src/types/llm.ts` - `ChatCompletionMessage`, `NormalizedLlmResponse`,
  `FinishReason`.

## Data / contracts

`DictationSettings` (exact fields from the overview data model; consumed by
items 4-8 per the lock note):
- `cleanup_enabled: boolean`
- `filler_removal: boolean`
- `repetition_removal: boolean`
- `self_correction_handling: boolean`
- `punctuation_enabled: boolean`
- `capitalization_enabled: boolean`
- `formatting_rules: object` (typed `Record<string, unknown>`; accepted but
  unused in this feature, owned by item 8)

Temporary defaults until item 11 persists user choices (in-memory only, never
stored): all booleans `true`, `formatting_rules: {}`.

`CleanupState = 'idle' | 'cleaning' | 'done'`.

`CleanupResult`:
- `text: string` - final text, always non-null; the raw transcript on fallback.
- `source: 'cleaned' | 'raw'`
- `reason: 'empty-response' | 'truncated' | 'content-filter' |
  'provider-error' | 'no-provider' | null`

Evaluator decision table (input: `NormalizedLlmResponse` from the existing
channel, plus the raw transcript):
| Response condition | Result |
|---|---|
| `text` (trimmed) non-empty and `finish_reason` is not `length` or `content_filter` | `source 'cleaned'`, `text` = `response.text` trimmed, `reason` `null` |
| `text` (trimmed) empty | `source 'raw'`, `reason 'empty-response'` |
| `finish_reason` `length` | `source 'raw'`, `reason 'truncated'` |
| `finish_reason` `content_filter` | `source 'raw'`, `reason 'content-filter'` |
| invoke rejects (auth, network, timeout, missing key, provider not found) | `source 'raw'`, `reason 'provider-error'`, `error` = the invoke message from `llm.rs` |
| provider id missing/empty | `source 'raw'`, `reason 'no-provider'`, no API call |
| input transcript whitespace-only | `source 'raw'`, `reason` `null`, no API call |

The `reasoning` channel is never merged into the cleaned text (channel
separation established in item 6).

Request contract per cleanup call:
- `messages`: exactly the builder output, `[system, user]`, where
  `user.content` is the trimmed raw transcript.
- `temperature`: `0`.
- `max_tokens`: omitted (provider default applies; truncation is covered by
  the `length` fallback rule).
- Streaming: off (enforced by the Rust command).

Privacy/transport contract: the transcript text is the only user content sent,
and the selected provider endpoint is the only recipient. The API key is
resolved in Rust from the OS credential store and never crosses the JS
boundary, appears in the request payload, prompt, or logs. No other network
calls are added by this feature.

## Testing

No test runner is installed (testing is opt-in via `/tests`); there is no test
command to run and no browser test command. `buildCleanupMessages` and
`evaluateCleanupResult` are pure and deterministic by design, so they are test
seams: when `/tests` lands, `*.test.ts` files land next to them in
`src/lib/` covering flag toggling, determinism, and the full decision table.
Verification for this feature is `npm run build` (vue-tsc typecheck plus Vite
build) per step, plus the running-app walk-through named in Step 5, whose
evidence is captured by `/check` when that gate runs.

## Notes for the AI

- Follow the existing `App.vue` section pattern: same structural markup,
  `state-indicator`, `error-display`, and button classes; scoped CSS additions
  only for the new section.
- No new npm dependencies, no Rust changes, no Tauri command additions.
- Do not persist anything: no settings storage, no history, no cache of
  cleaned results, no transcript logging.
- Do not add chunking, retries, or queueing for long transcripts; item 20 owns
  performance behavior.
- Do not auto-run cleanup on `transcribe_stop`; the trigger belongs to the
  insertion/hotkey orchestrator (items 9-10).
- Render user content (raw transcript, cleaned text) only via Vue `{{ }}`
  interpolation; never `v-html` or raw HTML injection.
- `import type` for type-only imports; no `any`; functions under 50 lines;
  no comments except non-obvious why.
- No em dashes in generated content, per coding standards.
- The failing-provider scenario in Step 5 must be produced without deleting
  the user's real provider configuration (for example, temporarily add a
  throwaway provider with an unreachable base URL and remove it after).
- Baseline: `npm run build` passed at spec time (122 modules, vue-tsc clean).
