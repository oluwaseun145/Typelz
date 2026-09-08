import { describe, expect, it } from 'vitest'
import { buildCleanupMessages, DEFAULT_DICTATION_SETTINGS } from './buildCleanupPrompt'
import type { DictationSettings } from '../types/dictation'

function systemMessage(settings: DictationSettings): string {
  return buildCleanupMessages('Hello.', settings)[0].content
}

describe('buildCleanupMessages', () => {
  it('is a pure function of its inputs', () => {
    const a = buildCleanupMessages('Hello.', DEFAULT_DICTATION_SETTINGS)
    const b = buildCleanupMessages('Hello.', DEFAULT_DICTATION_SETTINGS)
    expect(a).toEqual(b)
  })

  it('keeps the user message to the trimmed transcript only', () => {
    const messages = buildCleanupMessages('  Hello there.  ', DEFAULT_DICTATION_SETTINGS)
    expect(messages).toHaveLength(2)
    expect(messages[1]).toEqual({ role: 'user', content: 'Hello there.' })
  })

  it('carries the self-correction clause (Test C)', () => {
    const msg = systemMessage(DEFAULT_DICTATION_SETTINGS)
    expect(msg).toContain('corrects themselves')
    expect(msg).toContain('actually')
    expect(msg).toContain('scratch that')
  })

  it('carries the spoken-numbers-and-dates clause (Test F)', () => {
    const msg = systemMessage(DEFAULT_DICTATION_SETTINGS)
    expect(msg).toContain('port 3001')
    expect(msg).toContain('port 8000')
    expect(msg).toContain('Wednesday at 3 PM')
    expect(msg).toContain('$25,000')
  })

  it('carries the tone-preservation clause (Test G)', () => {
    const msg = systemMessage(DEFAULT_DICTATION_SETTINGS)
    expect(msg).toContain('fucking terrible')
    expect(msg).toContain('profanity')
  })

  it('carries the filler-removal clause with examples', () => {
    const msg = systemMessage(DEFAULT_DICTATION_SETTINGS)
    expect(msg).toContain('um, uh')
    expect(msg).toContain('No, no, no')
  })

  it('omits a conditional clause when its setting is off', () => {
    const msg = systemMessage({ ...DEFAULT_DICTATION_SETTINGS, filler_removal: false })
    expect(msg).not.toContain('um, uh')
    expect(msg).not.toContain('No, no, no')
  })

  it('keeps always-on clauses regardless of settings', () => {
    const allOff: DictationSettings = {
      ...DEFAULT_DICTATION_SETTINGS,
      filler_removal: false,
      repetition_removal: false,
      self_correction_handling: false,
      punctuation_enabled: false,
      capitalization_enabled: false,
    }
    const msg = systemMessage(allOff)
    expect(msg).toContain('not a rewriter')
    expect(msg).toContain('port 3001')
    expect(msg).toContain('fucking terrible')
  })
})
