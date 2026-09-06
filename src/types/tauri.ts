/** Shape of the getTauriInfo command response. */
export interface TauriInfo {
  version: string
  platform: string
}

/** State event broadcast from the Rust core. */
export interface AppState {
  status: 'idle' | 'loading' | 'ready'
}

/** An available microphone device (returned by list_microphone_devices). */
export interface MicrophoneDevice {
  id: string
  name: string
}

/** Audio frame event payload (base64-encoded PCM bytes). */
export interface AudioFrameEvent {
  data: string
}

/** Error event from the microphone capture backend. */
export interface MicrophoneErrorEvent {
  message: string
}
