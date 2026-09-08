import { ref } from 'vue'
import { useChatCompletion } from './useChatCompletion'
import { buildCleanupMessages, DEFAULT_DICTATION_SETTINGS } from '../lib/buildCleanupPrompt'
import { evaluateCleanupResult } from '../lib/evaluateCleanupResponse'
import type {
  CleanupFallbackReason,
  CleanupResult,
  CleanupState,
  DictationSettings,
} from '../types/dictation'

function toMessage(error: unknown, fallback: string): string {
  if (error instanceof Error && error.message) {
    return error.message
  }
  const raw = String(error).trim()
  return raw || fallback
}

/**
 * Cleanup stage for the dictation pipeline: raw transcript in, final text out.
 * Never loses the dictation: every failure path lands on the raw transcript
 * with a fallback reason and, when useful, an actionable error message.
 */
export function useDictationCleanup() {
  const { sendCompletion, error: providerError } = useChatCompletion()
  const state = ref<CleanupState>('idle')
  /** The transcript that was submitted for cleanup (trimmed). */
  const rawTranscript = ref('')
  /** Final text to use: cleaned when cleanup succeeded, else the raw transcript. */
  const cleanedText = ref('')
  const isFallback = ref(false)
  const fallbackReason = ref<CleanupFallbackReason | null>(null)
  const error = ref<string | null>(null)

  function finishNow(result: CleanupResult, message: string | null): CleanupResult {
    cleanedText.value = result.text
    isFallback.value = result.source === 'raw' && result.reason !== null
    fallbackReason.value = result.reason
    if (message !== null) {
      error.value = message
    }
    state.value = 'done'
    return result
  }

  async function cleanTranscript(
    providerId: string,
    transcript: string,
    settings: DictationSettings = DEFAULT_DICTATION_SETTINGS,
  ): Promise<CleanupResult> {
    state.value = 'cleaning'
    error.value = null
    isFallback.value = false
    fallbackReason.value = null
    rawTranscript.value = transcript.trim()

    if (!providerId) {
      return finishNow(
        { text: rawTranscript.value, source: 'raw', reason: 'no-provider' },
        'No provider is configured. Add one in the LLM Providers section first.',
      )
    }

    if (!settings.cleanup_enabled || rawTranscript.value === '') {
      return finishNow({ text: rawTranscript.value, source: 'raw', reason: null }, null)
    }

    const messages = buildCleanupMessages(rawTranscript.value, settings)
    const response = await sendCompletion(providerId, messages, { temperature: 0 })
    if (response === null) {
      return finishNow(
        { text: rawTranscript.value, source: 'raw', reason: 'provider-error' },
        toMessage(providerError.value, 'Could not reach the LLM provider'),
      )
    }
    return finishNow(evaluateCleanupResult(response, rawTranscript.value), null)
  }

  function reset(): void {
    state.value = 'idle'
    rawTranscript.value = ''
    cleanedText.value = ''
    isFallback.value = false
    fallbackReason.value = null
    error.value = null
  }

  return {
    state,
    rawTranscript,
    cleanedText,
    isFallback,
    fallbackReason,
    error,
    cleanTranscript,
    reset,
  }
}
