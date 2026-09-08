# Current Feature

**Feature:** 4. BYOK provider configuration
**Branch:** feature/byok-provider-configuration
**Status:** verified

## Goal

Let the user add, validate, edit, and remove their own LLM provider
credentials from the settings window: OpenAI-compatible providers in v1, with
the raw API key stored only in the OS keychain and provider metadata (name,
base URL, model, capabilities, enabled) in a local JSON store. Items 5-6
(provider abstraction, response normalization) consume the `Provider` metadata
and keychain reference this feature defines.

## In scope

- Local JSON provider store in the Tauri config directory. This settles the
  overview TODO "local app-data store format ... settle at the first
  persisting feature (item 4)" with a single versioned JSON document.
- Raw API keys in the OS credential store via the `keyring` crate, one entry
  per provider, never in the JSON file, logs, events, or frontend persistence.
- Rust Tauri commands: `list_providers`, `add_provider`, `update_provider`,
  `remove_provider`, `validate_provider`, `test_provider_credentials`, with
  Rust-side re-validation of all input.
- UI input validation with Zod (settles the coding-standards validation TODO).
- "LLM Providers" section in the existing settings window: list, add/edit
  form, test-connection actions, remove with confirmation, actionable errors.

## Out of scope

- Choosing a default/active provider or model (item 11); listing or
  selecting models (item 5); making LLM calls (items 5-8).
- Provider kinds besides `openai_compatible`; native OpenAI/Anthropic/Google
  adapters (later).
- User-editable `capabilities` values: no capability flag is defined in the
  plans yet (item 21 consumes them). The field is persisted as an opaque
  object defaulting to `{}` and is not exposed in the v1 UI.
- Linux keychain wiring now (the `keyring` crate already targets it; v1 runs
  Windows only, item 22).
- Sharing, sync, or history of provider configurations.

## Build loop

Per `blueprint/config.json`: `stepReview: feature`, `checkpointCommits:
disabled` - implement all steps, then present one review packet before
approval. No commits until `/complete` creates the feature commit. Work on
branch `feature/byok-provider-configuration`.

## Build steps

- [x] **1. Rust: provider store + credential seam.** New
  `src-tauri/src/providers.rs`: `Provider` metadata struct (serde,
  snake_case on the wire), `AppDataStore` reading/writing
  `typelz.json` (`{"schema_version": 1, "providers": []}`) under the Tauri
  config dir with atomic writes (temp file + rename), and a
  `CredentialStore` trait (`set` / `get` / `delete`) with a
  `KeyringCredentialStore` backend (`keyring` crate, service
  `com.typelz.app`, entry name = provider id) plus an in-memory backend for
  tests. Add `keyring = "3"` and `uuid = { version = "1", features = ["v4"] }`
  to dependencies, `tempfile = "3"` to dev-dependencies.
  **Done when:** `cargo check` and `cargo test` pass in `src-tauri/`; module
  tests cover JSON round-trip, redaction (no key material in serialized
  output or summaries), and update-without-key semantics via the in-memory
  seam. No commands registered yet.
- [x] **2. Rust: CRUD + validation commands.** In `providers.rs` (wired from
  `lib.rs`): `list_providers`, `add_provider(name, base_url, model, api_key)`
  (uuid v4 id, `kind: "openai_compatible"`, `capabilities: {}`, `enabled:
  true`), `update_provider(id, {name?, base_url?, model?, enabled?,
  api_key?})` (PATCH semantics; non-empty `api_key` rotates the keychain
  entry, missing/empty keeps it), `remove_provider(id)` (keychain entry
  first, then metadata; keychain failure aborts with the provider intact),
  `validate_provider(id)` (stored metadata + stored key; errors if either is
  missing), `test_provider_credentials({base_url, api_key})` (draft values,
  persists nothing). Validation = `GET {base_url}/models` with
  `Authorization: Bearer <key>`, 10 s timeout via the existing `reqwest`
  blocking client; map 200 to ok, 401/403 to "API key rejected", 404 to a
  message reminding that the base URL is the API root (e.g.
  `https://api.openai.com/v1`), other statuses and DNS/timeout failures to
  actionable network messages. Rust-side validation: non-empty name and
  model, absolute http(s) URL for base_url. Follow the existing
  `Result<T, String>` command convention.
  **Done when:** `cargo check` and `cargo test` pass; all six commands
  registered in `invoke_handler`; unit tests cover URL rules, status-to
  message mapping (pure function), and add/update/remove ordering through
  the in-memory seam.
- [x] **3. Frontend contracts.** New `src/types/provider.ts`
  (`ProviderSummary`, command payload types, `ProviderValidationResult`),
  `src/lib/provider-validation.ts` (Zod schema for name, base URL, model,
  API key; field-error helper), and `src/composables/useProviders.ts`
  (invoke wrappers following the `useMicrophone.ts` pattern, error
  normalization). `npm install zod`; record the Zod decision in
  `blueprint/context/coding-standards.md` under Validation, replacing the
  TODO. **Done when:** `npm run build` passes.
- [x] **4. Settings UI: list + add.** Replace the stub body of
  `SettingsWindow.vue` with the providers section; new
  `src/components/settings/ProvidersSection.vue` (plain scoped CSS, existing
  token variables): list in insertion order (name, kind label, base URL,
  model, enabled state, stored-key indicator - never the key), empty state
  ("No providers configured" + Add action), add form (name, base URL, model,
  password-type API key) with per-field Zod errors, a Test Connection button
  (calls `test_provider_credentials` with draft values before saving; keeps
  the form; disables while in flight), and Save (calls `add_provider`,
  refreshes the list, resets the form). **Done when:** `npm run build`
  passes and in `npx tauri dev` the settings section shows the empty state,
  flags an invalid URL and an empty name field on submit, and a saved
  provider appears in the list with its key never rendered.
- [x] **5. Settings UI: edit, remove, validate saved.** Edit opens the form
  pre-filled with key blank plus a "leave blank to keep the current key"
  hint: Test uses `validate_provider(id)` when the key field is blank and
  `test_provider_credentials` when a new key is typed; Save calls
  `update_provider`. Delete asks for explicit confirmation, calls
  `remove_provider`, and refreshes. **Done when:** `npm run build` passes
  and in `npx tauri dev`: an edit persists across a window reopen, a blank
  key field keeps the stored key (a follow-up validation still succeeds), a
  confirmed delete removes the provider and its keychain entry (no Typelz
  entry left in `cmdkey /list`), and unknown-id or keychain failures show an
  actionable error line.

## Files / areas

- `src-tauri/src/providers.rs` (new) - metadata types, `AppDataStore`,
  `CredentialStore` seam + `KeyringCredentialStore`, commands, validation
  logic
- `src-tauri/src/lib.rs` - module declaration, handler registration (no
  changes to existing capture/transcription commands)
- `src-tauri/Cargo.toml` - `keyring`, `uuid`, dev `tempfile`
- `src/types/provider.ts` (new)
- `src/lib/provider-validation.ts` (new)
- `src/composables/useProviders.ts` (new)
- `src/components/settings/ProvidersSection.vue` (new)
- `src/components/settings/SettingsWindow.vue` - hosts the new section
- `blueprint/context/coding-standards.md` - record the validation-lib choice

## Data / contracts

**App data file** `<Tauri config dir>/typelz.json`:

```json
{ "schema_version": 1, "providers": [] }
```

Provider object (single source of truth for items 5-6; snake_case on the
wire, matching existing payloads such as `downloaded_bytes`):

| field | type | rule |
| --- | --- | --- |
| `id` | string | uuid v4, minted in Rust on add |
| `name` | string | required, trimmed, non-empty |
| `kind` | string | `"openai_compatible"` (only v1 value) |
| `base_url` | string | required; absolute http(s) URL; trimmed; stored as given |
| `model` | string | required, non-empty |
| `capabilities` | object | opaque, default `{}`; not UI-editable in v1 |
| `enabled` | bool | default `true` on add |

**Keychain:** service `com.typelz.app`, entry name = provider `id`, value =
raw key (UTF-8). Referenced per provider; never duplicated into app data.

**Commands** (Rust -> webview, `Result<T, String>` style):

| command | args | returns |
| --- | --- | --- |
| `list_providers` | - | `ProviderSummary[]` (insertion order) |
| `add_provider` | `{ name, base_url, model, api_key }` | `ProviderSummary` |
| `update_provider` | `{ id, name?, base_url?, model?, enabled?, api_key? }` | `ProviderSummary` |
| `remove_provider` | `id` | - |
| `validate_provider` | `id` | `ProviderValidationResult` |
| `test_provider_credentials` | `{ base_url, api_key }` | `ProviderValidationResult` |

`ProviderSummary = { id, name, kind, base_url, model, capabilities, enabled,
has_key: boolean }` - `has_key` reflects keychain state; no command ever
returns the raw key.
`ProviderValidationResult = { ok: boolean, message: string }`.
Errors are actionable strings (unknown id, missing stored key, keychain
unavailable, URL/auth/network failures) and must never contain the key,
authorization header, or raw response bodies beyond a short quoted detail.

**Validation request:** `GET {base_url with one trailing slash}/models`,
`Authorization: Bearer <key>`, 10 s timeout. Status mapping: 200 ok;
401/403 key rejected; 404 base-URL-root hint; other HTTP, DNS, and timeout
failures as network errors. No other network calls; the check happens only
on explicit user action (add/edit test, saved-provider test).

**Frontend validation (Zod):** name min 1 after trim; base URL absolute
http(s); model min 1; api_key min 1 on add (optional on edit). Rust
re-validates every mutation; the UI is not the trust boundary.

## Testing

- No JS test command is declared, so no JS test gate; browser evidence
  comes from the real Tauri window (`npx tauri dev`) per the Browser
  Verification standard.
- Rust unit/integration tests (`cargo test` in `src-tauri/`): JSON
  round-trip and atomic write, redaction of key material from summaries and
  serialized state, PATCH semantics including blank-key-keeps-stored,
  remove ordering (keychain first) through the in-memory `CredentialStore`,
  URL validation cases, and status-to-message mapping.
  The real keyring backend is not unit tested (OS store); its behavior is
  proven in the live checks above.
- Manual keychain check after remove: `cmdkey /list` shows no
  `com.typelz.app` entry for the provider id.
- Final gate before `/complete`: `npm run build` plus `cargo check` and
  `cargo test` green.

## Notes for the AI

- Decisions made in this spec (flag for review, change only by editing the
  spec): single versioned `typelz.json` in the Tauri config dir is the app
  data store (item 11 extends the same document); validation is
  `GET /models`, not a trial chat completion (no token spend per save);
  10 s timeout; `capabilities` persisted but not UI-editable until flags are
  defined (item 21); delete uses an explicit confirmation step; saving does
  not require a passed validation, since add/validate/edit/remove are
  separate operations per the build plan.
- Security: keys exist only in keychain, the in-flight request, or form
  state. Never log a key (tauri-plugin-log is active), never echo it in
  command errors, events, or the JSON file; the password input is
  `type="password"`, and provider names/URLs render as escaped text only.
- Follow existing conventions: `Result<T, String>` commands, snake_case
  wire payloads, `@tauri-apps/api/core` invoke via a composable, `<script
  setup>` SFCs, plain scoped CSS with the existing variables, no `any`,
  `import type` for type-only imports, functions under 50 lines.
- Vue's default escaping covers user-supplied names/URLs; do not use
  `v-html` anywhere in this feature.
- Form UX contract: every field has a label with matching `for`/`id`; field
  errors are associated with their input, clear when the user corrects the
  value, and a failed submit moves focus to the first invalid field;
  validation/save results arrive in an `aria-live="polite"` status line;
  in-flight actions disable their trigger buttons.
- Do not touch the microphone, transcription, or model sections of
  `App.vue`; the providers UI lives under `SettingsWindow`.
- On Windows the `keyring` crate uses Credential Manager; treat
  keychain-unavailable as a distinct actionable error, not a config bug.
- After `/complete`, the overview TODO for the app-data store format is
  settled by this feature and should be updated in the next `/overview` run.
