# Typelz

System-wide AI voice dictation for Windows and Linux: local Parakeet speech
recognition plus a BYOK LLM that cleans and formats your speech before it is
inserted into the active application.

> TODO: expand once the project plan is finalized and the first features land.

## Commands

- Dev server: `npm run dev` (http://localhost:5173)
- Build: `npm run build`
- Preview production build: `npm run preview`

## Stack

Vue 3 + TypeScript + Vite. A Tauri (Rust) native layer is planned for
microphone capture, global hotkeys, and system-wide text insertion.
