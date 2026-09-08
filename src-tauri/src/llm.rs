use std::sync::Arc;
use std::time::Duration;

use serde::{Deserialize, Serialize};

use crate::providers::{AppDataStore, CredentialStore};

/// Default timeout for a chat completion round trip, in seconds.
pub const CHAT_COMPLETION_TIMEOUT_SECS: u64 = 30;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct ChatCompletionMessage {
    pub role: String,
    pub content: String,
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
    pub finish_reason: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct ChatCompletionUsage {
    pub prompt_tokens: u32,
    pub completion_tokens: u32,
    pub total_tokens: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct ChatCompletionResponse {
    pub id: String,
    pub model: String,
    pub choices: Vec<ChatCompletionChoice>,
    pub usage: ChatCompletionUsage,
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

/// Resolve the stored provider and its stored key, then run one completion.
fn chat_for_provider(
    app: tauri::AppHandle,
    credentials: Arc<dyn CredentialStore>,
    provider_id: String,
    messages: Vec<ChatCompletionMessage>,
    temperature: Option<f64>,
    max_tokens: Option<u32>,
) -> Result<ChatCompletionResponse, String> {
    let store = AppDataStore::load(&AppDataStore::app_path(&app)?)?;
    let provider = store
        .get(&provider_id)
        .cloned()
        .ok_or_else(|| format!("Provider not found: {provider_id}"))?;
    let api_key = credentials
        .get(&provider.id)?
        .ok_or_else(|| format!("No API key is stored for provider {provider_id}."))?;

    let request = ChatCompletionRequest {
        model: provider.model.clone(),
        messages,
        temperature,
        max_tokens,
        stream: false,
    };
    complete_once(&provider.base_url, &api_key, &request)
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
) -> Result<ChatCompletionResponse, String> {
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
                },
                ChatCompletionMessage {
                    role: "user".to_string(),
                    content: "Hello world".to_string(),
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
        assert_eq!(response.choices[0].finish_reason, "stop");
        assert_eq!(response.usage.total_tokens, 15);
    }

    #[test]
    fn response_deserializes_without_optional_fields() {
        let full: ChatCompletionResponse = serde_json::from_str(FULL_RESPONSE).unwrap();
        let minimal: ChatCompletionResponse = serde_json::from_str(MINIMAL_RESPONSE).unwrap();
        assert_eq!(full.choices[0].message, minimal.choices[0].message);
        assert_eq!(full.usage, minimal.usage);
    }

    #[test]
    fn response_without_usage_is_invalid() {
        let body = r#"{"id":"x","model":"m","choices":[{"message":{"role":"assistant","content":"c"},"finish_reason":"stop"}]}"#;
        let err = serde_json::from_str::<ChatCompletionResponse>(body).unwrap_err();
        assert!(err.to_string().contains("usage"), "{err}");
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
        assert_eq!(remade.usage, response.usage);
        assert_eq!(remade.choices[0].message.content, response.choices[0].message.content);
    }
}
