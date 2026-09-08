import type { DictationSettings } from '../types/dictation'
import type { ChatCompletionMessage } from '../types/llm'

/**
 * Temporary defaults until the dictation settings feature (build-plan item 11)
 * persists user choices. In-memory only; never stored.
 */
export const DEFAULT_DICTATION_SETTINGS: DictationSettings = {
  cleanup_enabled: true,
  filler_removal: true,
  repetition_removal: true,
  self_correction_handling: true,
  punctuation_enabled: true,
  capitalization_enabled: true,
  formatting_rules: {},
}

const ALWAYS_ON_CLAUSES: string[] = [
  'You are a transcription cleanup engine, not a chat assistant.',
  'Return only the cleaned text: no preamble, no closing remarks, no quotes around it, no markdown, no explanations.',
  "Preserve the speaker's meaning exactly: do not add, remove, summarize, answer, or translate anything, and do not change the language. Keep names, technical terms, and proper nouns as spoken.",
  'Fix grammar, spelling, and word order without changing meaning.',
  'Keep the paragraph structure implied by the transcript. Do not add lists, headings, numbers, checklists, tables, or any other formatting.',
]

const CONDITIONAL_CLAUSES = {
  filler_removal:
    'Remove filler words and sounds (for example: um, uh, you know, like) without losing meaning.',
  repetition_removal: 'Remove accidentally repeated words, phrases, and stutters.',
  self_correction_handling:
    'For false starts and self-corrections, drop the abandoned attempt and keep the final corrected form.',
  punctuation_enabled: 'Add standard punctuation.',
  capitalization_enabled: 'Capitalize the start of sentences and proper nouns.',
} as const

/**
 * Pure and deterministic cleanup prompt. The system message carries the always
 * on contract plus the conditional clauses enabled by `settings`; the user
 * message is the trimmed transcript and nothing else.
 */
export function buildCleanupMessages(
  rawTranscript: string,
  settings: DictationSettings,
): ChatCompletionMessage[] {
  const clauses: string[] = [...ALWAYS_ON_CLAUSES]
  if (settings.filler_removal) clauses.push(CONDITIONAL_CLAUSES.filler_removal)
  if (settings.repetition_removal) clauses.push(CONDITIONAL_CLAUSES.repetition_removal)
  if (settings.self_correction_handling) {
    clauses.push(CONDITIONAL_CLAUSES.self_correction_handling)
  }
  if (settings.punctuation_enabled) clauses.push(CONDITIONAL_CLAUSES.punctuation_enabled)
  if (settings.capitalization_enabled) clauses.push(CONDITIONAL_CLAUSES.capitalization_enabled)

  return [
    { role: 'system', content: clauses.join('\n') },
    { role: 'user', content: rawTranscript.trim() },
  ]
}
