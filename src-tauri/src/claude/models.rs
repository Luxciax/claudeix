use crate::app_paths::{claudeix_data_dir, display_path};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};

const CATALOG_VERSION: u32 = 1;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ClaudeModelsFile {
    pub path: String,
    pub exists: bool,
    pub content: String,
    pub revision: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct ClaudeModelDef {
    pub id: String,
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub reasoning: Option<bool>,
    #[serde(default)]
    pub input: Option<Vec<String>>,
    #[serde(default)]
    pub context_window: Option<u64>,
    #[serde(default)]
    pub max_tokens: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct ClaudeProvider {
    #[serde(default)]
    pub base_url: String,
    #[serde(default = "default_api")]
    pub api: String,
    #[serde(default)]
    pub api_key: Option<String>,
    #[serde(default)]
    pub models: Vec<ClaudeModelDef>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ClaudeModelsCatalog {
    #[serde(default = "catalog_version")]
    pub version: u32,
    #[serde(default)]
    pub enabled_models: Vec<String>,
    #[serde(default)]
    pub providers: BTreeMap<String, ClaudeProvider>,
}

impl Default for ClaudeModelsCatalog {
    fn default() -> Self {
        Self {
            version: CATALOG_VERSION,
            enabled_models: Vec::new(),
            providers: BTreeMap::new(),
        }
    }
}

fn catalog_version() -> u32 {
    CATALOG_VERSION
}

fn default_api() -> String {
    "anthropic-messages".to_string()
}

fn catalog_path() -> Result<PathBuf, String> {
    Ok(claudeix_data_dir()?.join("models.json"))
}

fn parse_catalog(content: &str) -> Result<ClaudeModelsCatalog, String> {
    let catalog: ClaudeModelsCatalog = serde_json::from_str(content)
        .map_err(|error| format!("Claudeix model catalog must be valid JSON: {error}"))?;
    if catalog.version != CATALOG_VERSION {
        return Err(format!(
            "unsupported Claudeix model catalog version: {}",
            catalog.version
        ));
    }
    for (provider_id, provider) in &catalog.providers {
        if provider_id.trim().is_empty() {
            return Err("provider id cannot be empty".to_string());
        }
        match provider.api.as_str() {
            "anthropic-messages" | "anthropic-auth-token" | "bedrock" | "vertex" | "foundry" => {}
            other => return Err(format!("unsupported Claude Code provider type: {other}")),
        }
        for model in &provider.models {
            if model.id.trim().is_empty() {
                return Err(format!("provider {provider_id} contains an empty model id"));
            }
        }
    }
    Ok(catalog)
}

fn read_catalog() -> Result<ClaudeModelsCatalog, String> {
    let path = catalog_path()?;
    if !path.is_file() {
        return Ok(ClaudeModelsCatalog::default());
    }
    let content = fs::read_to_string(&path)
        .map_err(|error| format!("cannot read {}: {error}", path.display()))?;
    parse_catalog(&content)
}

fn write_synced(path: &Path, bytes: &[u8]) -> Result<(), String> {
    let mut file = OpenOptions::new()
        .create_new(true)
        .write(true)
        .open(path)
        .map_err(|error| format!("cannot create model catalog temp file: {error}"))?;
    file.write_all(bytes)
        .and_then(|_| file.sync_all())
        .map_err(|error| format!("cannot write model catalog temp file: {error}"))
}

#[cfg(windows)]
fn atomic_replace(source: &Path, destination: &Path) -> Result<(), String> {
    use std::os::windows::ffi::OsStrExt;
    use windows_sys::Win32::Storage::FileSystem::{
        MoveFileExW, MOVEFILE_REPLACE_EXISTING, MOVEFILE_WRITE_THROUGH,
    };

    let source_wide = source
        .as_os_str()
        .encode_wide()
        .chain(std::iter::once(0))
        .collect::<Vec<_>>();
    let destination_wide = destination
        .as_os_str()
        .encode_wide()
        .chain(std::iter::once(0))
        .collect::<Vec<_>>();
    let result = unsafe {
        MoveFileExW(
            source_wide.as_ptr(),
            destination_wide.as_ptr(),
            MOVEFILE_REPLACE_EXISTING | MOVEFILE_WRITE_THROUGH,
        )
    };
    if result == 0 {
        return Err(format!(
            "cannot replace Claudeix model catalog: {}",
            std::io::Error::last_os_error()
        ));
    }
    Ok(())
}

#[cfg(not(windows))]
fn atomic_replace(source: &Path, destination: &Path) -> Result<(), String> {
    fs::rename(source, destination)
        .map_err(|error| format!("cannot replace Claudeix model catalog: {error}"))
}

#[tauri::command]
pub fn claude_models_read() -> Result<ClaudeModelsFile, String> {
    read_file(&catalog_path()?)
}

fn read_file(path: &Path) -> Result<ClaudeModelsFile, String> {
    let (exists, content) = match fs::read_to_string(path) {
        Ok(content) => {
            parse_catalog(&content)?;
            (true, content)
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => (
            false,
            serde_json::to_string_pretty(&ClaudeModelsCatalog::default())
                .map_err(|e| e.to_string())?
                + "\n",
        ),
        Err(error) => return Err(format!("cannot read model catalog: {error}")),
    };
    Ok(ClaudeModelsFile {
        path: display_path(&path),
        exists,
        revision: if exists {
            format!("{:x}", Sha256::digest(content.as_bytes()))
        } else {
            "missing".into()
        },
        content,
    })
}

fn revision(path: &Path) -> Result<String, String> {
    match fs::read(path) {
        Ok(bytes) => Ok(format!("{:x}", Sha256::digest(bytes))),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok("missing".into()),
        Err(error) => Err(format!("cannot read model catalog: {error}")),
    }
}

#[tauri::command]
pub fn claude_models_write(
    content: String,
    expected_revision: String,
) -> Result<ClaudeModelsFile, String> {
    write_catalog_file(&catalog_path()?, &content, &expected_revision)
}

fn write_catalog_file(
    path: &Path,
    content: &str,
    expected_revision: &str,
) -> Result<ClaudeModelsFile, String> {
    static WRITE_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());
    let _process_lock = WRITE_LOCK
        .lock()
        .map_err(|_| "model catalog lock poisoned")?;
    parse_catalog(content)?;
    // Preserve extension fields instead of round-tripping through the typed subset.
    let value: serde_json::Value = serde_json::from_str(content).map_err(|e| e.to_string())?;
    let normalized = serde_json::to_string_pretty(&value).map_err(|e| e.to_string())? + "\n";
    let parent = path.parent().ok_or("model catalog path has no parent")?;
    fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    let mut lock_options = OpenOptions::new();
    lock_options
        .create(true)
        .truncate(false)
        .read(true)
        .write(true);
    #[cfg(windows)]
    {
        use std::os::windows::fs::OpenOptionsExt;
        lock_options.share_mode(0);
    }
    let _lock = lock_options.open(parent.join("models.lock")).map_err(|_| {
        "model catalog is busy; retry after the other operation finishes".to_string()
    })?;
    if revision(path)? != expected_revision {
        return Err("Conflict: model catalog changed; reload before saving".into());
    }
    // A malformed existing file must be repaired explicitly, never replaced by an empty UI state.
    if path.exists() {
        read_file(path)?;
    }
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let temp = parent.join(format!(
        ".models.claudeix-{}-{}.tmp",
        std::process::id(),
        COUNTER.fetch_add(1, Ordering::Relaxed)
    ));
    if let Err(error) = write_synced(&temp, normalized.as_bytes()) {
        let _ = fs::remove_file(&temp);
        return Err(error);
    }
    let result = (|| {
        if revision(path)? != expected_revision {
            return Err("Conflict: model catalog changed before commit".into());
        }
        atomic_replace(&temp, path)?;
        Ok(ClaudeModelsFile {
            path: display_path(path),
            exists: true,
            revision: format!("{:x}", Sha256::digest(normalized.as_bytes())),
            content: normalized,
        })
    })();
    if result.is_err() {
        let _ = fs::remove_file(&temp);
    }
    result
}

#[tauri::command]
pub async fn claude_models_fetch(
    base_url: String,
    api: String,
    api_key: Option<String>,
) -> Result<Vec<String>, String> {
    let base = base_url.trim().trim_end_matches('/');
    if base.is_empty() {
        return Err("Base URL is required for model discovery".to_string());
    }
    let url = if base.ends_with("/v1") {
        format!("{base}/models")
    } else {
        format!("{base}/v1/models")
    };

    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(30))
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .map_err(|error| format!("cannot build HTTP client: {error}"))?;
    let mut request = client.get(url).header("anthropic-version", "2023-06-01");
    if let Some(key) = api_key.as_deref().filter(|value| !value.trim().is_empty()) {
        request = match api.as_str() {
            "anthropic-messages" => request.header("x-api-key", key.trim()),
            "anthropic-auth-token" => request.bearer_auth(key.trim()),
            _ => return Err("unsupported discovery authentication".into()),
        };
    }
    let response = request
        .send()
        .await
        .map_err(|error| format!("model discovery request failed: {error}"))?;
    let status = response.status();
    if !status.is_success() {
        return Err(format!("provider returned HTTP {status}"));
    }
    let value: serde_json::Value = response
        .json()
        .await
        .map_err(|error| format!("provider returned invalid JSON: {error}"))?;
    let list = value
        .get("data")
        .and_then(serde_json::Value::as_array)
        .or_else(|| value.get("models").and_then(serde_json::Value::as_array))
        .ok_or_else(|| "provider response has no data/models array".to_string())?;

    let mut ids = Vec::new();
    for item in list {
        let id = item
            .get("id")
            .and_then(serde_json::Value::as_str)
            .or_else(|| item.get("name").and_then(serde_json::Value::as_str));
        if let Some(id) = id {
            let id = id.trim_start_matches("models/").trim();
            if !id.is_empty() && !ids.iter().any(|existing| existing == id) {
                ids.push(id.to_string());
            }
        }
    }
    if ids.is_empty() {
        return Err("provider returned an empty model list".to_string());
    }
    Ok(ids)
}

pub fn configure_runtime_provider(
    command: &mut Command,
    provider_id: Option<&str>,
    model_id: Option<&str>,
) -> Result<(), String> {
    let Some(provider_id) = provider_id.filter(|value| !value.trim().is_empty()) else {
        return Ok(());
    };
    let catalog = read_catalog()?;
    let provider = catalog
        .providers
        .get(provider_id)
        .ok_or_else(|| format!("Claude Code provider not found: {provider_id}"))?;

    if !model_id
        .map(|id| provider.models.iter().any(|model| model.id == id))
        .unwrap_or(false)
    {
        return Err("selected provider/model is no longer available; select a model again".into());
    }

    for key in [
        "CLAUDE_CODE_USE_BEDROCK",
        "CLAUDE_CODE_USE_VERTEX",
        "CLAUDE_CODE_USE_FOUNDRY",
        "ANTHROPIC_BASE_URL",
        "ANTHROPIC_API_KEY",
        "ANTHROPIC_AUTH_TOKEN",
        "ANTHROPIC_BEDROCK_BASE_URL",
        "ANTHROPIC_VERTEX_BASE_URL",
        "ANTHROPIC_FOUNDRY_BASE_URL",
        "ANTHROPIC_FOUNDRY_API_KEY",
    ] {
        command.env_remove(key);
    }

    match provider.api.as_str() {
        "anthropic-messages" => {
            if !provider.base_url.trim().is_empty() {
                command.env("ANTHROPIC_BASE_URL", provider.base_url.trim());
            }
            if let Some(key) = provider
                .api_key
                .as_deref()
                .filter(|value| !value.trim().is_empty())
            {
                command.env("ANTHROPIC_API_KEY", key.trim());
            }
        }
        "anthropic-auth-token" => {
            if !provider.base_url.trim().is_empty() {
                command.env("ANTHROPIC_BASE_URL", provider.base_url.trim());
            }
            if let Some(key) = provider
                .api_key
                .as_deref()
                .filter(|value| !value.trim().is_empty())
            {
                command.env("ANTHROPIC_AUTH_TOKEN", key.trim());
            }
        }
        "bedrock" => {
            command.env("CLAUDE_CODE_USE_BEDROCK", "1");
            if !provider.base_url.trim().is_empty() {
                command.env("ANTHROPIC_BEDROCK_BASE_URL", provider.base_url.trim());
            }
            if let Some(key) = provider
                .api_key
                .as_deref()
                .filter(|value| !value.trim().is_empty())
            {
                command.env("AWS_BEARER_TOKEN_BEDROCK", key.trim());
            }
        }
        "vertex" => {
            command.env("CLAUDE_CODE_USE_VERTEX", "1");
            if !provider.base_url.trim().is_empty() {
                command.env("ANTHROPIC_VERTEX_BASE_URL", provider.base_url.trim());
            }
        }
        "foundry" => {
            command.env("CLAUDE_CODE_USE_FOUNDRY", "1");
            if !provider.base_url.trim().is_empty() {
                command.env("ANTHROPIC_FOUNDRY_BASE_URL", provider.base_url.trim());
            }
            if let Some(key) = provider
                .api_key
                .as_deref()
                .filter(|value| !value.trim().is_empty())
            {
                command.env("ANTHROPIC_FOUNDRY_API_KEY", key.trim());
            }
        }
        other => return Err(format!("unsupported Claude Code provider type: {other}")),
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_catalog_is_valid() {
        let encoded = serde_json::to_string(&ClaudeModelsCatalog::default()).unwrap();
        assert!(parse_catalog(&encoded).is_ok());
    }

    #[test]
    fn catalog_revision_blocks_stale_writes_and_preserves_extension_fields() {
        let dir = std::env::temp_dir().join(format!(
            "claudeix-models-test-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir_all(&dir).unwrap();
        let path = dir.join("models.json");
        let content =
            r#"{"version":1,"providers":{},"enabledModels":[],"extension":{"keep":true}}"#;
        let first = write_catalog_file(&path, content, "missing").unwrap();
        assert!(first.content.contains("extension"));
        assert!(write_catalog_file(&path, content, "missing")
            .unwrap_err()
            .contains("Conflict"));
        fs::write(&path, "invalid JSON").unwrap();
        assert!(write_catalog_file(&path, content, &revision(&path).unwrap()).is_err());
        assert_eq!(fs::read_to_string(&path).unwrap(), "invalid JSON");
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn rejects_unknown_provider_type() {
        let mut catalog = ClaudeModelsCatalog::default();
        catalog.providers.insert(
            "bad".to_string(),
            ClaudeProvider {
                api: "openai-completions".to_string(),
                ..Default::default()
            },
        );
        assert!(parse_catalog(&serde_json::to_string(&catalog).unwrap()).is_err());
    }
}
