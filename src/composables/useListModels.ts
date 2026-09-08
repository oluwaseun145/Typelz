import { ref } from 'vue'
import { invoke } from '@tauri-apps/api/core'
import type { ListModelResponse } from '../types/llm'

function toMessage(error: unknown, fallback: string): string {
  if (error instanceof Error && error.message) {
    return error.message
  }
  const raw = String(error).trim()
  return raw || fallback
}

/** Composable for listing the models a stored provider reports. */
export function useListModels() {
  const loading = ref(false)
  const error = ref<string | null>(null)
  const models = ref<ListModelResponse[]>([])

  async function listModels(providerId: string): Promise<ListModelResponse[] | null> {
    loading.value = true
    error.value = null
    models.value = []
    try {
      const result = await invoke<ListModelResponse[]>('list_provider_models', {
        providerId,
      })
      models.value = result
      return result
    } catch (err) {
      error.value = toMessage(err, 'Could not list models from the LLM provider')
      return null
    } finally {
      loading.value = false
    }
  }

  return {
    loading,
    error,
    models,
    listModels,
  }
}
