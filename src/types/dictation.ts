/** Dictation cleanup behavior switches. Field-exact per the overview data model. */
export interface DictationSettings {
  cleanup_enabled: boolean
  filler_removal: boolean
  repetition_removal: boolean
  self_correction_handling: boolean
  punctuation_enabled: boolean
  capitalization_enabled: boolean
  /** Reserved for the text-formatting feature (build-plan item 8); unused until then. */
  formatting_rules: Record<string, unknown>
}

/** Lifecycle of one cleanup attempt. */
export type CleanupState = 'idle' | 'cleaning' | 'done'

/** Why a cleanup attempt fell back to the raw transcript; null when not applicable. */
export type CleanupFallbackReason =
  | 'empty-response'
  | 'truncated'
  | 'content-filter'
  | 'provider-error'
  | 'no-provider'

/** Outcome of a cleanup attempt. `text` is always the final text to use. */
export interface CleanupResult {
  /** Final text, always non-null; the raw transcript on fallback. */
  text: string
  /** Where the final text came from. */
  source: 'cleaned' | 'raw'
  /** Fallback cause; null when cleaned, or when there was nothing to clean. */
  reason: CleanupFallbackReason | null
}
