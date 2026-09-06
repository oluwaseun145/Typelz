import { ref, onMounted, onUnmounted } from 'vue'
import { invoke } from '@tauri-apps/api/core'
import { listen } from '@tauri-apps/api/event'
import type { TauriInfo, AppState } from '../types/tauri'

export function useTauri() {
  const info = ref<TauriInfo | null>(null)
  const state = ref<AppState | null>(null)
  const loading = ref(true)
  const error = ref<string | null>(null)

  let unsubscribe: (() => void) | undefined

  async function fetchInfo() {
    try {
      info.value = await invoke<TauriInfo>('get_tauri_info')
    } catch (err) {
      const message = err instanceof Error ? err.message : String(err)
      error.value = `Tauri command failed: ${message}`
    } finally {
      loading.value = false
    }
  }

  async function init() {
    loading.value = true
    await fetchInfo()

    try {
      unsubscribe = await listen<AppState>('state-change', (event) => {
        state.value = event.payload
      })
    } catch (err) {
      const message = err instanceof Error ? err.message : String(err)
      error.value = `Failed to listen for events: ${message}`
    }
  }

  onMounted(init)
  onUnmounted(() => unsubscribe?.())

  return { info, state, loading, error, init }
}
