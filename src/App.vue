<script setup lang="ts">
import { computed } from 'vue'
import SettingsWindow from './components/settings/SettingsWindow.vue'
import { useMicrophone } from './composables/useMicrophone.ts'
import { useTranscription } from './composables/useTranscription.ts'
import type { ModelStatus } from './types/tauri.ts'

const {
  devices,
  selectedDeviceId,
  state,
  error,
  loadDevices,
  startCapture,
  stopCapture,
  onError,
} = useMicrophone()

const {
  isModelLoading,
  isModelReady,
  lastTranscript,
  modelStatus,
  downloadProgress,
  error: transcriptionError,
  startTranscription,
  stopTranscription,
  onResult,
  onModelStatus,
} = useTranscription()

async function handleStart(): Promise<void> {
  await startCapture(selectedDeviceId.value ?? undefined)
}

async function handleStop(): Promise<void> {
  await stopCapture()
}

// Keep transcript and model progress state current; capture-pipeline errors
// (including transcription failures) surface in the microphone section.
onResult()
onModelStatus()
onError(() => {})

const isModelDownloading = computed(() => isModelLoading.value && downloadProgress.value !== null)

function modelStatusText(status: ModelStatus | null): string {
  if (!status) return 'Unknown'
  if ('Ready' in status) return 'Model ready'
  if ('Downloading' in status) {
    const { downloaded_bytes: d, total_bytes: t } = status.Downloading
    return t > 0
      ? `Downloading (${(d / 1048576).toFixed(1)} / ${(t / 1048576).toFixed(1)} MB)`
      : 'Downloading...'
  }
  if ('NotDownloaded' in status) return `Not downloaded (${status.NotDownloaded.missing_files.join(', ')})`
  if ('Partial' in status) return `Partially downloaded, missing: ${status['Partial'].missing.join(', ')}`
  return status.Error.message
}

// Load devices on mount.
loadDevices().catch(() => {})
</script>

<template>
  <div class="app-shell">
    <!-- Microphone capture section -->
    <section class="mic-section">
      <h2>Microphone Capture</h2>

      <div v-if="devices.length === 0 && !error" class="status">Loading devices...</div>

      <template v-else>
        <div class="device-selector">
          <label for="mic-device">Device:</label>
          <select
            id="mic-device"
            :value="selectedDeviceId ?? ''"
            @change="selectedDeviceId = ($event.target as HTMLSelectElement).value || null"
          >
            <option value="" disabled>-- Select a microphone --</option>
            <option
              v-for="device in devices"
              :key="device.id"
              :value="device.id"
            >
              {{ device.name }}
            </option>
          </select>
        </div>

        <div class="controls">
          <button
            class="btn btn-primary"
            :disabled="state === 'capturing' || !selectedDeviceId"
            @click="handleStart"
          >
            {{ state === 'capturing' ? 'Capturing...' : 'Start Capture' }}
          </button>
          <button
            class="btn btn-secondary"
            :disabled="state !== 'capturing'"
            @click="handleStop"
          >
            Stop
          </button>
        </div>

        <div class="state-display">
          <span class="state-indicator" :class="state">
            {{ state === 'idle' ? '\u25cf Idle' : state === 'capturing' ? '\u25cf Capturing' : '\u25cf Error' }}
          </span>
        </div>

        <div v-if="error" class="error-display">
          <p class="error-text">{{ error }}</p>
        </div>
      </template>
    </section>

    <!-- Transcription section -->
    <section class="transcribe-section">
      <h2>Transcribe</h2>

      <div class="model-status">
        <span class="state-indicator" :class="{ ready: isModelReady, downloading: isModelDownloading }">
          {{ modelStatusText(modelStatus) }}
        </span>
      </div>

      <div v-if="isModelDownloading && downloadProgress" class="status">
        Downloading {{ downloadProgress.file ?? 'model file' }}...
        {{ downloadProgress.total > 0
          ? Math.round((downloadProgress.downloaded / downloadProgress.total) * 100) + '%'
          : '...' }}
      </div>

      <div class="controls">
        <button
          class="btn btn-primary"
          :disabled="isModelLoading || isModelReady"
          @click="startTranscription"
        >
          {{ isModelLoading ? 'Loading model...' : isModelReady ? 'Model ready' : 'Start Transcription' }}
        </button>
        <button
          class="btn btn-secondary"
          :disabled="!isModelReady"
          @click="stopTranscription"
        >
          Stop
        </button>
      </div>

      <p class="hint">
        Press Start Transcription to load the model, then use the microphone
        controls above to record. Press Stop when done to get the transcript.
      </p>

      <div v-if="transcriptionError" class="error-display">
        <p class="error-text">{{ transcriptionError }}</p>
      </div>

      <div class="transcript-output" :class="{ empty: lastTranscript.length === 0 }">
        {{ lastTranscript || 'No transcript yet.' }}
      </div>
    </section>

    <!-- Settings (from feature 1) -->
    <SettingsWindow />
  </div>
</template>

<style scoped>
.app-shell {
  display: flex;
  flex-direction: column;
  align-items: center;
  min-height: 100svh;
  padding: 32px;
  gap: 32px;
}

.mic-section {
  width: 100%;
  max-width: 480px;
  padding: 24px;
  border: 1px solid var(--border);
  border-radius: 8px;
}

.mic-section h2 {
  margin: 0 0 16px;
  font-size: 18px;
  color: var(--text-h);
}

.device-selector {
  margin-bottom: 16px;
}

.device-selector label {
  display: block;
  margin-bottom: 4px;
  font-size: 14px;
  color: var(--text);
}

.device-selector select {
  width: 100%;
  padding: 8px 12px;
  border: 1px solid var(--border);
  border-radius: 4px;
  background: var(--bg);
  color: var(--text);
  font-size: 14px;
}

.controls {
  display: flex;
  gap: 8px;
  margin-bottom: 16px;
}

.btn {
  padding: 8px 16px;
  border: none;
  border-radius: 4px;
  font-size: 14px;
  cursor: pointer;
  transition: opacity 0.2s;
}

.btn:disabled {
  opacity: 0.5;
  cursor: not-allowed;
}

.btn-primary {
  background: var(--primary);
  color: white;
}

.btn-secondary {
  background: var(--border);
  color: var(--text);
}

.state-display {
  margin-bottom: 8px;
}

.state-indicator {
  font-size: 14px;
  padding: 4px 8px;
  border-radius: 4px;
}

.state-indicator.idle {
  color: var(--text);
  opacity: 0.6;
}

.state-indicator.capturing {
  color: var(--primary);
  font-weight: 500;
}

.state-indicator.error {
  color: #e53e3e;
}

.error-display {
  margin-top: 8px;
}

.error-text {
  font-size: 13px;
  color: #e53e3e;
  background: #fff5f5;
  padding: 8px 12px;
  border-radius: 4px;
  margin: 0;
}

.status {
  color: var(--text);
  opacity: 0.7;
  font-size: 14px;
}

.transcribe-section {
  width: 100%;
  max-width: 480px;
  padding: 24px;
  border: 1px solid var(--border);
  border-radius: 8px;
}

.transcribe-section h2 {
  margin: 0 0 16px;
  font-size: 18px;
  color: var(--text-h);
}

.model-status {
  margin-bottom: 16px;
}

.state-indicator.ready {
  color: var(--primary);
  font-weight: 500;
}

.state-indicator.downloading {
  color: var(--text);
  opacity: 0.8;
}

.hint {
  margin: 0 0 16px;
  font-size: 13px;
  color: var(--text);
  opacity: 0.7;
}

.transcript-output {
  padding: 12px;
  border: 1px solid var(--border);
  border-radius: 4px;
  background: var(--bg);
  color: var(--text);
  font-size: 14px;
  line-height: 1.5;
  white-space: pre-wrap;
  min-height: 64px;
}

.transcript-output.empty {
  opacity: 0.6;
}
</style>
