use crate::app_paths::{claudeix_database_path, display_path};
use crate::claude::config::{self, ClaudeConfigInfo};
use crate::claude::locator::{self, ClaudeStatus};
use crate::claude::scanner::{load_session, scan_workspace, SessionCatalog};
use crate::models::session::{SessionArchiveRequest, SessionDetail, SessionSummary};
use crate::transcript::audit;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};
use tauri::State;

fn now_millis() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis() as u64)
        .unwrap_or(0)
}

#[tauri::command]
pub fn claude_status() -> ClaudeStatus {
    locator::status()
}

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
    let mut summaries = sessions
        .into_iter()
        .map(|(summary, _)| summary)
        .collect::<Vec<_>>();
    audit::apply_session_metadata(&claudeix_database_path()?, &mut summaries)?;
    Ok(summaries)
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
    let mut detail = load_session(&identity)?;
    audit::apply_session_metadata(
        &claudeix_database_path()?,
        std::slice::from_mut(&mut detail.summary),
    )?;
    Ok(detail)
}

#[tauri::command]
pub fn session_refresh(
    state: State<'_, SessionCatalog>,
    session_key: String,
) -> Result<SessionDetail, String> {
    session_get(state, session_key)
}

#[tauri::command]
pub fn session_set_archived(
    state: State<'_, SessionCatalog>,
    request: SessionArchiveRequest,
) -> Result<SessionDetail, String> {
    let identity = state
        .0
        .lock()
        .map_err(|error| error.to_string())?
        .get(&request.session_key)
        .cloned()
        .ok_or_else(|| {
            "session is not present in the current catalog; scan the workspace first".to_string()
        })?;
    audit::set_session_archived(
        &claudeix_database_path()?,
        &identity.session_id,
        &display_path(&identity.source_path),
        request.archived,
        now_millis(),
    )?;
    let mut detail = load_session(&identity)?;
    audit::apply_session_metadata(
        &claudeix_database_path()?,
        std::slice::from_mut(&mut detail.summary),
    )?;
    Ok(detail)
}
