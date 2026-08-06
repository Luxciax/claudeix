use serde::Serialize;
use serde_json::Value;
use std::fs::{self, File};
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::{Arc, Mutex};
use std::time::UNIX_EPOCH;
use tauri::{AppHandle, Emitter, Manager, State};

pub struct ClaudeProc(pub Mutex<Option<Arc<Mutex<Child>>>>);

impl Default for ClaudeProc {
    fn default() -> Self {
        Self(Mutex::new(None))
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ClaudeStatus {
    installed: bool,
    version: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ClaudeSessionMeta {
    session_id: String,
    cwd: String,
    title: String,
    preview: String,
    source_path: String,
    updated_at: u64,
}

fn claude_command() -> Command {
    #[cfg(windows)]
    {
        let mut command = Command::new("cmd");
        command.args(["/D", "/S", "/C", "claude"]);
        command
    }
    #[cfg(not(windows))]
    {
        Command::new("claude")
    }
}

#[tauri::command]
pub fn claude_status() -> ClaudeStatus {
    let output = claude_command().arg("--version").output();
    match output {
        Ok(output) if output.status.success() => ClaudeStatus {
            installed: true,
            version: String::from_utf8_lossy(&output.stdout).trim().to_string(),
        },
        Ok(output) => ClaudeStatus {
            installed: false,
            version: String::from_utf8_lossy(&output.stderr).trim().to_string(),
        },
        Err(error) => ClaudeStatus {
            installed: false,
            version: error.to_string(),
        },
    }
}

fn collect_jsonl(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect_jsonl(&path, out);
        } else if path.extension().and_then(|value| value.to_str()) == Some("jsonl") {
            out.push(path);
        }
    }
}

fn normalize(path: &str) -> String {
    path.replace('\\', "/").trim_end_matches('/').to_lowercase()
}

fn message_text(value: &Value) -> String {
    let content = value
        .get("message")
        .and_then(|message| message.get("content"))
        .or_else(|| value.get("content"));
    match content {
        Some(Value::String(text)) => text.clone(),
        Some(Value::Array(items)) => items
            .iter()
            .filter_map(|item| item.get("text").and_then(Value::as_str))
            .collect::<Vec<_>>()
            .join("\n"),
        _ => String::new(),
    }
}

fn parse_session(path: &Path, workspace: &str) -> Option<ClaudeSessionMeta> {
    let file = File::open(path).ok()?;
    let mut session_id = String::new();
    let mut cwd = String::new();
    let mut title = String::new();
    let mut preview = String::new();

    for line in BufReader::new(file).lines().map_while(Result::ok) {
        let Ok(value) = serde_json::from_str::<Value>(&line) else {
            continue;
        };
        if session_id.is_empty() {
            session_id = value
                .get("sessionId")
                .or_else(|| value.get("session_id"))
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_string();
        }
        if cwd.is_empty() {
            cwd = value
                .get("cwd")
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_string();
        }
        let kind = value.get("type").and_then(Value::as_str).unwrap_or("");
        if matches!(kind, "user" | "human") {
            let text = message_text(&value).trim().to_string();
            if !text.is_empty() {
                if title.is_empty() {
                    title = text.chars().take(40).collect();
                }
                preview = text.chars().take(100).collect();
            }
        }
    }

    if normalize(&cwd) != normalize(workspace) {
        return None;
    }
    if session_id.is_empty() {
        session_id = path.file_stem()?.to_string_lossy().to_string();
    }
    let updated_at = fs::metadata(path)
        .and_then(|meta| meta.modified())
        .ok()
        .and_then(|time| time.duration_since(UNIX_EPOCH).ok())
        .map(|duration| duration.as_millis() as u64)
        .unwrap_or(0);
    Some(ClaudeSessionMeta {
        session_id,
        cwd,
        title: if title.is_empty() {
            "未命名会话".to_string()
        } else {
            title
        },
        preview,
        source_path: path.to_string_lossy().to_string(),
        updated_at,
    })
}

#[tauri::command]
pub fn claude_sessions(project_root: String) -> Result<Vec<ClaudeSessionMeta>, String> {
    let root = crate::app_paths::claude_projects_dir()?;
    let mut files = Vec::new();
    collect_jsonl(&root, &mut files);
    let mut sessions: Vec<_> = files
        .iter()
        .filter_map(|path| parse_session(path, &project_root))
        .collect();
    sessions.sort_by(|a, b| b.updated_at.cmp(&a.updated_at));
    Ok(sessions)
}

#[tauri::command]
pub fn claude_run(
    app: AppHandle,
    state: State<'_, ClaudeProc>,
    cwd: String,
    prompt: String,
    session_id: Option<String>,
    permission_mode: Option<String>,
) -> Result<(), String> {
    if prompt.trim().is_empty() {
        return Err("prompt is empty".into());
    }
    let mut guard = state.0.lock().map_err(|error| error.to_string())?;
    if guard.is_some() {
        return Err("Claude Code is already running".into());
    }

    let mut command = claude_command();
    command.args([
        "-p",
        prompt.trim(),
        "--output-format",
        "stream-json",
        "--verbose",
        "--include-partial-messages",
        "--permission-mode",
        permission_mode.as_deref().unwrap_or("default"),
    ]);
    if let Some(id) = session_id.filter(|value| !value.trim().is_empty()) {
        command.args(["--resume", id.trim()]);
    }
    command
        .current_dir(&cwd)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());

    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        command.creation_flags(CREATE_NO_WINDOW);
    }

    let mut child = command
        .spawn()
        .map_err(|error| format!("failed to start Claude Code: {error}"))?;
    let stdout = child.stdout.take().ok_or("Claude Code has no stdout")?;
    let stderr = child.stderr.take().ok_or("Claude Code has no stderr")?;
    let child = Arc::new(Mutex::new(child));
    *guard = Some(Arc::clone(&child));
    drop(guard);

    {
        let app = app.clone();
        std::thread::spawn(move || {
            for line in BufReader::new(stdout).lines().map_while(Result::ok) {
                if !line.trim().is_empty() {
                    let _ = app.emit("claude://line", line);
                }
            }
        });
    }
    {
        let app = app.clone();
        std::thread::spawn(move || {
            for line in BufReader::new(stderr).lines().map_while(Result::ok) {
                let _ = app.emit("claude://stderr", line);
            }
        });
    }
    std::thread::spawn(move || {
        let code = child
            .lock()
            .ok()
            .and_then(|mut child| child.wait().ok())
            .and_then(|status| status.code());
        if let Ok(mut running) = app.state::<ClaudeProc>().0.lock() {
            *running = None;
        }
        let _ = app.emit("claude://exit", code);
    });
    Ok(())
}

#[tauri::command]
pub fn claude_stop(state: State<'_, ClaudeProc>) -> Result<(), String> {
    let mut guard = state.0.lock().map_err(|error| error.to_string())?;
    if let Some(child) = guard.take() {
        if let Ok(mut child) = child.lock() {
            let _ = child.kill();
            let _ = child.wait();
        }
    }
    Ok(())
}
