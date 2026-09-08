import { ref } from 'vue'
import { useChatCompletion } from './useChatCompletion'
import {
  buildCleanupMessages,
  DEFAULT_DICTATION_SETTINGS,
} from '../lib/buildCleanupPrompt'
import { buildFormatMessages, formattingEnabled } from '../lib/buildFormatPrompt'
import { evaluateCleanupResult } from '../lib/evaluateCleanupResponse'
import { evaluateFormatResult } from '../lib/evaluateFormatResult'
import type { DictationSettings, PipelineState, StageStatus } from '../types/dictation'

function toMessage(error: unknown, fallback: string): string {
  if (error instanceof Error && error.message) {
    return error.message
  }
  const raw = String(error).trim()
  return raw || fallback
}

/**
 * Two-stage dictation pipeline: raw transcript in, final text out. Cleanup
 * runs first, formatting second on the stage-1 output. Never loses the
 * dictation: every stage failure carries its input through, and the provider's
 * actionable message surfaces as `error` (the farther failing stage wins).
 */
export function useDictationPipeline() {
  const { sendCompletion, error: providerError } = useChatCompletion()
  const state = ref<PipelineState>('idle')
  /** Final text after the last stage (cleaned/formatted or carried through). */
  const finalText = ref('')
  const cleanupStatus = ref<StageStatus>('off')
  const formattingStatus = ref<StageStatus>('off')
  const error = ref<string | null>(null)

  async function processTranscript(
    providerId: string,
    transcript: string,
    settings: DictationSettings = DEFAULT_DICTATION_SETTINGS,
  ): Promise<void> {
    state.value = 'processing'
    error.value = null
    cleanupStatus.value = 'off'
    formattingStatus.value = 'off'
    const input = transcript.trim()

    if (!providerId) {
      error.value = 'No provider is configured. Add one in the LLM Providers section first.'
      finalText.value = input
      formattingStatus.value = 'skipped'
      state.value = 'done'
      return
    }

    if (input === '') {
      finalText.value = ''
      formattingStatus.value = 'skipped'
      state.value = 'done'
      return
    }

    let current = input

    // Stage 1: cleanup. A failure carries the raw transcript forward; the
    // formatting stage still runs when enabled (uniform carry-through rule).
    if (settings.cleanup_enabled) {
      const response = await sendCompletion(
        providerId,
        buildCleanupMessages(current, settings),
        { temperature: 0 },
      )
      if (response === null) {
        cleanupStatus.value = 'fallback'
        error.value = toMessage(providerError.value, 'Could not reach the LLM provider')
      } else {
        const result = evaluateCleanupResult(response, current)
        if (result.source === 'cleaned') {
          current = result.text
          cleanupStatus.value = 'applied'
        } else {
          cleanupStatus.value = 'fallback'
        }
      }
    }

    // Stage 2: formatting over the stage-1 output.
    if (formattingEnabled(settings.formatting_rules)) {
      const response = await sendCompletion(
        providerId,
        buildFormatMessages(current, settings.formatting_rules),
        { temperature: 0 },
      )
      if (response === null) {
        formattingStatus.value = 'fallback'
        error.value = toMessage(providerError.value, 'Could not reach the LLM provider')
      } else {
        const result = evaluateFormatResult(response, current)
        if (result.applied) {
          current = result.text
          formattingStatus.value = 'applied'
        } else {
          formattingStatus.value = 'fallback'
        }
      }
    } else {
      formattingStatus.value = 'skipped'
    }

    finalText.value = current
    state.value = 'done'
  }

  function reset(): void {
    state.value = 'idle'
    finalText.value = ''
    cleanupStatus.value = 'off'
    formattingStatus.value = 'off'
    error.value = null
  }

  return {
    state,
    finalText,
    cleanupStatus,
    formattingStatus,
    error,
    processTranscript,
    reset,
  }
}
