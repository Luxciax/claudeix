use std::path::{Path, PathBuf};

pub fn home_dir() -> Result<PathBuf, String> {
    if let Some(path) = dirs::home_dir() {
        return Ok(path);
    }

    for key in ["USERPROFILE", "HOME"] {
        if let Some(value) = std::env::var_os(key) {
            if !value.is_empty() {
                return Ok(PathBuf::from(value));
            }
        }
    }

    #[cfg(windows)]
    {
        let drive = std::env::var_os("HOMEDRIVE").unwrap_or_default();
        let tail = std::env::var_os("HOMEPATH").unwrap_or_default();
        if !drive.is_empty() && !tail.is_empty() {
            let mut value = drive;
            value.push(tail);
            return Ok(PathBuf::from(value));
        }
    }

    Err("cannot determine the current user's home directory".into())
}

pub fn claude_config_dir() -> Result<PathBuf, String> {
    if let Some(value) = std::env::var_os("CLAUDE_CONFIG_DIR") {
        if !value.is_empty() {
            return Ok(PathBuf::from(value));
        }
    }
    Ok(home_dir()?.join(".claude"))
}

pub fn claude_projects_dir() -> Result<PathBuf, String> {
    Ok(claude_config_dir()?.join("projects"))
}

pub fn claudeix_data_dir() -> Result<PathBuf, String> {
    Ok(home_dir()?.join(".claudeix"))
}

pub fn claudeix_backups_dir() -> Result<PathBuf, String> {
    Ok(claudeix_data_dir()?.join("backups"))
}

pub fn claude_trash_dir(config_dir: &Path) -> PathBuf {
    config_dir.join(".claudeix-trash")
}

pub fn canonical_or_original(path: &Path) -> PathBuf {
    path.canonicalize().unwrap_or_else(|_| path.to_path_buf())
}

pub fn display_path(path: &Path) -> String {
    path.to_string_lossy().replace('\\', "/")
}

pub fn normalize_path_key(path: &Path) -> String {
    let value = display_path(&canonical_or_original(path))
        .trim_end_matches('/')
        .to_string();
    #[cfg(windows)]
    {
        value.to_lowercase()
    }
    #[cfg(not(windows))]
    {
        value
    }
}
