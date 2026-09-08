# Coding Standards

> Conventions for Typelz: Vue 3 + TypeScript + Vite, with a planned Tauri
> (Rust) native layer. `TODO` marks a convention not settled yet.

## Framework and rendering

- Vue 3 single-file components with `<script setup>` and the Composition API
- Client-rendered SPA built by Vite; no SSR or server components
- The browser UI is the settings/tray surface of a desktop app. The heavy
  pipeline (audio capture, Parakeet inference, text insertion) lives in the
  Tauri native layer and is reached through typed commands/events, not
  direct web APIs, once that layer exists
- No web-only assumptions for features that belong in the native layer
  (global hotkeys, clipboard insertion, microphone device lists)

## Package manager

- npm, with `package-lock.json` committed
- Add dependencies with `npm install`; never mix lockfiles

## TypeScript

- Strict mode is on by default via `@vue/tsconfig`; `noUnusedLocals` and
  `noUnusedParameters` are enforced
- The build runs `vue-tsc -b`, so type errors fail `npm run build`
- No `any` types - use proper typing or `unknown`
- Define interfaces for all props, Tauri command payloads, provider request/
  response shapes, and data models
- Use `import type` for type-only imports (`verbatimModuleSyntax` is on)
- Shared types live in `src/types/[feature].ts`

## Project structure

- `src/main.ts` - app entry
- `src/App.vue` - root component
- `src/components/[feature]/` - feature components
- `src/composables/` - reusable Composition API logic
- `src/lib/` - pure logic (parsers, formatters, validators, provider adapters)
- `src/types/` - shared type definitions
- Pure logic that a future test gate will cover belongs in `src/lib/`, not
  inside components

## Styling

- Plain CSS today (`src/style.css` plus scoped `<style>` blocks in SFCs)
- No Tailwind or component library yet
- > TODO: styling approach (plain CSS vs Tailwind) is undecided; decide in the
  first UI feature's spec before introducing a framework

## Data access and API boundaries

- No backend and no database yet; the core dictation path is offline-first
- LLM provider calls go directly from the app to the user-selected provider;
  there is no server-owned model access
- API keys live in the OS credential store, never in app data files, logs, or
  the Vite bundle
- > TODO: Tauri command/event contracts are defined in the build-plan item that
  adds the native layer

## Validation and error handling

- Zod for frontend input validation (chosen in the BYOK provider
  configuration spec, build-plan item 4); Rust re-validates every mutation,
  so the UI is not the trust boundary
- Surface provider, microphone, and pipeline errors as actionable,
  human-readable messages; never send audio anywhere
- Distinguish local errors (audio, Parakeet) from provider errors
  (auth, network, timeout) in error handling

## Testing

No test runner is installed; testing is opt-in. Run `/tests` to add Vitest
(wired as a `test` script in `package.json` and the Commands section of
`AGENTS.md`) with a small example test. Adding it is a deliberate step, never
a silent mid-step install.

When `AGENTS.md` declares a `test` command, tests become a gate for
logic-bearing steps:

- **What to test:** pure logic in `src/lib/` - transcript cleanup rules,
  formatters, validators, provider response normalizers. Assertable inputs,
  real edge cases (empty, missing, malformed).
- **What not to test:** components and integration surfaces. Verify those with
  the dev server, screenshots, and the build.
- **The gate:** a step that adds in-scope logic ships a passing test in the
  same reviewable diff; the test command must be green before approval and
  before `/complete` merges.
- An empty suite should fail, not pass.
- Test files live next to source (`feature.test.ts`), run via the project's
  test command.
- Use `vi.mock()` for external dependencies (provider HTTP calls, Tauri
  commands) and `vi.useFakeTimers()` for time-dependent logic.

When `AGENTS.md` declares a `Verify` command, treat it as the umbrella
automated gate: only the checks this project actually has, in order
typecheck, tests, build. `/ci` owns Verify and CI setup.

## Browser Verification

For UI behavior, prefer real browser evidence over reading the code.

- Browser automation is separately opt-in through `/browser-tests`.
- If no Browser tests command is declared, use the dev server, screenshots,
  and the build for UI evidence. Do not add a runner mid-feature.
- Browser evidence is especially important for settings flows that click,
  type, submit, or depend on client-side state.

## Tauri / Rust

> TODO: no Rust code exists yet. When build-plan item 1 adds the native layer,
> record: Rust edition and toolchain, `cargo fmt`/`clippy` requirements,
> command naming conventions, and how errors cross the JS boundary.

## Code Quality

- No commented-out code unless specified
- No unused imports or variables (enforced by tsconfig)
- Keep functions under 50 lines when possible
- Prefer small components and small composables over one large `App.vue`

## Comments

Write code that explains itself; comment only what the code cannot say.
Over-commenting is a common AI tell, so resist it.

- Comment the **why**, not the **what**. Delete any comment that restates the code.
- No banner/header blocks, section dividers, or step-by-step narration of obvious
  code. A file does not need a comment announcing each region.
- A comment earns its place only when it captures something the code can't: a
  non-obvious decision, a gotcha or workaround, why a value is what it is, or
  a link to a spec or issue.
- Prefer self-documenting names and small functions over explanatory comments.
- Keep doc comments minimal: a one-line purpose on an exported type or function is
  plenty; don't write JSDoc that just repeats the signature.
- When in doubt, leave the comment out.

## Writing

- No em dashes (U+2014) in generated content: docs, comments, commit messages,
  READMEs, specs. They read as AI-generated.
- Use a hyphen for `term - description` separators; rephrase prose with commas,
  parentheses, or a colon. Avoid en dashes and the ellipsis character too.
