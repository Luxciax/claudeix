//! Workspace-root bridge used by the Claudeix session UI.

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
