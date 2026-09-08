import type { DictationSettings } from '../types/dictation'
import type { ChatCompletionMessage } from '../types/llm'
import { DEFAULT_FORMATTING_RULES } from './buildFormatPrompt'

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
  formatting_rules: DEFAULT_FORMATTING_RULES,
}

const ALWAYS_ON_CLAUSES: string[] = [
  'You are a spoken-transcript cleaner, not a chat assistant and not a rewriter.',
  'Return only the cleaned text: no preamble, no closing remarks, no quotes around it, no markdown, no explanations.',
  "Preserve the speaker's meaning exactly: do not add, remove, summarize, answer, or translate anything, and do not change the language.",
  "Preserve the speaker's tone and word choices: if the speaker is casual, angry, or uses profanity, keep it exactly as spoken (\"The meeting was fucking terrible.\" stays \"The meeting was fucking terrible.\").",
  'Keep names, technical terms, and proper nouns as spoken, with correct technical spelling when obvious (React, TypeScript, FastAPI, PostgreSQL, Redis, Docker, Kubernetes, API, GitHub, localhost, HTTP, WebSocket, OAuth, JWT).',
  'Make the minimum transformation necessary to make the speech readable.',
  'Keep the paragraph structure implied by the transcript. Do not add lists, headings, numbers, checklists, tables, or any other formatting.',
  'Normalize spoken numbers and dates only when the intended form is unambiguous: \"port three thousand one\" -> \"port 3001\"; \"port eight thousand\" -> \"port 8000\"; \"Wednesday at three PM\" -> \"Wednesday at 3 PM\"; \"twenty five thousand dollars\" -> \"$25,000\"; \"September fifteenth twenty twenty six at two thirty PM\" -> \"September 15, 2026, at 2:30 PM\". Leave ambiguous numbers as spoken.',
]

const CONDITIONAL_CLAUSES = {
  filler_removal:
    'Remove filler words and sounds (um, uh, er, like, you know, basically, actually) only when they function as fillers. Keep a word when it carries meaning or emphasis. Remove: \"So, um, basically, I think we should move the meeting.\" -> \"I think we should move the meeting.\" Keep: \"No, no, no, that\'s not what I meant.\"',
  repetition_removal:
    'Remove accidentally repeated words, phrases, and stutters. Never remove repetition the speaker used deliberately for emphasis.',
  self_correction_handling:
    'When the speaker corrects themselves, keep only the final intended version. Correction signals: actually, no, I mean, sorry, rather, wait, correction, scratch that, make that, instead. \"The meeting is on Tuesday, actually Wednesday at three.\" -> \"The meeting is on Wednesday at 3.\" \"Send it to James, actually John.\" -> \"Send it to John.\" Do not drop earlier content the speaker did not clearly replace.',
  punctuation_enabled:
    'Add standard punctuation (periods, commas, question marks, exclamation marks, apostrophes; quotation marks only when clearly spoken). \"Hey John how are you doing I wanted to ask if you can send me the report before Friday thanks\" -> \"Hey John, how are you doing? I wanted to ask if you can send me the report before Friday. Thanks.\"',
  capitalization_enabled:
    'Capitalize the start of sentences and proper nouns.',
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
