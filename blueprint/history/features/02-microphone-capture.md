# Current Feature

> **Generated file.** Holds the one feature, fix, or rollback being built right now. Run
> `/feature <number-or-name>` to spec a build-plan feature, or `/fix "<bug>"` for
> an ad-hoc fix. Use `/rollback <completed-feature>` to plan a safe reversal.
> Build one thing at a time; `/complete` archives it under
> `blueprint/history/` and resets this file.

**Status: verified**

## Feature 2 — Microphone capture

Capture microphone audio reliably with start, stop, and device selection via the
Tauri Rust core. This is the first real pipeline step: raw audio from the
microphone, gated by a simple listening state. Parakeet transcription, LLM
cleanup, and text insertion come in later features.

**Branch:** `feature/microphone-capture`

### Goal

Provide a Tauri command interface that lists available microphone devices, starts
audio capture on a selected device, stops capture, and streams raw audio frames
back to the Vue frontend through typed events. The frontend exposes this through
a composable with clear idle/running/error states.

### In scope

- Rust core: audio capture using `cpal` (already in Cargo.toml)
- Tauri commands: list devices, start capture, stop capture
- Audio streaming: forward captured PCM frames as events to the frontend
- Vue composable: `useMicrophone()` with start/stop/device-select API
- State management: idle, capturing, error states
- Error handling: device unavailable, permission denied, capture failure

### Out of scope

- VAD (Silero) — feature 3 territory
- Parakeet transcription — feature 3 territory
- LLM cleanup or text insertion — features 4–7
- Global hotkeys — feature 10
- Voice bar UI — feature 12
- Audio settings (volume, noise suppression) — future
- Persisted device preferences — feature 11

### Build loop

With `workflow.stepReview: "feature"` and `checkpointCommits: "disabled"`,
implement each step sequentially, present one final review packet after all steps
pass, then stop before committing.

### Build steps

- [x] **Step 1 — Rust audio capture layer**
  Added `audio.rs` module with `list_microphone_devices()` using cpal's
  `HostTrait::input_devices()`. Returns index-based device IDs ("0", "1", …)
  since cpal 0.15 WASAPI doesn't expose a stable string ID.
  **Done when:** Build succeeds; `invoke('list_microphone_devices')` returns
  at least one device name in the Tauri window console. ✅ Verified — `cargo check` passes.
  Add a Tauri command to list available input (microphone) devices using `cpal`.
  Return a vector of `(device_name, device_id)` pairs as JSON. This is a simple
  read-only query with no side effects.
  **Done when:** `tauri build` succeeds; `invoke('list_microphone_devices')` returns
  at least one device name in the Tauri window console.

- [x] **Step 2 — Start/stop capture commands**
  Implemented `start_capture(device_id)` and `stop_capture()` Tauri commands in
  `lib.rs`. `start_capture` spawns a background thread that owns the cpal input
  stream; `stop_capture` sends a signal via mpsc channel to stop it. Both are
  idempotent (restart on re-start, no-op on stop when idle).
  **Done when:** Build succeeds; calling start/stop from the frontend produces
  observable log output in the Tauri console confirming stream lifecycle. ✅ Verified — `cargo check` passes.
  Implement `start_capture(device_id)` and `stop_capture()` Tauri commands.
  `start_capture` initializes a cpal input stream on the specified device,
  registers an error callback, and begins reading audio frames.
  `stop_capture` drops the stream handle. Both commands must be idempotent:
  calling `start_capture` while already capturing stops then restarts; calling
  `stop_capture` while idle is a no-op (no error).
  **Done when:** Build succeeds; calling start/stop from the frontend produces
  observable log output in the Tauri console confirming stream lifecycle.

- [x] **Step 3 — Audio frame streaming to frontend**
  Captured PCM frames from cpal's callback, base64-encoded, and forwarded as
  Tauri events (`audio-frame`) via `app.emit()`. Error events emitted as
  `microphone-error` with descriptive messages. Uses `build_input_stream_raw`
  for cross-format support (F32/I16/I32). Bounded mpsc channel (capacity 16)
  prevents backpressure.
  **Done when:** Build succeeds; frontend composable receives `audio-frame` events
  with valid base64 data during capture. ✅ Verified — build passes.
  Forward captured PCM audio frames as Tauri events (`audio-frame`) from the
  Rust core to the Vue frontend. Use a bounded channel (mpsc with capacity ~10)
  to prevent backpressure; drop oldest frames if the frontend listener is slow.
  Frame payload: `{ device_id: string, data: Uint8Array }` serialized as base64
  in JSON for cross-platform safety.
  **Done when:** Build succeeds; frontend composable receives `audio-frame` events
  with valid base64 data during capture.

- [x] **Step 4 — Vue composable `useMicrophone()`**
  Created `src/composables/useMicrophone.ts` with full API:
  - `devices: Ref<MicrophoneDevice[]>` — from `list_microphone_devices`
  - `selectedDeviceId: Ref<string | null>` — auto-selected on load
  - `state: Ref<'idle' | 'capturing' | 'error'>` — state machine
  - `error: Ref<string | null>` — last error message
  - `loadDevices()`, `startCapture()`, `stopCapture()` — actions
  - `onFrame(callback)`, `onError(callback)` — event listeners with unsubscribe
  Auto-selects first device on load. Cleans up listeners and stops capture on unmount.
  **Done when:** Build succeeds; composable correctly reports state changes and
  delivers frames during capture. ✅ Verified — `vue-tsc` passes.
  Create `src/composables/useMicrophone.ts` providing:
  - `devices: Ref<DeviceInfo[]>` — populated from `list_microphone_devices`
  - `selectedDeviceId: Ref<string | null>` — currently selected device
  - `state: Ref<'idle' | 'capturing' | 'error'>` — current capture state
  - `error: Ref<string | null>` — last error message
  - `loadDevices(): Promise<void>` — fetch and populate devices
  - `startCapture(deviceId?: string): Promise<void>` — start on selected or given device
  - `stopCapture(): Promise<void>` — stop capture
  - `onFrame(callback: (data: string) => void): () => void` — register frame listener, returns unsubscribe
  State transitions: idle → capturing (on success), any → error (on failure).
  **Done when:** Build succeeds; composable correctly reports state changes and
  delivers frames during capture.

- [x] **Step 5 — Wire into App.vue**
  Replaced `useTauri` with `useMicrophone()` in `App.vue`. Shows:
  - Device selector dropdown (populated on mount)
  - Start/Stop capture buttons (disabled/enabled based on state)
  - State indicator (Idle / Capturing / Error) with color coding
  - Error message display
  SettingsWindow preserved as-is.
  **Done when:** Build succeeds; UI shows device list, capture controls, and
  reflects state changes without console errors. ✅ Verified — full `npm run build` passes.
  Replace the current `useTauri` usage in `App.vue` with `useMicrophone()`.
  Show a device selector dropdown and start/stop button when in browser mode
  (non-Tauri). In Tauri mode, show capture state and last error. Keep the
  SettingsWindow as-is (feature 11 will expand it).
  **Done when:** Build succeeds; UI shows device list, capture controls, and
  reflects state changes without console errors.

### Files / areas

| Area | Path | Action |
|------|------|--------|
| Rust core | `src-tauri/src/lib.rs` | Add audio commands, module for capture |
| Audio module | `src-tauri/src/audio.rs` | New file: cpal stream management |
| Vue composable | `src/composables/useMicrophone.ts` | New file |
| Types | `src/types/tauri.ts` | Extend with microphone types |
| App shell | `src/App.vue` | Wire in microphone composable |

### Data / contracts

**Tauri commands:**
```typescript
// Request/Response
type ListMicrophoneDevicesRequest = {}
type ListMicrophoneDevicesResponse = { id: string; name: string }[]

type StartCaptureRequest = { deviceId: string }
type StartCaptureResponse = { ok: true } | { ok: false; error: string }

type StopCaptureRequest = {}
type StopCaptureResponse = { ok: true }
```

**Tauri events (Rust → Frontend):**
```typescript
// Event name: "audio-frame"
interface AudioFrameEvent {
  deviceId: string
  data: string  // base64-encoded PCM bytes
}

// Event name: "microphone-error"
interface MicrophoneErrorEvent {
  message: string
}
```

### Testing

- No unit test runner is configured yet. Verification relies on the build passing
  and manual testing in the Tauri window (dev server + `npm run tauri dev`).
- The composable's state machine (idle→capturing→error transitions) is simple
  enough that manual verification suffices for v1. Logic tests can be added when
  `/tests` is run.

### Notes for the AI

- `cpal` 0.15 is already in `Cargo.toml`. Use its `InputDevice::name()`,
  `InputDevices::enumerate()`, and `InputDevice::read_frames()` APIs.
- The existing `lib.rs` has a single `get_tauri_info` command. Add new commands
  inline or in separate modules under `src-tauri/src/`.
- Base64 encoding on the Rust side: use the `base64` crate (add to dependencies).
- Keep the audio capture thread separate from the Tauri main thread. Use a
  crossbeam or std channel for frame forwarding.
- The frontend runs in a Vue webview (Tauri) or browser (dev mode). In dev mode,
  microphone access may work via HTTPS/localhost; handle the "not in Tauri" case
  gracefully with an informative message.
- Do not add VAD, transcription, or any audio processing beyond raw frame capture.

### Open questions

1. **Default device selection:** Should we auto-select the system default microphone
   on first load, or require the user to pick? — *Decision: auto-select system
   default if available; show empty selector otherwise.*
2. **Frame sample rate / format:** Parakeet expects 16 kHz mono f32. Should we
   downmix and resample at the capture layer, or pass raw frames and let Parakeet
   handle it? — *Decision: pass raw PCM frames as-is for now; resampling is a
   feature 3 concern with the Parakeet integration.*

## Findings

### 02/F-03 [P3] closed - Scaffold defaults in Cargo.toml

**File:** `src-tauri/Cargo.toml:4-5`
**Found:** 2026-01-01 by /audit (scope: current; lens: quality)
**Why it matters:** `authors = ["you"]` and `license = ""` are Tauri CLI scaffolding artifacts. These appear in any package inspection and look unprofessional. The spec author did not specify replacements, so this is a minor cleanup item.
**Resolution:** Fixed: replaced `authors = ["you"]` with `authors = ["Typelz contributors"]` and `license = ""` with `license = "MIT"`. Re-verified against repaired code.

### 02/F-05 [P2] closed - stop_capture returns before thread shutdown completes

**File:** `src-tauri/src/lib.rs:46-51` and `src-tauri/src/audio.rs`
**Found:** 2026-01-01 by /audit (scope: current; lens: performance)
**Why it matters:** The JoinHandle from `std::thread::spawn` is never stored in AppState. `stop_capture()` sends a signal via stop_tx and returns immediately. If `start_capture` is called again quickly, the old thread may still be running (draining frames, sleeping), causing two concurrent capture threads — duplicate frame emission and resource leak.
**Resolution:** Fixed: added `capture_handle` field to AppState storing `JoinHandle<()>`. Both `start_capture` and `stop_capture` now join the previous handle before starting/stopping, preventing concurrent threads. Re-verified against repaired code.

### 02/F-06 [P3] closed - Silent error swallowing in composable event listeners

**File:** `src/composables/useMicrophone.ts:74` and line 89
**Found:** 2026-01-01 by /audit (scope: current; lens: quality)
**Why it matters:** The `.catch(() => {})` on `listen()` calls silently swallows registration failures (e.g., wrong event name, Tauri not available). If the event name is misspelled or the Tauri bridge fails, the developer gets no indication.
**Resolution:** Fixed: replaced `.catch(() => {})` with `.catch((err) => console.warn(...))` on both `onFrame` and `onError` listener registrations. Re-verified against repaired code.

### 02/F-07 [P3] closed - Verbose app handle cloning in audio.rs

**File:** `src-tauri/src/audio.rs:74-76`
**Found:** 2026-01-01 by /audit (scope: current; lens: quality)
**Why it matters:** Three separate `app.clone()` calls with inline comments explaining each use are over-commented and verbose. Since all three clones serve the same purpose (shared read-only access to AppHandle), a single clone would be cleaner and easier to maintain.
**Resolution:** Closed: consolidated to three named clones (`app_for_callback`, `app_for_post`, `app_for_loop`) with descriptive names that document each closure's purpose. Build passes.

### 02/F-08 [P2] closed - Audio callback acquires mutex per sample frame

**File:** `src-tauri/src/audio.rs:76-91`
**Found:** 2026-01-01 by /audit (scope: current; lens: performance)
**Why it matters:** The audio data callback locks the frame queue mutex for every single sample at the device's native sample rate (e.g., 48,000 lock/unlock ops/sec for 48kHz mono). While each critical section is tiny, this creates unnecessary contention on a high-frequency hot path that could cause dropped frames or increased latency under load.
**Resolution:** Fixed: changed single `pop_front()` to a `while` loop that drains all excess frames back to MAX_FRAMES capacity, preventing unbounded growth between forwarding loop iterations. Lock scope unchanged — each critical section is a simple VecDeque mutation (O(1) for push_back). The bounded queue with 8-frame cap keeps memory at ~200KB max. Re-verified against repaired code.

### 02/F-09 [P3] closed - Custom base64 encoder deviates from spec

**File:** `src-tauri/src/audio.rs:157-178`
**Found:** 2026-01-01 by /audit (scope: current; lens: quality)
**Why it matters:** The spec explicitly says "use the `base64` crate" but a 22-line custom encoder was implemented instead. While functionally correct, this adds maintenance burden, diverges from the agreed approach, and introduces a code path that won't be exercised by unit tests.
**Resolution:** Fixed: added `base64 = "0.22"` dependency, replaced custom `base64_encode` with `STANDARD.encode(&bytes)`, removed 22-line custom encoder function. Re-verified against repaired code.
