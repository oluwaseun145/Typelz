# Current Feature

**Type:** Fix
**Branch:** fix/transcription-pipeline-bugs
**Status:** verified

## Problem

Five bugs prevent the transcription pipeline from working end-to-end:

1. `isTranscribing` is never set back to `false` after `transcribe_start` succeeds — the "Start Transcription" button stays locked on "Preparing model..." indefinitely.
2. The utterance buffer in `audio.rs` is read on a `Boundary` VAD event but never cleared — every subsequent boundary re-queues the same accumulated audio, duplicating transcriptions.
3. The `onModelStatus` handler receives the `ready` event but does not reset any loading state, so there is no hook to clean up the transitional UI even if state is refactored.
4. `transcribe_stop` is a synchronous Tauri command — it blocks the main thread while joining the capture thread and any in-flight inference, freezing the app on Stop.
5. Speech still buffered when Stop is pressed is never flushed (VAD only emits an utterance on a Silence boundary), so continuous speech with no pause produces no transcript.

## Round 2 problem (debug handoff, 2026-09-07)

The round-1 flow works mechanically, but diagnosis found three defects that still
break the start -> stop -> transcript loop on this machine, plus silent failures:

6. Mic capture delivers the device's native multi-channel stream treated as mono:
   the callback reads the interleaved channel data with no downmix. The only input
   device on this machine is a 4-channel Realtek mic (48 kHz F32), so VAD and
   Parakeet receive time-stretched, channel-scrambled audio — mostly empty or
   garbage transcripts.
7. The engine's `last_transcript` and the frontend `lastTranscript` are never
   cleared when a new session starts, and `transcribe_stop` returns the stale
   value — so after the first run the transcript box keeps showing the previous
   recording.
8. `transcribe_start` emits the `model-status ready` event before the engine is
   actually loaded; the frontend flips to ready on that event, so capture can
   start while the engine is still `None` and `handle_utterance` silently drops
   those utterances.

No observability: the pipeline only reports inference errors; capture config,
VAD activity, and inference timing are invisible, so failures cannot be
triaged from the `tauri dev` terminal.

## Round 3 problem (user report + live trace, 2026-09-07)

The mono fix verified live (real transcripts on a 4-channel mic). Remaining
behavioral defects from the first hands-on run:

9. Behavior mismatch: the pipeline transcribes and displays **live** per VAD
   boundary, but the expected flow (and the UI hint) is record -> stop -> one
   transcript of the whole recording.
10. `transcribe_stop` returns only the **last** utterance's text (the engine's
    `last_transcript`), so a multi-sentence recording loses everything except
    the final fragment.
11. WebRTC VAD runs in the most sensitive `Quality` mode and fires on room
    noise as 0.09-0.3 s "utterances"; each is sent to inference and some
    produce random 5-12 character fragments ("picking up randoms").

## In scope

- Fix the `isTranscribing` flag lifecycle in `useTranscription.ts` so it reflects "model load in progress" only while `transcribe_start` is awaiting, and resets to `false` on both success and failure.
- Introduce a separate `isModelReady` ref so the UI can distinguish "loading" from "ready" without conflating the two into one boolean.
- Clear `utterance_buffer` after pushing to `utterance_queue` on a VAD `Boundary` event in `audio.rs`.
- Update `App.vue` to use the new refs correctly (button disabled state, label text, model status indicator).
- Make `transcribe_stop` async so the capture-thread join + inference run on a worker thread, not the UI.
- Flush the remaining `utterance_buffer` contents as a final utterance when the capture thread exits.
- Request 1-channel capture in `audio.rs`; when the device cannot deliver mono, downmix the interleaved stream to mono in the callback (testable pure function).
- Emit one capture-config trace line (sample rate, native channel count, mono vs downmix mode) when the stream starts.
- Add VAD speech-start / utterance-end traces with utterance length in seconds, plus a stop-time flush length trace.
- Add a per-utterance inference wall-time and result-length trace in `TranscriptionEngine::transcribe` (success path; errors already print).
- Add `TranscriptionEngine::clear_transcript()` and call it from `transcribe_start` once the engine is guaranteed loaded.
- Move engine load into the `transcribe_start` blocking task so the `model-status ready` event is emitted only after `TranscriptionEngine::new` has completed.
- Clear the frontend `lastTranscript` at the start of `startTranscription()`.
- Accumulate each successful utterance into a per-session transcript in the
  engine (pure, unit-tested join helper); reset it at every new session or
  new capture start.
- Skip utterances shorter than 300 ms before inference, with a skip trace
  (noise blips never reach the model).
- Run WebRTC VAD at `LowBitrate` aggressiveness instead of the default
  `Quality` mode at 16 kHz.
- Stop emitting live per-utterance `transcription-result` events; deliver the
  full session transcript once capture ends — `transcribe_stop` returns it,
  and `stop_capture` emits it when non-empty.

## Out of scope

- Any new features, UI redesign, or Rust rewrite beyond the targeted fixes.
- VAD frame-chunking improvement (only the first 160 samples per callback are evaluated); acceptable for now.

## Build steps

- [x] **Step 1** — Fix `useTranscription.ts`: split `isTranscribing` into `isModelLoading` + `isModelReady`; reset `isModelLoading` on success and failure; set `isModelReady` from the `ready` model-status event.
  - Done when: `startTranscription()` sets `isModelLoading = true` before the invoke, clears it after (success or catch), and `onModelStatus` sets `isModelReady = true` on `status === 'ready'`.

- [x] **Step 2** — Fix `audio.rs`: clear `utterance_buffer` after pushing to `utterance_queue` on `VadState::Boundary`.
  - Done when: the `Boundary` arm locks `utterance_buffer` mutably and calls `.clear()` after the push.

- [x] **Step 3** — Update `App.vue`: replace `isTranscribing` usages with `isModelLoading` / `isModelReady` where appropriate; update button disabled logic and label.
  - Done when: "Start Transcription" is disabled only while `isModelLoading` is true; label shows "Loading model..." during load and "Start Transcription" otherwise; model status indicator uses `isModelReady`.

- [x] **Step 4** — Fix `lib.rs`: make `transcribe_stop` async via `spawn_blocking` so the capture-thread join and final inference never block the UI thread.
  - Done when: `transcribe_stop` is `async fn`, takes the stop sender/handle/engine out of `AppState` before spawning, joins inside the blocking task, and the UI stays responsive while inference runs.

- [x] **Step 5** — Fix `audio.rs`: flush the `utterance_buffer` as a final utterance after the stop loop exits, so speech that never reached a VAD boundary is still transcribed.
  - Done when: after the trailing queue drain, a non-empty `utterance_buffer` is converted to i16 PCM and routed through `handle_utterance`.
- [x] **Step 6** — Fix `audio.rs`: capture mono. cpal 0.15.3 exposes no `set_channels`, so capture stays on the device's default config and the callback downmixes interleaved frames to mono via a pure `downmix_to_mono` helper (passthrough when the device is already mono).
  - Done when: a multi-channel device yields 16 kHz mono into the VAD and model, `downmix_to_mono` is unit-tested (2ch/4ch values, passthrough for 1 channel, trailing partial chunk dropped), and the stream start prints one trace line with rate, native channel count, and mode.
- [x] **Step 7** — Fix `audio.rs` + `transcribe.rs`: lightweight pipeline trace. VAD speech-start and utterance-end (with seconds), stop-time flush length, and per-utterance inference wall-time plus result length in `TranscriptionEngine::transcribe`.
  - Done when: a live session prints the capture line, VAD transitions with utterance lengths, and one inference timing line per utterance in the `tauri dev` terminal, with no per-frame logging while silent.
- [x] **Step 8** — Fix `transcribe.rs` + `lib.rs`: new sessions start clean. Add `TranscriptionEngine::clear_transcript()` and call it from `transcribe_start` after the engine is loaded; test it against the real engine when the model cache is present (cache-absent skip pattern).
  - Done when: `transcribe_start` clears the engine's `last_transcript` on every successful start, and the conditional test proves clear resets `last_transcript` to empty.
- [x] **Step 9** — Fix `lib.rs` + `useTranscription.ts`: `ready` means loaded. Move model prepare + engine load into one `spawn_blocking` task and emit `model-status ready` only after the engine is loaded; clear frontend `lastTranscript` at the start of `startTranscription()`.
  - Done when: no `ready` event can reach the frontend before `TranscriptionEngine::new` completes, the transcript box is empty on Start Transcription, and download-progress event wiring is unchanged.
- [x] **Step 10** — Fix `transcribe.rs`: session transcript. Add a pure `join_transcript(existing, new) -> String` helper (trim, single-space join; empty inputs yield empty) with unit tests; add a `session_transcript` field that `transcribe()` appends to, `clear_transcript()` resets, and a `session_transcript()` accessor exposes.
  - Done when: the join helper is unit-tested (both empty, one empty, both non-empty, whitespace trim) and a conditional real-engine test proves clear resets the session.
- [x] **Step 11** — Fix `audio.rs`: minimum utterance duration and no live display. `handle_utterance` skips utterances with fewer than 300 ms of 16 kHz samples (4 800 samples) with a skip trace, and no longer emits `transcription-result` per utterance (the stop path owns delivery).
  - Done when: a sub-300 ms utterance produces a skip trace and no inference, longer utterances are transcribed and accumulate, and no `transcription-result` event is emitted mid-capture.
- [x] **Step 12** — Fix `vad.rs`: construct the detector with `Vad::new_with_rate_and_mode(Rate16kHz, LowBitrate)` instead of `new_with_rate(Rate16kHz)`.
  - Done when: the detector runs LowBitrate mode at 16 kHz and the existing VAD tests still pass.
- [x] **Step 13** — Fix `lib.rs`: stops deliver the session. `transcribe_stop` returns `session_transcript()` after the join; `stop_capture` emits `transcription-result` with the non-empty session after the join; `start_capture` clears the session when a new capture begins with a loaded engine.
  - Done when: pressing either Stop button delivers the concatenated transcript of the whole session, an empty session yields an empty result and no event, and re-capturing starts from an empty session.

## Done when (acceptance)

- Pressing "Start Transcription" shows a brief loading state, then returns the button to its normal enabled state once the model is ready.
- Recording audio after model load produces a transcript in the transcript output area — including when the user is still speaking (no pause) at Stop.
- No duplicate transcriptions from the same audio segment.
- Pressing Stop never hangs the app UI; the window stays responsive while the final utterance is transcribed.
- `npm run build` passes with no type errors and `cargo build` compiles cleanly.
- The `tauri dev` terminal prints the capture-config line, VAD transitions with utterance lengths, and per-utterance inference timing during a live dictation session.
- A second recording shows a fresh transcript of the new audio in the transcript box — no leftover text from the previous session.
- Pressing "Start Transcription" empties the transcript box immediately.
- The transcript box stays unchanged during recording; after Stop it shows one transcript covering the whole recording (not just the last fragment).
- The trace shows no sub-300 ms utterance being sent to inference, and a 10-second dictation yields far fewer VAD segments than before.
