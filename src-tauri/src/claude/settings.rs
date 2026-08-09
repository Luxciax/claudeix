use crate::app_paths::{claude_config_dir, display_path};
use crate::claude::locator;
use serde::Serialize;
use serde_json::Value;
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::Stdio;

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ClaudeSettingsFile {
    pub path: String,
    pub exists: bool,
    pub content: String,
}

fn settings_path(scope: &str, workspace_root: Option<&str>) -> Result<PathBuf, String> {
    match scope {
        "global" => Ok(claude_config_dir()?.join("settings.json")),
        "project" => {
            let root = workspace_root
                .filter(|value| !value.trim().is_empty())
                .ok_or_else(|| "workspaceRoot is required for project Claude settings".to_string())?;
            Ok(PathBuf::from(root).join(".claude").join("settings.json"))
        }
        _ => Err(format!("unsupported Claude settings scope: {scope}")),
    }
}

fn validate_settings(content: &str) -> Result<(), String> {
    let value: Value = serde_json::from_str(content)
        .map_err(|error| format!("Claude settings must be valid JSON: {error}"))?;
    if !value.is_object() {
        return Err("Claude settings root must be a JSON object".to_string());
    }
    Ok(())
}

fn write_synced(path: &Path, bytes: &[u8]) -> Result<(), String> {
    let mut file = OpenOptions::new()
        .create_new(true)
        .write(true)
        .open(path)
        .map_err(|error| format!("cannot create settings temp file {}: {error}", path.display()))?;
    file.write_all(bytes)
        .map_err(|error| format!("cannot write settings temp file {}: {error}", path.display()))?;
    file.sync_all()
        .map_err(|error| format!("cannot sync settings temp file {}: {error}", path.display()))
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
            "cannot atomically replace Claude settings {}: {}",
            destination.display(),
            std::io::Error::last_os_error()
        ));
    }
    Ok(())
}

#[cfg(not(windows))]
fn atomic_replace(source: &Path, destination: &Path) -> Result<(), String> {
    fs::rename(source, destination).map_err(|error| {
        format!(
            "cannot atomically replace Claude settings {}: {error}",
            destination.display()
        )
    })
}

#[tauri::command]
pub fn claude_settings_read(
    scope: String,
    workspace_root: Option<String>,
) -> Result<ClaudeSettingsFile, String> {
    let path = settings_path(&scope, workspace_root.as_deref())?;
    let exists = path.is_file();
    let content = if exists {
        fs::read_to_string(&path)
            .map_err(|error| format!("cannot read Claude settings {}: {error}", path.display()))?
    } else {
        "{}\n".to_string()
    };
    if exists {
        validate_settings(&content)?;
    }
    Ok(ClaudeSettingsFile {
        path: display_path(&path),
        exists,
        content,
    })
}

#[tauri::command]
pub fn claude_settings_write(
    scope: String,
    workspace_root: Option<String>,
    content: String,
) -> Result<ClaudeSettingsFile, String> {
    validate_settings(&content)?;
    let path = settings_path(&scope, workspace_root.as_deref())?;
    let parent = path
        .parent()
        .ok_or_else(|| "Claude settings path has no parent directory".to_string())?;
    fs::create_dir_all(parent)
        .map_err(|error| format!("cannot create Claude settings directory {}: {error}", parent.display()))?;

    if path.is_file() {
        let backup = path.with_file_name("settings.json.claudeix.bak");
        fs::copy(&path, &backup)
            .map_err(|error| format!("cannot back up Claude settings to {}: {error}", backup.display()))?;
    }

    let temp = parent.join(format!(".settings.claudeix-{}.tmp", std::process::id()));
    if temp.exists() {
        return Err(format!("settings temp path already exists: {}", temp.display()));
    }
    if let Err(error) = write_synced(&temp, content.as_bytes()).and_then(|_| atomic_replace(&temp, &path)) {
        let _ = fs::remove_file(&temp);
        return Err(error);
    }
    claude_settings_read(scope, workspace_root)
}

#[tauri::command]
pub async fn claude_plugins_list(available: Option<bool>) -> Result<Value, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let mut command = locator::build_command()?;
        command.args(["plugin", "list", "--json"]);
        if available.unwrap_or(false) {
            command.arg("--available");
        }
        let output = command
            .stdin(Stdio::null())
            .output()
            .map_err(|error| format!("cannot list Claude plugins: {error}"))?;
        if !output.status.success() {
            return Err(String::from_utf8_lossy(&output.stderr).trim().to_string());
        }
        serde_json::from_slice::<Value>(&output.stdout)
            .map_err(|error| format!("Claude plugin list returned invalid JSON: {error}"))
    })
    .await
    .map_err(|error| format!("Claude plugin worker failed: {error}"))?
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ClaudePluginActionResult {
    pub action: String,
    pub plugin: String,
    pub output: String,
}

#[tauri::command]
pub async fn claude_plugin_action(
    action: String,
    plugin: String,
) -> Result<ClaudePluginActionResult, String> {
    let action = action.trim().to_ascii_lowercase();
    if !matches!(
        action.as_str(),
        "enable" | "disable" | "install" | "uninstall" | "update"
    ) {
        return Err(format!("unsupported Claude plugin action: {action}"));
    }
    let plugin = plugin.trim().to_string();
    if plugin.is_empty() {
        return Err("plugin name is required".to_string());
    }
    let action_for_worker = action.clone();
    let plugin_for_worker = plugin.clone();
    let output = tauri::async_runtime::spawn_blocking(move || {
        let mut command = locator::build_command()?;
        command.args(["plugin", action_for_worker.as_str(), plugin_for_worker.as_str()]);
        let output = command
            .stdin(Stdio::null())
            .output()
            .map_err(|error| format!("cannot run Claude plugin {action_for_worker}: {error}"))?;
        let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        if !output.status.success() {
            return Err(if stderr.is_empty() { stdout } else { stderr });
        }
        Ok(if stdout.is_empty() { stderr } else { stdout })
    })
    .await
    .map_err(|error| format!("Claude plugin worker failed: {error}"))??;

    Ok(ClaudePluginActionResult {
        action,
        plugin,
        output,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validates_object_settings_only() {
        assert!(validate_settings(r#"{"model":"opus"}"#).is_ok());
        assert!(validate_settings("[]").is_err());
        assert!(validate_settings("not json").is_err());
    }
}
