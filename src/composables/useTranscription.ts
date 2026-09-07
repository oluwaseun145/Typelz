import { ref, onMounted, onUnmounted } from 'vue'
import { invoke } from '@tauri-apps/api/core'
import { listen } from '@tauri-apps/api/event'
import type { Event, UnlistenFn } from '@tauri-apps/api/event'
import type {
  TranscriptionResult,
  ModelStatus,
  ModelStatusEvent,
  DownloadProgress,
} from '../types/tauri'

/**
 * Composable for local Parakeet transcription.
 *
 * Starts transcription mode (downloading/loading the model if needed),
 * collects per-utterance results from the `transcription-result` event, and
 * surfaces model download progress from `model-status` events. Capture itself
 * is controlled by the existing microphone composable.
 */
export function useTranscription() {
  /** True only while `transcribe_start` is in-flight (model downloading / loading). */
  const isModelLoading = ref(false)
  /** True once the backend engine is loaded and ready to transcribe. */
  const isModelReady = ref(false)
  const lastTranscript = ref('')
  const modelStatus = ref<ModelStatus | null>(null)
  const downloadProgress = ref<DownloadProgress | null>(null)
  const error = ref<string | null>(null)

  let unsubscribeResult: UnlistenFn | undefined
  let unsubscribeStatus: UnlistenFn | undefined

  function toError(err: unknown, prefix: string): string {
    const message = err instanceof Error ? err.message : String(err)
    return `${prefix}: ${message}`
  }

  /** Refresh the model cache status from the backend. */
  async function refreshModelStatus(): Promise<void> {
    try {
      modelStatus.value = await invoke<ModelStatus>('get_model_status')
    } catch (err) {
      error.value = toError(err, 'Failed to query model status')
    }
  }

  /** Start transcription mode: downloads/loads the model if needed. */
  async function startTranscription(): Promise<void> {
    if (isModelLoading.value) return
    error.value = null
    isModelLoading.value = true
    // New session: clear the previous session's result so it cannot be
    // mistaken for the upcoming recording's transcript.
    lastTranscript.value = ''
    try {
      await invoke('transcribe_start')
      // Engine is now loaded; loading phase is over.
      isModelReady.value = true
    } catch (err) {
      error.value = toError(err, 'Failed to start transcription')
    } finally {
      // Always clear the loading flag whether the invoke succeeded or failed.
      isModelLoading.value = false
    }
  }

  /** Stop capture (final flush) and record the latest transcript. */
  async function stopTranscription(): Promise<void> {
    if (!isModelReady.value) return
    try {
      lastTranscript.value = await invoke<string>('transcribe_stop')
    } catch (err) {
      error.value = toError(err, 'Failed to stop transcription')
    } finally {
      isModelReady.value = false
    }
  }

  /** Registers a callback for per-utterance transcription results. Returns an unsubscribe function. */
  function onResult(callback?: (result: TranscriptionResult) => void): () => void {
    listen<TranscriptionResult>('transcription-result', (event: Event<TranscriptionResult>) => {
      lastTranscript.value = event.payload.transcript
      callback?.(event.payload)
    }).then((unlisten: UnlistenFn) => {
      unsubscribeResult = unlisten
    }).catch((err) => console.warn('Failed to register transcription listener:', err))

    return () => {
      unsubscribeResult?.()
      unsubscribeResult = undefined
    }
  }

  /** Registers a callback for model download progress events. Returns an unsubscribe function. */
  function onModelStatus(callback?: (status: ModelStatusEvent) => void): () => void {
    listen<ModelStatusEvent>('model-status', (event: Event<ModelStatusEvent>) => {
      const payload = event.payload
      if (payload.status === 'downloading' && payload.progress) {
        downloadProgress.value = {
          file: payload.file ?? null,
          downloaded: payload.progress.downloaded,
          total: payload.progress.total,
        }
      } else if (payload.status === 'ready') {
        downloadProgress.value = null
        // Engine confirmed ready by backend event — clear any residual loading
        // state and sync the frontend model status cache.
        isModelLoading.value = false
        isModelReady.value = true
        refreshModelStatus().catch(() => {})
      }
      callback?.(payload)
    }).then((unlisten: UnlistenFn) => {
      unsubscribeStatus = unlisten
    }).catch((err) => console.warn('Failed to register model status listener:', err))

    return () => {
      unsubscribeStatus?.()
      unsubscribeStatus = undefined
    }
  }

  onMounted(() => {
    refreshModelStatus().catch(() => {})
  })

  onUnmounted(() => {
    unsubscribeResult?.()
    unsubscribeResult = undefined
    unsubscribeStatus?.()
    unsubscribeStatus = undefined
  })

  return {
    isModelLoading,
    isModelReady,
    lastTranscript,
    modelStatus,
    downloadProgress,
    error,
    refreshModelStatus,
    startTranscription,
    stopTranscription,
    onResult,
    onModelStatus,
  }
}
