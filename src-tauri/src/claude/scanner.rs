use crate::app_paths::{claude_config_dir, claude_projects_dir};
use crate::claude::parser::parse_session_file;
use crate::models::session::{SessionDetail, SessionIdentity, SessionSummary};
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

#[derive(Default)]
pub struct SessionCatalog(pub Mutex<HashMap<String, SessionIdentity>>);

fn collect_jsonl(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect_jsonl(&path, out);
        } else if path.extension().and_then(|value| value.to_str()) == Some("jsonl") {
            let is_subagent = path
                .file_stem()
                .and_then(|value| value.to_str())
                .map(|value| value.starts_with("agent-"))
                .unwrap_or(false);
            if !is_subagent {
                out.push(path);
            }
        }
    }
}

pub fn scan_workspace(workspace_root: &Path) -> Result<Vec<(SessionSummary, SessionIdentity)>, String> {
    let config_dir = claude_config_dir()?;
    let projects_dir = claude_projects_dir()?;
    if !projects_dir.is_dir() {
        return Ok(Vec::new());
    }
    let mut files = Vec::new();
    collect_jsonl(&projects_dir, &mut files);
    let mut sessions = Vec::new();
    for path in files {
        let detail = match parse_session_file(&path, &config_dir, Some(workspace_root)) {
            Ok(detail) => detail,
            Err(error) if error == "session belongs to a different workspace" => continue,
            Err(_) => continue,
        };
        let identity = SessionIdentity {
            config_dir: config_dir.clone(),
            workspace_root: PathBuf::from(&detail.summary.workspace_root),
            session_id: detail.summary.session_id.clone(),
            source_path: PathBuf::from(&detail.summary.source_path),
        };
        sessions.push((detail.summary, identity));
    }
    sessions.sort_by(|left, right| right.0.updated_at.cmp(&left.0.updated_at));
    Ok(sessions)
}

pub fn load_session(identity: &SessionIdentity) -> Result<SessionDetail, String> {
    parse_session_file(
        &identity.source_path,
        &identity.config_dir,
        Some(&identity.workspace_root),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scans_real_workspace_when_requested() {
        let Ok(workspace) = std::env::var("CLAUDEIX_REAL_WORKSPACE") else {
            return;
        };
        let sessions = scan_workspace(Path::new(&workspace)).unwrap();
        assert!(!sessions.is_empty());
        assert!(sessions.iter().all(|(_, identity)| {
            !identity
                .source_path
                .file_stem()
                .and_then(|value| value.to_str())
                .map(|value| value.starts_with("agent-"))
                .unwrap_or(false)
        }));
        let detail = load_session(&sessions[0].1).unwrap();
        assert!(!detail.records.is_empty());
    }
}
