use crate::claude::config::{self, ClaudeConfigInfo};
use crate::claude::scanner::{load_session, scan_workspace, SessionCatalog};
use crate::models::session::{SessionDetail, SessionSummary};
use std::path::PathBuf;
use tauri::State;

#[tauri::command]
pub fn claude_config() -> Result<ClaudeConfigInfo, String> {
    config::resolve()
}

#[tauri::command]
pub fn session_scan(
    state: State<'_, SessionCatalog>,
    workspace_root: String,
    _force: Option<bool>,
) -> Result<Vec<SessionSummary>, String> {
    let sessions = scan_workspace(&PathBuf::from(workspace_root))?;
    let mut catalog = state.0.lock().map_err(|error| error.to_string())?;
    catalog.clear();
    for (summary, identity) in &sessions {
        catalog.insert(summary.key.clone(), identity.clone());
    }
    Ok(sessions.into_iter().map(|(summary, _)| summary).collect())
}

#[tauri::command]
pub fn session_get(
    state: State<'_, SessionCatalog>,
    session_key: String,
) -> Result<SessionDetail, String> {
    let identity = state
        .0
        .lock()
        .map_err(|error| error.to_string())?
        .get(&session_key)
        .cloned()
        .ok_or_else(|| "session is not present in the current catalog; scan the workspace first".to_string())?;
    load_session(&identity)
}

#[tauri::command]
pub fn session_refresh(
    state: State<'_, SessionCatalog>,
    session_key: String,
) -> Result<SessionDetail, String> {
    session_get(state, session_key)
}
