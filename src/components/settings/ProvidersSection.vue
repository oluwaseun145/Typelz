<script setup lang="ts">
import { nextTick, onMounted, reactive, ref } from 'vue'
import { useProviders } from '../../composables/useProviders'
import { useChatCompletion } from '../../composables/useChatCompletion'
import {
  validateAddForm,
  validateEditForm,
  type ProviderFormErrors,
} from '../../lib/provider-validation'
import type { ProviderSummary } from '../../types/provider'

type Mode = 'list' | 'add' | 'edit'

type RowStatus = { checking: true } | { checking: false; ok: boolean; message: string }

const { providers, loading, error, refresh, add, update, remove, validate, test } = useProviders()
const { sendCompletion, loading: chatLoading, error: chatError } = useChatCompletion()

const mode = ref<Mode>('list')
const editingId = ref<string | null>(null)
const editingProvider = ref<ProviderSummary | null>(null)
const testing = ref(false)
const saving = ref(false)
const testResult = ref<{ ok: boolean; message: string } | null>(null)
const fieldErrors = ref<ProviderFormErrors>({})
const rowStatus = ref<Record<string, RowStatus>>({})
const chatResult = ref<Record<string, string>>({})

const form = reactive({
  name: '',
  base_url: '',
  model: '',
  api_key: '',
  enabled: true,
})

const nameInput = ref<HTMLInputElement | null>(null)
const baseInput = ref<HTMLInputElement | null>(null)
const modelInput = ref<HTMLInputElement | null>(null)
const keyInput = ref<HTMLInputElement | null>(null)

const fieldOrder = ['name', 'base_url', 'model', 'api_key'] as const

onMounted(() => {
  void refresh()
})

function clearFieldError(field: keyof ProviderFormErrors): void {
  if (fieldErrors.value[field]) {
    fieldErrors.value = { ...fieldErrors.value, [field]: undefined }
  }
}

function focusFirstInvalid(): void {
  const first = fieldOrder.find((field) => fieldErrors.value[field])
  if (!first) return
  void nextTick(() => {
    const inputs: Record<string, HTMLInputElement | null> = {
      name: nameInput.value,
      base_url: baseInput.value,
      model: modelInput.value,
      api_key: keyInput.value,
    }
    inputs[first]?.focus()
  })
}

function fillForm(provider: ProviderSummary | null): void {
  form.name = provider?.name ?? ''
  form.base_url = provider?.base_url ?? ''
  form.model = provider?.model ?? ''
  form.api_key = ''
  form.enabled = provider?.enabled ?? true
}

function openAddForm(): void {
  editingId.value = null
  editingProvider.value = null
  fillForm(null)
  resetFeedback()
  mode.value = 'add'
  void nextTick(() => nameInput.value?.focus())
}

function openEditForm(provider: ProviderSummary): void {
  editingId.value = provider.id
  editingProvider.value = provider
  fillForm(provider)
  resetFeedback()
  mode.value = 'edit'
  void nextTick(() => nameInput.value?.focus())
}

function resetFeedback(): void {
  fieldErrors.value = {}
  testResult.value = null
}

function closeForm(): void {
  if (saving.value || testing.value) return
  mode.value = 'list'
  editingId.value = null
  editingProvider.value = null
  testResult.value = null
}

function canTestDraft(): boolean {
  return form.base_url.trim() !== '' && form.api_key.trim() !== ''
}

function canTestStoredKey(): boolean {
  return (
    mode.value === 'edit' &&
    form.api_key.trim() === '' &&
    editingProvider.value?.has_key === true
  )
}

async function runTest(): Promise<void> {
  const useStoredKey = !canTestDraft() && canTestStoredKey()
  if (testing.value || (!canTestDraft() && !useStoredKey)) return
  testing.value = true
  testResult.value = null
  try {
    const result = canTestDraft()
      ? await test({
          base_url: form.base_url.trim(),
          api_key: form.api_key.trim(),
        })
      : await validate(editingId.value as string)
    testResult.value = result ?? { ok: false, message: error.value ?? 'Connection test failed' }
  } finally {
    testing.value = false
  }
}

async function save(): Promise<void> {
  const values = {
    name: form.name.trim(),
    base_url: form.base_url.trim(),
    model: form.model.trim(),
    api_key: form.api_key.trim(),
  }
  fieldErrors.value =
    mode.value === 'edit' ? validateEditForm(values) : validateAddForm(values)
  if (fieldOrder.some((field) => fieldErrors.value[field])) {
    focusFirstInvalid()
    return
  }
  saving.value = true
  testResult.value = null
  try {
    if (mode.value === 'edit' && editingId.value) {
      const updated = await update(editingId.value, {
        name: values.name,
        base_url: values.base_url,
        model: values.model,
        enabled: form.enabled,
        api_key: values.api_key || undefined,
      })
      if (updated) {
        mode.value = 'list'
        editingId.value = null
        editingProvider.value = null
      }
    } else {
      const created = await add(values)
      if (created) {
        mode.value = 'list'
      }
    }
  } finally {
    saving.value = false
  }
}

function rowChecking(id: string): boolean {
  return rowStatus.value[id]?.checking === true
}

function rowResultText(id: string): string | null {
  const status = rowStatus.value[id]
  if (!status || status.checking) return null
  return status.message
}

function rowResultOk(id: string): boolean {
  const status = rowStatus.value[id]
  return !!status && !status.checking && status.ok
}

async function runRowTest(provider: ProviderSummary): Promise<void> {
  if (rowChecking(provider.id)) return
  rowStatus.value = { ...rowStatus.value, [provider.id]: { checking: true } }
  const result = await validate(provider.id)
  const status: RowStatus = result
    ? { checking: false, ok: result.ok, message: result.message }
    : { checking: false, ok: false, message: error.value ?? 'Validation failed' }
  rowStatus.value = { ...rowStatus.value, [provider.id]: status }
}

async function removeProvider(provider: ProviderSummary): Promise<void> {
  const confirmed = window.confirm(
    `Remove provider "${provider.name}"? This also deletes its stored API key.`,
  )
  if (!confirmed) return
  await remove(provider.id)
  rowStatus.value = {}
}

async function runChatTest(provider: ProviderSummary): Promise<void> {
  chatResult.value = { ...chatResult.value, [provider.id]: 'Sending...' }
  const response = await sendCompletion(provider.id, [
    { role: 'user', content: 'Say hello in one sentence.' },
  ])
  if (response) {
    chatResult.value = {
      ...chatResult.value,
      [provider.id]: response.choices[0]?.message?.content ?? 'No response content',
    }
  } else {
    chatResult.value = {
      ...chatResult.value,
      [provider.id]: chatError.value ?? 'Chat completion failed',
    }
  }
}
</script>

<template>
  <div class="providers">
    <p v-if="loading" class="status-line">Loading providers...</p>

    <div v-else-if="providers.length === 0" class="empty">
      <p>No providers configured. Add your first LLM provider to use dictation cleanup.</p>
      <button class="btn btn-primary" :disabled="mode !== 'list'" @click="openAddForm">
        Add provider
      </button>
    </div>

    <template v-else>
      <ul class="provider-list">
        <li v-for="provider in providers" :key="provider.id" class="provider-card">
          <div class="provider-head">
            <span class="provider-name">{{ provider.name }}</span>
            <span class="provider-badge">{{ provider.kind }}</span>
            <span v-if="!provider.has_key" class="provider-badge warn">No API key</span>
            <span v-if="!provider.enabled" class="provider-badge dim">Disabled</span>
            <span class="row-actions">
              <button
                class="row-btn"
                :disabled="rowChecking(provider.id) || saving"
                @click="runRowTest(provider)"
              >
                {{ rowChecking(provider.id) ? 'Checking...' : 'Test' }}
              </button>
              <button
                class="row-btn"
                :disabled="chatLoading || saving || !provider.has_key"
                @click="runChatTest(provider)"
              >
                {{ chatLoading && chatResult[provider.id] === 'Sending...' ? 'Chatting...' : 'Chat' }}
              </button>
              <button class="row-btn" :disabled="saving || mode !== 'list'" @click="openEditForm(provider)">
                Edit
              </button>
              <button
                class="row-btn row-btn-danger"
                :disabled="saving || mode !== 'list'"
                @click="removeProvider(provider)"
              >
                Remove
              </button>
            </span>
          </div>
          <p class="provider-detail">{{ provider.base_url }} / {{ provider.model }}</p>
          <p
            v-if="rowResultText(provider.id)"
            class="row-result"
            :class="rowResultOk(provider.id) ? 'ok' : 'fail'"
          >
            {{ rowResultText(provider.id) }}
          </p>
          <p v-if="chatResult[provider.id]" class="row-result chat-result">
            {{ chatResult[provider.id] }}
          </p>
        </li>
      </ul>
      <div v-if="mode === 'list'" class="list-actions">
        <button class="btn btn-secondary" @click="openAddForm">Add provider</button>
      </div>
    </template>

    <form v-if="mode !== 'list'" class="provider-form" novalidate @submit.prevent="save">
      <h3 class="form-title">
        {{ mode === 'edit' && editingProvider ? `Edit ${editingProvider.name}` : 'Add a provider' }}
      </h3>

      <div class="field">
        <label for="provider-name">Name</label>
        <input
          id="provider-name"
          ref="nameInput"
          v-model="form.name"
          type="text"
          autocomplete="off"
          :aria-invalid="fieldErrors.name ? true : undefined"
          :aria-describedby="fieldErrors.name ? 'provider-name-error' : undefined"
          @input="clearFieldError('name')"
        />
        <p v-if="fieldErrors.name" id="provider-name-error" class="field-error">
          {{ fieldErrors.name }}
        </p>
      </div>

      <div class="field">
        <label for="provider-base-url">Base URL</label>
        <input
          id="provider-base-url"
          ref="baseInput"
          v-model="form.base_url"
          type="text"
          autocomplete="off"
          placeholder="https://api.openai.com/v1"
          :aria-invalid="fieldErrors.base_url ? true : undefined"
          :aria-describedby="fieldErrors.base_url ? 'provider-base-url-error' : undefined"
          @input="clearFieldError('base_url')"
        />
        <p v-if="fieldErrors.base_url" id="provider-base-url-error" class="field-error">
          {{ fieldErrors.base_url }}
        </p>
      </div>

      <div class="field">
        <label for="provider-model">Model</label>
        <input
          id="provider-model"
          ref="modelInput"
          v-model="form.model"
          type="text"
          autocomplete="off"
          placeholder="gpt-4o-mini"
          :aria-invalid="fieldErrors.model ? true : undefined"
          :aria-describedby="fieldErrors.model ? 'provider-model-error' : undefined"
          @input="clearFieldError('model')"
        />
        <p v-if="fieldErrors.model" id="provider-model-error" class="field-error">
          {{ fieldErrors.model }}
        </p>
      </div>

      <div class="field">
        <label for="provider-api-key">API key</label>
        <input
          id="provider-api-key"
          ref="keyInput"
          v-model="form.api_key"
          type="password"
          autocomplete="off"
          placeholder="sk-..."
          :aria-invalid="fieldErrors.api_key ? true : undefined"
          :aria-describedby="fieldErrors.api_key ? 'provider-api-key-error' : undefined"
          @input="clearFieldError('api_key')"
        />
        <p v-if="fieldErrors.api_key" id="provider-api-key-error" class="field-error">
          {{ fieldErrors.api_key }}
        </p>
        <p v-else-if="mode === 'edit' && editingProvider" class="field-hint">
          {{
            editingProvider.has_key
              ? 'Leave blank to keep the current key.'
              : 'No key stored yet. Enter one to validate and save it.'
          }}
        </p>
      </div>

      <div v-if="mode === 'edit'" class="field field-check">
        <label class="check-label" for="provider-enabled">
          <input id="provider-enabled" v-model="form.enabled" type="checkbox" />
          Enabled
        </label>
      </div>

      <div aria-live="polite">
        <p v-if="testResult" class="test-result" :class="testResult.ok ? 'ok' : 'fail'">
          {{ testResult.message }}
        </p>
        <p v-if="error" class="error-line">{{ error }}</p>
      </div>

      <div class="form-buttons">
        <button type="submit" class="btn btn-primary" :disabled="saving || testing">
          {{ saving ? 'Saving...' : 'Save' }}
        </button>
        <button
          type="button"
          class="btn btn-secondary"
          :disabled="saving || testing || (!canTestDraft() && !canTestStoredKey())"
          @click="runTest"
        >
          {{ testing ? 'Testing...' : 'Test Connection' }}
        </button>
        <button type="button" class="btn btn-ghost" :disabled="saving || testing" @click="closeForm">
          Cancel
        </button>
      </div>
    </form>
  </div>
</template>

<style scoped>
.providers {
  display: flex;
  flex-direction: column;
  gap: 16px;
  max-width: 560px;
}

.status-line {
  font-size: 14px;
  opacity: 0.7;
}

.empty {
  display: flex;
  flex-direction: column;
  align-items: flex-start;
  gap: 12px;
  color: var(--text);
  font-size: 15px;
}

.provider-list {
  list-style: none;
  margin: 0;
  padding: 0;
  display: flex;
  flex-direction: column;
  gap: 8px;
}

.provider-card {
  border: 1px solid var(--border);
  border-radius: 8px;
  padding: 12px 16px;
}

.provider-head {
  display: flex;
  align-items: center;
  gap: 8px;
  flex-wrap: wrap;
}

.provider-name {
  font-size: 15px;
  font-weight: 600;
  color: var(--text-h);
}

.provider-badge {
  font-size: 11px;
  padding: 2px 8px;
  border-radius: 999px;
  background: var(--accent-bg);
  color: var(--accent);
}

.provider-badge.warn {
  background: rgba(180, 83, 9, 0.15);
  color: #b45309;
}

.provider-badge.dim {
  background: var(--border);
  color: var(--text);
}

.provider-detail {
  margin-top: 6px;
  font-family: var(--mono);
  font-size: 12px;
  color: var(--text);
  opacity: 0.8;
  word-break: break-all;
}

.row-actions {
  margin-left: auto;
  display: inline-flex;
  gap: 6px;
}

.row-btn {
  padding: 3px 10px;
  border: 1px solid var(--border);
  border-radius: 4px;
  background: transparent;
  color: var(--text);
  font-size: 12px;
  cursor: pointer;
}

.row-btn:hover:not(:disabled) {
  border-color: var(--accent);
  color: var(--accent);
}

.row-btn-danger:hover:not(:disabled) {
  border-color: #e53e3e;
  color: #e53e3e;
}

.row-btn:disabled {
  opacity: 0.5;
  cursor: not-allowed;
}

.row-result {
  margin: 6px 0 0;
  font-size: 12px;
}

.row-result.ok {
  color: #15803d;
}

.row-result.fail {
  color: #e53e3e;
}

.chat-result {
  color: var(--accent);
  font-style: italic;
}

.list-actions {
  display: flex;
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
  background: var(--accent);
  color: #fff;
}

.btn-secondary {
  background: var(--border);
  color: var(--text);
}

.btn-ghost {
  background: transparent;
  border: 1px solid var(--border);
  color: var(--text);
}

.provider-form {
  display: flex;
  flex-direction: column;
  gap: 12px;
  border: 1px solid var(--border);
  border-radius: 8px;
  padding: 16px;
}

.form-title {
  font-size: 15px;
  color: var(--text-h);
  margin: 0;
}

.field {
  display: flex;
  flex-direction: column;
  gap: 4px;
}

.field label {
  font-size: 13px;
  color: var(--text);
}

.field input {
  padding: 8px 12px;
  border: 1px solid var(--border);
  border-radius: 4px;
  background: var(--bg);
  color: var(--text);
  font-size: 14px;
}

.field input[aria-invalid='true'] {
  border-color: #e53e3e;
}

.field-check {
  flex-direction: row;
  align-items: center;
  gap: 8px;
}

.check-label {
  display: inline-flex;
  align-items: center;
  gap: 6px;
  font-size: 13px;
  color: var(--text);
  cursor: pointer;
}

.field-error {
  margin: 0;
  font-size: 12px;
  color: #e53e3e;
}

.field-hint {
  margin: 0;
  font-size: 12px;
  color: var(--text);
  opacity: 0.7;
}

.test-result {
  margin: 0;
  font-size: 13px;
}

.test-result.ok {
  color: #15803d;
}

.test-result.fail {
  color: #e53e3e;
}

.error-line {
  margin: 0;
  font-size: 13px;
  color: #e53e3e;
}

.form-buttons {
  display: flex;
  gap: 8px;
}
</style>
