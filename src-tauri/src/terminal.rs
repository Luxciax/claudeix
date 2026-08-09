use serde::Serialize;
use std::path::Path;
use std::process::Command;

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TerminalCommandResult {
    pub output: String,
    pub exit_code: Option<i32>,
    pub cancelled: bool,
    pub truncated: bool,
    pub full_output_path: Option<String>,
}

fn run_command(command: &str, cwd: &str) -> Result<TerminalCommandResult, String> {
    let dir = Path::new(cwd);
    if !dir.is_dir() {
        return Err(format!("terminal cwd is not a directory: {cwd}"));
    }

    #[cfg(target_os = "windows")]
    let output = {
        let mut child = Command::new("cmd");
        crate::process_utils::hide_console(&mut child);
        child
            .args(["/D", "/S", "/C", command])
            .current_dir(dir)
            .output()
            .map_err(|error| format!("cannot start terminal command: {error}"))?
    };

    #[cfg(not(target_os = "windows"))]
    let output = Command::new("sh")
        .args(["-lc", command])
        .current_dir(dir)
        .output()
        .map_err(|error| format!("cannot start terminal command: {error}"))?;

    let mut text = String::from_utf8_lossy(&output.stdout).into_owned();
    text.push_str(&String::from_utf8_lossy(&output.stderr));

    Ok(TerminalCommandResult {
        output: text,
        exit_code: output.status.code(),
        cancelled: false,
        truncated: false,
        full_output_path: None,
    })
}

#[tauri::command]
pub async fn terminal_exec(command: String, cwd: String) -> Result<TerminalCommandResult, String> {
    if command.trim().is_empty() {
        return Ok(TerminalCommandResult {
            output: String::new(),
            exit_code: Some(0),
            cancelled: false,
            truncated: false,
            full_output_path: None,
        });
    }

    tauri::async_runtime::spawn_blocking(move || run_command(&command, &cwd))
        .await
        .map_err(|error| format!("terminal worker failed: {error}"))?
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn runs_command_in_requested_workspace() {
        let cwd = std::env::current_dir().unwrap();
        #[cfg(target_os = "windows")]
        let command = "cd";
        #[cfg(not(target_os = "windows"))]
        let command = "pwd";

        let result = run_command(command, &cwd.to_string_lossy()).unwrap();
        assert_eq!(result.exit_code, Some(0));
        assert!(!result.output.trim().is_empty());
    }
}
