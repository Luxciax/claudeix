use crate::app_paths::{claudeix_data_dir, display_path};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashSet};
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};

const STORE_VERSION: u32 = 1;
const SUPPORTED_KINDS: &[&str] = &["anthropic", "gateway", "bedrock", "vertex", "foundry"];

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct ClaudeProviderModel {
    pub id: String,
    #[serde(default)]
    pub label: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub behaves_as: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct ClaudeProviderProfile {
    pub id: String,
    pub name: String,
    pub kind: String,
    #[serde(default)]
    pub base_url: String,
    #[serde(default)]
    pub api_key: String,
    #[serde(default)]
    pub auth_token: String,
    #[serde(default)]
    pub model: String,
    #[serde(default)]
    pub opus_model: String,
    #[serde(default)]
    pub sonnet_model: String,
    #[serde(default)]
    pub haiku_model: String,
    #[serde(default)]
    pub subagent_model: String,
    #[serde(default)]
    pub enable_model_discovery: bool,
    #[serde(default)]
    pub model_picker_enabled: bool,
    #[serde(default)]
    pub replace_builtin_models: bool,
    #[serde(default)]
    pub models: Vec<ClaudeProviderModel>,
    #[serde(default)]
    pub custom_headers: String,
    #[serde(default)]
    pub extra_env: BTreeMap<String, String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ClaudeProviderStore {
    pub version: u32,
    #[serde(default)]
    pub active_profile_id: Option<String>,
    #[serde(default)]
    pub profiles: Vec<ClaudeProviderProfile>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,
}

impl Default for ClaudeProviderStore {
    fn default() -> Self {
        Self {
            version: STORE_VERSION,
            active_profile_id: None,
            profiles: Vec::new(),
            path: None,
        }
    }
}

fn store_path() -> Result<PathBuf, String> {
    Ok(claudeix_data_dir()?.join("claude-providers.json"))
}

fn validate(store: &ClaudeProviderStore) -> Result<(), String> {
    if store.version != STORE_VERSION {
        return Err(format!("unsupported Claude provider store version: {}", store.version));
    }

    let mut ids = HashSet::new();
    for profile in &store.profiles {
        let id = profile.id.trim();
        if id.is_empty() {
            return Err("provider id cannot be empty".to_string());
        }
        if !ids.insert(id.to_string()) {
            return Err(format!("duplicate provider id: {id}"));
        }
        if profile.name.trim().is_empty() {
            return Err(format!("provider {id} has an empty name"));
        }
        if !SUPPORTED_KINDS.contains(&profile.kind.as_str()) {
            return Err(format!("provider {id} uses unsupported kind: {}", profile.kind));
        }

        let mut model_ids = HashSet::new();
        for model in &profile.models {
            let model_id = model.id.trim();
            if model_id.is_empty() {
                return Err(format!("provider {id} contains a model with an empty id"));
            }
            if !model_ids.insert(model_id.to_string()) {
                return Err(format!("provider {id} contains duplicate model id: {model_id}"));
            }
        }
    }

    if let Some(active) = store.active_profile_id.as_deref() {
        if !active.trim().is_empty() && !ids.contains(active) {
            return Err("active provider profile does not exist".to_string());
        }
    }
    Ok(())
}

fn read_store_from(path: &Path) -> Result<ClaudeProviderStore, String> {
    if !path.is_file() {
        let mut store = ClaudeProviderStore::default();
        store.path = Some(display_path(path));
        return Ok(store);
    }

    let content = fs::read_to_string(path)
        .map_err(|error| format!("cannot read Claude provider profiles: {error}"))?;
    let mut store: ClaudeProviderStore = serde_json::from_str(&content)
        .map_err(|error| format!("Claude provider profiles are invalid JSON: {error}"))?;
    validate(&store)?;
    store.path = Some(display_path(path));
    Ok(store)
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
            "cannot atomically replace Claude provider profiles: {}",
            std::io::Error::last_os_error()
        ));
    }
    Ok(())
}

#[cfg(not(windows))]
fn atomic_replace(source: &Path, destination: &Path) -> Result<(), String> {
    fs::rename(source, destination)
        .map_err(|error| format!("cannot replace provider profile store: {error}"))
}

fn write_store_to(path: &Path, mut store: ClaudeProviderStore) -> Result<ClaudeProviderStore, String> {
    store.version = STORE_VERSION;
    store.path = None;
    validate(&store)?;

    let parent = path
        .parent()
        .ok_or_else(|| "Claude provider profile path has no parent directory".to_string())?;
    fs::create_dir_all(parent)
        .map_err(|error| format!("cannot create Claudeix data directory: {error}"))?;

    let content = serde_json::to_vec_pretty(&store)
        .map_err(|error| format!("cannot serialize Claude provider profiles: {error}"))?;
    let temp = parent.join(format!(".claude-providers-{}.tmp", std::process::id()));
    if temp.exists() {
        fs::remove_file(&temp)
            .map_err(|error| format!("cannot clear stale provider temp file: {error}"))?;
    }

    let mut file = OpenOptions::new()
        .create_new(true)
        .write(true)
        .open(&temp)
        .map_err(|error| format!("cannot create provider temp file: {error}"))?;
    file.write_all(&content)
        .and_then(|_| file.write_all(b"\n"))
        .and_then(|_| file.sync_all())
        .map_err(|error| format!("cannot write provider profiles: {error}"))?;
    drop(file);

    if let Err(error) = atomic_replace(&temp, path) {
        let _ = fs::remove_file(&temp);
        return Err(error);
    }

    read_store_from(path)
}

#[tauri::command]
pub fn claude_provider_profiles_read() -> Result<ClaudeProviderStore, String> {
    read_store_from(&store_path()?)
}

#[tauri::command]
pub fn claude_provider_profiles_write(store: ClaudeProviderStore) -> Result<ClaudeProviderStore, String> {
    write_store_to(&store_path()?, store)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validates_unique_profiles() {
        let mut store = ClaudeProviderStore::default();
        store.profiles.push(ClaudeProviderProfile {
            id: "gateway-a".into(),
            name: "Gateway A".into(),
            kind: "gateway".into(),
            ..Default::default()
        });
        assert!(validate(&store).is_ok());

        store.profiles.push(store.profiles[0].clone());
        assert!(validate(&store).unwrap_err().contains("duplicate"));
    }
}
