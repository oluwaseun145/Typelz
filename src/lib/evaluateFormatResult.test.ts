import { describe, expect, it } from 'vitest'
import { evaluateFormatResult } from './evaluateFormatResult'
import type { NormalizedLlmResponse } from '../types/llm'

function response(text: string, finishReason: NormalizedLlmResponse['finish_reason'] = 'stop'): NormalizedLlmResponse {
  return {
    id: 'test',
    model: 'test-model',
    text,
    reasoning: null,
    tool_calls: [],
    usage: { prompt_tokens: 1, completion_tokens: 1, total_tokens: 2 },
    finish_reason: finishReason,
  }
}

describe('evaluateFormatResult', () => {
  it('applies the formatted output on a normal stop', () => {
    const result = evaluateFormatResult(response('- item one\n- item two'), 'Hello.')
    expect(result.applied).toBe(true)
    expect(result.text).toBe('- item one\n- item two')
    expect(result.reason).toBeNull()
  })

  it('carries the input through when the response is truncated', () => {
    const result = evaluateFormatResult(response('cut off', 'length'), 'Hello.')
    expect(result.applied).toBe(false)
    expect(result.text).toBe('Hello.')
    expect(result.reason).toBe('truncated')
  })

  it('carries the input through on content filtering', () => {
    const result = evaluateFormatResult(response('filtered', 'content_filter'), 'Hello.')
    expect(result.applied).toBe(false)
    expect(result.text).toBe('Hello.')
    expect(result.reason).toBe('content-filter')
  })

  it('carries the input through when the response is empty', () => {
    const result = evaluateFormatResult(response('   '), 'Hello.')
    expect(result.applied).toBe(false)
    expect(result.text).toBe('Hello.')
    expect(result.reason).toBe('empty-response')
  })

  it('trims surrounding whitespace from applied output', () => {
    const result = evaluateFormatResult(response('  - item  \n'), 'Hello.')
    expect(result.applied).toBe(true)
    expect(result.text).toBe('- item')
  })
})
