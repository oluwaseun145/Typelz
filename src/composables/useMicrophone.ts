import { ref, onUnmounted } from 'vue'
import { invoke } from '@tauri-apps/api/core'
import { listen } from '@tauri-apps/api/event'
import type { Event, UnlistenFn } from '@tauri-apps/api/event'
import type { MicrophoneDevice, AudioFrameEvent, MicrophoneErrorEvent } from '../types/tauri'

export type MicrophoneState = 'idle' | 'capturing' | 'error'

/**
 * Composable for microphone audio capture.
 *
 * Provides device listing, start/stop capture, and raw audio frame streaming.
 */
export function useMicrophone() {
  const devices = ref<MicrophoneDevice[]>([])
  const selectedDeviceId = ref<string | null>(null)
  const state = ref<MicrophoneState>('idle')
  const error = ref<string | null>(null)

  let unsubscribeFrame: UnlistenFn | undefined
  let unsubscribeError: UnlistenFn | undefined

  /** Fetch available microphone devices from the Tauri backend. */
  async function loadDevices(): Promise<void> {
    try {
      const list = await invoke<MicrophoneDevice[]>('list_microphone_devices')
      devices.value = list
      if (list.length > 0 && !selectedDeviceId.value) {
        selectedDeviceId.value = list[0].id
      }
    } catch (err) {
      const message = err instanceof Error ? err.message : String(err)
      error.value = `Failed to list devices: ${message}`
      state.value = 'error'
    }
  }

  /** Start audio capture on the selected (or given) device. */
  async function startCapture(deviceId?: string): Promise<void> {
    const id = deviceId ?? selectedDeviceId.value
    if (!id) {
      error.value = 'No microphone selected'
      state.value = 'error'
      return
    }

    try {
      state.value = 'capturing'
      error.value = null
      await invoke('start_capture', { deviceId: id })
    } catch (err) {
      const message = err instanceof Error ? err.message : String(err)
      error.value = `Failed to start capture: ${message}`
      state.value = 'error'
    }
  }

  /** Stop audio capture. */
  async function stopCapture(): Promise<void> {
    try {
      await invoke('stop_capture')
      state.value = 'idle'
      error.value = null
    } catch (err) {
      const message = err instanceof Error ? err.message : String(err)
      error.value = `Failed to stop capture: ${message}`
      state.value = 'error'
    }
  }

  /** Register a callback for incoming audio frames. Returns an unsubscribe function. */
  function onFrame(callback: (data: string) => void): () => void {
    listen<AudioFrameEvent>('audio-frame', (event: Event<AudioFrameEvent>) => {
      callback(event.payload.data)
    }).then((unlisten: UnlistenFn) => {
      unsubscribeFrame = unlisten
    }).catch((err) => console.warn('Failed to register microphone frame listener:', err))

    return () => {
      unsubscribeFrame?.()
      unsubscribeFrame = undefined
    }
  }

  /** Register a callback for microphone errors. Returns an unsubscribe function. */
  function onError(callback: (message: string) => void): () => void {
    listen<MicrophoneErrorEvent>('microphone-error', (event: Event<MicrophoneErrorEvent>) => {
      error.value = event.payload.message
      state.value = 'error'
      callback(event.payload.message)
    }).then((unlisten: UnlistenFn) => {
      unsubscribeError = unlisten
    }).catch((err) => console.warn('Failed to register microphone error listener:', err))

    return () => {
      unsubscribeError?.()
      unsubscribeError = undefined
    }
  }

  /** Clean up all listeners on unmount. */
  function cleanup(): void {
    unsubscribeFrame?.()
    unsubscribeError?.()
    if (state.value === 'capturing') {
      stopCapture().catch(() => {})
    }
  }

  onUnmounted(cleanup)

  return {
    devices,
    selectedDeviceId,
    state,
    error,
    loadDevices,
    startCapture,
    stopCapture,
    onFrame,
    onError,
  }
}
