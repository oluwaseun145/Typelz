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
**Resolution:** Fixed: removed dead `onMounted` wrapper from App.vue. The composable's own `onMounted(init)` handles initialization. Build passes.

### F-02 [P3] fixed - Empty `<script setup>` block in SettingsWindow.vue

**File:** `src/components/settings/SettingsWindow.vue:1`
**Found:** 2026-01-01 by /audit (scope: current; lens: quality)
**Why it matters:** The file has an empty `<script setup lang="ts">` block containing only a JSDoc comment. Per coding standards ("No commented-out code"), this should be removed entirely — if no script logic exists, omit the script block.
**Suggested fix:** Delete the entire `<script setup lang="ts">/** ... */</script>` line.
**Resolution:** Fixed: removed empty script block from SettingsWindow.vue. Build passes.

### F-04 [P1] fixed - Unbounded memory growth in audio frame forwarding loop

**File:** `src-tauri/src/audio.rs`
**Found:** 2026-01-01 by /audit (scope: current; lens: performance)
**Why it matters:** The 100ms timeout loop forwards frames through base64 encoding and Tauri event emission synchronously. At 48kHz/32-bit mono, each frame is ~19KB; base64 expands to ~25KB. If `app_for_loop.emit()` takes >100ms (e.g., frontend listener slow), frames accumulate in the mpsc channel with no backpressure drop. Memory grows unbounded until the user stops capture or OOMs.
**Suggested fix:** Use a bounded channel with explicit drop-on-full behavior: replace `frame_rx.recv_timeout(...)` + `app_for_loop.emit()` with `match frame_tx.try_send(bytes) { Ok(_) => {}, Err(mpsc::TrySendError::Full(_)) => /* dropped */, Err(_) => break }`.
**Resolution:** Fixed: replaced unbounded mpsc channel with `Arc<Mutex<VecDeque<Vec<u8>>>>` bounded to 8 frames. Data callback pushes to the queue (dropping oldest when full), forwarding loop drains via `mem::swap` (minimizing lock hold time). Maximum memory: ~200KB at 48kHz/32-bit mono (8 frames x ~25KB base64 each). Build passes.
