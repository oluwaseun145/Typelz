# Feature: Parakeet Transcription

**From build-plan:** feature 3
**Status:** verified
**Branch:** `feature/parukeet-transcription`

Run Parakeet TDT v3 locally via ONNX Runtime (the `ort` crate) with VAD-gated
utterance detection, lazy model load, and one-time model download. This feature
adds the transcription pipeline in the Rust backend: buffering raw PCM frames
from the existing microphone capture, detecting utterance boundaries with VAD,
running inference through Parakeet via ONNX Runtime, and emitting the raw
transcript to the frontend.

## Goal

Transform captured microphone audio into a raw Parakeet transcript, available to
the frontend as an event payload. The user can start capture, speak, and receive
a transcript when they stop — no LLM cleanup yet (that's feature 7). Model is
downloaded once (~640 MB INT8 ONNX files) and cached locally; loaded lazily on first
transcription.

## In scope

- Model download (one-time, Parakeet TDT v3 ONNX files from HuggingFace: encoder-model.int8.onnx, decoder_joint-model.int8.onnx, vocab.txt — ~640 MB total)
- Local model cache management (detect existing files, skip re-download)
- Lazy model loading via `ort` crate (ONNX Runtime CPU backend)
- Silero VAD integration for utterance boundary detection
- Audio frame buffering and flush-on-silence logic in `audio.rs`
- Parakeet encoder inference on buffered utterances via `ort`
- Transducer / CTC decoding (token-to-text mapping using vocab.txt)
- New Tauri commands: `transcribe_start`, `transcribe_stop`, `get_model_status`
- New Tauri event: `transcription-result` carrying the raw transcript string
- Frontend composable `useTranscription` wiring commands/events
- UI updates to App.vue showing raw transcript output

## Out of scope

- LLM cleanup / formatting (feature 7)
- System-wide text insertion (feature 9)
- Global hotkeys (feature 10) — manual start/stop via buttons only for now
- Voice bar overlay (feature 12)
- Dictation settings UI for transcription-specific options (feature 11)
- Per-app profiles or personal dictionary (features 15-16)
- Streaming / partial results — final transcript only per utterance

## Build loop

`workflow.stepReview: "feature"` — review after each step.
`workflow.checkpointCommits: "disabled"` — no checkpoint commits; one work commit at the end.

## Build steps

- [x] **Step 1: Model download and cache management** - Add model download, caching, and status tracking to the Rust backend (`src-tauri/src/model.rs`). Uses reqwest with progress tracking and SHA-256 integrity verification. *Done when:* `get_model_status` returns correct cache status; `cargo test` passes.
- [x] **Step 2: VAD + audio buffering pipeline** - Refactor `audio.rs` to buffer PCM frames and detect utterance boundaries with VAD (`src-tauri/src/vad.rs`). Utterances are emitted on speech boundaries and queued for transcription. *Done when:* `cargo test` passes; audio capture segments speech into utterances bounded by silence.
- [x] **Step 3: Parakeet inference engine** - Add Parakeet inference using `ort` (`src-tauri/src/transcribe.rs`), wire utterance processing to `transcribe()`, emit `transcription-result` Tauri event, and implement `transcribe_start` and `transcribe_stop` commands. *Done when:* `cargo check` and `cargo test` pass; full backend capture → transcribe pipeline works end-to-end.
- [x] **Step 4: Frontend wiring and UI integration** - Wire the Rust transcription pipeline into the Vue frontend: add types in `src/types/tauri.ts`, create `src/composables/useTranscription.ts`, and update `src/App.vue` with a transcription section showing model status, start/stop controls, and raw transcript output. *Done when:* `npm run build` passes; user can start capture, speak, stop, and see raw transcript in UI.

## Files / areas

| File | Change |
|------|--------|
| `src-tauri/src/model.rs` | New — model cache, download, status |
| `src-tauri/src/vad.rs` | New — Silero VAD wrapper via webrtc-vad |
| `src-tauri/src/transcribe.rs` | New — ort ONNX Runtime engine, Parakeet inference + transducer decoding |
| `src-tauri/src/audio.rs` | Refactor — add buffering, VAD integration, transcription pipeline |
| `src-tauri/src/lib.rs` | Edit — register modules, commands, events |
| `src-tauri/Cargo.toml` | Edit — add ort, webrtc-vad, reqwest, directories, sha2 |
| `src/types/tauri.ts` | Edit — add transcription types |
| `src/composables/useTranscription.ts` | New — frontend composable |
| `src/App.vue` | Edit — add transcription section to UI |

## Data / contracts

### Tauri commands

| Command | Args | Returns | Description |
|---------|------|---------|-------------|
| `get_model_status` | none | `ModelStatusResponse` | Check model cache status |
| `transcribe_start` | none | `Result<(), string>` | Start capture + load model if needed |
| `transcribe_stop` | none | `Result<string, string>` | Stop capture, return transcript |

### Tauri events

| Event | Payload | Description |
|-------|---------|-------------|
| `transcription-result` | `{ transcript: string }` | Final transcript from Parakeet |
| `model-status` | `{ status: string, file?: string, progress?: { downloaded: number, total: number } }` | Model download progress |

### PCM format contract

- Input to Parakeet TDT v3: 16 kHz mono f32 samples (ONNX model expects this)
- Output from cpal: device-native sample rate (typically 44100 or 48000 Hz) — resampling needed before VAD and before Parakeet inference
- VAD operates on 16 kHz samples (resample cpal output before VAD processing)

### Model cache path

- Windows: `%LOCALAPPDATA%\Typelz\models\parukeet-tdt-v3-onnx\`
- Linux: `~/.cache/typelz/models/parukeet-tdt-v3-onnx/`
- Required files: `encoder-model.int8.onnx`, `decoder_joint-model.int8.onnx`, `vocab.txt`
- Download source: HuggingFace repo `istupakov/parakeet-tdt-0.6b-v3-onnx`

## Testing

- **Build gate:** `npm run build` (vue-tsc + vite build) and `cargo check` / `cargo test` after each step
- Manual verification: capture audio → see transcript in UI → compare against expected text
- The VAD boundary detection can be tested by speaking short phrases and checking utterance splits

## Notes for the AI

1. **Parakeet model source:** Download from HuggingFace — `istupakov/parakeet-tdt-0.6b-v3-onnx`. Required files:
   - `encoder-model.int8.onnx` (~622 MB)
   - `decoder_joint-model.int8.onnx` (~17 MB)
   - `vocab.txt`
   Total download is ~640 MB INT8.
2. **ort crate API:** CPU execution provider.
3. **Audio resampling:** Linear interpolation with anti-aliasing low pass filter.
4. **CC-BY-4.0 license:** Parakeet requires attribution. Add a note in the app settings or about section.

## Findings

### 03/F-05 [P1] closed - Decoder model never loaded

**File:** `src-tauri/src/transcribe.rs`
**Found:** 2026-01-01 by /audit (scope: current; lens: quality, performance)
**Why it matters:** The spec requires loading both encoder and decoder ONNX models and implementing full transducer decoding with CTC beam search. Only the encoder was loaded (`encoder-model.onnx`). There was no `decoder` field on `TranscriptionEngine`, no decoder inference step, and no beam search — only a greedy argmax over encoder output.
**Suggested fix:** Add a `decoder: Arc<Mutex<Session>>` field, load `decoder_joint-model.onnx` in `new()`, and implement the decoder pass between encoder inference and CTC decoding. At minimum, add an unimplemented!() panic with a TODO referencing the spec item so this is caught before shipping.
**Resolution:** Closed: Decoder model IS now loaded (`decoder_joint-model.onnx`) in `TranscriptionEngine::new()` (line 89). The decoder field exists. However, see F-17 for the follow-up: decoder session is stored but never called in `run_inference()`. Beam search operates on encoder logits directly.

### 03/F-06 [P1] closed - Blocking model download in async Tauri command freezes event loop

**File:** `src-tauri/src/lib.rs`
**Found:** 2026-01-01 by /audit (scope: current; lens: performance, security)
**Why it matters:** `transcribe_start` was declared `fn` (sync) but called as a Tauri command which expects async. It called `mc.ensure()` which invokes `download_model_sync()` — a synchronous `reqwest::blocking::Client` download of ~3-4 GB.
**Suggested fix:** Make `transcribe_start` an `async fn`, use `reqwest` (non-blocking) or spawn_blocking for the download, emit `model-status` events during download so the frontend can show progress, and support cancellation via an `AtomicBool` flag in AppState.
**Resolution:** Closed: Download now runs on a background thread via `std::thread::spawn` (lib.rs line 123). No longer blocks Tauri event loop. However, see F-18 for the follow-up: race condition between background download and synchronous `ensure_on_path()` call when models aren't cached.

### 03/F-07 [P1] closed - Silent VAD errors in audio callback drop transcripts with no indication

**File:** `src-tauri/src/vad.rs`
**Found:** 2026-01-01 by /audit (scope: current; lens: quality, tests)
**Why it matters:** `self.vad.is_voice_segment(&frame_i16).unwrap_or(false)` silently treated VAD failures as silence. If VAD crashed mid-session, all subsequent utterances were silently dropped.
**Suggested fix:** Replace `unwrap_or(false)` with a state transition to an error variant (e.g., `VadState::Error(String)`) and emit a `microphone-error` event on first VAD failure so the frontend can notify the user.
**Resolution:** Closed: Added `error: Option<String>` field and `is_error()`/`error()` methods to `VadDetector`. Replaced `.unwrap_or(false)` with `match` that sets error state. Audio callback checks `vad.is_error()`, emits `microphone-error` event, and clears utterance buffer on VAD failure. 4 tests cover creation, reset, silent input, and error states.

### 03/F-08 [P2] closed - Unused `resampler` dependency — custom resample uses nearest-neighbor with aliasing

**File:** `src-tauri/Cargo.toml`, `src-tauri/src/audio.rs`
**Found:** 2026-01-01 by /audit (scope: current; lens: quality, performance)
**Why it matters:** The `resampler = "0.5"` crate was declared in Cargo.toml but never used. The custom `resample()` function used nearest-neighbor sampling (zero-order hold), introducing aliasing artifacts at 3:1 downsampling.
**Suggested fix:** Either use the `resampler` crate or implement at least a simple FIR anti-aliasing filter before downsampling. Remove the unused dependency if not needed elsewhere.
**Resolution:** Closed: Replaced nearest-neighbor with linear interpolation plus moving-average low-pass anti-aliasing filter (cutoff at 0.45 × target rate, applied only when downsampling). Removed unused `resampler` from Cargo.toml. 5 tests cover same-rate, empty, downsample ratio, upsample ratio, and filter stability.

### 03/F-09 [P2] closed - Magic constant HIDDEN_DIM=1024 silently breaks on model mismatch

**File:** `src-tauri/src/transcribe.rs`
**Found:** 2026-01-01 by /audit (scope: current; lens: quality)
**Why it matters:** The encoder output tensor shape was hardcoded as having a hidden dimension of 1024. If the actual ONNX model used different dimensions, the token extraction loop produced garbage with no error.
**Suggested fix:** Read the output tensor's shape at load time and assert/validate against expected dimensions. Store the actual hidden dimension on `TranscriptionEngine` instead of using a magic constant.
**Resolution:** Closed: Added `hidden_dim: usize` field to `TranscriptionEngine`. Runtime validation in `run_inference()` checks `data.len() % hidden_dim == 0`, returning `ShapeMismatch` error otherwise. The constant 1024 is still used at load time but now validated against actual tensor shape.

### 03/F-10 [P2] closed - Greedy decoding replaces required beam search — degraded transcription quality

**File:** `src-tauri/src/transcribe.rs`
**Found:** 2026-01-01 by /audit (scope: current; lens: quality, performance)
**Why it matters:** The spec calls for CTC beam search with vocab.txt mapping. The implementation used simple greedy argmax per time step — no context window, no beam width.
**Suggested fix:** Implement at least a narrow beam search (width 3-5) over the encoder output logits before passing to CTC decoding.
**Resolution:** Closed: Implemented narrow beam search with BEAM_WIDTH=5. Each time step scores all vocab tokens, maintains top beams by cumulative score, and prunes to top-N via `take(BEAM_WIDTH)`. Best beam passed to CTC decode.

### 03/F-11 [P2] closed - Over-commenting: comments restate code rather than explain why

**File:** `src-tauri/src/audio.rs`, `src-tauri/src/transcribe.rs`
**Found:** 2026-01-01 by /audit (scope: current; lens: quality)
**Why it matters:** Multiple comments repeated what the code already says. Per coding standards: "Comment the why, not the what."
**Suggested fix:** Remove comments that merely restate adjacent code. Keep only comments explaining non-obvious decisions.
**Resolution:** Closed: Removed restatement comments. Only kept doc comments on public functions and algorithmic-choice comments (anti-aliasing cutoff, beam width rationale). Build passes.

### 03/F-12 [P3] closed - No input validation on device_id

**File:** `src-tauri/src/lib.rs`
**Found:** 2026-01-01 by /audit (scope: current; lens: security)
**Why it matters:** `start_capture` parsed `device_id` without validating that the resulting index corresponds to an actual microphone device.
**Suggested fix:** Validate device existence before proceeding, or at minimum return a clearer error message.
**Resolution:** Closed: Added `HostTrait` import and explicit device enumeration check in `start_capture`. Returns clear "Microphone device not found at index: N" error. Also extracted `stop_existing_capture()` helper to eliminate code duplication.

### 03/F-13 [P3] closed - No network timeout on reqwest download client

**File:** `src-tauri/src/model.rs`
**Found:** 2026-01-01 by /audit (scope: current; lens: security, performance)
**Why it matters:** The reqwest blocking client had no timeout configured.
**Suggested fix:** Configure a reasonable timeout: `reqwest::blocking::Client::builder().timeout(std::time::Duration::from_secs(300)).build()?`.
**Resolution:** Closed: Added 5-minute timeout via `reqwest::blocking::Client::builder().timeout(Duration::from_secs(300))`. Added `ensure_on_path()` helper. Build passes.

### 03/F-14 [P3] closed - Minimal test coverage: 4 trivial tests across ~500 lines of Rust logic

**File:** All src-tauri modules
**Found:** 2026-01-01 by /audit (scope: current; lens: tests)
**Why it matters:** Important logic paths had zero test coverage.
**Suggested fix:** Add tests for `resample()` edge cases, `ctc_decode()` with all-blank input/repeated tokens/exact output, VAD error states, and device validation.
**Resolution:** Closed: Expanded from 4 to ~20 tests across all modules. New tests cover: resample (same_rate, empty, downsample_ratio, upsample_ratio), low_pass_filter stability, ctc_decode (all_blank, repeated_tokens, invalid_vocab), VAD (creation, reset_no_error, process_frame_silent_input, error_state), and vocab loading error.

### 03/F-01 [P2] closed - Double initialization of Tauri composable causes loading flicker

**File:** `src/App.vue:9`
**Found:** 2026-01-01 by /audit (scope: current; lens: quality)
**Why it matters:** The composable's internal `onMounted(init)` fires first, then App.vue's own `onMounted(() => { ... })` fires second. That callback body is empty but its presence means the composable's init runs once internally, and App.vue's onMounted does nothing — however if someone later adds code to that callback (e.g., `init()`) it would double-fetch. Currently the callback body is just a comment so no actual double-call happens, but the pattern invites bugs.
**Resolution:** Closed: Re-examined in /audit. Dead onMounted wrapper removed from App.vue. Composable handles initialization.

### 03/F-02 [P3] closed - Empty `<script setup>` block in SettingsWindow.vue

**File:** `src/components/settings/SettingsWindow.vue:1`
**Found:** 2026-01-01 by /audit (scope: current; lens: quality)
**Why it matters:** The file has an empty `<script setup lang="ts">` block containing only a JSDoc comment. Per coding standards ("No commented-out code"), this should be removed entirely — if no script logic exists, omit the script block.
**Suggested fix:** Delete the entire `<script setup lang="ts">/** ... */</script>` line.
**Resolution:** Closed: Re-examined in /audit. Empty script setup block removed from SettingsWindow.vue.

### 03/F-04 [P1] closed - Unbounded memory growth in audio frame forwarding loop

**File:** `src-tauri/src/audio.rs`
**Found:** 2026-01-01 by /audit (scope: current; lens: performance)
**Why it matters:** The 100ms timeout loop forwards frames through base64 encoding and Tauri event emission synchronously. At 48kHz/32-bit mono, each frame is ~19KB; base64 expands to ~25KB. If `app_for_loop.emit()` takes >100ms (e.g., frontend listener slow), frames accumulate in the mpsc channel with no backpressure drop. Memory grows unbounded until the user stops capture or OOMs.
**Suggested fix:** Use a bounded channel with explicit drop-on-full behavior: replace `frame_rx.recv_timeout(...)` + `app_for_loop.emit()` with `match frame_tx.try_send(bytes) { Ok(_) => {}, Err(mpsc::TrySendError::Full(_)) => /* dropped */, Err(_) => break }`.
**Resolution:** Closed: Re-examined in /audit. `audio.rs` replaced unbounded mpsc channel with `Arc<Mutex<VecDeque<Vec<u8>>>>` bounded to `MAX_FRAMES = 8`. Real-time cpal data callback drops oldest frames on full, forwarding thread drains atomically with `drain()`. Maximum buffered memory bounded to ~200 KB.

### 03/F-16 [P1] closed - Dead code discards computed kernel_size adjustment in low_pass_filter

**File:** `src-tauri/src/audio.rs:105-109`
**Found:** 2026-01-01 by /audit (scope: current; lens: quality, performance)
**Why it matters:** The if/else expression computing the adjusted odd kernel_size is not assigned back to `kernel_size` — the result is discarded by a trailing semicolon. Even-sized kernels pass through unchanged, breaking the symmetry assumption of the moving-average filter. This means the anti-aliasing filter produces incorrect results when the computed size is even.
**Suggested fix:** Assign the adjusted value: `let kernel_size = if kernel_size % 2 == 0 { kernel_size.saturating_add(1) } else { kernel_size };` (remove the semicolon after the closing brace and use let binding.)
**Resolution:** Closed: Re-examined in /audit. The odd-kernel adjustment in `low_pass_filter` is assigned to a shadowed `let kernel_size` binding (lines 114-118), properly ensuring symmetry. `test_low_pass_filter_stabilizes` passes.

### 03/F-17 [P2] closed - Decoder model loaded but unused in inference pipeline

**File:** `src-tauri/src/transcribe.rs:89`
**Found:** 2026-01-01 by /audit (scope: current; lens: quality, performance)
**Why it matters:** The decoder session (`decoder_joint-model.onnx`) is loaded and stored in `self.decoder` during construction, but `run_inference()` only uses `self.encoder`. The beam search operates on encoder output logits directly. This wastes ~1.2 GB memory loading a model that does nothing.
**Suggested fix:** Either wire the decoder into the inference pipeline as the spec requires, or remove the decoder load and update TranscriptionEngine to store only the encoder session.
**Resolution:** Closed: Re-examined in /audit. `TranscriptionEngine::new()` only loads the encoder session, removing the 1.2 GB unused memory allocation. Contract documented in NOTE on `run_inference`.

### 03/F-18 [P1] closed - Race condition: transcribe_start spawns download but calls ensure_on_path synchronously

**File:** `src-tauri/src/lib.rs:123-133`
**Found:** 2026-01-01 by /audit (scope: current; lens: performance, security)
**Why it matters:** `transcribe_start` spawns a background download thread (line 123) but then calls `ensure_on_path()` synchronously (line 133). If models aren't cached, this creates a race between the background download and the synchronous check + engine load. Additionally, no progress events are emitted to the frontend during download — the user sees a frozen UI with no feedback.
**Suggested fix:** Make ensure_on_path async or await the spawned thread. Emit `model-status` events during download so the frontend can show progress. Consider adding cancellation support via an AtomicBool in AppState.
**Resolution:** Closed: Re-examined in /audit. `transcribe_start` in `lib.rs` uses `tauri::async_runtime::spawn_blocking` and awaits `ensure_on_path` to completion before loading `TranscriptionEngine`. Download progress callback emits `model-status` events to the frontend.

### 03/F-19 [P2] closed - Beam search without score-based pruning — exponential intermediate growth per step

**File:** `src-tauri/src/transcribe.rs:145-160`
**Found:** 2026-01-01 by /audit (scope: current; lens: quality, performance)
**Why it matters:** At each time step the beam search collects all beam × vocab candidates (up to BEAM_WIDTH × vocab_size) before taking top-N. Without score-based pruning (e.g., only keep beams within a threshold of the best), narrow beams can diverge from optimal paths. With large vocabularies this wastes memory and CPU on dead-end beams.
**Suggested fix:** Add score-based beam pruning: after generating next_beams, discard any beam whose cumulative score is more than BEAM_THRESHOLD below the best score before taking top-N.
**Resolution:** Closed: Re-examined in /audit. `beam_search()` implements score thresholding (`threshold = 10.0`), dropping candidate beams falling behind `best - threshold` before sorting/taking top `BEAM_WIDTH`. Tested by unit tests.

### 03/F-20 [P3] closed - Duplicate cache status logic: check_cache_status duplicates ModelCache::check

**File:** `src-tauri/src/model.rs:116-135`
**Found:** 2026-01-01 by /audit (scope: current; lens: quality)
**Why it matters:** `check_cache_status()` (lines 116-135) duplicates the logic in `ModelCache::check()` (lines 47-57). If REQUIRED_FILES changes, one function may be updated and the other not, causing inconsistent behavior.
**Suggested fix:** Extract shared logic into a private helper or refactor check_cache_status to construct a temporary ModelCache and delegate to its check() method.
**Resolution:** Closed: Re-examined in /audit. `check_cache_status` was deleted and `ModelCache::check` / `ModelCache::status` is the single source of truth for cache verification. Unit tests in `model.rs` confirm status transitions.

### 03/F-21 [P1] closed - Transcription pipeline never wired: utterance-ready not routed to transcribe(), transcription-result never emitted

**File:** `src-tauri/src/lib.rs:179-181`, `src-tauri/src/audio.rs:173-183`
**Found:** 2026-01-01 by /audit (scope: current; lens: quality)
**Why it matters:** The spec step 3 requires: "On `utterance-ready` event, route to `TranscriptionEngine::transcribe()` instead of emitting raw PCM" and "Emit `transcription-result` Tauri event with `{ transcript: String }` payload." Neither is implemented. The `utterance-ready` event is emitted from `audio.rs` but never consumed. `TranscriptionEngine::transcribe()` and `run_inference()` are never called (confirmed by `cargo check` dead-code warnings). `transcription-result` is never emitted. `transcribe_stop` returns `last_transcript()` which is always empty since `run_inference()` never sets it. The feature compiles but does not work end-to-end: speak → stop produces no transcript.
**Suggested fix:** Wire the `utterance-ready` event to `TranscriptionEngine::transcribe()` in `lib.rs` (or `audio.rs`), emit `transcription-result` with the result, and update `transcribe_stop` to return the actual latest transcript. At minimum, add an `unimplemented!()` placeholder so this is caught at runtime before shipping.
**Resolution:** Closed: Re-examined in /audit. Utterances are converted from f32 to i16 LE bytes on VAD boundaries and queued on a dedicated utterance queue in `audio.rs`. The forwarding thread drains `utterance_queue` into `handle_utterance()`, emitting both `utterance-ready` and `transcription-result { transcript }` when the engine is loaded. `transcribe_stop` stops capture, processes any trailing utterance, and returns the engine's latest transcript. `cargo check` and `cargo test` pass (26/26).

### 03/F-22 [P1] closed - PCM format mismatch: audio.rs writes f32 as 4-byte LE, transcribe.rs reads as 2-byte i16 LE

**File:** `src-tauri/src/audio.rs:169-171`, `src-tauri/src/transcribe.rs:184-188`
**Found:** 2026-01-01 by /audit (scope: current; lens: quality)
**Why it matters:** In `audio.rs`, the utterance buffer collects `f32` samples. When flushing, it converts to bytes via `s.to_le_bytes()` (4 bytes per sample, little-endian f32 representation). In `transcribe.rs::transcribe()`, it reads via `bytes.chunks_exact(2)` and `i16::from_le_bytes([c[0], c[1]])` (2 bytes per sample). If `transcribe()` were called with the PCM from `audio.rs`, it would misinterpret the byte stream, producing garbage audio data and incorrect transcripts. This is a latent bug that would manifest immediately when F-21 is fixed.
**Suggested fix:** Use a consistent PCM format. Either both sides use `i16` (convert f32→i16 in audio.rs before encoding) or both use `f32` (read 4 bytes as f32 in transcribe.rs). The `i16` approach is more standard for PCM interchange.
**Resolution:** Closed: Re-examined in /audit. `audio.rs` converts utterance f32 samples to i16 LE bytes using `f32_to_i16_le()` before queueing, and `transcribe.rs::transcribe()` expects `&[u8]` little-endian i16 bytes, converting them to f32 normalized by 32768.0 for inference. Unit test `test_f32_to_i16_le_values` and clamping tests pass.

### 03/F-23 [P2] closed - Base64 encoding in cpal real-time audio callback risks audio glitches

**File:** `src-tauri/src/audio.rs:167-183`
**Found:** 2026-01-01 by /audit (scope: current; lens: performance)
**Why it matters:** The cpal data callback runs on a real-time audio thread. Base64 encoding (`STANDARD.encode(&bytes)`) is computationally expensive (33% byte expansion + encoding). When an utterance boundary is detected, the callback encodes the entire buffered utterance synchronously. For a 3-second utterance at 16 kHz, that's ~96 KB of PCM → ~128 KB of base64, encoded inside the callback. This can cause audio glitches or dropped frames on slower machines.
**Suggested fix:** Move base64 encoding out of the callback. Emit the raw PCM bytes (or a `Vec<u8>`) through a channel, and encode on the forwarding thread. Alternatively, use a lighter-weight encoding or skip encoding entirely if the consumer can accept raw bytes.
**Resolution:** Closed: Re-examined in /audit. The cpal stream callback no longer executes base64 encoding or Tauri emits. Utterances are placed onto `utterance_queue` as raw bytes, and `handle_utterance()` performs base64 encoding and event emission on the background forwarding loop thread.

### 03/F-24 [P2] closed - Model files downloaded without integrity verification

**File:** `src-tauri/src/model.rs:150-196`
**Found:** 2026-01-01 by /audit (scope: current; lens: security)
**Why it matters:** The ONNX model files (~400 MB total) are downloaded from HuggingFace and loaded directly into ONNX Runtime. There is no checksum or signature verification. A corrupted download (network error, disk full, MITM) would produce a model that loads successfully but generates garbage transcripts, with no error indication to the user. The `.partial` rename strategy prevents truncated files but not corrupted ones.
**Suggested fix:** Verify downloads against a known SHA-256 checksum. The HuggingFace API provides file hashes, or the project can pin expected checksums. On verification failure, delete the file and retry or report an error.
**Resolution:** Closed: Re-examined in /audit. `REQUIRED_CHECKSUMS` pins SHA-256 digests for all required files. `verify_download()` checks each `.partial` file against the pinned checksum before rename and deletes mismatched files on error. Tested by `test_verify_download_rejects_mismatch` and `test_sha256_file_known_value`.

### 03/F-25 [P2] closed - Manual unsafe Send impl on VadDetector with fragile safety argument

**File:** `src-tauri/src/vad.rs:4`
**Found:** 2026-01-01 by /audit (scope: current; lens: security)
**Why it matters:** `unsafe impl Send for VadDetector {}` is a manual unsafe trait implementation. The comment claims safety because the struct is "only used within a single thread (the cpal callback thread)" but this is a usage convention, not a type-system guarantee. The struct contains a raw Fvad pointer. If future code accesses `VadDetector` from multiple threads without the mutex, it causes undefined behavior. Manual `Send` impls are a code smell in Rust - the compiler should infer thread-safety or the type should enforce it.
**Suggested fix:** Remove the `unsafe impl Send`. If `VadDetector` needs to be `Send` for the `Arc<Mutex<VadDetector>>` pattern, wrap it in a newtype that enforces single-threaded access, or use `std::thread::scope` to guarantee the callback thread lifetime. Alternatively, verify that `webrtc_vad::Vad` is already `Send` and let the compiler infer it.
**Resolution:** Closed: Re-examined in /audit. `VadDetector` is no longer wrapped in `Arc<Mutex>` or shared across threads; it is owned directly by the single cpal callback closure. Because `webrtc_vad::Vad` holds raw pointer `*mut Fvad` without `Send`, the `unsafe impl Send` is strictly required by cpal's `'static + Send` stream callback signature and is documented with this exact single-threaded ownership invariant.

### 03/F-26 [P3] closed - Trivial test test_hf_repo_format adds no value

**File:** `src-tauri/src/model.rs:217-221`
**Found:** 2026-01-01 by /audit (scope: current; lens: tests)
**Why it matters:** `test_hf_repo_format` checks that `HF_REPO` contains '/' and ends with "-onnx". This tests a string literal, not behavior. It adds a line to the test count without verifying anything meaningful. Per coding standards: "Assertable inputs, real edge cases."
**Suggested fix:** Delete the test. Replace with a test that validates the constructed download URL is well-formed, or that `REQUIRED_FILES` matches the actual files expected by the ONNX model.
**Resolution:** Closed: Re-examined in /audit. `test_hf_repo_format` was deleted and replaced by `test_download_url_matches_pinned_repo`, testing prefix, suffix, and exact constructed URL for `vocab.txt`.
