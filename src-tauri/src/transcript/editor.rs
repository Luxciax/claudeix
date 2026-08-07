use crate::app_paths::{claudeix_backups_dir, display_path, normalize_path_key};
use crate::claude::parser::parse_session_file;
use crate::claude::runtime_registry::RuntimeRegistry;
use crate::claude::scanner::{load_session, SessionCatalog};
use crate::models::edit::{
    BackupSnapshot, TranscriptEditRequest, TranscriptEditResult, TranscriptRestoreRequest,
    TranscriptRestoreResult,
};
use crate::models::session::{SessionDetail, SessionIdentity};
use crate::models::transcript::EditTarget;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::collections::HashSet;
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};
use tauri::State;

const MAX_EDIT_BYTES: usize = 4 * 1024 * 1024;

#[derive(Default)]
pub struct TranscriptEditor {
    active_sessions: Mutex<HashSet<String>>,
}

struct EditLease<'a> {
    editor: &'a TranscriptEditor,
    session_key: String,
}

impl Drop for EditLease<'_> {
    fn drop(&mut self) {
        if let Ok(mut active) = self.editor.active_sessions.lock() {
            active.remove(&self.session_key);
        }
    }
}

impl TranscriptEditor {
    fn acquire(&self, session_key: &str) -> Result<EditLease<'_>, String> {
        let mut active = self
            .active_sessions
            .lock()
            .map_err(|error| error.to_string())?;
        if !active.insert(session_key.to_string()) {
            return Err("this Claude session is already being edited or restored".to_string());
        }
        Ok(EditLease {
            editor: self,
            session_key: session_key.to_string(),
        })
    }

    pub fn is_locked(&self, session_key: &str) -> bool {
        self.active_sessions
            .lock()
            .map(|active| active.contains(session_key))
            .unwrap_or(true)
    }
}

#[derive(Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct BackupManifest {
    snapshots: Vec<BackupSnapshot>,
}

fn now_millis() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis() as u64)
        .unwrap_or(0)
}

fn unique_id() -> String {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_nanos())
        .unwrap_or(0);
    format!("{nanos}-{}", std::process::id())
}

fn sha256(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    format!("{:x}", hasher.finalize())
}

fn safe_session_dir(session_id: &str) -> String {
    session_id
        .chars()
        .map(|character| {
            if character.is_ascii_alphanumeric() || matches!(character, '-' | '_') {
                character
            } else {
                '_'
            }
        })
        .collect()
}

fn session_backup_dir(root: &Path, session_id: &str) -> PathBuf {
    root.join(safe_session_dir(session_id))
}

fn manifest_path(root: &Path, session_id: &str) -> PathBuf {
    session_backup_dir(root, session_id).join("manifest.json")
}

fn source_is_inside_projects(identity: &SessionIdentity) -> bool {
    let source = normalize_path_key(&identity.source_path);
    let projects = normalize_path_key(&identity.config_dir.join("projects"));
    source == projects || source.starts_with(&(projects + "/"))
}

fn load_manifest(root: &Path, session_id: &str) -> Result<BackupManifest, String> {
    let path = manifest_path(root, session_id);
    if !path.is_file() {
        return Ok(BackupManifest::default());
    }
    let bytes = fs::read(&path)
        .map_err(|error| format!("cannot read backup manifest {}: {error}", path.display()))?;
    serde_json::from_slice(&bytes)
        .map_err(|error| format!("invalid backup manifest {}: {error}", path.display()))
}

fn write_file_synced(path: &Path, bytes: &[u8], create_new: bool) -> Result<(), String> {
    let mut options = OpenOptions::new();
    options.write(true).create(true);
    if create_new {
        options.create_new(true);
    } else {
        options.truncate(true);
    }
    let mut file = options
        .open(path)
        .map_err(|error| format!("cannot create {}: {error}", path.display()))?;
    file.write_all(bytes)
        .and_then(|_| file.flush())
        .and_then(|_| file.sync_all())
        .map_err(|error| format!("cannot persist {}: {error}", path.display()))
}

fn temp_path_for(destination: &Path) -> Result<PathBuf, String> {
    let parent = destination
        .parent()
        .ok_or_else(|| format!("path has no parent: {}", destination.display()))?;
    let file_name = destination
        .file_name()
        .and_then(|value| value.to_str())
        .unwrap_or("transcript");
    Ok(parent.join(format!(".{file_name}.claudeix-{}.tmp", unique_id())))
}

#[cfg(windows)]
fn atomic_replace(source: &Path, destination: &Path) -> Result<(), String> {
    use std::os::windows::ffi::OsStrExt;
    use windows_sys::Win32::Storage::FileSystem::{
        MoveFileExW, MOVEFILE_REPLACE_EXISTING, MOVEFILE_WRITE_THROUGH,
    };

    let source_wide = source
        .as_os_str()
        .encode_wide()
        .chain(std::iter::once(0))
        .collect::<Vec<_>>();
    let destination_wide = destination
        .as_os_str()
        .encode_wide()
        .chain(std::iter::once(0))
        .collect::<Vec<_>>();
    let result = unsafe {
        MoveFileExW(
            source_wide.as_ptr(),
            destination_wide.as_ptr(),
            MOVEFILE_REPLACE_EXISTING | MOVEFILE_WRITE_THROUGH,
        )
    };
    if result == 0 {
        return Err(format!(
            "cannot atomically replace {}: {}",
            destination.display(),
            std::io::Error::last_os_error()
        ));
    }
    Ok(())
}

#[cfg(not(windows))]
fn atomic_replace(source: &Path, destination: &Path) -> Result<(), String> {
    fs::rename(source, destination).map_err(|error| {
        format!(
            "cannot atomically replace {}: {error}",
            destination.display()
        )
    })
}

fn write_atomic(destination: &Path, bytes: &[u8]) -> Result<(), String> {
    let temp = temp_path_for(destination)?;
    write_file_synced(&temp, bytes, true)?;
    if let Err(error) = atomic_replace(&temp, destination) {
        return Err(error);
    }
    Ok(())
}

fn save_manifest(root: &Path, session_id: &str, manifest: &BackupManifest) -> Result<(), String> {
    let path = manifest_path(root, session_id);
    let parent = path
        .parent()
        .ok_or_else(|| "backup manifest has no parent directory".to_string())?;
    fs::create_dir_all(parent)
        .map_err(|error| format!("cannot create backup directory {}: {error}", parent.display()))?;
    let bytes = serde_json::to_vec_pretty(manifest).map_err(|error| error.to_string())?;
    write_atomic(&path, &bytes)
}

fn create_snapshot(
    root: &Path,
    identity: &SessionIdentity,
    source_bytes: &[u8],
    reason: &str,
    target: Option<EditTarget>,
    old_text_hash: Option<String>,
    new_text_hash: Option<String>,
) -> Result<BackupSnapshot, String> {
    let directory = session_backup_dir(root, &identity.session_id);
    fs::create_dir_all(&directory).map_err(|error| {
        format!(
            "cannot create backup directory {}: {error}",
            directory.display()
        )
    })?;
    let id = unique_id();
    let backup_path = directory.join(format!("{id}-before.jsonl"));
    write_file_synced(&backup_path, source_bytes, true)?;
    let snapshot = BackupSnapshot {
        id,
        session_id: identity.session_id.clone(),
        source_path: display_path(&identity.source_path),
        backup_path: display_path(&backup_path),
        created_at: now_millis(),
        source_hash: sha256(source_bytes),
        file_size: source_bytes.len() as u64,
        reason: reason.to_string(),
        target,
        old_text_hash,
        new_text_hash,
    };
    let mut manifest = load_manifest(root, &identity.session_id)?;
    manifest.snapshots.push(snapshot.clone());
    save_manifest(root, &identity.session_id, &manifest)?;
    Ok(snapshot)
}

fn validate_detail(identity: &SessionIdentity) -> Result<SessionDetail, String> {
    let detail = load_session(identity)?;
    if !detail.diagnostics.is_empty() {
        return Err(format!(
            "session contains {} invalid JSONL line(s); repair them before editing",
            detail.diagnostics.len()
        ));
    }
    Ok(detail)
}

fn validate_candidate(path: &Path, identity: &SessionIdentity) -> Result<(), String> {
    let detail = parse_session_file(
        path,
        &identity.config_dir,
        Some(&identity.workspace_root),
    )?;
    if !detail.diagnostics.is_empty() {
        return Err("edited transcript contains invalid JSONL".to_string());
    }
    if detail.summary.session_id != identity.session_id {
        return Err("edited transcript changed the session id".to_string());
    }
    Ok(())
}

fn split_lines_preserving_newlines(text: &str) -> Vec<String> {
    if text.is_empty() {
        return Vec::new();
    }
    let mut lines = Vec::new();
    let mut start = 0;
    for (index, byte) in text.bytes().enumerate() {
        if byte == b'\n' {
            lines.push(text[start..=index].to_string());
            start = index + 1;
        }
    }
    if start < text.len() {
        lines.push(text[start..].to_string());
    }
    lines
}

fn line_body_and_ending(line: &str) -> (&str, &str) {
    if let Some(body) = line.strip_suffix("\r\n") {
        (body, "\r\n")
    } else if let Some(body) = line.strip_suffix('\n') {
        (body, "\n")
    } else {
        (line, "")
    }
}

fn record_uuid(value: &Value) -> Option<&str> {
    value
        .get("uuid")
        .or_else(|| value.get("id"))
        .and_then(Value::as_str)
}

fn record_role(value: &Value) -> Option<&str> {
    value
        .get("message")
        .and_then(|message| message.get("role"))
        .and_then(Value::as_str)
        .or_else(|| match value.get("type").and_then(Value::as_str) {
            Some("user" | "human") => Some("user"),
            Some("assistant") => Some("assistant"),
            _ => None,
        })
}

fn edit_target_text(
    value: &mut Value,
    target: &EditTarget,
    new_text: &str,
    allow_assistant_text: bool,
) -> Result<String, String> {
    let role = record_role(value);
    if role != Some("user") && !(allow_assistant_text && role == Some("assistant")) {
        return Err("this message is not editable in the current mode".to_string());
    }
    if role == Some("assistant")
        && !(target.field == "text" && target.content_index.is_some())
    {
        return Err("assistant editing only supports visible text content blocks".to_string());
    }
    if let Some(expected_uuid) = target.record_uuid.as_deref() {
        if record_uuid(value) != Some(expected_uuid) {
            return Err("record uuid changed; reload the session before editing".to_string());
        }
    }
    let content = if let Some(message) = value
        .get_mut("message")
        .and_then(Value::as_object_mut)
    {
        message.get_mut("content")
    } else {
        value.get_mut("content")
    }
    .ok_or_else(|| "target record has no editable content".to_string())?;

    match (target.field.as_str(), target.content_index) {
        ("content", None) => {
            let old = content
                .as_str()
                .ok_or_else(|| "target content is no longer a string".to_string())?
                .to_string();
            *content = Value::String(new_text.to_string());
            Ok(old)
        }
        ("text", Some(index)) => {
            let item = content
                .as_array_mut()
                .and_then(|items| items.get_mut(index))
                .ok_or_else(|| "target content block no longer exists".to_string())?;
            if item.get("type").and_then(Value::as_str) != Some("text") {
                return Err("target content block is no longer text".to_string());
            }
            let text = item
                .get_mut("text")
                .ok_or_else(|| "target text field no longer exists".to_string())?;
            let old = text
                .as_str()
                .ok_or_else(|| "target text field is no longer a string".to_string())?
                .to_string();
            *text = Value::String(new_text.to_string());
            Ok(old)
        }
        _ => Err("unsupported edit target".to_string()),
    }
}

fn build_edited_bytes(
    source_bytes: &[u8],
    target: &EditTarget,
    new_text: &str,
    allow_assistant_text: bool,
) -> Result<(Vec<u8>, String), String> {
    let text = String::from_utf8(source_bytes.to_vec())
        .map_err(|_| "Claude transcript is not valid UTF-8".to_string())?;
    let mut lines = split_lines_preserving_newlines(&text);
    let line = lines
        .get_mut(target.line_index)
        .ok_or_else(|| "target JSONL line no longer exists".to_string())?;
    let (body, ending) = line_body_and_ending(line);
    if sha256(body.as_bytes()) != target.expected_record_hash {
        return Err("target record changed; reload the session before editing".to_string());
    }
    let mut value = serde_json::from_str::<Value>(body)
        .map_err(|error| format!("target JSONL line is invalid: {error}"))?;
    let old_text = edit_target_text(&mut value, target, new_text, allow_assistant_text)?;
    let replacement = serde_json::to_string(&value).map_err(|error| error.to_string())?;
    *line = format!("{replacement}{ending}");
    Ok((lines.concat().into_bytes(), old_text))
}

fn identity_from_catalog(
    catalog: &SessionCatalog,
    session_key: &str,
) -> Result<SessionIdentity, String> {
    catalog
        .0
        .lock()
        .map_err(|error| error.to_string())?
        .get(session_key)
        .cloned()
        .ok_or_else(|| "session is not present in the current catalog; scan it first".to_string())
}

fn ensure_editable_runtime_state(
    runtime: &RuntimeRegistry,
    identity: &SessionIdentity,
    session_key: &str,
) -> Result<(), String> {
    if runtime.session_active(session_key) {
        return Err("stop the Claudeix runtime before editing this session".to_string());
    }
    let external = runtime.probe_external_identity(identity);
    if external.active {
        return Err(format!(
            "external Claude Code process {} may be writing this session",
            external
                .pid
                .map(|value| value.to_string())
                .unwrap_or_else(|| "unknown".to_string())
        ));
    }
    Ok(())
}

fn apply_edit(
    identity: &SessionIdentity,
    request: &TranscriptEditRequest,
    backup_root: &Path,
) -> Result<TranscriptEditResult, String> {
    if request.new_text.len() > MAX_EDIT_BYTES {
        return Err("edited text exceeds the 4 MiB safety limit".to_string());
    }
    if !source_is_inside_projects(identity) {
        return Err("session source path is outside the Claude projects directory".to_string());
    }
    let current = validate_detail(identity)?;
    if current.file_fingerprint != request.expected_file_fingerprint {
        return Err("session changed on disk; reload it before editing".to_string());
    }
    let source_bytes = fs::read(&identity.source_path).map_err(|error| {
        format!(
            "cannot read Claude transcript {}: {error}",
            identity.source_path.display()
        )
    })?;
    let (edited_bytes, old_text) = build_edited_bytes(
        &source_bytes,
        &request.target,
        &request.new_text,
        request.allow_assistant_text,
    )?;
    let snapshot = create_snapshot(
        backup_root,
        identity,
        &source_bytes,
        "before_edit",
        Some(request.target.clone()),
        Some(sha256(old_text.as_bytes())),
        Some(sha256(request.new_text.as_bytes())),
    )?;

    let temp = temp_path_for(&identity.source_path)?;
    write_file_synced(&temp, &edited_bytes, true)?;
    validate_candidate(&temp, identity)?;
    let recheck = validate_detail(identity)?;
    if recheck.file_fingerprint != request.expected_file_fingerprint {
        return Err("session changed while the edit was being prepared; source file was not replaced".to_string());
    }
    atomic_replace(&temp, &identity.source_path)?;
    let detail = validate_detail(identity)?;
    Ok(TranscriptEditResult { detail, snapshot })
}

fn restore_snapshot(
    identity: &SessionIdentity,
    request: &TranscriptRestoreRequest,
    backup_root: &Path,
) -> Result<TranscriptRestoreResult, String> {
    if !source_is_inside_projects(identity) {
        return Err("session source path is outside the Claude projects directory".to_string());
    }
    let current = validate_detail(identity)?;
    if current.file_fingerprint != request.expected_file_fingerprint {
        return Err("session changed on disk; reload it before restoring".to_string());
    }
    let manifest = load_manifest(backup_root, &identity.session_id)?;
    let target = manifest
        .snapshots
        .iter()
        .find(|snapshot| snapshot.id == request.snapshot_id)
        .cloned()
        .ok_or_else(|| "backup snapshot not found".to_string())?;
    if normalize_path_key(Path::new(&target.source_path))
        != normalize_path_key(&identity.source_path)
    {
        return Err("backup snapshot belongs to a different source file".to_string());
    }
    let backup_path = PathBuf::from(&target.backup_path);
    let backup_bytes = fs::read(&backup_path)
        .map_err(|error| format!("cannot read backup {}: {error}", backup_path.display()))?;
    if sha256(&backup_bytes) != target.source_hash {
        return Err("backup snapshot hash mismatch".to_string());
    }
    let validation_temp = temp_path_for(&identity.source_path)?;
    write_file_synced(&validation_temp, &backup_bytes, true)?;
    validate_candidate(&validation_temp, identity)?;

    let current_bytes = fs::read(&identity.source_path).map_err(|error| {
        format!(
            "cannot read current transcript {}: {error}",
            identity.source_path.display()
        )
    })?;
    let current_snapshot = create_snapshot(
        backup_root,
        identity,
        &current_bytes,
        "before_restore",
        None,
        None,
        None,
    )?;
    let recheck = validate_detail(identity)?;
    if recheck.file_fingerprint != request.expected_file_fingerprint {
        return Err("session changed while restore was being prepared; source file was not replaced".to_string());
    }
    atomic_replace(&validation_temp, &identity.source_path)?;
    let detail = validate_detail(identity)?;
    Ok(TranscriptRestoreResult {
        detail,
        current_snapshot,
        restored_snapshot_id: target.id,
    })
}

#[tauri::command]
pub fn transcript_edit(
    editor: State<'_, TranscriptEditor>,
    runtime: State<'_, RuntimeRegistry>,
    catalog: State<'_, SessionCatalog>,
    request: TranscriptEditRequest,
) -> Result<TranscriptEditResult, String> {
    let _lease = editor.acquire(&request.session_key)?;
    let identity = identity_from_catalog(&catalog, &request.session_key)?;
    ensure_editable_runtime_state(&runtime, &identity, &request.session_key)?;
    apply_edit(&identity, &request, &claudeix_backups_dir()?)
}

#[tauri::command]
pub fn transcript_snapshots_list(
    catalog: State<'_, SessionCatalog>,
    session_key: String,
) -> Result<Vec<BackupSnapshot>, String> {
    let identity = identity_from_catalog(&catalog, &session_key)?;
    let mut snapshots = load_manifest(&claudeix_backups_dir()?, &identity.session_id)?
        .snapshots
        .into_iter()
        .filter(|snapshot| {
            normalize_path_key(Path::new(&snapshot.source_path))
                == normalize_path_key(&identity.source_path)
        })
        .collect::<Vec<_>>();
    snapshots.sort_by(|left, right| right.created_at.cmp(&left.created_at));
    Ok(snapshots)
}

#[tauri::command]
pub fn transcript_snapshot_restore(
    editor: State<'_, TranscriptEditor>,
    runtime: State<'_, RuntimeRegistry>,
    catalog: State<'_, SessionCatalog>,
    request: TranscriptRestoreRequest,
) -> Result<TranscriptRestoreResult, String> {
    let _lease = editor.acquire(&request.session_key)?;
    let identity = identity_from_catalog(&catalog, &request.session_key)?;
    ensure_editable_runtime_state(&runtime, &identity, &request.session_key)?;
    restore_snapshot(&identity, &request, &claudeix_backups_dir()?)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn fixture() -> (PathBuf, SessionIdentity, TranscriptEditRequest) {
        let root = std::env::temp_dir().join(format!("claudeix-edit-test-{}", unique_id()));
        let config = root.join(".claude");
        let project = config.join("projects").join("fixture");
        let workspace = root.join("workspace");
        fs::create_dir_all(&project).unwrap();
        fs::create_dir_all(&workspace).unwrap();
        let source = project.join("session-1.jsonl");
        fs::write(
            &source,
            concat!(
                "{\"type\":\"user\",\"sessionId\":\"session-1\",\"cwd\":\"",
                "WORKSPACE",
                "\",\"uuid\":\"user-1\",\"message\":{\"role\":\"user\",\"content\":\"before\"}}\n",
                "{\"type\":\"assistant\",\"sessionId\":\"session-1\",\"cwd\":\"",
                "WORKSPACE",
                "\",\"uuid\":\"assistant-1\",\"message\":{\"role\":\"assistant\",\"content\":[{\"type\":\"text\",\"text\":\"reply\"}]}}\n"
            )
            .replace("WORKSPACE", &display_path(&workspace)),
        )
        .unwrap();
        let identity = SessionIdentity {
            config_dir: config,
            workspace_root: workspace,
            session_id: "session-1".to_string(),
            source_path: source,
        };
        let detail = load_session(&identity).unwrap();
        let target = detail.records[0].editable_targets[0].clone();
        let request = TranscriptEditRequest {
            session_key: identity.key(),
            target,
            expected_file_fingerprint: detail.file_fingerprint,
            new_text: "after".to_string(),
            allow_assistant_text: false,
        };
        (root, identity, request)
    }

    #[test]
    fn edits_original_file_and_restores_snapshot() {
        let (root, identity, request) = fixture();
        let backup_root = root.join("backups");
        let result = apply_edit(&identity, &request, &backup_root).unwrap();
        assert_eq!(result.detail.records[0].visible_text(), "after");
        let source = fs::read_to_string(&identity.source_path).unwrap();
        assert!(source.contains("after"));
        assert!(!source.contains("\"content\":\"before\""));

        let restore = restore_snapshot(
            &identity,
            &TranscriptRestoreRequest {
                session_key: identity.key(),
                snapshot_id: result.snapshot.id,
                expected_file_fingerprint: result.detail.file_fingerprint,
            },
            &backup_root,
        )
        .unwrap();
        assert_eq!(restore.detail.records[0].visible_text(), "before");
        assert!(restore.current_snapshot.backup_path.ends_with("-before.jsonl"));
    }

    #[test]
    fn rejects_stale_record_hash_without_replacing_source() {
        let (root, identity, mut request) = fixture();
        request.target.expected_record_hash = "stale".to_string();
        let before = fs::read(&identity.source_path).unwrap();
        let error = apply_edit(&identity, &request, &root.join("backups")).unwrap_err();
        assert!(error.contains("target record changed"));
        assert_eq!(fs::read(&identity.source_path).unwrap(), before);
    }

    #[test]
    fn assistant_text_requires_advanced_mode() {
        let (root, identity, mut request) = fixture();
        let detail = load_session(&identity).unwrap();
        request.target = detail.records[1].editable_targets[0].clone();
        request.expected_file_fingerprint = detail.file_fingerprint;
        let error = apply_edit(&identity, &request, &root.join("backups")).unwrap_err();
        assert!(error.contains("not editable"));

        let detail = load_session(&identity).unwrap();
        request.expected_file_fingerprint = detail.file_fingerprint;
        request.allow_assistant_text = true;
        request.new_text = "edited reply".to_string();
        let result = apply_edit(&identity, &request, &root.join("backups")).unwrap();
        assert_eq!(result.detail.records[1].visible_text(), "edited reply");
        let source = fs::read_to_string(&identity.source_path).unwrap();
        assert!(source.contains("edited reply"));
        assert!(source.contains("\"uuid\":\"assistant-1\""));
    }
}
