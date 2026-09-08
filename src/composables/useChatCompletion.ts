import { ref } from 'vue'
import { invoke } from '@tauri-apps/api/core'
import type { ChatCompletionMessage, NormalizedLlmResponse } from '../types/llm'

/** Optional sampling parameters; omitted fields fall back to provider defaults. */
export type ChatCompletionOptions = {
  temperature?: number
  maxTokens?: number
}

function toMessage(error: unknown, fallback: string): string {
  if (error instanceof Error && error.message) {
    return error.message
  }
  const raw = String(error).trim()
  return raw || fallback
}

/** Composable for the chat completion command against a stored provider. */
export function useChatCompletion() {
  const loading = ref(false)
  const error = ref<string | null>(null)

  async function sendCompletion(
    providerId: string,
    messages: ChatCompletionMessage[],
    options?: ChatCompletionOptions,
  ): Promise<NormalizedLlmResponse | null> {
    loading.value = true
    error.value = null
    try {
      // Keys are camelCase per Tauri's default argument naming; `null` (not
      // `undefined`) keeps the optional keys present for Rust.
      return await invoke<NormalizedLlmResponse>('send_chat_completion', {
        providerId,
        messages,
        temperature: options?.temperature ?? null,
        maxTokens: options?.maxTokens ?? null,
      })
    } catch (err) {
      error.value = toMessage(err, 'Could not reach the LLM provider')
      return null
    } finally {
      loading.value = false
    }
  }

  return {
    loading,
    error,
    sendCompletion,
  }
}
