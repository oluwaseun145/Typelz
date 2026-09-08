import { describe, expect, it } from 'vitest'
import {
  applyFormatGuardrail,
  introducesStructure,
  listSignal,
} from './format-guardrail'
import { DEFAULT_FORMATTING_RULES } from './buildFormatPrompt'

const rules = DEFAULT_FORMATTING_RULES

describe('listSignal', () => {
  it('classifies an explicit count introduction as strong', () => {
    expect(listSignal('I need to do three things: send the email, check the server, and call John.')).toBe('strong')
    expect(listSignal('There are two reasons for this.')).toBe('strong')
  })

  it('classifies ordered markers as strong', () => {
    expect(listSignal('First we set up the repo, second we write the tests.')).toBe('strong')
    expect(listSignal('Number one, the budget; number two, the timeline.')).toBe('strong')
  })

  it('classifies a task-list introduction as strong', () => {
    expect(listSignal('Things I need to do: send the report, call John, deploy the app.')).toBe('strong')
  })

  it('classifies a bare need-list utterance as strong', () => {
    expect(listSignal('I need milk, bread, and eggs.')).toBe('strong')
    expect(listSignal('We have to buy paper, ink, and folders.')).toBe('strong')
  })

  it('classifies a narrative sentence containing items as weak', () => {
    expect(listSignal('I went to the store and bought milk, bread, and eggs.')).toBe('weak')
  })

  it('classifies a task-sounding prose sentence as weak', () => {
    expect(
      listSignal('I need to send the email today because the client is waiting and I need to get it out before the end of the day.'),
    ).toBe('weak')
  })

  it('classifies empty text as weak', () => {
    expect(listSignal('')).toBe('weak')
    expect(listSignal('   ')).toBe('weak')
  })
})

describe('introducesStructure', () => {
  it('detects bullet, numbered, checklist, and heading markers', () => {
    expect(introducesStructure('- item one\n- item two')).toBe(true)
    expect(introducesStructure('1. item one\n2. item two')).toBe(true)
    expect(introducesStructure('- [ ] task one\n- [x] task two')).toBe(true)
    expect(introducesStructure('# A heading\nSome text.')).toBe(true)
  })

  it('treats plain paragraphs as structure-free', () => {
    expect(introducesStructure('A plain paragraph of text.')).toBe(false)
    expect(introducesStructure('Line one.\nLine two.')).toBe(false)
  })
})

describe('applyFormatGuardrail', () => {
  it('passes through output that introduces no structure', () => {
    const result = applyFormatGuardrail(
      'I need to send the email today because the client is waiting.',
      'I need to send the email today because the client is waiting.',
      rules,
    )
    expect(result.reverted).toBe(false)
    expect(result.text).toBe('I need to send the email today because the client is waiting.')
  })

  it('reverts invented list structure on weak input (Test A)', () => {
    const input =
      'I need to send the email today because the client is waiting and I need to get it out before the end of the day.'
    const llmOutput = '- Send the email today because the client is waiting\n- Get it out before the end of the day'
    const result = applyFormatGuardrail(input, llmOutput, rules)
    expect(result.reverted).toBe(true)
    expect(result.text).toBe(
      'Send the email today because the client is waiting\nGet it out before the end of the day',
    )
  })

  it('preserves a list the speaker clearly expressed (Test B)', () => {
    const input = 'I need to do three things: send the email, check the server, and call John.'
    const llmOutput = '1. Send the email\n2. Check the server\n3. Call John'
    const result = applyFormatGuardrail(input, llmOutput, rules)
    expect(result.reverted).toBe(false)
    expect(result.text).toBe(llmOutput)
  })

  it('preserves a bare need-list (Test D)', () => {
    const input = 'I need milk, bread, and eggs.'
    const llmOutput = '- Milk\n- Bread\n- Eggs'
    const result = applyFormatGuardrail(input, llmOutput, rules)
    expect(result.reverted).toBe(false)
    expect(result.text).toBe(llmOutput)
  })

  it('reverts a list invented from a narrative sentence (Test E)', () => {
    const input = 'I went to the store and bought milk, bread, and eggs.'
    const llmOutput = '- Milk\n- Bread\n- Eggs'
    const result = applyFormatGuardrail(input, llmOutput, rules)
    expect(result.reverted).toBe(true)
    expect(result.text).toBe('Milk\nBread\nEggs')
  })

  it('passes through prose unchanged (Test H)', () => {
    const input =
      'I wanted to give you a quick update on the project. The authentication system is still being worked on.\n\nWe are expecting everything to be ready by Friday.'
    const result = applyFormatGuardrail(input, input, rules)
    expect(result.reverted).toBe(false)
    expect(result.text).toBe(input)
  })

  it('does not touch output when formatting is disabled', () => {
    const result = applyFormatGuardrail(
      'I went to the store and bought milk, bread, and eggs.',
      '- Milk\n- Bread\n- Eggs',
      { ...rules, bullet_lists_enabled: false, numbered_lists_enabled: false, checklists_enabled: false, headings_enabled: false, paragraphs_enabled: false },
    )
    expect(result.reverted).toBe(false)
    expect(result.text).toBe('- Milk\n- Bread\n- Eggs')
  })
})
