import type { CleanupResult } from '../types/dictation'
import type { NormalizedLlmResponse } from '../types/llm'

/**
 * Decide cleaned vs raw fallback for a normalized provider response.
 * Pure and deterministic; implements the feature spec's decision table. The
 * reasoning channel is never merged into the returned text.
 */
export function evaluateCleanupResult(
  response: NormalizedLlmResponse,
  rawTranscript: string,
): CleanupResult {
  // Truncation and content filtering fall back even when partial text exists:
  // a cut-off cleaned transcript is worse than the raw one.
  if (response.finish_reason === 'length') {
    return { text: rawTranscript, source: 'raw', reason: 'truncated' }
  }
  if (response.finish_reason === 'content_filter') {
    return { text: rawTranscript, source: 'raw', reason: 'content-filter' }
  }
  if (response.text.trim() === '') {
    return { text: rawTranscript, source: 'raw', reason: 'empty-response' }
  }
  return { text: response.text.trim(), source: 'cleaned', reason: null }
}
