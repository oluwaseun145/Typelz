/** One message in a chat completion conversation, OpenAI wire format. */
export interface ChatCompletionMessage {
  role: 'system' | 'user' | 'assistant'
  content: string
}

/** Chat completion request body sent to an OpenAI-compatible endpoint. */
export interface ChatCompletionRequest {
  model: string
  messages: ChatCompletionMessage[]
  temperature?: number
  max_tokens?: number
}

/** The assistant reply within a choice. */
export interface ChatCompletionChoice {
  message: ChatCompletionMessage
  finish_reason: string
}

/** Token accounting for a completed request. */
export interface ChatCompletionUsage {
  prompt_tokens: number
  completion_tokens: number
  total_tokens: number
}

/** Successful chat completion response. */
export interface ChatCompletionResponse {
  id: string
  model: string
  choices: ChatCompletionChoice[]
  usage: ChatCompletionUsage
}

/** The `error` object a provider returns in a failed response body. */
export interface ChatCompletionError {
  code: string
  message: string
}
