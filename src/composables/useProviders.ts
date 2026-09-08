import { ref } from 'vue'
import { invoke } from '@tauri-apps/api/core'
import type {
  AddProviderInput,
  ProviderSummary,
  ProviderValidationResult,
  TestCredentialsInput,
  UpdateProviderInput,
} from '../types/provider'

function toMessage(error: unknown, fallback: string): string {
  if (error instanceof Error && error.message) {
    return error.message
  }
  const raw = String(error).trim()
  return raw || fallback
}

/** Composable for the BYOK provider configuration commands. */
export function useProviders() {
  const providers = ref<ProviderSummary[]>([])
  const loading = ref(false)
  const error = ref<string | null>(null)

  async function refresh(): Promise<void> {
    loading.value = true
    try {
      providers.value = await invoke<ProviderSummary[]>('list_providers')
      error.value = null
    } catch (err) {
      error.value = toMessage(err, 'Could not load providers')
    } finally {
      loading.value = false
    }
  }

  async function add(input: AddProviderInput): Promise<ProviderSummary | null> {
    try {
      const created = await invoke<ProviderSummary>('add_provider', { input })
      error.value = null
      await refresh()
      return created
    } catch (err) {
      error.value = toMessage(err, 'Could not save the provider')
      return null
    }
  }

  async function update(id: string, input: UpdateProviderInput): Promise<ProviderSummary | null> {
    try {
      const updated = await invoke<ProviderSummary>('update_provider', { id, input })
      error.value = null
      await refresh()
      return updated
    } catch (err) {
      error.value = toMessage(err, 'Could not update the provider')
      return null
    }
  }

  async function remove(id: string): Promise<boolean> {
    try {
      await invoke('remove_provider', { id })
      error.value = null
      await refresh()
      return true
    } catch (err) {
      error.value = toMessage(err, 'Could not remove the provider')
      return false
    }
  }

  async function validate(id: string): Promise<ProviderValidationResult | null> {
    try {
      error.value = null
      return await invoke<ProviderValidationResult>('validate_provider', { id })
    } catch (err) {
      error.value = toMessage(err, 'Could not validate the provider')
      return null
    }
  }

  async function test(input: TestCredentialsInput): Promise<ProviderValidationResult | null> {
    try {
      error.value = null
      return await invoke<ProviderValidationResult>('test_provider_credentials', { input })
    } catch (err) {
      error.value = toMessage(err, 'Could not test the credentials')
      return null
    }
  }

  return {
    providers,
    loading,
    error,
    refresh,
    add,
    update,
    remove,
    validate,
    test,
  }
}
