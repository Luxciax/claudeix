//! Workspace-root bridge used by the Claudeix session UI.

use serde::Serialize;
use std::fs;
use std::path::Path;

const MAX_READ_BYTES: u64 = 4 * 1024 * 1024;

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FsEntry {
    pub name: String,
    pub path: String,
    pub is_dir: bool,
}

/// The workspace root: the last-opened project when one is on record (and its
/// directory still exists), otherwise the process cwd — which keeps the
/// "launch from a project directory" dev workflow working.
#[tauri::command]
pub fn workspace_root() -> Result<String, String> {
    if let Some(project) = crate::projects::last_project() {
        return Ok(project);
    }
    std::env::current_dir()
        .map(|p| p.to_string_lossy().replace('\\', "/"))
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn fs_list_dir(path: String) -> Result<Vec<FsEntry>, String> {
    let dir = Path::new(&path);
    if !dir.is_dir() {
        return Ok(Vec::new());
    }
    let mut entries = fs::read_dir(dir)
        .map_err(|error| format!("cannot list {}: {error}", dir.display()))?
        .filter_map(Result::ok)
        .filter_map(|entry| {
            let file_type = entry.file_type().ok()?;
            Some(FsEntry {
                name: entry.file_name().to_string_lossy().into_owned(),
                path: entry.path().to_string_lossy().replace('\\', "/"),
                is_dir: file_type.is_dir(),
            })
        })
        .collect::<Vec<_>>();
    entries.sort_by(|left, right| {
        right
            .is_dir
            .cmp(&left.is_dir)
            .then_with(|| left.name.to_lowercase().cmp(&right.name.to_lowercase()))
    });
    Ok(entries)
}

#[tauri::command]
pub fn fs_read_file(path: String) -> Result<String, String> {
    let file = Path::new(&path);
    let metadata = fs::metadata(file)
        .map_err(|error| format!("cannot stat {}: {error}", file.display()))?;
    if metadata.len() > MAX_READ_BYTES {
        return Err(format!(
            "file too large to preview: {} bytes (limit {})",
            metadata.len(),
            MAX_READ_BYTES
        ));
    }
    fs::read_to_string(file)
        .map_err(|error| format!("cannot read {}: {error}", file.display()))
}
