import { formattingEnabled } from './buildFormatPrompt'
import type { FormattingRules } from '../types/dictation'

/// How strongly the input text signals that the speaker intended a list.
/// The formatting stage may only introduce structure on strong evidence; on
/// weak evidence it must keep prose (rule 14: confidence-based formatting).
export type ListSignal = 'strong' | 'weak'

const EXPLICIT_COUNT: RegExp =
  /\b(?:\d+|one|two|three|four|five|six|seven|eight|nine|ten)\s+(?:things|items|tasks|points|steps|reasons|ideas)\b/i
const ENUMERATION_INTRO: RegExp = /\bthe following\b/i
const ORDERED_MARKERS: RegExp =
  /\b(?:first|second|third|fourth|fifth|number one|number two|number three)\b/i
const TASK_LIST_INTRO: RegExp =
  /\b(?:things?\s+(?:i|we)\s+(?:need to do|have to do|must do)|to\s*do\s*list|my\s+(?:to\s*do|tasks?))\b/i
const NEED_VERBS: ReadonlyArray<string> = [
  'i need',
  'i have to',
  'i must',
  "i've got to",
  'i want',
  'we need',
  'we have to',
  'we must',
]
const CLAUSE_MARKERS: ReadonlyArray<string> = [
  ' because ',
  ' since ',
  ' so that ',
  ' and then ',
  ' and i ',
  ' and we ',
  ' when ',
  ' while ',
  ' before ',
  ' after ',
  ' although ',
  ' if ',
  ' which ',
  ' who ',
]

/// Classifies whether the input text strongly signals an intended list.
/// Strong signals are explicit counts, ordered markers, a task-list
/// introduction, or a bare need-list utterance (the whole sentence is just
/// "I need X, Y, and Z"). Everything else, including narrative sentences that
/// happen to contain an enumeration, is weak.
export function listSignal(text: string): ListSignal {
  const normalized = text.trim()
  if (normalized === '') {
    return 'weak'
  }

  if (
    EXPLICIT_COUNT.test(normalized) ||
    ENUMERATION_INTRO.test(normalized) ||
    ORDERED_MARKERS.test(normalized) ||
    TASK_LIST_INTRO.test(normalized)
  ) {
    return 'strong'
  }

  // Bare need-list: the whole utterance is a single need-verb sentence whose
  // only content is a comma-separated list of items, with no subordinate
  // clauses that would make it ordinary prose.
  const lower = normalized.toLowerCase()
  const matchedNeed = NEED_VERBS.find((verb) => lower.startsWith(verb))
  if (matchedNeed !== undefined) {
    const remainder = normalized.slice(matchedNeed.length).trim()
    if (remainder.length > 0 && !CLAUSE_MARKERS.some((m) => lower.includes(m))) {
      const items = remainder
        .replace(/^(?:to|and|&)\s+/, '')
        .split(/,\s*(?:and\s+|&)?|,\s*|\s+and\s+|\s+&\s+/)
        .map((item) => item.replace(/[.!?]+$/, '').trim())
        .filter((item) => item.length > 0)
      if (items.length >= 2) {
        return 'strong'
      }
    }
  }

  return 'weak'
}

const LIST_MARKER: RegExp = /^\s*(?:[-*]|\d+\.|[-*]\s*\[[ xX]\])\s+/m
const HEADING_MARKER: RegExp = /^#{1,6}\s+/m

/// True when the formatted output introduces list, checklist, numbered-list,
/// or heading structure that was not a plain paragraph.
export function introducesStructure(text: string): boolean {
  return LIST_MARKER.test(text) || HEADING_MARKER.test(text)
}

/// Strips list / checklist / numbered-list / heading markers back into plain
/// prose lines. Used when the formatting stage invented structure the input
/// did not support.
function stripStructure(text: string): string {
  const lines = text.split('\n')
  const cleaned = lines.map((line) =>
    line
      .replace(/^\s*(?:[-*]|\d+\.|[-*]\s*\[[ xX]\])\s+/, '')
      .replace(/^#{1,6}\s+/, '')
      .trim(),
  )
  return cleaned.filter((line) => line.length > 0).join('\n')
}

/// The outcome of applying the formatting guardrail to a stage-2 output.
export interface FormatGuardrailResult {
  /// The text to carry forward (guarded output, or the input on revert).
  text: string
  /// True when the guardrail reverted invented structure back to prose.
  reverted: boolean
}

/// Applies the formatting guardrail to the stage-2 output.
///
/// If the output introduces list / checklist / numbered-list / heading
/// structure but the input only weakly signals a list, the invented structure
/// is stripped back to prose so the output never carries more structure than
/// the input's evidence supports. Otherwise the output is passed through.
export function applyFormatGuardrail(
  input: string,
  llmOutput: string,
  rules: FormattingRules,
): FormatGuardrailResult {
  if (!formattingEnabled(rules)) {
    return { text: llmOutput, reverted: false }
  }
  if (!introducesStructure(llmOutput)) {
    return { text: llmOutput, reverted: false }
  }
  if (listSignal(input) === 'strong') {
    return { text: llmOutput, reverted: false }
  }
  return { text: stripStructure(llmOutput), reverted: true }
}
