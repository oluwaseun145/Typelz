<script setup lang="ts">
import ProvidersSection from './ProvidersSection.vue'
</script>

<template>
  <div class="settings-window">
    <header class="settings-header">
      <h1>Typelz Settings</h1>
    </header>
    <main class="settings-body">
      <section class="settings-section">
        <h2>LLM Providers</h2>
        <ProvidersSection />
      </section>
    </main>
  </div>
</template>

<style scoped>
.settings-window {
  display: flex;
  flex-direction: column;
  height: 100%;
  min-width: 480px;
}

.settings-header {
  padding: 24px 32px 16px;
  border-bottom: 1px solid var(--border);
}

.settings-header h1 {
  font-size: 24px;
  margin: 0;
  color: var(--text-h);
}

.settings-body {
  flex: 1;
  overflow-y: auto;
  padding: 24px 32px 32px;
}

.settings-section h2 {
  font-size: 18px;
  margin: 0 0 16px;
  color: var(--text-h);
}
</style>
