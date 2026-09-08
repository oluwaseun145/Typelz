import { describe, expect, it } from 'vitest'
import { buildFormatMessages, DEFAULT_FORMATTING_RULES, formattingEnabled } from './buildFormatPrompt'
import type { FormattingRules } from '../types/dictation'

function systemMessage(rules: FormattingRules): string {
  return buildFormatMessages('Hello.', rules)[0].content
}

describe('buildFormatMessages', () => {
  it('is a pure function of its inputs', () => {
    const a = buildFormatMessages('Hello.', DEFAULT_FORMATTING_RULES)
    const b = buildFormatMessages('Hello.', DEFAULT_FORMATTING_RULES)
    expect(a).toEqual(b)
  })

  it('keeps the user message to the trimmed input only', () => {
    const messages = buildFormatMessages('  Hello there.  ', DEFAULT_FORMATTING_RULES)
    expect(messages).toHaveLength(2)
    expect(messages[1]).toEqual({ role: 'user', content: 'Hello there.' })
  })

  it('carries the never-invent-structure contract', () => {
    const msg = systemMessage(DEFAULT_FORMATTING_RULES)
    expect(msg).toContain('Never invent structure')
    expect(msg).toContain('Default to preserving')
  })

  it('carries strong-signal evidence rules (Tests B/D)', () => {
    const msg = systemMessage(DEFAULT_FORMATTING_RULES)
    expect(msg).toContain('explicit count')
    expect(msg).toContain('I need milk, bread, and eggs.')
  })

  it('carries the anti-over-formatting rule for narrative prose (Test E)', () => {
    const msg = systemMessage(DEFAULT_FORMATTING_RULES)
    expect(msg).toContain('I went to the store and bought milk, bread, and eggs.')
  })

  it('carries the paragraph discipline rule (Test H)', () => {
    const msg = systemMessage(DEFAULT_FORMATTING_RULES)
    expect(msg).toContain('one-sentence paragraphs')
  })

  it('names disabled structures in a "Do not use" clause', () => {
    const rules: FormattingRules = {
      ...DEFAULT_FORMATTING_RULES,
      bullet_lists_enabled: false,
      numbered_lists_enabled: false,
    }
    const msg = systemMessage(rules)
    expect(msg).toContain('Do not use: bullet lists, numbered lists.')
  })

  it('omits the "Do not use" clause when every structure is enabled', () => {
    const msg = systemMessage(DEFAULT_FORMATTING_RULES)
    expect(msg).not.toContain('Do not use')
  })

  it('reports formattingEnabled correctly', () => {
    expect(formattingEnabled(DEFAULT_FORMATTING_RULES)).toBe(true)
    expect(
      formattingEnabled({
        paragraphs_enabled: false,
        bullet_lists_enabled: false,
        numbered_lists_enabled: false,
        headings_enabled: false,
        checklists_enabled: false,
      }),
    ).toBe(false)
  })
})
