import type { FormattingRules } from '../types/dictation'
import type { ChatCompletionMessage } from '../types/llm'

/**
 * Temporary defaults until the dictation settings feature (build-plan item 11)
 * persists user choices. In-memory only; never stored.
 */
export const DEFAULT_FORMATTING_RULES: FormattingRules = {
  paragraphs_enabled: true,
  bullet_lists_enabled: true,
  numbered_lists_enabled: true,
  headings_enabled: true,
  checklists_enabled: true,
}

const ALWAYS_ON_CLAUSES: string[] = [
  'You are a text formatting engine, not a chat assistant.',
  'Return only the formatted text: no preamble, no closing remarks, no quotes around it, no code fences, no explanations.',
  'Do not change, add, remove, or correct any words. Structure only.',
  'Convert only the structures the speaker actually expressed. Never invent structure or content.',
  'Default to preserving the input structure. Only add structure on strong speaker evidence; when in doubt, keep prose.',
]

const STRUCTURE_CLAUSES = {
  paragraphs_enabled:
    'Insert a paragraph break only where the speaker clearly shifts subject or thought. Never split into one-sentence paragraphs.',
  bullet_lists_enabled:
    'Convert to a Markdown bullet list (- item) only with strong evidence the speaker listed items: an explicit count (\"I need to do three things: ...\"), \"the following\", a bare item list that is the whole utterance (\"I need milk, bread, and eggs.\"), or clear repeated item structure with nothing else. Never convert ordinary prose that merely contains items: \"I went to the store and bought milk, bread, and eggs.\" stays a sentence.',
  numbered_lists_enabled:
    'Convert to a Markdown numbered list (1. item) only when order was expressed: first/second/third, number one/number two, clearly ordered steps.',
  headings_enabled:
    'Use a heading only where the speaker explicitly named a section that governs following content. This is rare in speech; default to no headings.',
  checklists_enabled:
    'Convert to a Markdown checklist (- [ ] item) only when the speaker presented an explicit task list: \"Things I need to do: send the report, call John, deploy the app.\" A single imperative sentence inside prose (\"Send the report to John and tell me...\") is not a checklist.',
} as const

const DISABLED_STRUCTURE_NAMES: Record<keyof FormattingRules, string> = {
  paragraphs_enabled: 'paragraph breaks',
  bullet_lists_enabled: 'bullet lists',
  numbered_lists_enabled: 'numbered lists',
  headings_enabled: 'headings',
  checklists_enabled: 'checklists',
}

/** True when at least one structure type is enabled. */
export function formattingEnabled(rules: FormattingRules): boolean {
  return (
    rules.paragraphs_enabled ||
    rules.bullet_lists_enabled ||
    rules.numbered_lists_enabled ||
    rules.headings_enabled ||
    rules.checklists_enabled
  )
}

/**
 * Pure and deterministic formatting prompt. The system message carries the
 * always-on contract, one clause per enabled structure, and a "Do not use"
 * clause naming the disabled structures (only when at least one is enabled).
 * The user message is the trimmed stage input and nothing else.
 */
export function buildFormatMessages(
  text: string,
  rules: FormattingRules,
): ChatCompletionMessage[] {
  const clauses: string[] = [...ALWAYS_ON_CLAUSES]
  const disabled: string[] = []
  if (rules.paragraphs_enabled) clauses.push(STRUCTURE_CLAUSES.paragraphs_enabled)
  else disabled.push(DISABLED_STRUCTURE_NAMES.paragraphs_enabled)
  if (rules.bullet_lists_enabled) clauses.push(STRUCTURE_CLAUSES.bullet_lists_enabled)
  else disabled.push(DISABLED_STRUCTURE_NAMES.bullet_lists_enabled)
  if (rules.numbered_lists_enabled) clauses.push(STRUCTURE_CLAUSES.numbered_lists_enabled)
  else disabled.push(DISABLED_STRUCTURE_NAMES.numbered_lists_enabled)
  if (rules.headings_enabled) clauses.push(STRUCTURE_CLAUSES.headings_enabled)
  else disabled.push(DISABLED_STRUCTURE_NAMES.headings_enabled)
  if (rules.checklists_enabled) clauses.push(STRUCTURE_CLAUSES.checklists_enabled)
  else disabled.push(DISABLED_STRUCTURE_NAMES.checklists_enabled)

  const hasStructure = clauses.length > ALWAYS_ON_CLAUSES.length
  if (hasStructure && disabled.length > 0) {
    clauses.push(`Do not use: ${disabled.join(', ')}.`)
  }

  return [
    { role: 'system', content: clauses.join('\n') },
    { role: 'user', content: text.trim() },
  ]
}
