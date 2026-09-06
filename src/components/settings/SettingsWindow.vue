<template>
  <div class="settings-window">
    <header class="settings-header">
      <h1>Typelz Settings</h1>
    </header>
    <main class="settings-body">
      <p class="empty-state">No settings configured yet.</p>
    </main>
  </div>
</template>

<style scoped>
.settings-window {
  display: flex;
  flex-direction: column;
  height: 100%;
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
  display: flex;
  align-items: center;
  justify-content: center;
  padding: 32px;
}

.empty-state {
  color: var(--text);
  font-size: 16px;
}
</style>
