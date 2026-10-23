use crate::process_utils;
use std::io::{BufRead, BufReader, Write};
use std::process::{Child, ChildStdin, Stdio};
use std::sync::{Arc, Mutex};
use tauri::{AppHandle, Emitter, State};

#[derive(Default, Clone)]
pub struct PiRuntimeState {
    inner: Arc<Mutex<Option<PiProcess>>>,
}

struct PiProcess {
    pid: u32,
    cwd: Option<String>,
    resume_path: Option<String>,
    child: Arc<Mutex<Child>>,
    stdin: Arc<Mutex<ChildStdin>>,
}

impl PiRuntimeState {
    pub fn pid(&self) -> Option<u32> {
        self.inner
            .lock()
            .ok()
            .and_then(|slot| slot.as_ref().map(|process| process.pid))
    }

    pub fn conflicts_with(&self, session_key: &str, workspace_root: &str) -> bool {
        let Ok(slot) = self.inner.lock() else {
            return true;
        };
        let Some(process) = slot.as_ref() else {
            return false;
        };
        if let Some(resume_path) = process.resume_path.as_deref() {
            return crate::app_paths::normalize_path_key(std::path::Path::new(resume_path))
                == crate::app_paths::normalize_path_key(std::path::Path::new(session_key));
        }
        process.cwd.as_deref().is_some_and(|cwd| {
            crate::app_paths::normalize_path_key(std::path::Path::new(cwd))
                == crate::app_paths::normalize_path_key(std::path::Path::new(workspace_root))
        })
    }

    pub fn stop_all(&self) {
        let process = self.inner.lock().ok().and_then(|mut slot| slot.take());
        if let Some(process) = process {
            if let Ok(mut child) = process.child.lock() {
                let _ = child.kill();
                let _ = child.wait();
            }
        }
    }

    fn clear_if_pid(&self, pid: u32) {
        if let Ok(mut slot) = self.inner.lock() {
            if slot.as_ref().is_some_and(|process| process.pid == pid) {
                *slot = None;
            }
        }
    }
}

#[tauri::command]
pub fn pi_start(
    app: AppHandle,
    state: State<'_, PiRuntimeState>,
    cwd: Option<String>,
    binary: Option<String>,
    resume_path: Option<String>,
) -> Result<(), String> {
    let mut slot = state.inner.lock().map_err(|error| error.to_string())?;
    if slot.is_some() {
        return Ok(());
    }

    let binary_label = binary.as_deref().unwrap_or("pi");
    let mut command = crate::pi_command::command(binary.as_deref())?;
    command.args(["--mode", "rpc"]);
    let normalized_resume_path = resume_path
        .as_deref()
        .map(str::trim)
        .filter(|path| !path.is_empty())
        .map(str::to_string);
    if let Some(path) = normalized_resume_path.as_deref() {
        command.args(["--session", path]);
    }
    let normalized_cwd = cwd
        .as_deref()
        .map(str::trim)
        .filter(|cwd| !cwd.is_empty())
        .map(str::to_string);
    if let Some(cwd) = normalized_cwd.as_deref() {
        let path = std::path::Path::new(cwd);
        if !path.is_dir() {
            return Err(format!("Pi runtime workspace does not exist: {cwd}"));
        }
        command.current_dir(path);
    }
    command
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    process_utils::hide_console(&mut command);

    let mut child = command
        .spawn()
        .map_err(|error| format!("failed to spawn Pi CLI `{binary_label}`: {error}"))?;
    let pid = child.id();
    let stdin = child.stdin.take().ok_or("Pi runtime has no stdin")?;
    let stdout = child.stdout.take().ok_or("Pi runtime has no stdout")?;
    let stderr = child.stderr.take().ok_or("Pi runtime has no stderr")?;
    let child = Arc::new(Mutex::new(child));

    *slot = Some(PiProcess {
        pid,
        cwd: normalized_cwd,
        resume_path: normalized_resume_path,
        child: Arc::clone(&child),
        stdin: Arc::new(Mutex::new(stdin)),
    });
    drop(slot);

    {
        let app = app.clone();
        let state = state.inner().clone();
        let child = Arc::clone(&child);
        std::thread::spawn(move || {
            for line in BufReader::new(stdout).lines().map_while(Result::ok) {
                if !line.trim().is_empty() {
                    let _ = app.emit("pi://line", line);
                }
            }
            let code = child
                .lock()
                .ok()
                .and_then(|mut child| child.wait().ok())
                .and_then(|status| status.code());
            state.clear_if_pid(pid);
            let _ = app.emit("pi://exit", code);
        });
    }

    {
        let app = app.clone();
        std::thread::spawn(move || {
            for line in BufReader::new(stderr).lines().map_while(Result::ok) {
                if line.contains("Workspace history is disabled for this directory")
                    && line.contains("no project marker found")
                {
                    continue;
                }
                let _ = app.emit("pi://stderr", line);
            }
        });
    }

    Ok(())
}

#[tauri::command]
pub fn pi_send(state: State<'_, PiRuntimeState>, line: String) -> Result<(), String> {
    let stdin = {
        let slot = state.inner.lock().map_err(|error| error.to_string())?;
        slot.as_ref()
            .map(|process| Arc::clone(&process.stdin))
            .ok_or("Pi runtime is not running")?
    };
    let mut stdin = stdin.lock().map_err(|error| error.to_string())?;
    stdin
        .write_all(line.as_bytes())
        .and_then(|_| stdin.write_all(b"\n"))
        .and_then(|_| stdin.flush())
        .map_err(|error| format!("write to Pi runtime failed: {error}"))
}

#[tauri::command]
pub fn pi_stop(state: State<'_, PiRuntimeState>) -> Result<(), String> {
    state.stop_all();
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn runtime_state_starts_empty() {
        let state = PiRuntimeState::default();
        assert!(state.inner.lock().expect("state").is_none());
    }
}
