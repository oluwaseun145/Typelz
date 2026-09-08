# Feature: LLM Response Normalization

**From build-plan:** feature 6
**Status:** verified
**Branch:** `feature/llm-response-normalization`

## Goal

Establish a single canonical LLM response shape where `text` and `reasoning` are always separate channels, and `tool_calls`, `usage`, and `finish_reason` are normalized so every downstream consumer (cleanup in features 7-8, future streaming 20, thinking-model support 21) can rely on one stable contract regardless of provider quirks.

## In scope

- Extend Rust OpenAI wire types in `src-tauri/src/llm.rs` to tolerate provider-variant fields without breaking existing deserialization (`reasoning`, `reasoning_content`, `reasoning_details`, `tool_calls`, nullable/alt-cased `finish_reason`, extra choice/message keys like `index`/`logprobs`).
- Define normalized Rust types: `NormalizedResponse`, `NormalizedToolCall`, `NormalizedUsage`, `FinishReason` with `#[derive(Serialize)]` for Tauri.
- Implement pure `fn normalize_response(raw: ChatCompletionResponse) -> NormalizedResponse` (and `normalize_tool_calls` helper) that extracts `text`, `reasoning`, `tool_calls`, normalized `usage`, and canonical `finish_reason` from the raw response; never panics, never logs key material.
- Return the normalized shape from the `send_chat_completion` Tauri command (single canonical serialize shape) so the frontend no longer depends on raw provider fields.
- Define matching TypeScript normalized types in `src/types/llm.ts` (`NormalizedLlmResponse`, `NormalizedToolCall`, `NormalizedUsage`, `FinishReason`) and a pure TS normalizer in `src/lib/normalizeLlmResponse.ts` that mirrors the Rust logic for testability and non-Tauri callers. Keep raw wire types as internal `Raw*` where still needed for parsing.
- Update `src/composables/useChatCompletion.ts` to invoke the command as `NormalizedLlmResponse` and expose `NormalizedLlmResponse | null`.
- Update `src/components/settings/ProvidersSection.vue` chat test (`runChatTest`) to render `response.text` and, when present, `response.reasoning` in a separate channel (text first, reasoning second with distinct label), instead of reaching into `choices[0].message.content`.
- Preserve `finish_reason` and `usage` handling for empty-choice, null-content, missing-usage, and unknown-status providers without inventing provider billing semantics.

## Out of scope

- Streaming / SSE normalization (feature 20)
- Thinking-model UI or special prompting (feature 21) beyond the channel separation
- Dictation cleanup prompts or formatting rules (features 7, 8)
- Anthropic / Google native wire formats (still OpenAI-compatible adapter only; this feature normalizes variance *within* that family)
- Tool-call execution, retries, or persistence
- Changing `list_provider_models` shape or provider `capabilities` population (feature 5)

## Build loop

Build one small step at a time. Follow `workflow.stepReview` in `blueprint/config.json`: `feature` produces one review packet after all steps, while `every` pauses for review after each step. Offer checkpoint commits only when `workflow.checkpointCommits` is enabled. `/complete` makes the final feature commit. Never accept a review packet you have not read; split any diff that is too large to review.

Config: `stepReview: "feature"`, `checkpointCommits: "disabled"`.
One feature-level review packet after all steps pass; `/complete` creates the single feature commit.

## Build steps

Small, reviewable units. Each ends with something working. `/implement` checks these off as it finishes them, so progress survives a context clear: a fresh session reads which boxes are ticked and resumes from the first unchecked step.

- [x] **Step 1 - Widen Rust wire types to tolerate variant fields** - Add optional `reasoning`/`reasoning_content`/`reasoning_details` and `tool_calls` fields to `ChatCompletionMessage`, allow `finish_reason` to be `Option<String>` with case-insensitive handling, use `#[serde(default)]` and `flatten`/extra-field tolerance so existing fixtures (`FULL_RESPONSE`, `MINIMAL_RESPONSE`) still deserialize. Add `RawToolCall` / `RawFunction` structs with default ders. Do not change the command return shape yet. *Done when:* `cargo test llm::tests` still pass; new test deserializes a fixture containing `reasoning_content` and `tool_calls` and a fixture with `finish_reason: null` without error.

- [x] **Step 2 - Define normalized Rust types and pure normalize function** - Add `NormalizedResponse { id, model, text, reasoning: Option<String>, tool_calls: Vec<NormalizedToolCall>, usage: NormalizedUsage, finish_reason: FinishReason }`, `NormalizedToolCall { id, name, arguments: String }`, `NormalizedUsage { prompt_tokens: u32, completion_tokens: u32, total_tokens: u32 }`, `enum FinishReason { Stop, Length, ToolCalls, ContentFilter, Error, Unknown(String) }`. Implement `normalize_response` that: picks `choices[0]` (or empty fallback producing `text=""`, `finish_reason=Unknown`), extracts `text` from `content` (coerce null -> ""), extracts `reasoning` from first non-empty of `reasoning_content` / `reasoning` / array-joined fallback (trim, None if empty), maps each `tool_calls[].function` to normalized (default id/name/arguments to ""), normalizes `finish_reason` lowercased (`stop`->Stop, `length`->Length, `tool_calls`->ToolCalls, `content_filter`/ `content-filter`->ContentFilter, null/""->Unknown, anything else ->Unknown(original)), copies usage values (u32, total = provider total or prompt+completion if total==0 as defensive, but preserve provider total when non-zero). Add unit tests for happy, reasoning-present, tool-calls-present, empty-choices, null-content, unknown-finish. *Done when:* `cargo test` passes with new tests; `cargo check` clean; normalization never panics on missing fields.

- [x] **Step 3 - Define normalized TypeScript types and pure TS normalizer** - In `src/types/llm.ts` add `NormalizedLlmResponse`, `NormalizedToolCall`, `NormalizedUsage`, `FinishReason` (`"stop" | "length" | "tool_calls" | "content_filter" | "error" | string` narrowing) and keep raw types as `RawChatCompletionResponse` if still needed internally. Create `src/lib/normalizeLlmResponse.ts` exporting `normalizeLlmResponse(raw: unknown): NormalizedLlmResponse` and `parseFinishReason(v: unknown): FinishReason` with same field precedence as Rust (content -> text, reasoning_content/reasoning -> reasoning, tool_calls -> normalized, finish_reason mapping, usage normalization). Handle `content` possibly null/not-string, `tool_calls` possibly not array. No component logic. *Done when:* `npm run build` passes (`vue-tsc` + Vite); manual `node -e` import of the module can normalize the two Rust fixtures and matches Rust expectations.

- [x] **Step 4 - Switch Tauri command to normalized return and update consumers** - Change `send_chat_completion` return type to `NormalizedResponse` (serialize with `snake_case` already via `#[serde(rename_all)]` but TS expects camelCase? Keep TS adapter: Tauri serializes snake_case; TS type uses snake_case matching Rust or add serde rename - decide to keep snake_case end-to-end for v1 and document). Update `useChatCompletion.ts` to `invoke<NormalizedLlmResponse>('send_chat_completion', ...)` returning `NormalizedLlmResponse | null`. Update `ProvidersSection.vue` `runChatTest` to set `chatResult` from `response.text` and if `response.reasoning` present append `"Reasoning: " + reasoning` on a second line (or two separate refs); handle `finish_reason` not displayed but preserved. Adjust `chatResult` type from `Record<string,string>` to reflect multi-channel if needed. *Done when:* `cargo check` and `npm run build` pass; dev server `npm run dev` -> Settings > Providers > Test Chat shows text channel content, and with a fixture containing reasoning shows both channels separated (verified via console or UI).

- [x] **Step 5 - Verify full pipeline and lock contract** - Run `cargo test`, `npm run build`, spot-check dev server provider list / chat still works, reasoning-fixture and tool-calls-fixture return correct normalized shapes via `normalizeLlmResponse` unit sanity. Document contract in `src/types/llm.ts` doc comments: reasoning is `string | null` (null means provider did not emit), text is always string ("" if empty), tool_calls always array, usage always present, finish_reason always canonical. *Done when:* `cargo test` green, `npm run build` green, no `any` types, `No findings recorded` ledger unchanged, and branch diff is reviewable.

## Files / areas

- `src-tauri/src/llm.rs` - wire-type widening, normalized types, normalize function, tests, command return type
- `src/types/llm.ts` - raw type retention + new normalized types
- `src/lib/normalizeLlmResponse.ts` - new pure TS normalizer (canonical logic, no side effects)
- `src/composables/useChatCompletion.ts` - typed invoke to normalized shape
- `src/components/settings/ProvidersSection.vue` - chat test renders `text` + separate `reasoning` channel
- `blueprint/context/current-feature.md` - this spec

No changes to `src/composables/useProviders.ts`, `src/composables/useListModels.ts`, `src/lib/provider-validation.ts`, `src-tauri/src/providers.rs` (beyond what feature 5 already populates).

## Data / contracts

**Rust normalized (Tauri-serialized, snake_case):**
```rust
pub struct NormalizedResponse {
    pub id: String,            // provider id, "" if missing
    pub model: String,         // provider model, "" if missing
    pub text: String,          // always present, "" if no choices or null content; trimmed, never null
    pub reasoning: Option<String>, // None if provider emitted no reasoning field or it was empty/whitespace-only; Some(trimmed) otherwise
    pub tool_calls: Vec<NormalizedToolCall>, // always array, empty if none
    pub usage: NormalizedUsage,
    pub finish_reason: FinishReason, // canonical enum serialized as snake_case string, Unknown carries original
}
pub struct NormalizedToolCall { pub id: String, pub name: String, pub arguments: String } // arguments is JSON string, "" if missing
pub struct NormalizedUsage { pub prompt_tokens: u32, pub completion_tokens: u32, pub total_tokens: u32 } // 0 defaults if provider omitted individual counters; total == provider total when non-zero else prompt+completion
pub enum FinishReason { Stop, Length, ToolCalls, ContentFilter, Error, Unknown(String) } // Unknown preserves lowercased original
```

**TypeScript normalized (mirrors Rust, snake_case to match Tauri serialization for v1):**
```ts
export interface NormalizedLlmResponse {
  id: string; model: string;
  text: string; reasoning: string | null;
  tool_calls: NormalizedToolCall[];
  usage: NormalizedUsage;
  finish_reason: FinishReason;
}
export interface NormalizedToolCall { id: string; name: string; arguments: string; }
export interface NormalizedUsage { prompt_tokens: number; completion_tokens: number; total_tokens: number; }
export type FinishReason = "stop" | "length" | "tool_calls" | "content_filter" | "error" | string;
```

**Normalization rules (authoritative):**
- Source: always `choices[0]` per OpenAI spec; if `choices` empty or missing, produce `text=""`, `reasoning=null`, `tool_calls=[]`, `finish_reason="unknown"` with empty id/model pass-through.
- `text`: `choices[0].message.content` coerced: null/undefined/non-string -> ""; string -> trimmed but preserve internal whitespace; never carry reasoning.
- `reasoning`: first non-empty among `message.reasoning_content` (string), `message.reasoning` (string or string[] joined with "\n"), `message.reasoning_details[].text` joined; trim; "" -> None/null; never mixed into `text`.
- `tool_calls`: `message.tool_calls` if array, else `[]`; each entry maps `id` (string, "" fallback), `function.name` ("" fallback), `function.arguments` (if object, JSON.stringify; if string, pass; missing -> ""); drop entries that are not objects; preserve order.
- `finish_reason`: `choices[0].finish_reason` lowercased, trim; "stop"->stop, "length"->length, "tool_calls"/"tool-calls"->tool_calls, "content_filter"/"content-filter"->content_filter, null/"" -> "unknown", other -> lowercased original (preserved verbatim lowercased in Rust Unknown).
- `usage`: provider fields `prompt_tokens`, `completion_tokens`, `total_tokens` each u32, default 0 if missing or non-numeric; if `total_tokens == 0` and sum >0, total = prompt+completion (defensive only; do not recompute when provider supplied non-zero total).
- Serialization: Rust `#[serde(rename_all = "snake_case")]` on all normalized structs; TS consumes same keys; no camelCase mapping in v1 to avoid silent mismatches. JS boundary never sees raw API key.
- Error shape unchanged: `send_chat_completion` still `Result<NormalizedResponse, String>` with actionable `classify_chat_status` strings; no secret leakage.

## Testing

- No `test` or `Verify` script is declared in `AGENTS.md`/`package.json`; testing is currently opt-in per `coding-standards.md`. This feature does not silently install a runner.
- Until `/tests` is run: rely on `cargo test` (Rust) + `npm run build` (`vue-tsc -b` + Vite) as gates, plus dev-server manual check via Settings > Providers > Chat.
- In-scope pure logic that must be covered when a test runner is added (Vitest via `/tests`): `normalizeLlmResponse` in `src/lib/normalizeLlmResponse.ts` and `normalize_response` in `src-tauri/src/llm.rs` — cases: happy OpenAI, missing choices, null content, reasoning_content present, tool_calls with object/string arguments, empty tool_calls, unknown finish_reason, missing usage, content_filter. Tests live next to source (`normalizeLlmResponse.test.ts`) and run via the project's `test` command once configured; empty suite must fail.
- Browser harness not configured (`Browser tests` not declared); do not add Playwright mid-feature.

## Notes for the AI

- `reqwest::blocking` + `spawn_blocking` threading stays as in feature 5; normalized path does not introduce `async_trait`.
- Keep raw wire types private to `llm.rs` parsing step; only `NormalizedResponse` crosses the Tauri boundary after this feature. Add `#[serde(default)]` generously so new provider fields never break deserialization.
- Vue SFCs use `<script setup>` + Composition API; `src/composables/useChatCompletion.ts` is the only caller of `send_chat_completion`.
- Do not add a streaming field or a new Tauri command; mutate the existing command's return shape (this is a breaking but intentional API change locked by this feature).
- Keep functions under ~50 lines, no `any`, `import type` for types, no commented-out code.
- No em dashes in docs/comments.

## Open questions

- None blocking. The spec chooses `reasoning: string | null` and snake_case serialization for v1 to keep the channel separation explicit and the Tauri boundary trivial. If a future provider requires structured `reasoning_details[]` beyond a string, that will be an additive field on `NormalizedResponse` (feature 21), not a break.
