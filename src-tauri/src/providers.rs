use tauri::Manager;

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

/// The only provider kind available in v1.
pub const KIND_OPENAI_COMPATIBLE: &str = "openai_compatible";

/// Keychain service shared by all provider entries; matches the Tauri app
/// identifier so the entries stay grouped on Windows.
pub const CREDENTIAL_SERVICE: &str = "com.typelz.app";

const SCHEMA_VERSION: u32 = 1;
const FILE_NAME: &str = "typelz.json";

/// Provider metadata. The raw API key is deliberately not part of this type:
/// it lives only in the OS credential store.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct Provider {
    pub id: String,
    pub name: String,
    pub kind: String,
    pub base_url: String,
    pub model: String,
    #[serde(default = "empty_capabilities")]
    pub capabilities: Value,
    pub enabled: bool,
}

fn empty_capabilities() -> Value {
    Value::Object(Map::new())
}

impl Provider {
    /// Summary safe to send to the webview and log: no key material.
    pub fn summary(&self, has_key: bool) -> ProviderSummary {
        ProviderSummary {
            id: self.id.clone(),
            name: self.name.clone(),
            kind: self.kind.clone(),
            base_url: self.base_url.clone(),
            model: self.model.clone(),
            capabilities: self.capabilities.clone(),
            enabled: self.enabled,
            has_key,
        }
    }
}

/// What the UI (and later features) see for a stored provider.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "snake_case")]
pub struct ProviderSummary {
    pub id: String,
    pub name: String,
    pub kind: String,
    pub base_url: String,
    pub model: String,
    pub capabilities: Value,
    pub enabled: bool,
    pub has_key: bool,
}

/// Raw API key storage. The keyring implementation is the production backend;
/// tests use in-memory fakes so the OS store is never touched.
pub trait CredentialStore: Send + Sync {
    fn set(&self, provider_id: &str, api_key: &str) -> Result<(), String>;

    /// `None` when the provider has no stored key yet.
    fn get(&self, provider_id: &str) -> Result<Option<String>, String>;

    /// Idempotent: deleting a missing entry is not an error.
    fn delete(&self, provider_id: &str) -> Result<(), String>;
}

pub struct KeyringCredentialStore;

impl CredentialStore for KeyringCredentialStore {
    fn set(&self, provider_id: &str, api_key: &str) -> Result<(), String> {
        entry_for(provider_id)?
            .set_password(api_key)
            .map_err(|e| {
                format!("Could not save the API key to the system credential store: {e}")
            })?;

        // Read-back verification: confirm the keychain actually persisted the
        // credential. On Windows, set_password can appear to succeed while the
        // credential does not survive an app restart.
        match entry_for(provider_id)?.get_password() {
            Ok(reflected) if reflected == api_key => Ok(()),
            Ok(_) => Err(
                "The API key was saved but could not be read back from the system \
                 credential store. The key may not persist after a restart. \
                 Try saving again or check your OS credential manager."
                    .to_string(),
            ),
            Err(e) => Err(format!(
                "The API key was saved but the system credential store returned an \
                 error on read-back: {e}. The key may not persist after a restart."
            )),
        }
    }

    fn get(&self, provider_id: &str) -> Result<Option<String>, String> {
        match entry_for(provider_id)?.get_password() {
            Ok(password) => Ok(Some(password)),
            Err(keyring::Error::NoEntry) => Ok(None),
            Err(e) => Err(format!(
                "Could not read the API key from the system credential store: {e}"
            )),
        }
    }

    fn delete(&self, provider_id: &str) -> Result<(), String> {
        match entry_for(provider_id)?.delete_credential() {
            Ok(()) => Ok(()),
            Err(keyring::Error::NoEntry) => Ok(()),
            Err(e) => Err(format!(
                "Could not delete the API key from the system credential store: {e}"
            )),
        }
    }
}

fn entry_for(provider_id: &str) -> Result<keyring::Entry, String> {
    keyring::Entry::new(CREDENTIAL_SERVICE, provider_id)
        .map_err(|e| format!("Could not create a credential store entry: {e}"))
}

const CREDENTIALS_FILE: &str = "credentials.json";

/// Credential store that tries the OS keychain first and falls back to a
/// local file when the keychain write fails or does not persist. The file
/// uses base64 obfuscation (not encryption) to keep keys out of plaintext;
/// it is a v1 convenience, not a security boundary.
pub struct FallbackCredentialStore {
    keychain: KeyringCredentialStore,
    file_path: PathBuf,
}

#[derive(Debug, Default, Serialize, Deserialize)]
struct CredentialFile {
    entries: std::collections::HashMap<String, String>,
}

impl FallbackCredentialStore {
    pub fn new(app: &tauri::AppHandle) -> Result<Self, String> {
        let file_path = app
            .path()
            .resolve(CREDENTIALS_FILE, tauri::path::BaseDirectory::Config)
            .map_err(|e| format!("Could not resolve credentials file path: {e}"))?;
        Ok(Self {
            keychain: KeyringCredentialStore,
            file_path,
        })
    }

    fn load_file(&self) -> CredentialFile {
        std::fs::read(&self.file_path)
            .ok()
            .and_then(|bytes| serde_json::from_slice(&bytes).ok())
            .unwrap_or_default()
    }

    fn save_file(&self, data: &CredentialFile) -> Result<(), String> {
        let json = serde_json::to_string_pretty(data)
            .map_err(|e| format!("Could not encode credentials: {e}"))?;
        std::fs::write(&self.file_path, json)
            .map_err(|e| format!("Could not write credentials file: {e}"))
    }

    fn obfuscate(plain: &str) -> String {
        use base64::Engine;
        base64::engine::general_purpose::STANDARD.encode(plain.as_bytes())
    }

    fn deobfuscate(encoded: &str) -> Option<String> {
        use base64::Engine;
        base64::engine::general_purpose::STANDARD
            .decode(encoded)
            .ok()
            .and_then(|bytes| String::from_utf8(bytes).ok())
    }
}

impl CredentialStore for FallbackCredentialStore {
    fn set(&self, provider_id: &str, api_key: &str) -> Result<(), String> {
        // Try the OS keychain first.
        let keychain_ok = self.keychain.set(provider_id, api_key).is_ok();

        // Always persist to the file as a backup so the key survives even if
        // the keychain silently drops it on restart.
        let mut data = self.load_file();
        data.entries
            .insert(provider_id.to_string(), Self::obfuscate(api_key));
        self.save_file(&data)?;

        // The file write succeeded, so the key is stored. If the keychain also
        // worked, great. If not, the file is the backup and the key will be
        // retrieved from there on the next read.
        if !keychain_ok {
            log::warn!(
                "API key for provider {provider_id} was saved to the local backup file \
                 because the system credential store is unavailable."
            );
        }
        Ok(())
    }

    fn get(&self, provider_id: &str) -> Result<Option<String>, String> {
        // Try the OS keychain first.
        match self.keychain.get(provider_id) {
            Ok(Some(key)) => return Ok(Some(key)),
            Ok(None) => {}
            Err(_) => {}
        }

        // Fall back to the file.
        let data = self.load_file();
        Ok(data
            .entries
            .get(provider_id)
            .and_then(|encoded| Self::deobfuscate(encoded)))
    }

    fn delete(&self, provider_id: &str) -> Result<(), String> {
        // Best-effort delete from both stores.
        let _ = self.keychain.delete(provider_id);
        let mut data = self.load_file();
        data.entries.remove(provider_id);
        self.save_file(&data)
    }
}

#[derive(Debug, Serialize, Deserialize)]
struct AppDataFile {
    schema_version: u32,
    #[serde(default)]
    providers: Vec<Provider>,
}

/// The app's single JSON data document (`typelz.json` in the Tauri config
/// dir). Later features (settings, history) extend the same document.
#[derive(Debug)]
pub struct AppDataStore {
    path: PathBuf,
    data: AppDataFile,
}

impl AppDataStore {
    pub fn load(path: &Path) -> Result<Self, String> {
        match std::fs::read(path) {
            Ok(bytes) => {
                let data: AppDataFile = serde_json::from_slice(&bytes)
                    .map_err(|e| format!("Could not read {FILE_NAME}: {e}"))?;
                if data.schema_version != SCHEMA_VERSION {
                    return Err(format!(
                        "Unsupported {FILE_NAME} schema version {} (expected {SCHEMA_VERSION})",
                        data.schema_version
                    ));
                }
                Ok(Self {
                    path: path.to_path_buf(),
                    data,
                })
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Self {
                path: path.to_path_buf(),
                data: AppDataFile {
                    schema_version: SCHEMA_VERSION,
                    providers: Vec::new(),
                },
            }),
            Err(e) => Err(format!("Could not read {FILE_NAME}: {e}")),
        }
    }

    /// Path used by the running app: `<Tauri config dir>/typelz.json`.
    pub fn app_path(app: &tauri::AppHandle) -> Result<PathBuf, String> {
        app.path()
            .resolve(FILE_NAME, tauri::path::BaseDirectory::Config)
            .map_err(|e| format!("Could not resolve the app config directory: {e}"))
    }

    pub fn list(&self) -> Vec<Provider> {
        self.data.providers.clone()
    }

    pub fn get(&self, id: &str) -> Option<&Provider> {
        self.data.providers.iter().find(|p| p.id == id)
    }

    pub fn insert(&mut self, provider: Provider) -> Result<(), String> {
        if self.data.providers.iter().any(|p| p.id == provider.id) {
            return Err(format!("Provider already exists: {}", provider.id));
        }
        self.data.providers.push(provider);
        self.save()
    }

    pub fn update(
        &mut self,
        id: &str,
        f: impl FnOnce(&mut Provider) -> Result<(), String>,
    ) -> Result<(), String> {
        let provider = self
            .data
            .providers
            .iter_mut()
            .find(|p| p.id == id)
            .ok_or_else(|| format!("Provider not found: {id}"))?;
        f(provider)?;
        self.save()
    }

    pub fn remove(&mut self, id: &str) -> Result<(), String> {
        let position = self
            .data
            .providers
            .iter()
            .position(|p| p.id == id)
            .ok_or_else(|| format!("Provider not found: {id}"))?;
        self.data.providers.remove(position);
        self.save()
    }

    /// Temp file plus rename so a crashed write never leaves a truncated
    /// `typelz.json` behind.
    fn save(&self) -> Result<(), String> {
        if let Some(parent) = self.path.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|e| format!("Could not create the app config directory: {e}"))?;
        }
        let bytes = serde_json::to_vec_pretty(&self.data)
            .map_err(|e| format!("Could not serialize provider data: {e}"))?;
        let mut tmp = self.path.as_os_str().to_os_string();
        tmp.push(".tmp");
        std::fs::write(&tmp, &bytes).map_err(|e| format!("Could not write provider data: {e}"))?;
        if let Err(e) = std::fs::rename(&tmp, &self.path) {
            let _ = std::fs::remove_file(&tmp);
            return Err(format!("Could not save provider data: {e}"));
        }
        Ok(())
    }
}

/// Timeout for the live credential check in seconds.
pub const VALIDATION_TIMEOUT_SECS: u64 = 10;

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct AddProviderInput {
    pub name: String,
    pub base_url: String,
    pub model: String,
    pub api_key: String,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct UpdateProviderInput {
    pub name: Option<String>,
    pub base_url: Option<String>,
    pub model: Option<String>,
    pub enabled: Option<bool>,
    pub api_key: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct TestCredentialsInput {
    pub base_url: String,
    pub api_key: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct ValidationResult {
    pub ok: bool,
    pub message: String,
}

impl ValidationResult {
    fn failure(message: String) -> Self {
        Self {
            ok: false,
            message,
        }
    }
}

/// Owns one provider document plus its credential backing store. Commands
/// build one of these per call, which keeps the JSON reload current without
/// sharing mutable state across the Tauri runtime.
pub struct ProviderService {
    store: AppDataStore,
    credentials: Arc<dyn CredentialStore>,
}

impl ProviderService {
    pub fn from_app(
        app: &tauri::AppHandle,
        credentials: Arc<dyn CredentialStore>,
    ) -> Result<Self, String> {
        Ok(Self {
            store: AppDataStore::load(&AppDataStore::app_path(app)?)?,
            credentials,
        })
    }

    #[cfg(test)]
    pub fn for_test(path: &Path, credentials: Arc<dyn CredentialStore>) -> Result<Self, String> {
        Ok(Self {
            store: AppDataStore::load(path)?,
            credentials,
        })
    }

    pub fn list(&self) -> Result<Vec<ProviderSummary>, String> {
        Ok(self
            .store
            .list()
            .into_iter()
            .map(|p| {
                let has_key = self
                    .credentials
                    .get(&p.id)
                    .map(|k| k.is_some())
                    .unwrap_or(false);
                p.summary(has_key)
            })
            .collect())
    }

    pub fn add(&mut self, input: &AddProviderInput) -> Result<ProviderSummary, String> {
        let name = require_non_empty(&input.name, "Provider name")?;
        let base_url = validate_base_url(&input.base_url)?;
        let model = require_non_empty(&input.model, "Model")?;
        let api_key = require_non_empty(&input.api_key, "API key")?;

        let provider = Provider {
            id: uuid::Uuid::new_v4().to_string(),
            name,
            kind: KIND_OPENAI_COMPATIBLE.to_string(),
            base_url,
            model,
            capabilities: Value::Object(Map::new()),
            enabled: true,
        };

        // The keychain write happens first: if it fails there is no stale
        // provider row pointing at a missing key.
        self.credentials.set(&provider.id, &api_key)?;
        if let Err(e) = self.store.insert(provider.clone()) {
            let _ = self.credentials.delete(&provider.id);
            return Err(e);
        }
        self.summarize(&provider.id)
    }

    pub fn update(
        &mut self,
        id: &str,
        input: &UpdateProviderInput,
    ) -> Result<ProviderSummary, String> {
        self.store.get(id).ok_or_else(|| format!("Provider not found: {id}"))?;

        let new_name = input
            .name
            .as_deref()
            .map(|v| require_non_empty(v, "Provider name"))
            .transpose()?;
        let new_base_url = input
            .base_url
            .as_deref()
            .map(validate_base_url)
            .transpose()?;
        let new_model = input
            .model
            .as_deref()
            .map(|v| require_non_empty(v, "Model"))
            .transpose()?;

        // A present, non-blank key rotates the credential; a missing or blank
        // one keeps whatever is already stored.
        let api_key = input
            .api_key
            .as_deref()
            .map(str::trim)
            .filter(|v| !v.is_empty())
            .map(str::to_string);

        self.store.update(id, |p| {
            if let Some(v) = &new_name {
                p.name = v.clone();
            }
            if let Some(v) = &new_base_url {
                p.base_url = v.clone();
            }
            if let Some(v) = &new_model {
                p.model = v.clone();
            }
            if let Some(v) = input.enabled {
                p.enabled = v;
            }
            Ok(())
        })?;

        if let Some(key) = api_key {
            self.credentials.set(id, &key)?;
        }
        self.summarize(id)
    }

    pub fn remove(&mut self, id: &str) -> Result<(), String> {
        // Delete the keychain entry first so a failure leaves the provider
        // intact, then drop the metadata row.
        self.credentials.delete(id)?;
        self.store.remove(id)
    }

    pub fn validate(&self, id: &str) -> Result<ValidationResult, String> {
        let provider = self
            .store
            .get(id)
            .ok_or_else(|| format!("Provider not found: {id}"))?;
        let key = self
            .credentials
            .get(id)?
            .ok_or_else(|| format!("No API key is stored for provider {id}."))?;
        Ok(validate_credentials(&provider.base_url, &key))
    }

    pub fn test_credentials(&self, input: &TestCredentialsInput) -> ValidationResult {
        let base_url = match validate_base_url(&input.base_url) {
            Ok(v) => v,
            Err(e) => return ValidationResult::failure(e),
        };
        let key = input.api_key.trim();
        if key.is_empty() {
            return ValidationResult::failure("API key is required".to_string());
        }
        validate_credentials(&base_url, key)
    }

    fn summarize(&self, id: &str) -> Result<ProviderSummary, String> {
        let provider = self
            .store
            .get(id)
            .ok_or_else(|| format!("Provider not found: {id}"))?;
        let has_key = self
            .credentials
            .get(id)
            .map(|k| k.is_some())
            .unwrap_or(false);
        Ok(provider.summary(has_key))
    }
}

fn require_non_empty(value: &str, label: &str) -> Result<String, String> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return Err(format!("{label} is required"));
    }
    Ok(trimmed.to_string())
}

/// Trim the value and require an absolute `http://` or `https://` URL.
pub fn validate_base_url(raw: &str) -> Result<String, String> {
    let trimmed = raw.trim();
    match url::Url::parse(trimmed) {
        Ok(parsed) if matches!(parsed.scheme(), "http" | "https") => Ok(trimmed.to_string()),
        Ok(_) => Err(format!(
            "Base URL must use http or https: {trimmed}"
        )),
        Err(_) => Err(format!(
            "Base URL must be an absolute http(s) URL, e.g. https://api.openai.com/v1"
        )),
    }
}

fn models_url(base_url: &str) -> String {
    let root = base_url.trim_end_matches('/');
    format!("{root}/models")
}

pub fn classify_status(status: u16) -> ValidationResult {
    match status {
        200 => ValidationResult {
            ok: true,
            message: "Connected to the provider."
                .to_string(),
        },
        401 | 403 => ValidationResult::failure(
            "The provider rejected the API key. Check the key and try again.".to_string(),
        ),
        404 => ValidationResult::failure(format!(
            "The base URL does not expose /models. It should point at the API root, e.g. https://api.openai.com/v1 (HTTP 404)."
        )),
        s => ValidationResult::failure(format!(
            "The provider returned an unexpected HTTP {s}. Check the base URL and API key."
        )),
    }
}

/// Perform a single live check (`GET {base_url}/models`) with the supplied key.
/// Runs blocking network I/O, so callers must dispatch it to a worker thread.
pub fn validate_credentials(base_url: &str, api_key: &str) -> ValidationResult {
    let client = match reqwest::blocking::Client::builder()
        .timeout(Duration::from_secs(VALIDATION_TIMEOUT_SECS))
        .build()
    {
        Ok(c) => c,
        Err(e) => return ValidationResult::failure(format!("Could not start the HTTP client: {e}")),
    };

    match client
        .get(models_url(base_url))
        .header(reqwest::header::AUTHORIZATION, format!("Bearer {api_key}"))
        .send()
    {
        Ok(response) => classify_status(response.status().as_u16()),
        Err(e) if e.is_timeout() => ValidationResult::failure(format!(
            "The provider did not respond within {VALIDATION_TIMEOUT_SECS} seconds. Check the base URL and your network."
        )),
        Err(e) => ValidationResult::failure(format!(
            "Could not reach the provider at the base URL: {e}"
        )),
    }
}

/// Load the service and run a closure on a blocking thread so store I/O and
/// any network work stay off the async runtime's UI thread.
async fn run_service<T, F>(
    app: tauri::AppHandle,
    credentials: tauri::State<'_, Arc<dyn CredentialStore>>,
    f: F,
) -> Result<T, String>
where
    T: Send + 'static,
    F: FnOnce(ProviderService) -> Result<T, String> + Send + 'static,
{
    let credentials = Arc::clone(&credentials);
    tauri::async_runtime::spawn_blocking(move || {
        let service = ProviderService::from_app(&app, credentials)?;
        f(service)
    })
    .await
    .map_err(|e| format!("Provider task failed to run: {e}"))?
}

#[tauri::command]
pub async fn list_providers(
    app: tauri::AppHandle,
    credentials: tauri::State<'_, Arc<dyn CredentialStore>>,
) -> Result<Vec<ProviderSummary>, String> {
    run_service(app, credentials, |service| service.list()).await
}

#[tauri::command]
pub async fn add_provider(
    app: tauri::AppHandle,
    credentials: tauri::State<'_, Arc<dyn CredentialStore>>,
    input: AddProviderInput,
) -> Result<ProviderSummary, String> {
    run_service(app, credentials, move |mut service| service.add(&input)).await
}

#[tauri::command]
pub async fn update_provider(
    app: tauri::AppHandle,
    credentials: tauri::State<'_, Arc<dyn CredentialStore>>,
    id: String,
    input: UpdateProviderInput,
) -> Result<ProviderSummary, String> {
    run_service(app, credentials, move |mut service| service.update(&id, &input)).await
}

#[tauri::command]
pub async fn remove_provider(
    app: tauri::AppHandle,
    credentials: tauri::State<'_, Arc<dyn CredentialStore>>,
    id: String,
) -> Result<(), String> {
    run_service(app, credentials, move |mut service| service.remove(&id)).await
}

#[tauri::command]
pub async fn validate_provider(
    app: tauri::AppHandle,
    credentials: tauri::State<'_, Arc<dyn CredentialStore>>,
    id: String,
) -> Result<ValidationResult, String> {
    run_service(app, credentials, move |service| service.validate(&id)).await
}

#[tauri::command]
pub async fn test_provider_credentials(
    app: tauri::AppHandle,
    credentials: tauri::State<'_, Arc<dyn CredentialStore>>,
    input: TestCredentialsInput,
) -> Result<ValidationResult, String> {
    run_service(
        app,
        credentials,
        move |service| Ok(service.test_credentials(&input)),
    )
    .await
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;
    use std::sync::Mutex;

    use super::*;

    /// In-memory keychain fake. `fail_set` mirrors an unavailable store during
    /// writes so rollback paths are testable without the OS credential store.
    #[derive(Default)]
    struct FakingCredentialStore {
        fail_set: bool,
        entries: Mutex<HashMap<String, String>>,
    }

    impl FakingCredentialStore {
        fn stored(&self, provider_id: &str) -> Option<String> {
            self.entries.lock().unwrap().get(provider_id).cloned()
        }

        fn forget(&self, provider_id: &str) {
            self.entries.lock().unwrap().remove(provider_id);
        }
    }

    impl CredentialStore for FakingCredentialStore {
        fn set(&self, provider_id: &str, api_key: &str) -> Result<(), String> {
            if self.fail_set {
                return Err("credential store unavailable".to_string());
            }
            self.entries
                .lock()
                .unwrap()
                .insert(provider_id.to_string(), api_key.to_string());
            Ok(())
        }

        fn get(&self, provider_id: &str) -> Result<Option<String>, String> {
            Ok(self.entries.lock().unwrap().get(provider_id).cloned())
        }

        fn delete(&self, provider_id: &str) -> Result<(), String> {
            self.entries.lock().unwrap().remove(provider_id);
            Ok(())
        }
    }

    /// Same fake, except `delete` always fails, to prove remove aborts with
    /// the provider left intact.
    #[derive(Default)]
    struct DeleteFailingStore {
        inner: FakingCredentialStore,
    }

    impl DeleteFailingStore {
        fn stored(&self, provider_id: &str) -> Option<String> {
            self.inner.stored(provider_id)
        }
    }

    impl CredentialStore for DeleteFailingStore {
        fn set(&self, provider_id: &str, api_key: &str) -> Result<(), String> {
            self.inner.set(provider_id, api_key)
        }

        fn get(&self, provider_id: &str) -> Result<Option<String>, String> {
            self.inner.get(provider_id)
        }

        fn delete(&self, _provider_id: &str) -> Result<(), String> {
            Err("credential store unavailable".to_string())
        }
    }

    fn sample_provider(id: &str, name: &str) -> Provider {
        Provider {
            id: id.to_string(),
            name: name.to_string(),
            kind: KIND_OPENAI_COMPATIBLE.to_string(),
            base_url: "https://api.openai.com/v1".to_string(),
            model: "gpt-4o-mini".to_string(),
            capabilities: Value::Object(Map::new()),
            enabled: true,
        }
    }

    fn store_path(dir: &Path) -> PathBuf {
        dir.join("typelz.json")
    }

    #[test]
    fn loads_empty_when_file_missing() {
        let dir = tempfile::tempdir().unwrap();
        let store = AppDataStore::load(&store_path(dir.path())).unwrap();
        assert!(store.list().is_empty());
    }

    #[test]
    fn round_trips_providers() {
        let dir = tempfile::tempdir().unwrap();
        let path = store_path(dir.path());
        {
            let mut store = AppDataStore::load(&path).unwrap();
            store.insert(sample_provider("a", "Alpha")).unwrap();
            store.insert(sample_provider("b", "Beta")).unwrap();
        }
        let store = AppDataStore::load(&path).unwrap();
        let list = store.list();
        assert_eq!(list.len(), 2);
        assert_eq!(list[0].name, "Alpha");
        assert_eq!(list[1].base_url, "https://api.openai.com/v1");
        assert!(list[1].enabled);
    }

    #[test]
    fn rejects_unsupported_schema_version() {
        let dir = tempfile::tempdir().unwrap();
        let path = store_path(dir.path());
        std::fs::write(&path, r#"{"schema_version": 2, "providers": []}"#).unwrap();
        let err = AppDataStore::load(&path).unwrap_err();
        assert!(err.contains("schema version"), "{err}");
    }

    #[test]
    fn capabilities_default_to_empty_object() {
        let dir = tempfile::tempdir().unwrap();
        let path = store_path(dir.path());
        std::fs::write(
            &path,
            r#"{"schema_version":1,"providers":[{"id":"a","name":"A","kind":"openai_compatible","base_url":"https://x/v1","model":"m","enabled":true}]}"#,
        )
        .unwrap();
        let store = AppDataStore::load(&path).unwrap();
        assert_eq!(
            store.list()[0].capabilities,
            Value::Object(Map::new())
        );
    }

    #[test]
    fn persistence_exposes_no_key_fields() {
        let dir = tempfile::tempdir().unwrap();
        let path = store_path(dir.path());
        let mut store = AppDataStore::load(&path).unwrap();
        store.insert(sample_provider("a", "Alpha")).unwrap();
        let file = std::fs::read_to_string(path).unwrap();
        let summary = serde_json::to_string(&store.list()[0].summary(true)).unwrap();
        for surface in [file.as_str(), summary.as_str()] {
            assert!(!surface.contains("api_key"), "key field leaked: {surface}");
            assert!(!surface.contains("sk-secret"), "key material leaked: {surface}");
        }
        assert!(summary.contains(r#""has_key":true"#));
    }

    #[test]
    fn update_changes_fields() {
        let dir = tempfile::tempdir().unwrap();
        let mut store = AppDataStore::load(&store_path(dir.path())).unwrap();
        store.insert(sample_provider("a", "Alpha")).unwrap();
        store
            .update("a", |p| {
                p.name = "Renamed".to_string();
                Ok(())
            })
            .unwrap();
        assert_eq!(store.get("a").unwrap().name, "Renamed");
    }

    #[test]
    fn update_unknown_id_errors() {
        let dir = tempfile::tempdir().unwrap();
        let mut store = AppDataStore::load(&store_path(dir.path())).unwrap();
        let err = store.update("missing", |p| Ok(p.kind = "x".to_string())).unwrap_err();
        assert!(err.contains("not found"), "{err}");
    }

    #[test]
    fn remove_deletes_entry() {
        let dir = tempfile::tempdir().unwrap();
        let path = store_path(dir.path());
        let mut store = AppDataStore::load(&path).unwrap();
        store.insert(sample_provider("a", "Alpha")).unwrap();
        store.remove("a").unwrap();
        assert!(store.list().is_empty());
        let reloaded = AppDataStore::load(&path).unwrap();
        assert!(reloaded.list().is_empty());
    }

    #[test]
    fn remove_unknown_id_errors() {
        let dir = tempfile::tempdir().unwrap();
        let mut store = AppDataStore::load(&store_path(dir.path())).unwrap();
        let err = store.remove("missing").unwrap_err();
        assert!(err.contains("not found"), "{err}");
    }

    fn service_in(dir: &tempfile::TempDir, credentials: Arc<dyn CredentialStore>) -> ProviderService {
        ProviderService::for_test(&store_path(dir.path()), credentials).unwrap()
    }

    fn add_input(name: &str, base_url: &str, model: &str, api_key: &str) -> AddProviderInput {
        AddProviderInput {
            name: name.to_string(),
            base_url: base_url.to_string(),
            model: model.to_string(),
            api_key: api_key.to_string(),
        }
    }

    #[test]
    fn add_persists_metadata_and_key() {
        let dir = tempfile::tempdir().unwrap();
        let credentials = Arc::new(FakingCredentialStore::default());
        let mut service = service_in(&dir, credentials.clone());
        let summary = service
            .add(&add_input("OpenAI", "https://api.openai.com/v1", "gpt-4o-mini", "sk-test"))
            .unwrap();
        assert!(summary.has_key);
        assert!(summary.enabled);
        assert_eq!(summary.kind, KIND_OPENAI_COMPATIBLE);
        assert_eq!(summary.capabilities, Value::Object(Map::new()));
        assert_eq!(credentials.stored(&summary.id).as_deref(), Some("sk-test"));

        let reopened = service_in(&dir, Arc::new(FakingCredentialStore::default()));
        let list = reopened.list().unwrap();
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].name, "OpenAI");
        assert_eq!(list[0].base_url, "https://api.openai.com/v1");
    }

    #[test]
    fn add_rejects_blank_or_invalid_input() {
        let dir = tempfile::tempdir().unwrap();
        let mut service = service_in(&dir, Arc::new(FakingCredentialStore::default()));
        assert!(service.add(&add_input("   ", "https://x/v1", "m", "k")).is_err());
        assert!(service.add(&add_input("A", "not a url", "m", "k")).is_err());
        assert!(service.add(&add_input("A", "ftp://x/v1", "m", "k")).is_err());
        assert!(service.add(&add_input("A", "https://x/v1", "  ", "k")).is_err());
        assert!(service.add(&add_input("A", "https://x/v1", "m", " ")).is_err());
        assert!(service.list().unwrap().is_empty());
    }

    #[test]
    fn add_rolls_back_when_keychain_fails() {
        let dir = tempfile::tempdir().unwrap();
        let mut credentials = FakingCredentialStore::default();
        credentials.fail_set = true;
        let mut service = service_in(&dir, Arc::new(credentials));
        let err = service.add(&add_input("A", "https://x/v1", "m", "k")).unwrap_err();
        assert!(err.contains("credential store"), "{err}");
        assert!(service.list().unwrap().is_empty());
        let reopened = AppDataStore::load(&store_path(dir.path())).unwrap();
        assert!(reopened.list().is_empty());
    }

    #[test]
    fn update_without_key_keeps_stored_key() {
        let dir = tempfile::tempdir().unwrap();
        let credentials = Arc::new(FakingCredentialStore::default());
        let mut service = service_in(&dir, credentials.clone());
        let provider = service
            .add(&add_input("A", "https://x/v1", "m", "sk-1"))
            .unwrap();
        service
            .update(
                &provider.id,
                &UpdateProviderInput {
                    name: Some("B".to_string()),
                    ..Default::default()
                },
            )
            .unwrap();
        assert_eq!(service.list().unwrap()[0].name, "B");
        assert_eq!(service.list().unwrap()[0].has_key, true);
        assert_eq!(credentials.stored(&provider.id).as_deref(), Some("sk-1"));
    }

    #[test]
    fn update_blank_key_keeps_stored_key() {
        let dir = tempfile::tempdir().unwrap();
        let credentials = Arc::new(FakingCredentialStore::default());
        let mut service = service_in(&dir, credentials.clone());
        let provider = service
            .add(&add_input("A", "https://x/v1", "m", "sk-1"))
            .unwrap();
        service
            .update(
                &provider.id,
                &UpdateProviderInput {
                    api_key: Some("   ".to_string()),
                    ..Default::default()
                },
            )
            .unwrap();
        assert_eq!(credentials.stored(&provider.id).as_deref(), Some("sk-1"));
    }

    #[test]
    fn update_new_key_rotates_stored_key() {
        let dir = tempfile::tempdir().unwrap();
        let credentials = Arc::new(FakingCredentialStore::default());
        let mut service = service_in(&dir, credentials.clone());
        let provider = service
            .add(&add_input("A", "https://x/v1", "m", "sk-1"))
            .unwrap();
        service
            .update(
                &provider.id,
                &UpdateProviderInput {
                    api_key: Some("sk-2".to_string()),
                    ..Default::default()
                },
            )
            .unwrap();
        assert_eq!(credentials.stored(&provider.id).as_deref(), Some("sk-2"));
    }

    #[test]
    fn update_rejects_invalid_field_without_changing_the_provider() {
        let dir = tempfile::tempdir().unwrap();
        let mut service = service_in(&dir, Arc::new(FakingCredentialStore::default()));
        let provider = service
            .add(&add_input("A", "https://x/v1", "m", "k"))
            .unwrap();
        let err = service
            .update(
                &provider.id,
                &UpdateProviderInput {
                    base_url: Some("ftp://x".to_string()),
                    ..Default::default()
                },
            )
            .unwrap_err();
        assert!(err.contains("http"), "{err}");
        let list = service.list().unwrap();
        assert_eq!(list[0].base_url, "https://x/v1");
    }

    #[test]
    fn update_unknown_provider_id_errors() {
        let dir = tempfile::tempdir().unwrap();
        let mut service = service_in(&dir, Arc::new(FakingCredentialStore::default()));
        let err = service
            .update("missing", &UpdateProviderInput::default())
            .unwrap_err();
        assert!(err.contains("not found"), "{err}");
    }

    #[test]
    fn remove_deletes_metadata_and_key() {
        let dir = tempfile::tempdir().unwrap();
        let credentials = Arc::new(FakingCredentialStore::default());
        let mut service = service_in(&dir, credentials.clone());
        let provider = service
            .add(&add_input("A", "https://x/v1", "m", "sk-1"))
            .unwrap();
        service.remove(&provider.id).unwrap();
        assert!(service.list().unwrap().is_empty());
        assert_eq!(credentials.stored(&provider.id), None);
        let reopened = AppDataStore::load(&store_path(dir.path())).unwrap();
        assert!(reopened.list().is_empty());
    }

    #[test]
    fn remove_aborts_when_keychain_fails() {
        let dir = tempfile::tempdir().unwrap();
        let credentials = Arc::new(DeleteFailingStore::default());
        let mut service = service_in(&dir, credentials.clone());
        let provider = service
            .add(&add_input("A", "https://x/v1", "m", "sk-1"))
            .unwrap();
        let err = service.remove(&provider.id).unwrap_err();
        assert!(err.contains("credential store"), "{err}");
        // Provider row and key survive the failed remove.
        assert_eq!(service.list().unwrap().len(), 1);
        assert_eq!(credentials.stored(&provider.id).as_deref(), Some("sk-1"));
    }

    #[test]
    fn validate_unknown_provider_fails_without_network() {
        let dir = tempfile::tempdir().unwrap();
        let service = service_in(&dir, Arc::new(FakingCredentialStore::default()));
        let err = service.validate("missing").unwrap_err();
        assert!(err.contains("not found"), "{err}");
    }

    #[test]
    fn validate_reports_missing_stored_key() {
        let dir = tempfile::tempdir().unwrap();
        let credentials = Arc::new(FakingCredentialStore::default());
        let mut service = service_in(&dir, credentials.clone());
        let provider = service
            .add(&add_input("A", "https://x/v1", "m", "sk-1"))
            .unwrap();
        credentials.forget(&provider.id);
        let err = service.validate(&provider.id).unwrap_err();
        assert!(err.contains("No API key is stored"), "{err}");
    }

    #[test]
    fn validate_base_url_rules() {
        assert_eq!(
            validate_base_url("https://api.openai.com/v1").unwrap(),
            "https://api.openai.com/v1"
        );
        assert_eq!(validate_base_url("  https://x/v1  ").unwrap(), "https://x/v1");
        assert!(validate_base_url("ftp://x/v1").is_err());
        assert!(validate_base_url("just a host").is_err());
        assert!(validate_base_url("   ").is_err());
    }

    #[test]
    fn models_url_joins() {
        assert_eq!(models_url("https://x/v1"), "https://x/v1/models");
        assert_eq!(models_url("https://x/v1/"), "https://x/v1/models");
        assert_eq!(models_url("https://x/"), "https://x/models");
    }

    #[test]
    fn classify_status_maps() {
        assert!(classify_status(200).ok);
        let rejected = classify_status(403);
        assert!(!rejected.ok);
        assert!(rejected.message.contains("rejected the API key"), "{}", rejected.message);
        let not_found = classify_status(404);
        assert!(!not_found.ok);
        assert!(not_found.message.contains("/models"), "{}", not_found.message);
        let server = classify_status(500);
        assert!(!server.ok);
        assert!(server.message.contains("500"), "{}", server.message);
    }
}
