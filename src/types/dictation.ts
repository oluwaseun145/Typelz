/** Structure types the formatting stage may produce. */
export interface FormattingRules {
  paragraphs_enabled: boolean
  bullet_lists_enabled: boolean
  numbered_lists_enabled: boolean
  headings_enabled: boolean
  checklists_enabled: boolean
}

/** Dictation cleanup behavior switches. Field-exact per the overview data model. */
export interface DictationSettings {
  cleanup_enabled: boolean
  filler_removal: boolean
  repetition_removal: boolean
  self_correction_handling: boolean
  punctuation_enabled: boolean
  capitalization_enabled: boolean
  /** Structure the formatting stage (build-plan item 8) may apply. */
  formatting_rules: FormattingRules
}

/** Lifecycle of one pipeline run. */
export type PipelineState = 'idle' | 'processing' | 'done'

/**
 * Lifecycle of one cleanup attempt (kept for the cleanup stage's result shape).
 */
export type CleanupState = 'idle' | 'cleaning' | 'done'

/**
 * Why a stage could not apply its output and carried its input through; null
 * when applied, or when there was nothing to process.
 */
export type LlmStageFailureReason =
  | 'empty-response'
  | 'truncated'
  | 'content-filter'
  | 'provider-error'

/** Why a cleanup attempt fell back to the raw transcript; null when not applicable. */
export type CleanupFallbackReason =
  | LlmStageFailureReason
  | 'no-provider'

/** Per-stage outcome. `text` is always the stage output to carry forward. */
export interface FormatStageResult {
  /** Stage output: formatted text when applied, else the stage input carried through. */
  text: string
  /** Whether the provider's formatted output was used. */
  applied: boolean
  /** Failure cause; null when applied or when the stage did not run. */
  reason: LlmStageFailureReason | null
}

/**
 * UI status of one stage in the latest pipeline run. `off` = the stage's own
 * enable flag is false; `skipped` = formatting's per-structure flags are all
 * false; `fallback` = ran and failed, input carried through.
 */
export type StageStatus = 'off' | 'skipped' | 'applied' | 'fallback' | 'reverted'

/** Outcome of a cleanup attempt. `text` is always the final text to use. */
export interface CleanupResult {
  /** Final text, always non-null; the raw transcript on fallback. */
  text: string
  /** Where the final text came from. */
  source: 'cleaned' | 'raw'
  /** Fallback cause; null when cleaned, or when there was nothing to clean. */
  reason: CleanupFallbackReason | null
}
