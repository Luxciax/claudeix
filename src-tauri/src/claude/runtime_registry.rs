use crate::claude::event_parser::RuntimeEventParser;
use crate::claude::locator;
use crate::claude::scanner::SessionCatalog;
use crate::models::runtime::{
    ExternalRuntimeInfo, PermissionDecisionRequest, RuntimeEvent, RuntimeSnapshot,
    RuntimeStartRequest, RuntimeStatus,
};
use crate::models::session::SessionIdentity;
use crate::transcript::editor::{session_has_pending_recovery, TranscriptEditor};
use crate::trash::service::TrashService;
use std::collections::{HashMap, HashSet};
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tauri::{AppHandle, Emitter, Manager, State};
use sysinfo::System;

const EVENT_NAME: &str = "claudeix://runtime-event";

struct RuntimeEntry {
    snapshot: RuntimeSnapshot,
    child: Option<Arc<Mutex<Child>>>,
    stdin: Option<Arc<Mutex<ChildStdin>>>,
    lease_key: String,
}

struct PendingPermission {
    runtime_id: String,
}

#[derive(Default)]
struct RegistryState {
    runtimes: HashMap<String, RuntimeEntry>,
    leases: HashMap<String, String>,
    pending_permissions: HashMap<String, PendingPermission>,
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
        stdin: Arc<Mutex<ChildStdin>>,
        lease_key: String,
    ) -> Result<(), String> {
        let mut state = self.state.lock().map_err(|error| error.to_string())?;
        state.runtimes.insert(
            snapshot.runtime_id.clone(),
            RuntimeEntry {
                snapshot,
                child: Some(child),
                stdin: Some(stdin),
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

    fn close_input(&self, runtime_id: &str) {
        if let Ok(mut state) = self.state.lock() {
            if let Some(entry) = state.runtimes.get_mut(runtime_id) {
                entry.stdin = None;
            }
        }
    }

    fn register_permission(&self, runtime_id: &str, request_id: &str) {
        if let Ok(mut state) = self.state.lock() {
            let can_register = state
                .runtimes
                .get(runtime_id)
                .map(|entry| {
                    !matches!(
                        entry.snapshot.status,
                        RuntimeStatus::Stopping | RuntimeStatus::Completed | RuntimeStatus::Failed
                    )
                })
                .unwrap_or(false);
            if !can_register {
                return;
            }
            state.pending_permissions.insert(
                request_id.to_string(),
                PendingPermission {
                    runtime_id: runtime_id.to_string(),
                },
            );
            if let Some(entry) = state.runtimes.get_mut(runtime_id) {
                entry.snapshot.status = RuntimeStatus::WaitingPermission;
            }
        }
    }

    fn decide_permission(
        &self,
        request: &PermissionDecisionRequest,
    ) -> Result<RuntimeSnapshot, String> {
        let stdin = {
            let state = self.state.lock().map_err(|error| error.to_string())?;
            let pending = state
                .pending_permissions
                .get(&request.request_id)
                .ok_or_else(|| "permission request is no longer pending".to_string())?;
            if pending.runtime_id != request.runtime_id {
                return Err("permission request belongs to a different runtime".to_string());
            }
            state
                .runtimes
                .get(&request.runtime_id)
                .and_then(|entry| entry.stdin.as_ref())
                .cloned()
                .ok_or_else(|| "runtime stdin is not available".to_string())?
        };

        let response = permission_response(request)?;
        write_json_line(&stdin, &response)?;

        let mut state = self.state.lock().map_err(|error| error.to_string())?;
        state.pending_permissions.remove(&request.request_id);
        let still_waiting = state
            .pending_permissions
            .values()
            .any(|pending| pending.runtime_id == request.runtime_id);
        let entry = state
            .runtimes
            .get_mut(&request.runtime_id)
            .ok_or_else(|| "runtime not found".to_string())?;
        if entry.snapshot.status == RuntimeStatus::WaitingPermission {
            entry.snapshot.status = if still_waiting {
                RuntimeStatus::WaitingPermission
            } else {
                RuntimeStatus::Running
            };
        }
        Ok(entry.snapshot.clone())
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
            entry.stdin = None;
            (entry.lease_key.clone(), entry.snapshot.clone())
        };
        if state.leases.get(&lease_key).map(String::as_str) == Some(runtime_id) {
            state.leases.remove(&lease_key);
        }
        state
            .pending_permissions
            .retain(|_, pending| pending.runtime_id != runtime_id);
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

    fn owned_pids(&self) -> HashSet<u32> {
        self.state
            .lock()
            .map(|state| {
                state
                    .runtimes
                    .values()
                    .filter(|entry| {
                        !matches!(
                            entry.snapshot.status,
                            RuntimeStatus::Completed | RuntimeStatus::Failed
                        )
                    })
                    .map(|entry| entry.snapshot.pid)
                    .collect()
            })
            .unwrap_or_default()
    }

    pub fn session_active(&self, session_key: &str) -> bool {
        self.state
            .lock()
            .map(|state| {
                state.runtimes.values().any(|entry| {
                    entry.snapshot.session_key.as_deref() == Some(session_key)
                        && !matches!(
                            entry.snapshot.status,
                            RuntimeStatus::Completed | RuntimeStatus::Failed
                        )
                })
            })
            .unwrap_or(false)
    }

    pub fn probe_external_identity(&self, identity: &SessionIdentity) -> ExternalRuntimeInfo {
        let owned_pids = self.owned_pids();
        let current_pid = std::process::id();
        let system = System::new_all();

        for (pid, process) in system.processes() {
            let pid = pid.as_u32();
            if pid == current_pid || owned_pids.contains(&pid) {
                continue;
            }
            let name = process.name();
            let command_line = process
                .cmd()
                .iter()
                .map(String::as_str)
                .collect::<Vec<_>>()
                .join(" ");
            if !looks_like_claude_process(name, process.exe(), &command_line) {
                continue;
            }
            let Some(match_kind) = external_match_kind(
                &command_line,
                process.cwd(),
                &identity.session_id,
                &identity.workspace_root,
            ) else {
                continue;
            };
            return ExternalRuntimeInfo {
                active: true,
                pid: Some(pid),
                executable_path: process
                    .exe()
                    .map(|path| path.to_string_lossy().to_string()),
                match_kind: Some(match_kind.to_string()),
                warning: Some(
                    "An external Claude Code process may be writing to this session. Claudeix will not resume or edit it until the external process exits."
                        .to_string(),
                ),
            };
        }

        ExternalRuntimeInfo {
            active: false,
            pid: None,
            executable_path: None,
            match_kind: None,
            warning: None,
        }
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

fn looks_like_claude_process(name: &str, executable: Option<&Path>, command_line: &str) -> bool {
    let name = name.to_lowercase();
    let executable = executable
        .and_then(|path| path.file_name())
        .map(|value| value.to_string_lossy().to_lowercase())
        .unwrap_or_default();
    let command_line = command_line.to_lowercase();
    matches!(name.as_str(), "claude" | "claude.exe")
        || matches!(executable.as_str(), "claude" | "claude.exe")
        || command_line.contains("claude-code")
        || command_line.contains("claude.cmd")
        || command_line.contains("claude.exe")
}

fn external_match_kind(
    command_line: &str,
    cwd: Option<&Path>,
    session_id: &str,
    workspace: &Path,
) -> Option<&'static str> {
    if !session_id.is_empty()
        && command_line
            .to_lowercase()
            .contains(&session_id.to_lowercase())
    {
        return Some("session_id");
    }
    let workspace = normalized(workspace);
    if cwd.map(normalized).as_deref() == Some(workspace.as_str()) {
        return Some("workspace");
    }
    None
}

fn cli_permission_mode(value: &str) -> Result<&'static str, String> {
    match value {
        "manual" | "default" => Ok("manual"),
        "acceptEdits" => Ok("acceptEdits"),
        "plan" => Ok("plan"),
        _ => Err("unsupported permission mode".to_string()),
    }
}

fn initial_user_message(prompt: &str) -> serde_json::Value {
    serde_json::json!({
        "type": "user",
        "session_id": "",
        "message": {
            "role": "user",
            "content": [{ "type": "text", "text": prompt }]
        },
        "parent_tool_use_id": null
    })
}

fn permission_response(request: &PermissionDecisionRequest) -> Result<serde_json::Value, String> {
    match request.decision.as_str() {
        "allow" => Ok(serde_json::json!({
            "type": "control_response",
            "response": {
                "subtype": "success",
                "request_id": request.request_id,
                "response": { "behavior": "allow" }
            }
        })),
        "deny" => Ok(serde_json::json!({
            "type": "control_response",
            "response": {
                "subtype": "success",
                "request_id": request.request_id,
                "response": {
                    "behavior": "deny",
                    "message": request.message.as_deref().unwrap_or("Denied in Claudeix"),
                    "interrupt": false
                }
            }
        })),
        _ => Err("permission decision must be allow or deny".to_string()),
    }
}

fn emit(app: &AppHandle, event: RuntimeEvent) {
    let _ = app.emit(EVENT_NAME, event);
}

fn write_json_line(stdin: &Arc<Mutex<ChildStdin>>, value: &serde_json::Value) -> Result<(), String> {
    let line = serde_json::to_string(value).map_err(|error| error.to_string())?;
    let mut stdin = stdin.lock().map_err(|error| error.to_string())?;
    stdin
        .write_all(line.as_bytes())
        .and_then(|_| stdin.write_all(b"\r\n"))
        .and_then(|_| stdin.flush())
        .map_err(|error| format!("failed to write to Claude Code stdin: {error}"))
}

fn terminate_owned_process(pid: u32, child: &Arc<Mutex<Child>>) -> Result<(), String> {
    #[cfg(windows)]
    {
        let mut command = std::process::Command::new("taskkill");
        crate::process_utils::hide_console(&mut command);
        let taskkill = command
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
    editor: State<'_, TranscriptEditor>,
    trash: State<'_, TrashService>,
    catalog: State<'_, SessionCatalog>,
    request: RuntimeStartRequest,
) -> Result<RuntimeSnapshot, String> {
    let prompt = request.prompt.trim();
    if prompt.is_empty() {
        return Err("prompt is empty".to_string());
    }
    let permission_mode = cli_permission_mode(&request.permission_mode)?;

    let workspace = canonical_workspace(&request.workspace_root)?;
    let identity = if let Some(session_key) = request.session_key.as_deref() {
        if editor.is_locked(session_key) {
            return Err("this Claude session is currently being edited or restored".to_string());
        }
        if trash.is_locked(session_key) {
            return Err("this Claude session is currently being moved to the recycle bin".to_string());
        }
        let identity = catalog
            .0
            .lock()
            .map_err(|error| error.to_string())?
            .get(session_key)
            .cloned()
            .ok_or_else(|| "session is not present in the current catalog".to_string())?;
        if session_has_pending_recovery(&identity)? {
            return Err(
                "this Claude session has an unfinished edit/restore transaction; resolve recovery before running it"
                    .to_string(),
            );
        }
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
    if let Some(session_key) = request.session_key.as_deref() {
        if editor.is_locked(session_key) {
            registry.release_reservation(&lease_key, &runtime_id);
            return Err("this Claude session started an edit while runtime startup was being prepared".to_string());
        }
        if trash.is_locked(session_key) {
            registry.release_reservation(&lease_key, &runtime_id);
            return Err("this Claude session started a recycle-bin transaction while runtime startup was being prepared".to_string());
        }
    }

    let mut command = match locator::build_command() {
        Ok(command) => command,
        Err(error) => {
            registry.release_reservation(&lease_key, &runtime_id);
            return Err(error);
        }
    };
    if let Err(error) =
        crate::claude::models::configure_runtime_provider(&mut command, request.provider_id.as_deref())
    {
        registry.release_reservation(&lease_key, &runtime_id);
        return Err(error);
    }
    command.args([
        "-p",
        "--output-format",
        "stream-json",
        "--input-format",
        "stream-json",
        "--verbose",
        "--include-partial-messages",
        "--permission-prompt-tool",
        "stdio",
        "--permission-mode",
        permission_mode,
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
        .stdin(Stdio::piped())
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
    let stdin = match child.stdin.take() {
        Some(stdin) => Arc::new(Mutex::new(stdin)),
        None => {
            let _ = child.kill();
            registry.release_reservation(&lease_key, &runtime_id);
            return Err("Claude Code has no stdin".to_string());
        }
    };
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
    if let Err(error) = registry.register(
        snapshot.clone(),
        Arc::clone(&child),
        Arc::clone(&stdin),
        lease_key.clone(),
    ) {
        registry.release_reservation(&lease_key, &runtime_id);
        if let Ok(mut child) = child.lock() {
            let _ = child.kill();
        }
        return Err(error);
    }

    let initial_message = initial_user_message(prompt);
    if let Err(error) = write_json_line(&stdin, &initial_message) {
        let _ = terminate_owned_process(pid, &child);
        let _ = registry.finish(&runtime_id, None, Some(error.clone()));
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
            let turn_finished = serde_json::from_str::<serde_json::Value>(&line)
                .ok()
                .and_then(|value| {
                    value
                        .get("type")
                        .and_then(|kind| kind.as_str())
                        .map(str::to_string)
                })
                .as_deref()
                == Some("result");
            for event in parser.parse_line(&line, stdout_session_key.as_deref()) {
                match &event {
                    RuntimeEvent::SessionIdentified { session_id, .. } => {
                        stdout_app
                            .state::<RuntimeRegistry>()
                            .identify_session(&stdout_runtime_id, session_id);
                    }
                    RuntimeEvent::PermissionRequested { request_id, .. } => {
                        stdout_app
                            .state::<RuntimeRegistry>()
                            .register_permission(&stdout_runtime_id, request_id);
                    }
                    _ => {}
                }
                emit(&stdout_app, event);
            }
            if turn_finished {
                stdout_app
                    .state::<RuntimeRegistry>()
                    .close_input(&stdout_runtime_id);
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
pub fn runtime_permission_decide(
    registry: State<'_, RuntimeRegistry>,
    request: PermissionDecisionRequest,
) -> Result<RuntimeSnapshot, String> {
    registry.decide_permission(&request)
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

#[tauri::command]
pub fn runtime_probe_external(
    registry: State<'_, RuntimeRegistry>,
    catalog: State<'_, SessionCatalog>,
    session_key: String,
) -> Result<ExternalRuntimeInfo, String> {
    let identity = catalog
        .0
        .lock()
        .map_err(|error| error.to_string())?
        .get(&session_key)
        .cloned()
        .ok_or_else(|| "session is not present in the current catalog".to_string())?;
    Ok(registry.probe_external_identity(&identity))
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

    #[test]
    fn maps_manual_permission_mode_for_current_cli() {
        assert_eq!(cli_permission_mode("manual").unwrap(), "manual");
        assert_eq!(cli_permission_mode("default").unwrap(), "manual");
        assert!(cli_permission_mode("unknown").is_err());
    }

    #[test]
    fn builds_sdk_user_message() {
        let message = initial_user_message("hello");
        assert_eq!(message["type"], "user");
        assert_eq!(message["message"]["role"], "user");
        assert_eq!(message["message"]["content"][0]["text"], "hello");
        assert!(message["parent_tool_use_id"].is_null());
    }

    #[test]
    fn builds_allow_and_deny_permission_responses() {
        let allow = permission_response(&PermissionDecisionRequest {
            runtime_id: "runtime-1".to_string(),
            request_id: "request-1".to_string(),
            decision: "allow".to_string(),
            message: None,
        })
        .unwrap();
        assert_eq!(allow["response"]["response"]["behavior"], "allow");

        let deny = permission_response(&PermissionDecisionRequest {
            runtime_id: "runtime-1".to_string(),
            request_id: "request-2".to_string(),
            decision: "deny".to_string(),
            message: Some("not now".to_string()),
        })
        .unwrap();
        assert_eq!(deny["response"]["response"]["behavior"], "deny");
        assert_eq!(deny["response"]["response"]["message"], "not now");
    }

    #[test]
    fn detects_external_claude_by_session_or_workspace() {
        assert!(looks_like_claude_process(
            "node.exe",
            None,
            r#"node C:\npm\node_modules\@cometix\claude-code\cli.js --resume session-1"#,
        ));
        assert_eq!(
            external_match_kind(
                "claude --resume session-1",
                None,
                "session-1",
                Path::new(r"C:\work"),
            ),
            Some("session_id")
        );
        assert_eq!(
            external_match_kind(
                "claude",
                Some(Path::new(r"C:\work")),
                "session-2",
                Path::new(r"C:\work"),
            ),
            Some("workspace")
        );
    }
}
