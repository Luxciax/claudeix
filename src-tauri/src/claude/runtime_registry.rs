use crate::claude::event_parser::RuntimeEventParser;
use crate::claude::locator;
use crate::claude::scanner::SessionCatalog;
use crate::models::runtime::{
    RuntimeEvent, RuntimeSnapshot, RuntimeStartRequest, RuntimeStatus,
};
use std::collections::HashMap;
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::process::{Child, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tauri::{AppHandle, Emitter, Manager, State};

const EVENT_NAME: &str = "claudeix://runtime-event";

struct RuntimeEntry {
    snapshot: RuntimeSnapshot,
    child: Option<Arc<Mutex<Child>>>,
    lease_key: String,
}

#[derive(Default)]
struct RegistryState {
    runtimes: HashMap<String, RuntimeEntry>,
    leases: HashMap<String, String>,
}

pub struct RuntimeRegistry {
    state: Mutex<RegistryState>,
    counter: AtomicU64,
}

impl Default for RuntimeRegistry {
    fn default() -> Self {
        Self {
            state: Mutex::new(RegistryState::default()),
            counter: AtomicU64::new(1),
        }
    }
}

impl RuntimeRegistry {
    fn next_runtime_id(&self) -> String {
        let timestamp = now_millis();
        let counter = self.counter.fetch_add(1, Ordering::Relaxed);
        format!("runtime-{timestamp}-{counter}")
    }

    fn reserve_lease(&self, lease_key: &str, runtime_id: &str) -> Result<(), String> {
        let mut state = self.state.lock().map_err(|error| error.to_string())?;
        if let Some(owner) = state.leases.get(lease_key) {
            return Err(format!(
                "this Claude session is already running in runtime {owner}"
            ));
        }
        state
            .leases
            .insert(lease_key.to_string(), runtime_id.to_string());
        Ok(())
    }

    fn release_reservation(&self, lease_key: &str, runtime_id: &str) {
        if let Ok(mut state) = self.state.lock() {
            if state.leases.get(lease_key).map(String::as_str) == Some(runtime_id) {
                state.leases.remove(lease_key);
            }
        }
    }

    fn register(
        &self,
        snapshot: RuntimeSnapshot,
        child: Arc<Mutex<Child>>,
        lease_key: String,
    ) -> Result<(), String> {
        let mut state = self.state.lock().map_err(|error| error.to_string())?;
        state.runtimes.insert(
            snapshot.runtime_id.clone(),
            RuntimeEntry {
                snapshot,
                child: Some(child),
                lease_key,
            },
        );
        Ok(())
    }

    fn identify_session(&self, runtime_id: &str, session_id: &str) {
        if let Ok(mut state) = self.state.lock() {
            if let Some(entry) = state.runtimes.get_mut(runtime_id) {
                entry.snapshot.session_id = Some(session_id.to_string());
                if entry.snapshot.status == RuntimeStatus::Starting {
                    entry.snapshot.status = RuntimeStatus::Running;
                }
            }
        }
    }

    fn mark_running(&self, runtime_id: &str) {
        if let Ok(mut state) = self.state.lock() {
            if let Some(entry) = state.runtimes.get_mut(runtime_id) {
                if entry.snapshot.status == RuntimeStatus::Starting {
                    entry.snapshot.status = RuntimeStatus::Running;
                }
            }
        }
    }

    fn mark_stopping(&self, runtime_id: &str) -> Result<(RuntimeSnapshot, Arc<Mutex<Child>>), String> {
        let mut state = self.state.lock().map_err(|error| error.to_string())?;
        let entry = state
            .runtimes
            .get_mut(runtime_id)
            .ok_or_else(|| "runtime not found".to_string())?;
        if matches!(entry.snapshot.status, RuntimeStatus::Completed | RuntimeStatus::Failed) {
            return Err("runtime has already finished".to_string());
        }
        entry.snapshot.status = RuntimeStatus::Stopping;
        let child = entry
            .child
            .as_ref()
            .cloned()
            .ok_or_else(|| "runtime process is not available".to_string())?;
        Ok((entry.snapshot.clone(), child))
    }

    fn finish(
        &self,
        runtime_id: &str,
        exit_code: Option<i32>,
        error: Option<String>,
    ) -> Option<RuntimeSnapshot> {
        let mut state = self.state.lock().ok()?;
        let (lease_key, snapshot) = {
            let entry = state.runtimes.get_mut(runtime_id)?;
            let was_stopping = entry.snapshot.status == RuntimeStatus::Stopping;
            entry.snapshot.exit_code = exit_code;
            entry.snapshot.error = error.clone();
            entry.snapshot.status = if was_stopping || (exit_code == Some(0) && error.is_none()) {
                RuntimeStatus::Completed
            } else {
                RuntimeStatus::Failed
            };
            entry.child = None;
            (entry.lease_key.clone(), entry.snapshot.clone())
        };
        if state.leases.get(&lease_key).map(String::as_str) == Some(runtime_id) {
            state.leases.remove(&lease_key);
        }
        Some(snapshot)
    }

    pub fn snapshot(&self, runtime_id: &str) -> Result<RuntimeSnapshot, String> {
        self.state
            .lock()
            .map_err(|error| error.to_string())?
            .runtimes
            .get(runtime_id)
            .map(|entry| entry.snapshot.clone())
            .ok_or_else(|| "runtime not found".to_string())
    }

    pub fn list(&self) -> Result<Vec<RuntimeSnapshot>, String> {
        let mut snapshots = self
            .state
            .lock()
            .map_err(|error| error.to_string())?
            .runtimes
            .values()
            .map(|entry| entry.snapshot.clone())
            .collect::<Vec<_>>();
        snapshots.sort_by(|left, right| right.started_at.cmp(&left.started_at));
        Ok(snapshots)
    }

    pub fn stop_all(&self) {
        let running = {
            let Ok(mut state) = self.state.lock() else {
                return;
            };
            state
                .runtimes
                .values_mut()
                .filter_map(|entry| {
                    if matches!(entry.snapshot.status, RuntimeStatus::Completed | RuntimeStatus::Failed) {
                        return None;
                    }
                    entry.snapshot.status = RuntimeStatus::Stopping;
                    entry
                        .child
                        .as_ref()
                        .cloned()
                        .map(|child| (entry.snapshot.pid, child))
                })
                .collect::<Vec<_>>()
        };
        for (pid, child) in running {
            let _ = terminate_owned_process(pid, &child);
        }
    }
}

impl Drop for RuntimeRegistry {
    fn drop(&mut self) {
        self.stop_all();
    }
}

fn now_millis() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis() as u64)
        .unwrap_or(0)
}

fn canonical_workspace(path: &str) -> Result<PathBuf, String> {
    let workspace = PathBuf::from(path);
    if !workspace.is_dir() {
        return Err(format!("workspace does not exist: {}", workspace.display()));
    }
    std::fs::canonicalize(&workspace)
        .map_err(|error| format!("cannot resolve workspace {}: {error}", workspace.display()))
}

fn normalized(path: &Path) -> String {
    path.to_string_lossy()
        .replace('\\', "/")
        .trim_end_matches('/')
        .to_lowercase()
}

fn emit(app: &AppHandle, event: RuntimeEvent) {
    let _ = app.emit(EVENT_NAME, event);
}

fn terminate_owned_process(pid: u32, child: &Arc<Mutex<Child>>) -> Result<(), String> {
    #[cfg(windows)]
    {
        let taskkill = std::process::Command::new("taskkill")
            .args(["/PID", &pid.to_string(), "/T", "/F"])
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status();
        if matches!(taskkill, Ok(status) if status.success()) {
            return Ok(());
        }
    }
    let mut child = child.lock().map_err(|error| error.to_string())?;
    child.kill().map_err(|error| error.to_string())
}

#[tauri::command]
pub fn runtime_start(
    app: AppHandle,
    registry: State<'_, RuntimeRegistry>,
    catalog: State<'_, SessionCatalog>,
    request: RuntimeStartRequest,
) -> Result<RuntimeSnapshot, String> {
    let prompt = request.prompt.trim();
    if prompt.is_empty() {
        return Err("prompt is empty".to_string());
    }
    if !matches!(request.permission_mode.as_str(), "default" | "acceptEdits" | "plan") {
        return Err("unsupported permission mode".to_string());
    }

    let workspace = canonical_workspace(&request.workspace_root)?;
    let identity = if let Some(session_key) = request.session_key.as_deref() {
        let identity = catalog
            .0
            .lock()
            .map_err(|error| error.to_string())?
            .get(session_key)
            .cloned()
            .ok_or_else(|| "session is not present in the current catalog".to_string())?;
        if normalized(&identity.workspace_root) != normalized(&workspace) {
            return Err("session workspace does not match the selected workspace".to_string());
        }
        Some(identity)
    } else {
        None
    };

    let runtime_id = registry.next_runtime_id();
    let lease_key = request
        .session_key
        .as_ref()
        .map(|key| format!("session:{key}"))
        .unwrap_or_else(|| format!("new:{}", normalized(&workspace)));
    registry.reserve_lease(&lease_key, &runtime_id)?;

    let mut command = match locator::build_command() {
        Ok(command) => command,
        Err(error) => {
            registry.release_reservation(&lease_key, &runtime_id);
            return Err(error);
        }
    };
    command.args([
        "-p",
        prompt,
        "--output-format",
        "stream-json",
        "--verbose",
        "--include-partial-messages",
        "--permission-mode",
        request.permission_mode.as_str(),
    ]);
    if let Some(identity) = &identity {
        command.args(["--resume", identity.session_id.as_str()]);
    }
    if let Some(model) = request.model.as_deref().filter(|value| !value.trim().is_empty()) {
        command.args(["--model", model.trim()]);
    }
    if let Some(effort) = request.effort.as_deref().filter(|value| !value.trim().is_empty()) {
        command.args(["--effort", effort.trim()]);
    }
    command
        .current_dir(&workspace)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());

    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        command.creation_flags(CREATE_NO_WINDOW);
    }

    let mut child = match command.spawn() {
        Ok(child) => child,
        Err(error) => {
            registry.release_reservation(&lease_key, &runtime_id);
            return Err(format!("failed to start Claude Code: {error}"));
        }
    };
    let pid = child.id();
    let stdout = match child.stdout.take() {
        Some(stdout) => stdout,
        None => {
            let _ = child.kill();
            registry.release_reservation(&lease_key, &runtime_id);
            return Err("Claude Code has no stdout".to_string());
        }
    };
    let stderr = match child.stderr.take() {
        Some(stderr) => stderr,
        None => {
            let _ = child.kill();
            registry.release_reservation(&lease_key, &runtime_id);
            return Err("Claude Code has no stderr".to_string());
        }
    };
    let child = Arc::new(Mutex::new(child));
    let snapshot = RuntimeSnapshot {
        runtime_id: runtime_id.clone(),
        session_key: request.session_key.clone(),
        session_id: identity.as_ref().map(|identity| identity.session_id.clone()),
        workspace_root: workspace.to_string_lossy().to_string(),
        status: RuntimeStatus::Starting,
        pid,
        started_at: now_millis(),
        exit_code: None,
        error: None,
    };
    if let Err(error) = registry.register(snapshot.clone(), Arc::clone(&child), lease_key.clone()) {
        registry.release_reservation(&lease_key, &runtime_id);
        if let Ok(mut child) = child.lock() {
            let _ = child.kill();
        }
        return Err(error);
    }
    emit(
        &app,
        RuntimeEvent::RuntimeStarted {
            runtime_id: runtime_id.clone(),
            session_key: request.session_key.clone(),
            pid,
        },
    );
    registry.mark_running(&runtime_id);

    let stdout_app = app.clone();
    let stdout_runtime_id = runtime_id.clone();
    let stdout_session_key = request.session_key.clone();
    let stdout_handle = thread::spawn(move || {
        let mut parser = RuntimeEventParser::new(stdout_runtime_id.clone());
        for line in BufReader::new(stdout).lines().map_while(Result::ok) {
            if line.trim().is_empty() {
                continue;
            }
            for event in parser.parse_line(&line, stdout_session_key.as_deref()) {
                if let RuntimeEvent::SessionIdentified { session_id, .. } = &event {
                    stdout_app
                        .state::<RuntimeRegistry>()
                        .identify_session(&stdout_runtime_id, session_id);
                }
                emit(&stdout_app, event);
            }
        }
    });

    let stderr_app = app.clone();
    let stderr_runtime_id = runtime_id.clone();
    let stderr_handle = thread::spawn(move || {
        for line in BufReader::new(stderr).lines().map_while(Result::ok) {
            if !line.trim().is_empty() {
                emit(
                    &stderr_app,
                    RuntimeEvent::Stderr {
                        runtime_id: stderr_runtime_id.clone(),
                        text: line,
                    },
                );
            }
        }
    });

    let monitor_app = app.clone();
    let monitor_runtime_id = runtime_id.clone();
    thread::spawn(move || {
        let exit_status = loop {
            let result = child
                .lock()
                .map_err(|error| error.to_string())
                .and_then(|mut child| child.try_wait().map_err(|error| error.to_string()));
            match result {
                Ok(Some(status)) => break Ok(status),
                Ok(None) => thread::sleep(Duration::from_millis(100)),
                Err(error) => break Err(error),
            }
        };
        let _ = stdout_handle.join();
        let _ = stderr_handle.join();

        let (exit_code, process_error) = match exit_status {
            Ok(status) => (status.code(), None),
            Err(error) => (None, Some(error)),
        };
        let registry = monitor_app.state::<RuntimeRegistry>();
        if let Some(snapshot) = registry.finish(&monitor_runtime_id, exit_code, process_error.clone()) {
            if snapshot.status == RuntimeStatus::Completed {
                emit(
                    &monitor_app,
                    RuntimeEvent::RuntimeCompleted {
                        runtime_id: snapshot.runtime_id,
                        exit_code: snapshot.exit_code,
                        session_id: snapshot.session_id,
                        session_key: snapshot.session_key,
                    },
                );
            } else {
                emit(
                    &monitor_app,
                    RuntimeEvent::RuntimeFailed {
                        runtime_id: snapshot.runtime_id,
                        message: process_error
                            .or(snapshot.error)
                            .unwrap_or_else(|| format!("Claude Code exited with code {:?}", snapshot.exit_code)),
                        recoverable: true,
                        session_id: snapshot.session_id,
                        session_key: snapshot.session_key,
                    },
                );
            }
        }
    });

    Ok(snapshot)
}

#[tauri::command]
pub fn runtime_stop(
    registry: State<'_, RuntimeRegistry>,
    runtime_id: String,
) -> Result<RuntimeSnapshot, String> {
    let (snapshot, child) = registry.mark_stopping(&runtime_id)?;

    terminate_owned_process(snapshot.pid, &child)?;

    registry.snapshot(&runtime_id)
}

#[tauri::command]
pub fn runtime_get(
    registry: State<'_, RuntimeRegistry>,
    runtime_id: String,
) -> Result<RuntimeSnapshot, String> {
    registry.snapshot(&runtime_id)
}

#[tauri::command]
pub fn runtime_list(
    registry: State<'_, RuntimeRegistry>,
) -> Result<Vec<RuntimeSnapshot>, String> {
    registry.list()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn session_lease_rejects_second_runtime_and_releases_reservation() {
        let registry = RuntimeRegistry::default();
        registry.reserve_lease("session:a", "runtime-1").unwrap();
        let error = registry
            .reserve_lease("session:a", "runtime-2")
            .unwrap_err();
        assert!(error.contains("already running"));
        registry.release_reservation("session:a", "runtime-1");
        registry.reserve_lease("session:a", "runtime-2").unwrap();
    }
}
