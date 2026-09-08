import type { FormatStageResult } from '../types/dictation'
import type { NormalizedLlmResponse } from '../types/llm'

/**
 * Decide applied vs carry-through for a normalized provider response at the
 * formatting stage. Pure and deterministic; implements the feature spec's
 * decision table. The reasoning channel is never merged into the output.
 */
export function evaluateFormatResult(
  response: NormalizedLlmResponse,
  inputText: string,
): FormatStageResult {
  // Truncation and content filtering carry through even when partial text
  // exists: a cut-off formatted transcript is worse than the input as given.
  if (response.finish_reason === 'length') {
    return { text: inputText, applied: false, reason: 'truncated' }
  }
  if (response.finish_reason === 'content_filter') {
    return { text: inputText, applied: false, reason: 'content-filter' }
  }
  if (response.text.trim() === '') {
    return { text: inputText, applied: false, reason: 'empty-response' }
  }
  return { text: response.text.trim(), applied: true, reason: null }
}
