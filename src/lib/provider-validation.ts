import { z } from 'zod'

/** Raw form values, pre-trim. Mirrors the fields of a provider entry. */
export type ProviderFormValues = {
  name: string
  base_url: string
  model: string
  api_key: string
}

/** Field-level error messages for the provider form. */
export type ProviderFormErrors = Partial<Record<keyof ProviderFormValues, string>>

function isHttpUrl(value: string): boolean {
  try {
    const parsed = new URL(value)
    return parsed.protocol === 'http:' || parsed.protocol === 'https:'
  } catch {
    return false
  }
}

const nameField = z.string().trim().min(1, 'Provider name is required')
const baseUrlField = z
  .string()
  .trim()
  .min(1, 'Base URL is required')
  .refine(isHttpUrl, 'Base URL must be an absolute http(s) URL')
const modelField = z.string().trim().min(1, 'Model is required')

const addProviderSchema = z.object({
  name: nameField,
  base_url: baseUrlField,
  model: modelField,
  api_key: z.string().trim().min(1, 'API key is required'),
})

// On edit a blank key means "keep the stored one", so it accepts any string.
const editProviderSchema = z.object({
  name: nameField,
  base_url: baseUrlField,
  model: modelField,
  api_key: z.string(),
})

interface FieldIssue {
  path: readonly (string | number | symbol)[]
  message: string
}

function toFieldErrors(issues: readonly FieldIssue[]): ProviderFormErrors {
  const errors: ProviderFormErrors = {}
  for (const issue of issues) {
    const field = issue.path[0]
    if (field !== 'name' && field !== 'base_url' && field !== 'model' && field !== 'api_key') {
      continue
    }
    if (errors[field] === undefined) {
      errors[field] = issue.message
    }
  }
  return errors
}

/** Validate values for adding a provider (a key is required). */
export function validateAddForm(values: ProviderFormValues): ProviderFormErrors {
  const result = addProviderSchema.safeParse(values)
  return result.success ? {} : toFieldErrors(result.error.issues)
}

/** Validate values for editing a provider (a blank key keeps the stored one). */
export function validateEditForm(values: ProviderFormValues): ProviderFormErrors {
  const result = editProviderSchema.safeParse(values)
  return result.success ? {} : toFieldErrors(result.error.issues)
}
