import type {
  NormalizedLlmResponse,
  NormalizedToolCall,
  NormalizedUsage,
  FinishReason,
} from "../types/llm"

/** Canonicalize a raw finish reason string to the union type. */
export function parseFinishReason(raw: unknown): FinishReason {
  if (raw == null) return "unknown"
  const s = String(raw).trim().toLowerCase()
  if (s === "stop") return "stop"
  if (s === "length") return "length"
  if (s === "tool_calls" || s === "tool-calls") return "tool_calls"
  if (s === "content_filter" || s === "content-filter") return "content_filter"
  if (s === "") return "unknown"
  return s
}

function normalizeToolCalls(rawCalls: unknown): NormalizedToolCall[] {
  if (!Array.isArray(rawCalls)) return []
  return rawCalls
    .filter((c): c is Record<string, unknown> => c != null && typeof c === "object")
    .map((call) => {
      const fn = (call as Record<string, unknown>["function"]) as
        | Record<string, unknown>
        | undefined
      const args = fn?.arguments
      let argumentsStr = ""
      if (typeof args === "string") {
        argumentsStr = args
      } else if (args != null) {
        argumentsStr = JSON.stringify(args)
      }
      return {
        id: typeof call.id === "string" ? call.id : "",
        name: typeof fn?.name === "string" ? fn.name : "",
        arguments: argumentsStr,
      }
    })
}

function extractReasoning(message: Record<string, unknown>): string | null {
  const content = message.reasoning_content
  if (typeof content === "string" && content.trim() !== "") {
    return content.trim()
  }
  const reasoning = message.reasoning
  if (typeof reasoning === "string" && reasoning.trim() !== "") {
    return reasoning.trim()
  }
  if (Array.isArray(reasoning)) {
    const joined = reasoning
      .filter((v): v is string => typeof v === "string")
      .filter((s) => s.trim() !== "")
      .join("\n")
    if (joined.trim() !== "") return joined.trim()
  }
  const details = message.reasoning_details
  if (Array.isArray(details)) {
    const joined = details
      .map((v) => (v as Record<string, unknown>)?.text)
      .filter((t): t is string => typeof t === "string" && t.trim() !== "")
      .join("\n")
    if (joined.trim() !== "") return joined.trim()
  }
  return null
}

function normalizeUsage(raw: unknown): NormalizedUsage {
  const u = (raw ?? {}) as Record<string, unknown>
  return {
    prompt_tokens: typeof u.prompt_tokens === "number" ? u.prompt_tokens : 0,
    completion_tokens: typeof u.completion_tokens === "number" ? u.completion_tokens : 0,
    total_tokens:
      typeof u.total_tokens === "number" && u.total_tokens !== 0
        ? u.total_tokens
        : (typeof u.prompt_tokens === "number" ? u.prompt_tokens : 0) +
          (typeof u.completion_tokens === "number" ? u.completion_tokens : 0),
  }
}

/**
 * Pure normalization: turns a raw provider response into the canonical shape.
 * Never throws on missing fields.
 */
export function normalizeLlmResponse(raw: unknown): NormalizedLlmResponse {
  const r = (raw ?? {}) as Record<string, unknown>
  const choices = Array.isArray(r.choices) ? r.choices : []
  const first = choices[0] as Record<string, unknown> | undefined
  const message = (first?.message ?? {}) as Record<string, unknown>
  const text =
    typeof message.content === "string" ? message.content.trim() : ""

  return {
    id: typeof r.id === "string" ? r.id : "",
    model: typeof r.model === "string" ? r.model : "",
    text,
    reasoning: extractReasoning(message),
    tool_calls: normalizeToolCalls(message.tool_calls),
    usage: normalizeUsage(r.usage),
    finish_reason: parseFinishReason(first?.finish_reason),
  }
}
