use crate::app_paths::{claude_config_dir, claude_projects_dir, display_path};
use serde::Serialize;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ClaudeConfigInfo {
    pub config_dir: String,
    pub projects_dir: String,
    pub projects_dir_exists: bool,
    pub source: String,
}

pub fn resolve() -> Result<ClaudeConfigInfo, String> {
    let config_dir = claude_config_dir()?;
    let projects_dir = claude_projects_dir()?;
    let source = if std::env::var_os("CLAUDE_CONFIG_DIR").is_some() {
        "environment"
    } else {
        "default"
    };
    Ok(ClaudeConfigInfo {
        config_dir: display_path(&config_dir),
        projects_dir: display_path(&projects_dir),
        projects_dir_exists: projects_dir.is_dir(),
        source: source.to_string(),
    })
}
