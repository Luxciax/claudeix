use serde::Serialize;
use std::path::PathBuf;
use std::process::{Command, Stdio};

#[derive(Debug, Clone)]
pub struct ClaudeExecutable {
    pub path: PathBuf,
    program: PathBuf,
    prefix_args: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ClaudeStatus {
    pub installed: bool,
    pub version: String,
    pub executable_path: Option<String>,
}

fn command_for(executable: &ClaudeExecutable) -> Command {
    let mut command = Command::new(&executable.program);
    crate::process_utils::hide_console(&mut command);
    command.args(&executable.prefix_args);
    command
}

pub fn build_command() -> Result<Command, String> {
    Ok(command_for(&locate()?))
}

#[cfg(windows)]
pub fn locate() -> Result<ClaudeExecutable, String> {
    let mut command = Command::new("where.exe");
    crate::process_utils::hide_console(&mut command);
    let output = command
        .arg("claude")
        .stdin(Stdio::null())
        .output()
        .map_err(|error| format!("failed to search for Claude Code: {error}"))?;
    if !output.status.success() {
        return Err(String::from_utf8_lossy(&output.stderr).trim().to_string());
    }
    let candidates = String::from_utf8_lossy(&output.stdout)
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .map(PathBuf::from)
        .filter(|path| path.is_file())
        .collect::<Vec<_>>();
    let path = candidates
        .iter()
        .find(|path| {
            matches!(
                path.extension()
                    .and_then(|value| value.to_str())
                    .map(str::to_ascii_lowercase),
                Some(extension) if extension == "exe" || extension == "cmd" || extension == "bat"
            )
        })
        .cloned()
        .or_else(|| candidates.first().cloned())
        .ok_or_else(|| "Claude Code was reported by where.exe but no executable file exists".to_string())?;
    resolve_windows_launch(path)
}

#[cfg(windows)]
fn resolve_windows_launch(path: PathBuf) -> Result<ClaudeExecutable, String> {
    let extension = path
        .extension()
        .and_then(|value| value.to_str())
        .map(str::to_ascii_lowercase)
        .unwrap_or_default();
    if extension == "cmd" || extension == "bat" {
        if let (Some(node), Some(script)) = (find_windows_node(&path), find_npm_cli_script(&path)) {
            return Ok(ClaudeExecutable {
                path,
                program: node,
                prefix_args: vec![script.to_string_lossy().to_string()],
            });
        }
        return Ok(ClaudeExecutable {
            path: path.clone(),
            program: PathBuf::from("cmd"),
            prefix_args: vec![
                "/D".to_string(),
                "/S".to_string(),
                "/C".to_string(),
                path.to_string_lossy().to_string(),
            ],
        });
    }
    Ok(ClaudeExecutable {
        program: path.clone(),
        path,
        prefix_args: Vec::new(),
    })
}

#[cfg(windows)]
fn find_npm_cli_script(wrapper: &std::path::Path) -> Option<PathBuf> {
    let root = wrapper.parent()?;
    [
        root.join("node_modules")
            .join("@anthropic-ai")
            .join("claude-code")
            .join("cli.js"),
        root.join("node_modules")
            .join("@cometix")
            .join("claude-code")
            .join("cli.js"),
    ]
    .into_iter()
    .find(|path| path.is_file())
}

#[cfg(windows)]
fn find_windows_node(wrapper: &std::path::Path) -> Option<PathBuf> {
    let bundled = wrapper.parent()?.join("node.exe");
    if bundled.is_file() {
        return Some(bundled);
    }
    let mut command = Command::new("where.exe");
    crate::process_utils::hide_console(&mut command);
    let output = command
        .arg("node.exe")
        .stdin(Stdio::null())
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    String::from_utf8_lossy(&output.stdout)
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .map(PathBuf::from)
        .find(|path| path.is_file())
}

#[cfg(not(windows))]
pub fn locate() -> Result<ClaudeExecutable, String> {
    let output = Command::new("which")
        .arg("claude")
        .stdin(Stdio::null())
        .output()
        .map_err(|error| format!("failed to search for Claude Code: {error}"))?;
    if !output.status.success() {
        return Err(String::from_utf8_lossy(&output.stderr).trim().to_string());
    }
    let path = PathBuf::from(String::from_utf8_lossy(&output.stdout).trim());
    if !path.is_file() {
        return Err("Claude Code executable does not exist".to_string());
    }
    Ok(ClaudeExecutable {
        program: path.clone(),
        path,
        prefix_args: Vec::new(),
    })
}

pub fn status() -> ClaudeStatus {
    let executable = match locate() {
        Ok(executable) => executable,
        Err(error) => {
            return ClaudeStatus {
                installed: false,
                version: error,
                executable_path: None,
            }
        }
    };
    let mut command = command_for(&executable);
    match command.arg("--version").stdin(Stdio::null()).output() {
        Ok(output) if output.status.success() => ClaudeStatus {
            installed: true,
            version: String::from_utf8_lossy(&output.stdout).trim().to_string(),
            executable_path: Some(executable.path.to_string_lossy().to_string()),
        },
        Ok(output) => ClaudeStatus {
            installed: false,
            version: String::from_utf8_lossy(&output.stderr).trim().to_string(),
            executable_path: Some(executable.path.to_string_lossy().to_string()),
        },
        Err(error) => ClaudeStatus {
            installed: false,
            version: error.to_string(),
            executable_path: Some(executable.path.to_string_lossy().to_string()),
        },
    }
}

#[cfg(all(test, windows))]
mod windows_tests {
    use super::*;

    #[test]
    fn npm_cmd_uses_cmd_wrapper() {
        let wrapper = PathBuf::from(r"C:\Users\V\AppData\Roaming\npm\claude.cmd");
        let executable = resolve_windows_launch(wrapper.clone()).unwrap();
        let debug = format!("{:?}", command_for(&executable));
        if find_npm_cli_script(&wrapper).is_some() && find_windows_node(&wrapper).is_some() {
            assert!(debug.contains("node"));
            assert!(debug.contains("cli.js"));
        } else {
            assert!(debug.contains("cmd"));
            assert!(debug.contains("claude.cmd"));
        }
    }
}
