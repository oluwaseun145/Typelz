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

/** Transcription result event payload (emitted after each transcribed utterance). */
export interface TranscriptionResult {
  transcript: string
}

/**
 * Model cache status returned by get_model_status.
 * Matches the Rust `ModelStatus` enum, serialized externally-tagged by serde.
 */
export type ModelStatus =
  | { Ready: { path: string } }
  | { Downloading: { downloaded_bytes: number; total_bytes: number } }
  | { NotDownloaded: { missing_files: string[] } }
  | { 'Partial': { present: string[]; missing: string[] } }
  | { Error: { message: string } }

/** model-status event payload emitted while the model downloads. */
export interface ModelStatusEvent {
  status: string
  file?: string
  progress?: {
    downloaded: number
    total: number
  }
}

/** Live download progress for a single model file. */
export interface DownloadProgress {
  file: string | null
  downloaded: number
  total: number
}
