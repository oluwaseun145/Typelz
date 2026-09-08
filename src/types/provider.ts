/** Metadata for a stored provider. The raw API key is never part of it. */
export interface ProviderSummary {
  id: string
  name: string
  kind: string
  base_url: string
  model: string
  capabilities: Record<string, unknown>
  enabled: boolean
  has_key: boolean
}

/** Input for the add_provider command. */
export interface AddProviderInput {
  name: string
  base_url: string
  model: string
  api_key: string
}

/** PATCH-style input for update_provider; omitted fields stay unchanged. */
export interface UpdateProviderInput {
  name?: string
  base_url?: string
  model?: string
  enabled?: boolean
  /** Non-empty value rotates the stored key; omitted or blank keeps it. */
  api_key?: string
}

/** Input for the test_provider_credentials command. */
export interface TestCredentialsInput {
  base_url: string
  api_key: string
}

/** Outcome of a live provider credential check. */
export interface ProviderValidationResult {
  ok: boolean
  message: string
}
