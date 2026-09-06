# Feature 1: Desktop application shell

**Status:** verified
**Branch:** `feature/desktop-application-shell`
**Completed:** 2026-01-01

## Goal

Establish the Vue + TypeScript desktop application and Tauri integration: a working Tauri project scaffold, a minimal settings window component, a typed command/event channel between the Vue UI and Rust core, and a system tray icon that provides OS-level presence and entry to settings.

This is purely scaffolding — no microphone, transcription, or text insertion yet. The output is a buildable app with Tauri as the native host.

## Design reference

None for this step (scaffolding only; visual design comes later).

## In scope

- Scaffold `src-tauri/` via `npx @tauri-apps/cli init --ci` (Tauri 2.x) and fix bundle identifier to `com.typelz.app`
- Configure `tauri.conf.json`: keep the default settings window, configure tray icon with a minimal PNG placeholder, set app metadata
- Register a `getTauriInfo()` command in Rust that returns version/platform info via the typed IPC surface
- Define TypeScript types for all Tauri command payloads and event shapes in `src/types/tauri.ts`
- Create a composable `useTauri()` in `src/composables/useTauri.ts` that wraps `@tauri-apps/api` commands and event listeners with proper TypeScript typing
- Replace the default scaffolded UI in `App.vue` with a minimal settings window shell (loading state, empty state for when Tauri is not available, error display)
- Add a placeholder Settings component (`src/components/settings/SettingsWindow.vue`) with basic layout
- Wire the Vue app to display Tauri status via the IPC channel (e.g., `getTauriInfo` command returns version/build info)
- Configure the system tray icon via `tauri.conf.json` tray configuration

## Out of scope

- Microphone capture (item 2)
- Parakeet transcription (item 3)
- LLM provider integration (items 4-6)
- Dictation cleanup/formatting (items 7-8)
- Text insertion into active application (item 9)
- Global hotkeys (item 10)
- Real settings controls (items 11, 15-19) — only the window shell exists
- Voice bar overlay (item 12)
- Error handling for pipeline errors (item 13)
- Linux packaging (item 22)

## Build steps

- [x] **Step 1: Scaffold and fix Tauri project**
- [x] **Step 2: Register a `getTauriInfo()` command in Rust**
- [x] **Step 3: Define TypeScript IPC types and composable**
- [x] **Step 4: Wire system tray icon configuration**
- [x] **Step 5: Replace scaffolded UI with settings shell**
- [x] **Step 6: Verify the complete build**

## Files / areas

| Area | Action |
|------|--------|
| `src-tauri/Cargo.toml` | Modify — update description after scaffold |
| `src-tauri/tauri.conf.json` | Modify — fix identifier, add tray config |
| `src-tauri/src/lib.rs` | Modify — add `get_tauri_info` command + registration |
| `src-tauri/icons/tray-icon.png` | Create — copy from scaffolded icon.png |
| `src/types/tauri.ts` | Create — Tauri IPC type definitions |
| `src/composables/useTauri.ts` | Create — Tauri IPC composable |
| `src/App.vue` | Modify — replace scaffolded UI with settings shell |
| `src/components/settings/SettingsWindow.vue` | Create — placeholder settings window |
| `src/components/HelloWorld.vue` | Remove — no longer needed |
| `package.json` | Modify — add `@tauri-apps/api` dependency |

## Data / contracts

### Tauri IPC contract (typed, not yet fully implemented)

**Commands (Vue → Rust):**
- `getTauriInfo()` → `TauriInfo { version: string; platform: string }`

**Events (Rust → Vue):**
- `state-change` → `AppState { status: 'idle' | 'loading' | 'ready' }`

### App metadata
- Bundle identifier: `com.typelz.app`
- Display name: `Typelz`
- Version: `0.1.0`

## Testing

No test runner configured yet (`/tests` not run). Build verified via `npm run build`.

## Notes for the AI

- Tauri 2.x uses `@tauri-apps/api` v2. The `tauri.conf.json` structure has `"app.windows"` (keep the default window) and `"trayIcon"` at top level.
- The scaffolded `src-tauri/src/main.rs` delegates to `app_lib::run()` in `lib.rs`. Add commands in `lib.rs`, not `main.rs`.
- The project uses `verbatimModuleSyntax: true` in tsconfig — all type-only imports must use `import type`.
