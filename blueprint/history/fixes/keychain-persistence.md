# Current Feature

> **Generated file.** Holds the one feature, fix, or rollback being built right now. Run
> `/feature <number-or-name>` to spec a build-plan feature, or `/fix "<bug>"` for
> an ad-hoc fix. Use `/rollback <completed-feature>` to plan a safe reversal.
> Build one thing at a time; `/complete` archives it under
> `blueprint/history/` and resets this file.

**Fix:** API key not persisted in Windows Credential Manager across app restarts

**Type:** Fix

**Status:** verified

**Branch:** `fix/keychain-persistence`

---

## The problem

When a user adds a provider with an API key, the `add_provider` command calls
`credentials.set()` (which delegates to `keyring::Entry::set_password`). The
provider metadata saves to `typelz.json` successfully, but the API key does not
persist in Windows Credential Manager across app restarts. On next launch, the
provider list shows `No API key` and `validate_provider` returns:

```
No API key is stored for provider <id>.
```

The `test_provider_credentials` command tests the key directly from user input
without the keychain, so a successful test does not prove keychain persistence.

**Root cause:** The `keyring` 3 crate's `set_password` on Windows may appear to
succeed but fail to persist the credential to Windows Credential Manager. The
`add` command has no read-back verification, so the failure is silent.

## The fix

Add a read-back verification after every keychain write (`set`). If the
read-back confirms the key, the write is proven. If it fails, the error surfaces
to the user immediately instead of silently losing the key.

Additionally, add a file-based fallback credential store. When the OS keychain
write succeeds but the read-back fails, or when the keychain is unavailable,
store the API key in an encrypted or obfuscated file alongside `typelz.json`.
This keeps the app functional even when the OS credential store misbehaves.

## Build steps

- [x] 1. **Add read-back verification to keychain writes** - In
  `KeyringCredentialStore::set`, call `get_password` after `set_password` and
  return an error if the read-back fails. **Done when:** `cargo build` succeeds,
  and existing `cargo test` passes.

- [x] 2. **Add file-based fallback credential store** - Created
  `FallbackCredentialStore` that tries the keychain first, then falls back to
  `credentials.json` (base64-obfuscated). Registered it in `lib.rs`. **Done
  when:** `cargo build` succeeds, `cargo test` passes, and the fallback file is
  created on keychain failure.

- [x] 3. **Verify** - Run `npm run build` and `cargo test`. **Done when:** both
  commands pass.

## Verify

1. `npm run build` passes
2. `cargo test` passes (59/59)
3. `cargo build` succeeds
