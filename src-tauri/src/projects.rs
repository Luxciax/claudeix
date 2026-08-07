//! Project selection & recent-projects persistence for the desktop app.
//!
//! State lives in `~/.claudeix/desktop.json`. For one-way migration, Claudeix
//! can still read the legacy Pi Desktop state when the new file does not exist.
//! The legacy file is never modified or deleted.

use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

const MAX_RECENTS: usize = 10;

fn desktop_json_path() -> Result<PathBuf, String> {
    Ok(crate::app_paths::claudeix_data_dir()?.join("desktop.json"))
}

fn legacy_desktop_json_path() -> Result<PathBuf, String> {
    Ok(crate::app_paths::home_dir()?
        .join(".pi")
        .join("agent")
        .join("desktop.json"))
}

#[derive(Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct RecentProject {
    pub path: String,
    pub name: String,
    /// Unix epoch milliseconds — sortable without a datetime dependency.
    pub last_opened_at: u64,
}

#[derive(Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
struct DesktopState {
    last_project: Option<String>,
    recent_projects: Vec<RecentProject>,
}

fn read_state() -> DesktopState {
    for path in [desktop_json_path(), legacy_desktop_json_path()]
        .into_iter()
        .flatten()
    {
        let Ok(content) = fs::read_to_string(path) else {
            continue;
        };
        if let Ok(state) = serde_json::from_str(&content) {
            return state;
        }
    }
    DesktopState::default()
}

fn write_state(state: &DesktopState) -> Result<(), String> {
    let path = desktop_json_path()?;
    if let Some(dir) = path.parent() {
        fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    }
    let json = serde_json::to_string_pretty(state).map_err(|e| e.to_string())?;
    // temp + rename so a crash can't leave a truncated desktop.json
    let tmp = path.with_extension("json.tmp");
    fs::write(&tmp, json).map_err(|e| e.to_string())?;
    fs::rename(&tmp, &path).map_err(|e| e.to_string())
}

/// Forward slashes everywhere; strip the Windows `\\?\` verbatim prefix that
/// `fs::canonicalize` produces.
fn normalize(p: &Path) -> String {
    let s = p.to_string_lossy().replace('\\', "/");
    // Verbatim UNC (`\\?\UNC\server\share`) → `//server/share` so WSL project
    // roots come back as `//wsl.localhost/<distro>/…` the frontend can read.
    if let Some(rest) = s.strip_prefix("//?/UNC/") {
        return format!("//{rest}");
    }
    s.strip_prefix("//?/").map(str::to_string).unwrap_or(s)
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

/// Last opened project that still exists on disk — the preferred workspace root.
pub fn last_project() -> Option<String> {
    read_state().last_project.filter(|p| Path::new(p).is_dir())
}

/// Recent projects whose directories still exist, most recent first.
#[tauri::command]
pub fn projects_recent() -> Vec<RecentProject> {
    read_state()
        .recent_projects
        .into_iter()
        .filter(|r| Path::new(&r.path).is_dir())
        .collect()
}

/// Make `path` the current project: validate, canonicalize, persist as
/// `lastProject` and move to the front of the recents list. Returns the
/// normalized root the frontend should use from now on.
#[tauri::command]
pub fn project_open(path: String) -> Result<String, String> {
    let dir = Path::new(&path);
    if !dir.is_dir() {
        return Err(format!("not a directory: {path}"));
    }
    let canon = fs::canonicalize(dir).unwrap_or_else(|_| dir.to_path_buf());
    let root = normalize(&canon);
    let name = canon
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| root.clone());

    let mut st = read_state();
    st.last_project = Some(root.clone());
    st.recent_projects.retain(|r| r.path != root);
    st.recent_projects.insert(
        0,
        RecentProject {
            path: root.clone(),
            name,
            last_opened_at: now_ms(),
        },
    );
    st.recent_projects.truncate(MAX_RECENTS);
    write_state(&st)?;
    Ok(root)
}

/// Drop one entry from the recents list (the current project is untouched).
#[tauri::command]
pub fn project_remove_recent(path: String) -> Result<Vec<RecentProject>, String> {
    let mut st = read_state();
    st.recent_projects.retain(|r| r.path != path);
    write_state(&st)?;
    Ok(projects_recent())
}

/// Native folder picker. `async` so the blocking dialog runs off the main
/// thread (blocking there would deadlock on macOS).
#[tauri::command]
pub async fn project_pick(app: tauri::AppHandle) -> Result<Option<String>, String> {
    use tauri_plugin_dialog::DialogExt;
    match app.dialog().file().blocking_pick_folder() {
        Some(f) => {
            let p = f.into_path().map_err(|e| e.to_string())?;
            Ok(Some(normalize(&p)))
        }
        None => Ok(None),
    }
}
