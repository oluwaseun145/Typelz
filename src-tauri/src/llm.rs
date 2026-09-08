use std::sync::Arc;
use std::time::Duration;

use serde::{Deserialize, Serialize};

use crate::providers::{models_url, AppDataStore, CredentialStore, KIND_OPENAI_COMPATIBLE};

/// Default timeout for a chat completion round trip, in seconds.
pub const CHAT_COMPLETION_TIMEOUT_SECS: u64 = 30;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct ChatCompletionMessage {
    pub role: String,
    pub content: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reasoning_content: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reasoning: Option<serde_json::Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reasoning_details: Option<serde_json::Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tool_calls: Option<Vec<RawToolCall>>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct RawToolCall {
    #[serde(default)]
    pub id: Option<String>,
    #[serde(default)]
    pub function: RawFunction,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub struct RawFunction {
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub arguments: Option<serde_json::Value>,
}

/// OpenAI wire format; unset parameters are omitted so the provider applies
/// its own defaults.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "snake_case")]
pub struct ChatCompletionRequest {
    pub model: String,
    pub messages: Vec<ChatCompletionMessage>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub temperature: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_tokens: Option<u32>,
    /// Some providers default to streaming; force non-streaming so we get a
    /// single JSON response body.
    pub stream: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct ChatCompletionChoice {
    pub message: ChatCompletionMessage,
    #[serde(default)]
    pub finish_reason: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct ChatCompletionUsage {
    pub prompt_tokens: u32,
    pub completion_tokens: u32,
    pub total_tokens: u32,
}

/// A model as reported by a provider's model-listing endpoint.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct ModelInfo {
    pub id: String,
    pub name: String,
}

/// A provider backend that can run chat completions. Implementations do
/// blocking network I/O and must be dispatched to a worker thread.
pub trait ProviderAdapter: Send + Sync {
    fn complete(
        &self,
        messages: Vec<ChatCompletionMessage>,
        temperature: Option<f64>,
        max_tokens: Option<u32>,
    ) -> Result<ChatCompletionResponse, String>;

    fn list_models(&self) -> Result<Vec<ModelInfo>, String>;
}

/// The `data[]` envelope of an OpenAI-compatible model-list response.
#[derive(Debug, Deserialize)]
struct ModelListResponse {
    #[serde(default)]
    data: Vec<ModelListEntry>,
}

#[derive(Debug, Deserialize)]
struct ModelListEntry {
    id: String,
    #[serde(default)]
    name: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct ChatCompletionResponse {
    #[serde(default)]
    pub id: String,
    #[serde(default)]
    pub model: String,
    #[serde(default)]
    pub choices: Vec<ChatCompletionChoice>,
    #[serde(default)]
    pub usage: Option<ChatCompletionUsage>,
}

/// Canonical finish reason, normalized from provider-specific strings.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum FinishReason {
    Stop,
    Length,
    ToolCalls,
    ContentFilter,
    Error,
    Unknown(String),
}

impl FinishReason {
    fn from_raw(raw: Option<&str>) -> Self {
        match raw.map(str::trim).filter(|s| !s.is_empty()) {
            Some("stop") => Self::Stop,
            Some("length") => Self::Length,
            Some("tool_calls") | Some("tool-calls") => Self::ToolCalls,
            Some("content_filter") | Some("content-filter") => Self::ContentFilter,
            Some(other) => Self::Unknown(other.to_lowercase()),
            None => Self::Unknown(String::new()),
        }
    }
}

/// Normalized tool call with guaranteed string fields.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct NormalizedToolCall {
    pub id: String,
    pub name: String,
    pub arguments: String,
}

/// Normalized token usage with u32 defaults.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct NormalizedUsage {
    pub prompt_tokens: u32,
    pub completion_tokens: u32,
    pub total_tokens: u32,
}

/// The canonical response shape that crosses the Tauri boundary.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct NormalizedResponse {
    pub id: String,
    pub model: String,
    pub text: String,
    pub reasoning: Option<String>,
    pub tool_calls: Vec<NormalizedToolCall>,
    pub usage: NormalizedUsage,
    pub finish_reason: FinishReason,
}

fn normalize_tool_calls(raw_calls: Option<&Vec<RawToolCall>>) -> Vec<NormalizedToolCall> {
    let Some(calls) = raw_calls else {
        return Vec::new();
    };
    calls
        .iter()
        .map(|call| NormalizedToolCall {
            id: call.id.clone().unwrap_or_default(),
            name: call.function.name.clone().unwrap_or_default(),
            arguments: call
                .function
                .arguments
                .as_ref()
                .map(|v| {
                    if v.is_string() {
                        v.as_str().unwrap().to_string()
                    } else {
                        v.to_string()
                    }
                })
                .unwrap_or_default(),
        })
        .collect()
}

fn extract_reasoning(message: &ChatCompletionMessage) -> Option<String> {
    if let Some(content) = message.reasoning_content.as_ref().map(|s| s.trim()).filter(|s| !s.is_empty()) {
        return Some(content.to_string());
    }
    if let Some(value) = message.reasoning.as_ref() {
        return match value {
            serde_json::Value::String(s) => {
                let trimmed = s.trim();
                if trimmed.is_empty() { None } else { Some(trimmed.to_string()) }
            }
            serde_json::Value::Array(items) => {
                let joined: String = items
                    .iter()
                    .filter_map(|v| v.as_str())
                    .filter(|s| !s.trim().is_empty())
                    .collect::<Vec<_>>()
                    .join("\n");
                if joined.trim().is_empty() { None } else { Some(joined.trim().to_string()) }
            }
            other => {
                let s = other.to_string();
                let trimmed = s.trim();
                if trimmed.is_empty() { None } else { Some(trimmed.to_string()) }
            }
        };
    }
    if let Some(details) = message.reasoning_details.as_ref() {
        if let serde_json::Value::Array(items) = details {
            let joined: String = items
                .iter()
                .filter_map(|v| v.get("text").and_then(|t| t.as_str()))
                .filter(|s| !s.trim().is_empty())
                .collect::<Vec<_>>()
                .join("\n");
            if !joined.trim().is_empty() {
                return Some(joined.trim().to_string());
            }
        }
    }
    None
}

fn normalize_usage(usage: Option<&ChatCompletionUsage>) -> NormalizedUsage {
    let u = match usage {
        Some(u) => u,
        None => {
            return NormalizedUsage {
                prompt_tokens: 0,
                completion_tokens: 0,
                total_tokens: 0,
            }
        }
    };
    let prompt = u.prompt_tokens;
    let completion = u.completion_tokens;
    let total = if u.total_tokens == 0 {
        prompt.saturating_add(completion)
    } else {
        u.total_tokens
    };
    NormalizedUsage {
        prompt_tokens: prompt,
        completion_tokens: completion,
        total_tokens: total,
    }
}

/// Pure normalization: turns a raw provider response into the canonical shape.
pub fn normalize_response(raw: ChatCompletionResponse) -> NormalizedResponse {
    let (text, reasoning, tool_calls, finish_reason) = match raw.choices.first() {
        Some(choice) => {
            let text = choice.message.content.trim().to_string();
            let reasoning = extract_reasoning(&choice.message);
            let tool_calls = normalize_tool_calls(choice.message.tool_calls.as_ref());
            let finish_reason = FinishReason::from_raw(choice.finish_reason.as_deref());
            (text, reasoning, tool_calls, finish_reason)
        }
        None => (
            String::new(),
            None,
            Vec::new(),
            FinishReason::Unknown(String::new()),
        ),
    };
    NormalizedResponse {
        id: raw.id,
        model: raw.model,
        text,
        reasoning,
        tool_calls,
        usage: normalize_usage(raw.usage.as_ref()),
        finish_reason,
    }
}

/// The `error` object a provider returns in a failed response body.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "snake_case")]
struct ProviderErrorBody {
    #[serde(default)]
    message: Option<String>,
    #[serde(default)]
    code: Option<String>,
}

#[derive(Debug, Deserialize)]
struct ErrorEnvelope {
    #[serde(default)]
    error: Option<ProviderErrorBody>,
}

/// Extract the provider's own failure detail from a failed body, if present.
fn provider_error_detail(body: &str) -> Option<String> {
    let envelope: ErrorEnvelope = serde_json::from_str(body).ok()?;
    let error = envelope.error?;
    let message = error
        .message
        .map(|m| m.trim().to_string())
        .filter(|m| !m.is_empty())?;
    let code = error
        .code
        .as_deref()
        .map(str::trim)
        .filter(|c| !c.is_empty());
    match code {
        Some(c) => Some(format!("{message} ({c})")),
        None => Some(message),
    }
}

fn chat_url(base_url: &str) -> String {
    let root = base_url.trim_end_matches('/');
    format!("{root}/chat/completions")
}

/// Map a non-2xx status to an actionable message, preferring the provider's
/// own error detail when the body carries one.
fn classify_chat_status(status: u16, body: &str) -> String {
    let base = match status {
        401 | 403 => {
            "The provider rejected the API key. Check the key and try again."
                .to_string()
        }
        429 => {
            "The provider rate-limited the request. Wait a moment and try again."
                .to_string()
        }
        s => {
            format!(
                "The provider returned an unexpected HTTP {s}. Check the base URL and API key."
            )
        }
    };
    match provider_error_detail(body) {
        Some(detail) => format!("{base} {detail}"),
        None => base.to_string(),
    }
}

/// One chat completion round trip. Runs blocking network I/O, so callers must
/// dispatch it to a worker thread.
fn complete_once(
    base_url: &str,
    api_key: &str,
    request: &ChatCompletionRequest,
) -> Result<ChatCompletionResponse, String> {
    let client = reqwest::blocking::Client::builder()
        .timeout(Duration::from_secs(CHAT_COMPLETION_TIMEOUT_SECS))
        .build()
        .map_err(|e| format!("Could not start the HTTP client: {e}"))?;

    let body = serde_json::to_vec(request)
        .map_err(|e| format!("Could not encode the chat completion request: {e}"))?;

    let response = match client
        .post(chat_url(base_url))
        .header(reqwest::header::AUTHORIZATION, format!("Bearer {api_key}"))
        .header(reqwest::header::CONTENT_TYPE, "application/json")
        .body(body)
        .send()
    {
        Ok(response) => response,
        Err(e) if e.is_timeout() => {
            return Err(format!(
                "The provider did not respond within {CHAT_COMPLETION_TIMEOUT_SECS} seconds. Check the base URL and your network."
            ))
        }
        Err(e) => {
            return Err(format!(
                "Could not reach the provider at the base URL: {e}"
            ))
        }
    };

    let status = response.status().as_u16();
    let text = response
        .text()
        .map_err(|e| format!("Could not read the provider response: {e}"))?;

    if (200..300).contains(&status) {
        // Try strict parse first; some providers append trailing text or newlines.
        match serde_json::from_str::<ChatCompletionResponse>(&text) {
            Ok(resp) => return Ok(resp),
            Err(e) if e.to_string().contains("trailing") => {
                // Find the last '}' to cut off trailing garbage.
                if let Some(last) = text.rfind('}') {
                    if let Ok(resp) = serde_json::from_str::<ChatCompletionResponse>(&text[..=last]) {
                        return Ok(resp);
                    }
                }
                return Err(format!("The provider returned an invalid response: {e}"));
            }
            Err(_) => {
                // Response isn't valid JSON - likely HTML or plain text error page.
                let preview = text.chars().take(200).collect::<String>();
                return Err(format!(
                    "The provider returned an unexpected response (not JSON): {preview}"
                ));
            }
        };
    }

    Err(classify_chat_status(status, &text))
}

/// OpenAI-compatible backend: chat completions plus model listing against the
/// same base URL.
pub struct OpenAIAdapter {
    base_url: String,
    model: String,
    api_key: String,
}

impl OpenAIAdapter {
    fn new(base_url: String, model: String, api_key: String) -> Self {
        Self {
            base_url,
            model,
            api_key,
        }
    }
}

impl ProviderAdapter for OpenAIAdapter {
    fn complete(
        &self,
        messages: Vec<ChatCompletionMessage>,
        temperature: Option<f64>,
        max_tokens: Option<u32>,
    ) -> Result<ChatCompletionResponse, String> {
        let request = ChatCompletionRequest {
            model: self.model.clone(),
            messages,
            temperature,
            max_tokens,
            stream: false,
        };
        complete_once(&self.base_url, &self.api_key, &request)
    }

    fn list_models(&self) -> Result<Vec<ModelInfo>, String> {
        list_models_once(&self.base_url, &self.api_key)
    }
}

/// Build the adapter for a stored provider's kind. Unknown kinds fail here,
/// not at request time.
pub fn adapter_for_provider(
    base_url: &str,
    model: &str,
    api_key: &str,
    kind: &str,
) -> Result<Box<dyn ProviderAdapter>, String> {
    match kind {
        KIND_OPENAI_COMPATIBLE => Ok(Box::new(OpenAIAdapter::new(
            base_url.to_string(),
            model.to_string(),
            api_key.to_string(),
        ))),
        other => Err(format!("Unsupported provider kind: {other}")),
    }
}

fn parse_model_list(text: &str) -> Result<Vec<ModelInfo>, String> {
    let parsed: ModelListResponse = serde_json::from_str(text)
        .map_err(|e| format!("The provider returned an invalid model list: {e}"))?;
    Ok(parsed
        .data
        .into_iter()
        .map(|entry| ModelInfo {
            // Most OpenAI-compatible endpoints omit a display name.
            name: entry.name.unwrap_or_else(|| entry.id.clone()),
            id: entry.id,
        })
        .collect())
}

/// One model-listing round trip. Runs blocking network I/O, so callers must
/// dispatch it to a worker thread.
fn list_models_once(base_url: &str, api_key: &str) -> Result<Vec<ModelInfo>, String> {
    let client = reqwest::blocking::Client::builder()
        .timeout(Duration::from_secs(CHAT_COMPLETION_TIMEOUT_SECS))
        .build()
        .map_err(|e| format!("Could not start the HTTP client: {e}"))?;

    let response = match client
        .get(models_url(base_url))
        .header(reqwest::header::AUTHORIZATION, format!("Bearer {api_key}"))
        .send()
    {
        Ok(response) => response,
        Err(e) if e.is_timeout() => {
            return Err(format!(
                "The provider did not respond within {CHAT_COMPLETION_TIMEOUT_SECS} seconds. Check the base URL and your network."
            ))
        }
        Err(e) => {
            return Err(format!(
                "Could not reach the provider at the base URL: {e}"
            ))
        }
    };

    let status = response.status().as_u16();
    let text = response
        .text()
        .map_err(|e| format!("Could not read the provider response: {e}"))?;

    if !(200..300).contains(&status) {
        return Err(classify_chat_status(status, &text));
    }

    parse_model_list(&text)
}

/// Resolve the stored provider and its stored key, then run one completion.
fn chat_for_provider(
    app: tauri::AppHandle,
    credentials: Arc<dyn CredentialStore>,
    provider_id: String,
    messages: Vec<ChatCompletionMessage>,
    temperature: Option<f64>,
    max_tokens: Option<u32>,
) -> Result<NormalizedResponse, String> {
    let store = AppDataStore::load(&AppDataStore::app_path(&app)?)?;
    let provider = store
        .get(&provider_id)
        .cloned()
        .ok_or_else(|| format!("Provider not found: {provider_id}"))?;
    let api_key = credentials
        .get(&provider.id)?
        .ok_or_else(|| format!("No API key is stored for provider {provider_id}."))?;

    let adapter = adapter_for_provider(&provider.base_url, &provider.model, &api_key, &provider.kind)?;
    let raw = adapter.complete(messages, temperature, max_tokens)?;
    Ok(normalize_response(raw))
}

/// Send a chat completion to a stored provider. The API key is resolved from
/// the OS credential store; it never crosses the JS boundary.
#[tauri::command]
pub async fn send_chat_completion(
    app: tauri::AppHandle,
    credentials: tauri::State<'_, Arc<dyn CredentialStore>>,
    provider_id: String,
    messages: Vec<ChatCompletionMessage>,
    temperature: Option<f64>,
    max_tokens: Option<u32>,
) -> Result<NormalizedResponse, String> {
    let credentials = Arc::clone(&credentials);
    tauri::async_runtime::spawn_blocking(move || {
        chat_for_provider(
            app,
            credentials,
            provider_id,
            messages,
            temperature,
            max_tokens,
        )
    })
    .await
    .map_err(|e| format!("Chat completion task failed to run: {e}"))?
}

/// Resolve the stored provider and its stored key, then list its models.
fn models_for_provider(
    app: tauri::AppHandle,
    credentials: Arc<dyn CredentialStore>,
    provider_id: String,
) -> Result<Vec<ModelInfo>, String> {
    let store = AppDataStore::load(&AppDataStore::app_path(&app)?)?;
    let provider = store
        .get(&provider_id)
        .cloned()
        .ok_or_else(|| format!("Provider not found: {provider_id}"))?;
    let api_key = credentials
        .get(&provider.id)?
        .ok_or_else(|| format!("No API key is stored for provider {provider_id}."))?;

    let adapter = adapter_for_provider(&provider.base_url, &provider.model, &api_key, &provider.kind)?;
    adapter.list_models()
}

/// List the models a stored provider reports. The API key is resolved from
/// the OS credential store; it never crosses the JS boundary.
#[tauri::command]
pub async fn list_provider_models(
    app: tauri::AppHandle,
    credentials: tauri::State<'_, Arc<dyn CredentialStore>>,
    provider_id: String,
) -> Result<Vec<ModelInfo>, String> {
    let credentials = Arc::clone(&credentials);
    tauri::async_runtime::spawn_blocking(move || {
        models_for_provider(app, credentials, provider_id)
    })
    .await
    .map_err(|e| format!("Model list task failed to run: {e}"))?
}

#[cfg(test)]
mod tests {
    use super::*;

    const FULL_RESPONSE: &str = r#"{
        "id": "chatcmpl-abc123",
        "model": "gpt-4o-mini",
        "choices": [
            {
                "index": 0,
                "message": { "role": "assistant", "content": "Hi there." },
                "logprobs": null,
                "finish_reason": "stop"
            }
        ],
        "usage": {
            "prompt_tokens": 10,
            "completion_tokens": 5,
            "total_tokens": 15
        },
        "system_fingerprint": "fp_42"
    }"#;

    const MODEL_LIST_RESPONSE: &str = r#"{
        "object": "list",
        "data": [
            { "id": "gpt-4o", "object": "model", "created": 1, "owned_by": "openai" },
            { "id": "llama-3-70b", "object": "model", "name": "Llama 3 70B" }
        ]
    }"#;

    const MINIMAL_RESPONSE: &str = r#"{
        "id": "chatcmpl-abc123",
        "model": "gpt-4o-mini",
        "choices": [
            {
                "message": { "role": "assistant", "content": "Hi there." },
                "finish_reason": "stop"
            }
        ],
        "usage": {
            "prompt_tokens": 10,
            "completion_tokens": 5,
            "total_tokens": 15
        }
    }"#;

    fn sample_request() -> ChatCompletionRequest {
        ChatCompletionRequest {
            model: "gpt-4o-mini".to_string(),
            messages: vec![
                ChatCompletionMessage {
                    role: "system".to_string(),
                    content: "You are a helpful assistant.".to_string(),
                    reasoning_content: None,
                    reasoning: None,
                    reasoning_details: None,
                    tool_calls: None,
                },
                ChatCompletionMessage {
                    role: "user".to_string(),
                    content: "Hello world".to_string(),
                    reasoning_content: None,
                    reasoning: None,
                    reasoning_details: None,
                    tool_calls: None,
                },
            ],
            temperature: Some(0.2),
            max_tokens: Some(128),
            stream: false,
        }
    }

    #[test]
    fn request_serializes_to_openai_wire_format() {
        let json = serde_json::to_value(&sample_request()).unwrap();
        assert_eq!(json["model"], "gpt-4o-mini");
        assert_eq!(json["messages"][0]["role"], "system");
        assert_eq!(json["messages"][0]["content"], "You are a helpful assistant.");
        assert_eq!(json["messages"][1]["role"], "user");
        assert_eq!(json["messages"][1]["content"], "Hello world");
        assert_eq!(json["temperature"], 0.2);
        assert_eq!(json["max_tokens"], 128);
        assert_eq!(json["stream"], false);
    }

    #[test]
    fn request_omits_unset_sampling_parameters() {
        let request = ChatCompletionRequest {
            model: "m".to_string(),
            messages: vec![ChatCompletionMessage {
                role: "user".to_string(),
                content: "x".to_string(),
                reasoning_content: None,
                reasoning: None,
                reasoning_details: None,
                tool_calls: None,
            }],
            temperature: None,
            max_tokens: None,
            stream: false,
        };
        let json = serde_json::to_value(&request).unwrap();
        assert!(json.get("temperature").is_none());
        assert!(json.get("max_tokens").is_none());
    }

    #[test]
    fn response_deserializes_full_wire_shape() {
        let response: ChatCompletionResponse = serde_json::from_str(FULL_RESPONSE).unwrap();
        assert_eq!(response.id, "chatcmpl-abc123");
        assert_eq!(response.model, "gpt-4o-mini");
        assert_eq!(response.choices[0].message.role, "assistant");
        assert_eq!(response.choices[0].message.content, "Hi there.");
        assert_eq!(response.choices[0].finish_reason.as_deref(), Some("stop"));
        assert_eq!(response.usage.as_ref().unwrap().total_tokens, 15);
    }

    #[test]
    fn response_deserializes_without_optional_fields() {
        let full: ChatCompletionResponse = serde_json::from_str(FULL_RESPONSE).unwrap();
        let minimal: ChatCompletionResponse = serde_json::from_str(MINIMAL_RESPONSE).unwrap();
        assert_eq!(full.choices[0].message.role, minimal.choices[0].message.role);
        assert_eq!(full.usage.as_ref().unwrap().total_tokens, minimal.usage.as_ref().unwrap().total_tokens);
    }

    #[test]
    fn response_without_usage_is_valid() {
        let body = r#"{"id":"x","model":"m","choices":[{"message":{"role":"assistant","content":"c"},"finish_reason":"stop"}]}"#;
        let response: ChatCompletionResponse = serde_json::from_str(body).unwrap();
        assert!(response.usage.is_none());
    }

    #[test]
    fn response_with_null_finish_reason_is_valid() {
        let body = r#"{"id":"x","model":"m","choices":[{"message":{"role":"assistant","content":"c"},"finish_reason":null}],"usage":{"prompt_tokens":1,"completion_tokens":1,"total_tokens":2}}"#;
        let response: ChatCompletionResponse = serde_json::from_str(body).unwrap();
        assert_eq!(response.choices[0].finish_reason, None);
    }

    #[test]
    fn response_with_reasoning_content_deserializes() {
        let body = r#"{"id":"x","model":"m","choices":[{"message":{"role":"assistant","content":"c","reasoning_content":"let me think"},"finish_reason":"stop"}],"usage":{"prompt_tokens":1,"completion_tokens":1,"total_tokens":2}}"#;
        let response: ChatCompletionResponse = serde_json::from_str(body).unwrap();
        assert_eq!(response.choices[0].message.reasoning_content.as_deref(), Some("let me think"));
    }

    #[test]
    fn response_with_tool_calls_deserializes() {
        let body = r#"{"id":"x","model":"m","choices":[{"message":{"role":"assistant","content":"","tool_calls":[{"id":"call_1","function":{"name":"do_it","arguments":"{}"}}]},"finish_reason":"tool_calls"}],"usage":{"prompt_tokens":1,"completion_tokens":1,"total_tokens":2}}"#;
        let response: ChatCompletionResponse = serde_json::from_str(body).unwrap();
        let calls = response.choices[0].message.tool_calls.as_ref().unwrap();
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].id.as_deref(), Some("call_1"));
        assert_eq!(calls[0].function.name.as_deref(), Some("do_it"));
    }

    #[test]
    fn chat_url_joins() {
        assert_eq!(
            chat_url("https://api.openai.com/v1"),
            "https://api.openai.com/v1/chat/completions"
        );
        assert_eq!(
            chat_url("https://api.openai.com/v1/"),
            "https://api.openai.com/v1/chat/completions"
        );
        assert_eq!(
            chat_url("https://x/"),
            "https://x/chat/completions"
        );
    }

    #[test]
    fn classify_status_maps_auth_rate_limit_and_other() {
        let unauthorized = classify_chat_status(401, "");
        assert!(unauthorized.contains("rejected the API key"), "{unauthorized}");
        let forbidden = classify_chat_status(403, "");
        assert!(forbidden.contains("rejected the API key"), "{forbidden}");
        let rate_limited = classify_chat_status(429, "");
        assert!(rate_limited.contains("rate-limited"), "{rate_limited}");
        let server = classify_chat_status(500, "");
        assert!(server.contains("HTTP 500"), "{server}");
    }

    #[test]
    fn classify_status_appends_provider_error_detail() {
        let body = r#"{"error": {"message": "Incorrect API key provided", "code": "invalid_api_key"}}"#;
        let message = classify_chat_status(401, body);
        assert!(message.contains("rejected the API key"), "{message}");
        assert!(message.contains("Incorrect API key provided"), "{message}");
        assert!(message.contains("invalid_api_key"), "{message}");
    }

    #[test]
    fn provider_error_detail_handles_missing_parts() {
        assert_eq!(provider_error_detail(""), None);
        assert_eq!(provider_error_detail("not json"), None);
        assert_eq!(provider_error_detail(r#"{"error": null}"#), None);
        assert_eq!(provider_error_detail(r#"{"error": {}}"#), None);
        let stripped = provider_error_detail(r#"{"error": {"message": "  boom  "}}"#);
        assert_eq!(stripped.as_deref(), Some("boom"));
    }

    #[test]
    fn response_round_trips_through_json() {
        let response: ChatCompletionResponse = serde_json::from_str(MINIMAL_RESPONSE).unwrap();
        let remade: ChatCompletionResponse =
            serde_json::from_value(serde_json::to_value(&response).unwrap()).unwrap();
        assert_eq!(remade.usage.as_ref().unwrap().total_tokens, response.usage.as_ref().unwrap().total_tokens);
        assert_eq!(remade.choices[0].message.content, response.choices[0].message.content);
    }

    #[test]
    fn adapter_for_provider_returns_openai() {
        let adapter = adapter_for_provider(
            "https://api.openai.com/v1",
            "gpt-4o-mini",
            "sk-test",
            KIND_OPENAI_COMPATIBLE,
        )
        .expect("OpenAI-compatible kind should build an adapter");
        // Construction is the observable contract here; calling the methods
        // would need a live endpoint.
        let _: Box<dyn ProviderAdapter> = adapter;
    }

    #[test]
    fn adapter_for_provider_rejects_unknown_kind() {
        let result = adapter_for_provider("https://api.openai.com/v1", "m", "sk-test", "anthropic");
        let err = match result {
            Err(e) => e,
            Ok(_) => panic!("unknown kinds must fail at construction"),
        };
        assert!(err.contains("anthropic"), "{err}");
    }

    #[test]
    fn model_list_parse_extracts_id_and_name() {
        let models = parse_model_list(MODEL_LIST_RESPONSE).unwrap();
        assert_eq!(models, vec![
            ModelInfo {
                id: "gpt-4o".to_string(),
                name: "gpt-4o".to_string(),
            },
            ModelInfo {
                id: "llama-3-70b".to_string(),
                name: "Llama 3 70B".to_string(),
            },
        ]);
    }

    #[test]
    fn model_list_parse_rejects_not_json() {
        let err = parse_model_list("<html>bad gateway</html>").unwrap_err();
        assert!(err.contains("invalid model list"), "{err}");
    }

    #[test]
    fn normalize_response_happy_path() {
        let raw: ChatCompletionResponse = serde_json::from_str(FULL_RESPONSE).unwrap();
        let n = normalize_response(raw);
        assert_eq!(n.text, "Hi there.");
        assert_eq!(n.reasoning, None);
        assert!(n.tool_calls.is_empty());
        assert_eq!(n.finish_reason, FinishReason::Stop);
        assert_eq!(n.usage.total_tokens, 15);
    }

    #[test]
    fn normalize_response_extracts_reasoning_content() {
        let body = r#"{"id":"x","model":"m","choices":[{"message":{"role":"assistant","content":"done","reasoning_content":"let me think"},"finish_reason":"stop"}],"usage":{"prompt_tokens":1,"completion_tokens":1,"total_tokens":2}}"#;
        let raw: ChatCompletionResponse = serde_json::from_str(body).unwrap();
        let n = normalize_response(raw);
        assert_eq!(n.text, "done");
        assert_eq!(n.reasoning.as_deref(), Some("let me think"));
    }

    #[test]
    fn normalize_response_extracts_tool_calls() {
        let body = r#"{"id":"x","model":"m","choices":[{"message":{"role":"assistant","content":"","tool_calls":[{"id":"c1","function":{"name":"do_it","arguments":"{}"}}]},"finish_reason":"tool_calls"}],"usage":{"prompt_tokens":1,"completion_tokens":1,"total_tokens":2}}"#;
        let raw: ChatCompletionResponse = serde_json::from_str(body).unwrap();
        let n = normalize_response(raw);
        assert_eq!(n.tool_calls.len(), 1);
        assert_eq!(n.tool_calls[0].id, "c1");
        assert_eq!(n.tool_calls[0].name, "do_it");
        assert_eq!(n.finish_reason, FinishReason::ToolCalls);
    }

    #[test]
    fn normalize_response_empty_choices() {
        let body = r#"{"id":"x","model":"m","choices":[],"usage":{"prompt_tokens":0,"completion_tokens":0,"total_tokens":0}}"#;
        let raw: ChatCompletionResponse = serde_json::from_str(body).unwrap();
        let n = normalize_response(raw);
        assert_eq!(n.text, "");
        assert_eq!(n.reasoning, None);
        assert!(n.tool_calls.is_empty());
    }

    #[test]
    fn normalize_response_unknown_finish_reason() {
        let body = r#"{"id":"x","model":"m","choices":[{"message":{"role":"assistant","content":"c"},"finish_reason":"weird"}],"usage":{"prompt_tokens":1,"completion_tokens":1,"total_tokens":2}}"#;
        let raw: ChatCompletionResponse = serde_json::from_str(body).unwrap();
        let n = normalize_response(raw);
        assert_eq!(n.finish_reason, FinishReason::Unknown("weird".to_string()));
    }

    #[test]
    fn normalize_response_missing_usage_defaults_zero() {
        let body = r#"{"id":"x","model":"m","choices":[{"message":{"role":"assistant","content":"c"},"finish_reason":"stop"}]}"#;
        let raw: ChatCompletionResponse = serde_json::from_str(body).unwrap();
        let n = normalize_response(raw);
        assert_eq!(n.usage.prompt_tokens, 0);
        assert_eq!(n.usage.completion_tokens, 0);
        assert_eq!(n.usage.total_tokens, 0);
    }
}
