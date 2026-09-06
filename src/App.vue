<script setup lang="ts">
import SettingsWindow from './components/settings/SettingsWindow.vue'
import { useMicrophone } from './composables/useMicrophone.ts'

const {
  devices,
  selectedDeviceId,
  state,
  error,
  loadDevices,
  startCapture,
  stopCapture,
} = useMicrophone()

async function handleStart(): Promise<void> {
  await startCapture(selectedDeviceId.value ?? undefined)
}

async function handleStop(): Promise<void> {
  await stopCapture()
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
</style>
