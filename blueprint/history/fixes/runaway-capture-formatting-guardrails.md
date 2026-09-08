# Fix runaway capture loop and guard formatting output

**Type:** Fix
**Status:** verified
**Branch:** `fix/runaway-capture-formatting-guardrails`

## The problem

Two separate problems, one work item.

### A. Capture keeps running after Stop

While the user is not speaking, the terminal keeps emitting:

- `VAD: speech start` / `VAD: utterance end` bursts
- repeated `inference: N samples -> 0 chars` runs
- `skip: utterance too short` spam

Every event appears about ten times per burst, and one burst carries the
"same" utterance at different sample counts (`26720`, `32000`, `15840` in one
group). A single pipeline can only produce one buffer per utterance, so
roughly ten concurrent capture pipelines are running. Nothing stops them.

**Root cause 1 (single pipeline): the stop signal is consumed once.**
`src-tauri/src/audio.rs:416-422`: the forwarding loop checks
`stop_rx.try_recv().is_ok()` each iteration. `try_recv` consumes the single
stop message exactly once. If that iteration also holds queued utterances, the
break is skipped (`stop_requested && utterances.is_empty()`), the next
iteration never sees the signal again, and the loop runs forever. With
ambient noise the utterance queue is rarely empty, so Stop silently fails.

**Root cause 2 (many pipelines): the slot take/replace race orphans
streams.** `start_capture` (src-tauri/src/lib.rs:49-79) takes the slot
handles out under lock and only re-stores them when its `spawn_blocking` work
finishes. A stop that arrives while a start is still in flight takes `None`
and does nothing; a start that begins before the earlier start's late
`replace` lands makes the two replaces drop one pair of
`(stop_tx, handle)` forever. Each interleaved start/stop pair (for example a
Stop pressed during a slow start, then a quick Start) can orphan one capture
stream that no command can stop. Ten cycles of start/stop testing match the
tenfold log bursts. The UI shows Idle in this window because its state tracks
the invoke result, not the backend.

**Contributing (noise sensitivity): the VAD state machine has no
debounce.** `VadDetector::process_frame` (src-tauri/src/vad.rs:79-93)
transitions on a single 10 ms frame: one voice frame flips Silent to
Speeching, one non-voice frame flips Speeching to Boundary.
`webrtc-vad` 0.4.0 exposes only `Quality` (0) and `LowBitrate` (1), and the
code already uses the stricter of the two, so there is no more aggressive
mode available in this crate version. There is no consecutive-frame
confirmation and no energy gate, so fan or keyboard blips become 0.1-4 s
"utterances". The 300 ms minimum skips the shortest blips from inference but
not the log flood, and 0.3-4 s noise runs burn CPU decoding 0 chars.

**Contributing (likely, unverified live): 4-channel downmix attenuation.**
`downmix_to_mono` (src-tauri/src/audio.rs:152-160) averages all channels. If
the mic element sits in only one of the four channels of this machine's
Realtek device, genuine speech arrives about 12 dB quieter, which biases the
VAD against weak speech and the model against it. Needs a real dictation check
to confirm; no app was running in this session.

### B. The formatting stage can invent structure and has no regression guard

The Formatting & Cleanup stage (build-plan items 7-8) runs two sequential LLM
calls (cleanup, then formatting on the stage-1 output, both temperature 0).
All behavior lives in two prompt strings with no local decision logic and no
tests. The formatting stage is explicitly instructed to produce lists,
checklists, and headings, and the only guard is the applied/fallback decision
(truncated, content-filter, empty). Nothing detects that the speaker never
expressed a list, there is no confidence signal, and no regression test can
catch over-formatting, tone rewrites, or self-correction mishandling.

Pipeline inspection, where each behavior lives today:

| Concern | Where |
| --- | --- |
| Filler removal | LLM stage-1 prompt, `src/lib/buildCleanupPrompt.ts` (`filler_removal` clause, settings-gated) |
| Repetition removal | LLM stage-1 prompt, same file (`repetition_removal` clause) |
| Self-correction | LLM stage-1 prompt, same file (`self_correction_handling` clause) |
| Punctuation | LLM stage-1 prompt (`punctuation_enabled`), plus a raw pre-pass in Rust: `format_transcript` in `src-tauri/src/transcribe.rs:16-32` (capitalizes first char, appends trailing period) |
| List/paragraph detection | LLM stage-2 prompt, `src/lib/buildFormatPrompt.ts` (`STRUCTURE_CLAUSES`) |
| One model for all transforms? | No, two sequential calls to the user's BYOK provider (same model), orchestrated in `src/composables/useDictationPipeline.ts:65-106` |
| Confidence scores | None. Only binary applied/fallback in `src/lib/evaluateCleanupResponse.ts` and `src/lib/evaluateFormatResult.ts` |
| Regression tests | None for this stage: no JS test runner exists. Rust-side unit tests only (`src-tauri/src/{audio,vad,transcribe}.rs`) |

## The fix

### A. Rust capture

1. **Sticky stop flag.** In the audio forwarding loop, replace the one-shot
   `try_recv` check with a persistent boolean: consume the message once, keep
   the flag, and break once it is set and the queues are drained. Preserve the
   existing post-loop flush behavior.
2. **No orphaned streams.** Make the capture slot lifecycle race-free in
   `lib.rs`: a start that is in flight must be visible in the slot (a stop
   then marks the pending start to kill its own stream before publishing its
   handles), or concurrent starts/stops must be rejected with an actionable
   error. Either way: after any interleaving exactly one stream exists, and
   stop always stops it.
3. **VAD debounce.** Require at least 3 consecutive 10 ms voice frames before
   Silent to Speeching, and a short consistent silence tail (about 150 ms)
   before Speeching to Boundary. Keep the existing 300 ms minimum-utterance
   skip.
4. **Dominant-channel downmix (optional step, droppable).** Pick the channel
   with the highest RMS instead of averaging all channels. If review finds
   this out of proportion, drop it without affecting the rest.

Must not break:

- Stop transcript delivery: a non-empty session transcript is delivered
  exactly once (`stop_capture` emits `transcription-result`,
  `transcribe_stop` returns it).
- The trailing flush: speech still being captured when Stop is pressed is
  transcribed (`flush: final utterance`).
- Event contracts: `audio-frame`, `utterance-ready`, `microphone-error`,
  `model-status`, `transcription-result`.
- Existing Rust unit tests, except the `test_downmix_to_mono_*` expectations,
  which may be updated deliberately in the same diff only if step 4 is taken.

### B. Formatting & Cleanup

1. **Stage-1 prompt rewrite** (`src/lib/buildCleanupPrompt.ts`): the model is
   a speech cleaner, not a rewriter; minimum necessary transformation only.
   - Fillers (um, uh, er, like, you know, basically, actually) removed only
     when they function as fillers; keep them when they carry meaning.
   - Repetition removed only when accidental; keep deliberate emphasis
     ("No, no, no, that's not what I meant." stays).
   - Self-corrections: keep the final form via the trigger patterns
     actually, no, I mean, sorry, rather, wait, correction, scratch that,
     make that, instead; never drop earlier content the speaker did not
     clearly replace.
   - Preserve tone (including profanity), meaning, and technical terminology
     verbatim (React, FastAPI, PostgreSQL, localhost, JWT, ...).
   - Fix punctuation and capitalization; normalize spoken numbers and dates
     only when unambiguous (port three thousand one to 3001; Wednesday at
     three PM to Wednesday at 3 PM; twenty five thousand dollars to
     $25,000).
2. **Stage-2 prompt rewrite** (`src/lib/buildFormatPrompt.ts`): never invent
   structure. Lists, numbered lists, checklists, or a heading only on strong
   speaker evidence: explicit counts ("I have three things"), ordered markers
   (first, second, third, number one), "the following", an explicit task-list
   intro ("Things I need to do: ..."), or a bare "I need X, Y, and Z" that is
   the whole utterance. Task-sounding clauses inside ordinary prose stay
   prose ("I went to the store and bought milk, bread, and eggs." never
   becomes a checklist). Paragraph breaks only on a clear subject shift; no
   one-sentence paragraphs. When confidence is anything below high, preserve
   the original structure.
3. **Local guardrail** (new pure module, for example
   `src/lib/format-guardrail.ts`):
   - `listSignal(text)` returns strong or weak: strong only for the evidence
     above; weak for everything else, including narrative sentences that
     contain enumerations.
   - `applyFormatGuardrail(input, llmOutput, rules)`: if the output
     introduces list, checklist, numbered-list, or heading markers while the
     input's signal is weak, strip those markers back to prose; otherwise
     pass the output through. The output may never carry more structure than
     the input's evidence supports.
   - Wire it into stage 2 of `useDictationPipeline.ts`. Add a `'reverted'`
     value to `StageStatus` in `src/types/dictation.ts` and a status-line
     label in `App.vue` so a guardrail revert is visible.
4. **Tests.** Install Vitest per the Testing section of
   `blueprint/context/coding-standards.md` (test script in `package.json`,
   Commands section of `AGENTS.md` updated), then add the regression cases:
   - A: task-sounding two-clause prose; LLM output shaped as two checklist
     items; guardrail returns one prose paragraph.
   - B: "three things" intro with three items; strong signal; the LLM's
     numbered list survives.
   - D: "I need milk, bread, and eggs."; strong signal; list survives.
   - E: "I went to the store and bought milk, bread, and eggs."; weak
     signal; a list-shaped LLM answer is reverted to prose.
   - H: prose with a topic shift and no structural markers; guardrail passes
     it through unchanged (never invents structure).
   - C (self-correction), F (numbers), G (tone): LLM behavior, not
     unit-testable without a live BYOK provider. Cover with prompt-contract
     tests (the stage prompts contain the corresponding rule and example)
     plus the manual live run below.
   - Signal-detector unit tests classifying the A, B, D, E, H inputs
     strong/weak as above.

Must not break:

- The carry-through guarantee: on any stage failure (or no provider) the
  input text still arrives as `finalText`.
- The decision tables in `evaluateCleanupResult` / `evaluateFormatResult`
  (truncation, content-filter, empty).
- The pure, deterministic prompt functions `buildCleanupMessages` /
  `buildFormatMessages` (same input, same messages).
- The `DictationSettings` / `FormattingRules` switches: each clause stays
  toggleable; the guardrail applies only where the matching rule is enabled.

## Build steps

1. Install Vitest, wire the `test` script, update AGENTS.md Commands.
   Done when: `npm run test` runs and passes; `npm run build` still passes.
2. Rust: sticky stop flag in the audio forwarding loop.
   Done when: `cargo test` passes; with a noisy mic, Stop always ends the
   loop and no log lines follow the stop command's return (manual check).
3. Rust: race-free capture slot in `lib.rs` (no orphans; a stop during an
   in-flight start can never be a silent no-op).
   Done when: five rapid start/stop cycles leave at most one
   `capture started` per stream and no VAD/inference lines after the final
   Stop; `cargo test` passes.
4. Rust: VAD debounce (3 consecutive voice frames before speech start,
   silence tail before boundary).
   Done when: an idle mic for 30 s logs zero `speech start` lines; a real
   spoken word produces exactly one start/end pair.
5. Rust (optional, droppable): dominant-channel downmix; update the
   `test_downmix_to_mono_*` expectations in the same diff if taken.
   Done when: a manual dictation on this machine's 4-channel mic returns a
   non-empty, coherent transcript.
6. Rewrite the stage-1 cleanup prompt per the rules above.
   Done when: `npm run build` passes, prompt-contract tests pass, and the
   live run shows tone and meaning kept intact (no rewrites, profanity and
   technical terms preserved).
7. Rewrite the stage-2 formatting prompt per the rules above.
   Done when: `npm run build` passes, prompt-contract tests pass, and the
   live run keeps non-list inputs as prose.
8. Add `format-guardrail.ts` (signal detector + guardrail), wire it into the
   pipeline with the `'reverted'` status and UI label.
   Done when: tests A, B, D, E, H pass; a live over-formatted LLM answer
   comes back as prose and the UI shows the reverted status.
9. Complete the regression suite (signal-detector cases A/B/D/E/H,
   prompt-contract cases C/F/G).
   Done when: `npm run test` is fully green.

## Verify

- `npm run test` (new runner), `npm run build`, and `cargo test` in
  `src-tauri` all pass.
- Capture: five rapid start/stop cycles, then leave the mic idle for 30 s.
  Expect: no log lines after the final Stop, no `speech start` while idle,
  one start/end pair for a spoken word, transcript delivered once at Stop.
- Live LLM pass (BYOK provider configured, Process Transcript button): run
  cases A through H through the pipeline and inspect the result: A one
  paragraph, B three numbered items, C final corrected form, D a list, E
  prose, F digits (3001/8000), G profanity intact, H natural paragraphs.

## Outcome

Implemented all 9 steps on `fix/runaway-capture-formatting-guardrails`.

- `npm run test` → 38 passed (Vitest). New tests: `format-guardrail.test.ts`
  (regression cases A/B/D/E/H + signal detector), `buildCleanupPrompt.test.ts`
  (prompt contracts C/F/G + settings gating), `buildFormatPrompt.test.ts`
  (structure rules + disabled-structure naming + formattingEnabled),
  `evaluateFormatResult.test.ts` (decision table).
- `npm run build` → OK (vue-tsc + Vite).
- `cargo test --lib` → 96 passed. New tests: forwarding-loop stop latch,
  capture-slot claim/release, VAD debounce + energy gate, dominant-channel
  downmix.

Manual mic checks (steps 2/3/4/5 done-whens) need a running app with your
mic — run `/try latest` after merge for the walkthrough.
