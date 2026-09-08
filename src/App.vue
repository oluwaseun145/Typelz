<script setup lang="ts">
import { computed, ref } from 'vue'
import SettingsWindow from './components/settings/SettingsWindow.vue'
import { useMicrophone } from './composables/useMicrophone.ts'
import { useTranscription } from './composables/useTranscription.ts'
import { useProviders } from './composables/useProviders.ts'
import { useDictationPipeline } from './composables/useDictationPipeline.ts'
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

const { providers, refresh: refreshProviders } = useProviders()

const {
  state: pipelineState,
  finalText,
  cleanupStatus,
  formattingStatus,
  error: pipelineError,
  processTranscript,
} = useDictationPipeline()

const selectedProviderId = ref<string | null>(null)

function cleanupStatusLabel(): string {
  if (cleanupStatus.value === 'applied') return 'Cleanup: applied'
  if (cleanupStatus.value === 'fallback') return 'Cleanup: failed - raw transcript carried through'
  return 'Cleanup: off'
}

function formattingStatusLabel(): string {
  if (formattingStatus.value === 'applied') return 'Formatting: applied'
  if (formattingStatus.value === 'fallback') {
    return 'Formatting: failed - previous stage result carried through'
  }
  return 'Formatting: skipped'
}

async function handleProcess(): Promise<void> {
  const providerId = selectedProviderId.value
  if (!providerId) return
  await processTranscript(providerId, lastTranscript.value)
}

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

// Load devices and providers on mount.
loadDevices().catch(() => {})
refreshProviders().catch(() => {})
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

    <!-- Cleanup & Formatting section (build-plan items 7-8) -->
    <section class="cleanup-section">
      <h2>Cleanup &amp; Formatting</h2>

      <div v-if="providers.length === 0" class="status">
        No providers configured yet. Add one in the LLM Providers section below.
      </div>

      <template v-else>
        <div class="device-selector">
          <label for="cleanup-provider">Provider:</label>
          <select
            id="cleanup-provider"
            :value="selectedProviderId ?? ''"
            @change="selectedProviderId = ($event.target as HTMLSelectElement).value || null"
          >
            <option value="" disabled>-- Select a provider --</option>
            <option v-for="provider in providers" :key="provider.id" :value="provider.id">
              {{ provider.name }}
            </option>
          </select>
        </div>

        <div class="controls">
          <button
            class="btn btn-primary"
            :disabled="
              pipelineState === 'processing' || lastTranscript.trim() === '' || !selectedProviderId
            "
            @click="handleProcess"
          >
            {{ pipelineState === 'processing' ? 'Processing...' : 'Process Transcript' }}
          </button>
        </div>

        <p class="hint">
          Pick a provider and press Process Transcript: fillers, repetitions, and
          false starts are removed, punctuation and capitalization added, and
          spoken structure becomes lists, headings, and checklists. Meaning is
          preserved.
        </p>

        <div class="transcript-output" :class="{ empty: finalText.length === 0 }">
          {{ finalText || 'No processed text yet.' }}
        </div>

        <template v-if="pipelineState === 'done'">
          <p class="status-line" :class="{ failed: cleanupStatus === 'fallback' }">
            {{ cleanupStatusLabel() }}
          </p>
          <p class="status-line" :class="{ failed: formattingStatus === 'fallback' }">
            {{ formattingStatusLabel() }}
          </p>
        </template>

        <div v-if="pipelineError" class="error-display">
          <p class="error-text">{{ pipelineError }}</p>
        </div>
      </template>
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

.cleanup-section {
  width: 100%;
  max-width: 480px;
  padding: 24px;
  border: 1px solid var(--border);
  border-radius: 8px;
}

.cleanup-section h2 {
  margin: 0 0 16px;
  font-size: 18px;
  color: var(--text-h);
}

.status-line {
  margin: 4px 0 0;
  font-size: 13px;
  color: var(--text);
}

.status-line.failed {
  color: #b45309;
}
</style>
