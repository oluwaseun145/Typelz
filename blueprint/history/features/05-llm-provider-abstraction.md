# Current Feature

> **Generated file.** Holds the one feature, fix, or rollback being built right now. Run
> `/feature <number-or-name>` to spec a build-plan feature, or `/fix "<bug>"` for
> an ad-hoc fix. Use `/rollback <completed-feature>` to plan a safe reversal.
> Build one thing at a time; `/complete` archives it under
> `blueprint/history/` and resets this file.

## Feature 5 - LLM Provider Abstraction

**Branch:** `feature/llm-provider-abstraction`

**Status:** verified

### Goal

Extract the hardcoded OpenAI chat completion logic into a `ProviderAdapter` trait
with an OpenAI-compatible implementation, add model listing via the `/v1/models`
endpoint, and expose both through Tauri commands so later providers can plug in
without rewriting the command layer.

### In scope

- `ProviderAdapter` async trait in `src-tauri/src/llm.rs` with `complete` and
  `list_models` methods
- `OpenAIAdapter` struct implementing the trait, wrapping the existing
  `complete_once` and new `list_models_once` logic
- Factory function `adapter_for_provider` that builds the right adapter from
  a `Provider` config
- New `list_provider_models` Tauri command returning model IDs and names
- Refactored `send_chat_completion` to use the adapter trait internally
- TypeScript type `ListModelResponse` in `src/types/llm.ts`
- `useListModels` composable in `src/composables/`
- `capabilities` field populated with `supports_streaming` and
  `supports_system_messages` for OpenAI-compatible providers
- Existing unit tests pass unchanged; new tests for `list_models_once` and
  adapter construction

### Out of scope

- Response normalization (feature 6)
- Anthropic, Google, or other provider adapters
- Streaming SSE support
- Model selection UI beyond what already exists in provider settings
- System prompt construction for dictation cleanup (feature 7)

### Build loop

Config: `stepReview: "feature"`, `checkpointCommits: "disabled"`.
Review one feature-level packet after all steps pass.

### Build steps

- [x] **1. Define ProviderAdapter trait and OpenAIAdapter**
  Add an async trait `ProviderAdapter` to `src-tauri/src/llm.rs` with two
  methods: `complete(&self, messages, temperature, max_tokens) -> Result<ChatCompletionResponse, String>`
  and `list_models(&self) -> Result<Vec<ModelInfo>, String>`.
  Define `ModelInfo { id: String, name: String }` as a simple struct.
  Implement `OpenAIAdapter` struct holding `base_url: String` and
  `api_key: String`. Move the existing `complete_once` body into
  `OpenAIAdapter::complete`, and add `list_models_once` that GETs
  `{base_url}/models` and extracts model `id` and `name` fields from the
  OpenAI list response. Keep the existing `chat_url` helper and add
  `models_url`. The existing `classify_chat_status` and
  `provider_error_detail` helpers remain module-level.
  `adapter_for_provider(base_url, api_key, kind) -> Result<Box<dyn ProviderAdapter>, String>`
  returns `OpenAIAdapter` for `KIND_OPENAI_COMPATIBLE` and errors for
  unknown kinds.
  **Done when:** `cargo check -p app` passes, existing `llm::tests` still
  pass, new test `adapter_for_provider_returns_openai` confirms construction.

- [x] **2. Refactor send_chat_completion to use adapter**
  Update `chat_for_provider` to call `adapter_for_provider` then
  `adapter.complete(...)`. Remove the direct `complete_once` call. The
  Tauri command signature and return type stay identical. No frontend
  changes needed for this step.
  **Done when:** `cargo check -p app` passes, `llm::tests` pass, the
  `send_chat_completion` command still returns the same response shape.

- [x] **3. Add list_provider_models command**
  Add a new `#[tauri::command] pub async fn list_provider_models(...)` that
  resolves the provider and key (same pattern as `send_chat_completion`),
  builds the adapter, calls `adapter.list_models()`, and returns
  `Vec<ModelInfo>`. Register it in `lib.rs`. Add
  `ListModelResponse { id: string, name: string }` to `src/types/llm.ts`.
  Add `useListModels` composable wrapping `invoke('list_provider_models', { providerId })`.
  **Done when:** `cargo check -p app` passes, `npm run build` passes, new
  command appears in `lib.rs` registration.

- [x] **4. Populate provider capabilities**
  When a provider is created or updated via `ProviderService`, populate
  `capabilities` with `{ "supports_streaming": false,
  "supports_system_messages": true }` for `openai_compatible` kind. This
  is a static default, not a live probe. The capabilities are informational
  for now; feature 6 will gate behavior on them.
  **Done when:** new providers show populated capabilities in the UI list,
  existing providers retain their current capabilities value, `cargo check`
  and `npm run build` pass.

- [x] **5. Verify full pipeline**
  Run the Verify command (`npm run build`) to confirm typecheck and Vite
  production build pass. Run `cargo test` to confirm all Rust tests pass.
  Start the dev server and confirm the provider settings UI still shows the
  provider list, add/edit/test buttons work, and the chat test still sends
  a completion.
  **Done when:** `npm run build` succeeds, `cargo test` passes, dev server
  shows provider list with populated capabilities, chat test button works.

### Files / areas

**Modified:**
- `src-tauri/src/llm.rs` - trait definition, OpenAIAdapter, refactored
  command, new list_models command, new tests
- `src-tauri/src/lib.rs` - register `list_provider_models` command
- `src-tauri/src/providers.rs` - populate capabilities on create/update
- `src/types/llm.ts` - add `ListModelResponse`
- `src/composables/useChatCompletion.ts` - no changes needed (adapter is
  transparent)
- `src/composables/` (new file) - `useListModels.ts`

**Created:**
- None (all changes are in existing files)

### Data / contracts

**New Rust types:**
```rust
pub struct ModelInfo {
    pub id: String,
    pub name: String,
}

#[async_trait]
pub trait ProviderAdapter: Send + Sync {
    async fn complete(
        &self,
        messages: Vec<ChatCompletionMessage>,
        temperature: Option<f64>,
        max_tokens: Option<u32>,
    ) -> Result<ChatCompletionResponse, String>;

    async fn list_models(&self) -> Result<Vec<ModelInfo>, String>;
}
```

**New Tauri command:**
```
list_provider_models(app, credentials, provider_id: String) -> Result<Vec<ModelInfo>, String>
```

**New TypeScript type:**
```typescript
export interface ListModelResponse {
  id: string
  name: string
}
```

**Capabilities default for openai_compatible:**
```json
{ "supports_streaming": false, "supports_system_messages": true }
```

### Testing

- Rust: existing `llm::tests` must pass unchanged
- Rust: new unit test for `list_models_once` with mock JSON response
- Rust: new unit test for `adapter_for_provider` returning correct adapter
  type for `openai_compatible` and error for unknown kind
- TypeScript: `npm run build` passes (vue-tsc typecheck + Vite build)
- Manual: dev server provider list shows capabilities, chat test works

### Notes for the AI

- The `async_trait` crate is not in Cargo.toml. Use the nightly-compatible
  `-> impl Future` syntax or manually implement via a non-async trait with
  `spawn_blocking`. The simplest v1 path: keep the trait methods non-async
  (they already run on `spawn_blocking` workers) and let the Tauri command
  handle the threading. This avoids adding a dependency.
- `reqwest::blocking` is already used; the adapter methods should be
  blocking and called from `spawn_blocking`.
- The `models_url` helper should handle trailing slashes the same way
  `chat_url` does.
- The OpenAI `/v1/models` response shape is `{ "data": [{ "id": "gpt-4o",
  "object": "model", "owned_by": "openai", ... }] }`. Extract `id` and use
  `id` as `name` (most providers don't return a human-readable name).

### Open questions

None. The trait shape follows directly from the existing `complete_once`
function and the OpenAI `/v1/models` endpoint.
