# Findings

> **Generated file.** The findings ledger: review findings raised by `/audit`
> against the work in progress, each with a durable ID, severity (P0-P3), and
> status. `/implement` marks repaired findings `fixed`, a later `/audit` pass
> moves them to `closed`, and `/complete` refuses to merge while any P0 or P1
> finding is `open` or `fixed`, then archives resolved findings with the work
> and resets this file.

_No findings recorded. `/audit` appends findings here when it finds them._

### F-01 [P2] fixed - Double initialization of Tauri composable causes loading flicker

**File:** `src/App.vue:9`
**Found:** 2026-01-01 by /audit (scope: current; lens: quality)
**Why it matters:** The composable's internal `onMounted(init)` fires first, then App.vue's own `onMounted(() => { ... })` fires second. That callback body is empty but its presence means the composable's init runs once internally, and App.vue's onMounted does nothing — however if someone later adds code to that callback (e.g., `init()`) it would double-fetch. Currently the callback body is just a comment so no actual double-call happens, but the pattern invites bugs.
**Resolution:** Fixed: removed dead `onMounted` wrapper from App.vue. The composable's own `onMounted(init)` handles initialization. Build passes. The callback body is empty (just a comment), so no double-call occurs at runtime. However the dead `onMounted` wrapper serves no purpose and should be removed to avoid future accidental duplication.

### F-02 [P3] fixed - Empty `<script setup>` block in SettingsWindow.vue

**File:** `src/components/settings/SettingsWindow.vue:1`
**Found:** 2026-01-01 by /audit (scope: current; lens: quality)
**Why it matters:** The file has an empty `<script setup lang="ts">` block containing only a JSDoc comment. Per coding standards ("No commented-out code"), this should be removed entirely — if no script logic exists, omit the script block.
**Suggested fix:** Delete the entire `<script setup lang="ts">/** ... */</script>` line.
**Resolution:** Fixed: removed empty script block from SettingsWindow.vue. Build passes.

### F-03 [P3] open - Scaffold defaults in Cargo.toml

**File:** `src-tauri/Cargo.toml:4-5`
**Found:** 2026-01-01 by /audit (scope: current; lens: quality)
**Why it matters:** `authors = ["you"]` and `license = ""` are Tauri CLI scaffolding artifacts. These appear in any package inspection and look unprofessional. The spec author did not specify replacements, so this is a minor cleanup item.
**Suggested fix:** Replace with project-appropriate values (e.g., `authors = ["Typelz contributors"]`) or leave as-is until the first release step owns it.
