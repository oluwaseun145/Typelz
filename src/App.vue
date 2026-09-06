<script setup lang="ts">
import SettingsWindow from './components/settings/SettingsWindow.vue'
import { useTauri } from './composables/useTauri.ts'

const { loading, error } = useTauri()
</script>

<template>
  <div class="app-shell">
    <template v-if="loading">
      <p class="status">Connecting to Typelz...</p>
    </template>

    <template v-else-if="error">
      <div class="error-state">
        <p class="error-title">Connection failed</p>
        <p class="error-message">{{ error }}</p>
        <p class="error-hint">Run the app in Tauri to access native features.</p>
      </div>
    </template>

    <template v-else>
      <SettingsWindow />
    </template>
  </div>
</template>

<style scoped>
.app-shell {
  display: flex;
  flex-direction: column;
  align-items: center;
  justify-content: center;
  min-height: 100svh;
  padding: 32px;
}

.status,
.error-title,
.error-message,
.error-hint {
  color: var(--text);
  text-align: center;
}

.error-title {
  font-size: 20px;
  font-weight: 500;
  color: var(--text-h);
  margin-bottom: 8px;
}

.error-message {
  font-family: var(--mono);
  font-size: 14px;
  background: var(--code-bg);
  padding: 8px 12px;
  border-radius: 4px;
  margin-bottom: 12px;
  word-break: break-all;
}

.error-hint {
  font-size: 14px;
  opacity: 0.7;
}
</style>
