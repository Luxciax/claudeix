use crate::app_paths::{
    canonical_or_original, claudeix_backups_dir, claudeix_database_path, display_path,
    normalize_path_key,
};
use crate::claude::parser::parse_session_file;
use crate::claude::runtime_registry::RuntimeRegistry;
use crate::claude::scanner::{load_session, SessionCatalog};
use crate::models::edit::{
    BackupSnapshot, EditRecoveryItem, EditRecoveryResolveRequest, EditRecoveryResolveResult,
    TranscriptAuditEntry, TranscriptEditRequest, TranscriptEditResult, TranscriptRestoreRequest,
    TranscriptRestoreResult,
};
use crate::models::session::{
    SessionDetail, SessionIdentity, SessionRenameRequest, SessionRenameResult,
};
use crate::models::transcript::EditTarget;
use crate::trash::service::TrashService;
use crate::transcript::audit::{self, EditTransactionRecord};
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

fn rollback_to_snapshot(
    identity: &SessionIdentity,
    snapshot: &BackupSnapshot,
) -> Result<SessionDetail, String> {
    let backup_path = PathBuf::from(&snapshot.backup_path);
    let backup_bytes = fs::read(&backup_path)
        .map_err(|error| format!("cannot read rollback snapshot {}: {error}", backup_path.display()))?;
    if sha256(&backup_bytes) != snapshot.source_hash {
        return Err("rollback snapshot hash mismatch".to_string());
    }
    let temp = temp_path_for(&identity.source_path)?;
    write_file_synced(&temp, &backup_bytes, true)?;
    validate_candidate(&temp, identity)?;
    atomic_replace(&temp, &identity.source_path)?;
    validate_detail(identity)
}

fn new_transaction_record(
    identity: &SessionIdentity,
    operation: &str,
    source_hash_before: String,
    source_hash_after: String,
    target: Option<EditTarget>,
    old_content: Option<String>,
    new_content: Option<String>,
) -> EditTransactionRecord {
    let timestamp = now_millis();
    EditTransactionRecord {
        id: format!("{operation}-{}", unique_id()),
        session_id: identity.session_id.clone(),
        source_path: display_path(&identity.source_path),
        operation: operation.to_string(),
        status: "prepared".to_string(),
        source_hash_before,
        source_hash_after,
        backup_path: None,
        target,
        old_content,
        new_content,
        created_at: timestamp,
        updated_at: timestamp,
        error: None,
    }
}

fn apply_rename(
    identity: &SessionIdentity,
    request: &SessionRenameRequest,
    backup_root: &Path,
    database_path: &Path,
) -> Result<SessionRenameResult, String> {
    let title = request.title.trim();
    if title.is_empty() {
        return Err("session title must be non-empty".to_string());
    }
    if !source_is_inside_projects(identity) {
        return Err("session source path is outside the Claude projects directory".to_string());
    }
    let current = validate_detail(identity)?;
    if current.file_fingerprint != request.expected_file_fingerprint {
        return Err("session changed on disk; reload it before renaming".to_string());
    }
    let source_bytes = fs::read(&identity.source_path).map_err(|error| {
        format!(
            "cannot read Claude transcript {}: {error}",
            identity.source_path.display()
        )
    })?;
    if source_bytes.is_empty() {
        return Err("cannot rename an empty Claude transcript".to_string());
    }
    if !source_bytes.ends_with(b"\n") {
        return Err("Claude transcript does not end with a newline; repair it before renaming".to_string());
    }
    let record = serde_json::json!({
        "type": "custom-title",
        "customTitle": title,
        "sessionId": identity.session_id,
    });
    let mut appended = serde_json::to_vec(&record).map_err(|error| error.to_string())?;
    appended.push(b'\n');
    let mut expected_after = source_bytes.clone();
    expected_after.extend_from_slice(&appended);
    let mut transaction = new_transaction_record(
        identity,
        "rename",
        sha256(&source_bytes),
        sha256(&expected_after),
        None,
        Some(current.summary.title.clone()),
        Some(title.to_string()),
    );
    audit::begin(database_path, &transaction)?;

    let mut rollback_snapshot: Option<BackupSnapshot> = None;
    let mut source_touched = false;
    let result = (|| -> Result<SessionRenameResult, String> {
        let snapshot = create_snapshot(
            backup_root,
            identity,
            &source_bytes,
            "before_rename",
            None,
            Some(sha256(current.summary.title.as_bytes())),
            Some(sha256(title.as_bytes())),
        )?;
        rollback_snapshot = Some(snapshot.clone());
        transaction.backup_path = Some(snapshot.backup_path.clone());
        transaction.updated_at = now_millis();
        audit::update_status(
            database_path,
            &transaction.id,
            "backed_up",
            Some(&snapshot.backup_path),
            transaction.updated_at,
            None,
        )?;

        let recheck = validate_detail(identity)?;
        if recheck.file_fingerprint != request.expected_file_fingerprint {
            return Err(
                "session changed while rename was being prepared; title was not appended"
                    .to_string(),
            );
        }
        let mut file = OpenOptions::new()
            .append(true)
            .open(&identity.source_path)
            .map_err(|error| format!("cannot open Claude transcript for rename: {error}"))?;
        source_touched = true;
        file.write_all(&appended)
            .and_then(|_| file.flush())
            .and_then(|_| file.sync_all())
            .map_err(|error| format!("cannot append Claude custom-title record: {error}"))?;
        transaction.updated_at = now_millis();
        audit::update_status(
            database_path,
            &transaction.id,
            "replaced",
            None,
            transaction.updated_at,
            None,
        )?;

        let detail = validate_detail(identity)?;
        if detail.summary.title != title {
            return Err("Claude transcript did not resolve to the requested custom title".to_string());
        }
        let actual_after = fs::read(&identity.source_path)
            .map_err(|error| format!("cannot verify renamed Claude transcript: {error}"))?;
        if sha256(&actual_after) != transaction.source_hash_after {
            return Err("renamed Claude transcript hash does not match the prepared append".to_string());
        }
        transaction.updated_at = now_millis();
        audit::update_status(
            database_path,
            &transaction.id,
            "verified",
            None,
            transaction.updated_at,
            None,
        )?;
        audit::commit(database_path, &transaction, &snapshot.backup_path)?;
        Ok(SessionRenameResult { detail, snapshot })
    })();

    match result {
        Ok(result) => Ok(result),
        Err(error) if source_touched => {
            let Some(snapshot) = rollback_snapshot.as_ref() else {
                let _ = audit::update_status(
                    database_path,
                    &transaction.id,
                    "replaced",
                    None,
                    now_millis(),
                    Some(&error),
                );
                return Err(format!(
                    "rename failed after touching the transcript and no rollback snapshot is available: {error}"
                ));
            };
            match rollback_to_snapshot(identity, snapshot) {
                Ok(_) => {
                    let _ = audit::update_status(
                        database_path,
                        &transaction.id,
                        "rolled_back",
                        Some(&snapshot.backup_path),
                        now_millis(),
                        Some(&error),
                    );
                    Err(format!(
                        "rename failed, but Claudeix restored the original transcript: {error}"
                    ))
                }
                Err(rollback_error) => {
                    let combined = format!(
                        "rename failed after touching the transcript: {error}; automatic rollback also failed: {rollback_error}"
                    );
                    let _ = audit::update_status(
                        database_path,
                        &transaction.id,
                        "replaced",
                        Some(&snapshot.backup_path),
                        now_millis(),
                        Some(&combined),
                    );
                    Err(combined)
                }
            }
        }
        Err(error) => {
            let _ = audit::update_status(
                database_path,
                &transaction.id,
                "rolled_back",
                rollback_snapshot
                    .as_ref()
                    .map(|snapshot| snapshot.backup_path.as_str()),
                now_millis(),
                Some(&error),
            );
            Err(error)
        }
    }
}

fn recovery_config_dir(source_path: &Path) -> Result<PathBuf, String> {
    for ancestor in source_path.ancestors() {
        let is_projects = ancestor
            .file_name()
            .and_then(|value| value.to_str())
            .map(|value| value.eq_ignore_ascii_case("projects"))
            .unwrap_or(false);
        if is_projects {
            return ancestor
                .parent()
                .map(Path::to_path_buf)
                .ok_or_else(|| "Claude projects directory has no config parent".to_string());
        }
    }
    Err(format!(
        "cannot locate Claude config directory from transcript path {}",
        source_path.display()
    ))
}

fn recovery_identity(transaction: &EditTransactionRecord) -> Result<SessionIdentity, String> {
    let source_path = PathBuf::from(&transaction.source_path);
    let config_dir = recovery_config_dir(&source_path)?;
    let candidates = transaction
        .backup_path
        .as_ref()
        .map(PathBuf::from)
        .into_iter()
        .chain(std::iter::once(source_path.clone()));
    let mut last_error = None;
    for candidate in candidates {
        if !candidate.is_file() {
            continue;
        }
        match parse_session_file(&candidate, &config_dir, None) {
            Ok(detail) if detail.summary.session_id == transaction.session_id => {
                return Ok(SessionIdentity {
                    config_dir: canonical_or_original(&config_dir),
                    workspace_root: PathBuf::from(detail.summary.workspace_root),
                    session_id: transaction.session_id.clone(),
                    source_path: canonical_or_original(&source_path),
                });
            }
            Ok(_) => {
                last_error = Some("recovery transcript session id mismatch".to_string());
            }
            Err(error) => last_error = Some(error),
        }
    }
    Err(last_error.unwrap_or_else(|| {
        "cannot derive a valid Claude session identity for recovery".to_string()
    }))
}

fn file_hash_if_present(path: &Path) -> Result<Option<String>, String> {
    if !path.is_file() {
        return Ok(None);
    }
    fs::read(path)
        .map(|bytes| Some(sha256(&bytes)))
        .map_err(|error| format!("cannot read {} during recovery: {error}", path.display()))
}

fn recovery_item(transaction: &EditTransactionRecord) -> Result<EditRecoveryItem, String> {
    let source_path = PathBuf::from(&transaction.source_path);
    let current_hash = file_hash_if_present(&source_path)?;
    let backup_valid = transaction
        .backup_path
        .as_ref()
        .map(PathBuf::from)
        .filter(|path| path.is_file())
        .map(|path| {
            fs::read(&path)
                .map(|bytes| sha256(&bytes) == transaction.source_hash_before)
                .unwrap_or(false)
        })
        .unwrap_or(false);
    let detected_state = match current_hash.as_deref() {
        None => "source_missing",
        Some(hash) if hash == transaction.source_hash_before => "source_before",
        Some(hash) if hash == transaction.source_hash_after => "source_after",
        Some(_) => "source_unknown",
    }
    .to_string();
    Ok(EditRecoveryItem {
        transaction_id: transaction.id.clone(),
        session_id: transaction.session_id.clone(),
        source_path: transaction.source_path.clone(),
        operation: transaction.operation.clone(),
        status: transaction.status.clone(),
        backup_path: transaction.backup_path.clone(),
        source_hash_before: transaction.source_hash_before.clone(),
        source_hash_after: transaction.source_hash_after.clone(),
        created_at: transaction.created_at,
        can_keep_current: detected_state == "source_after",
        can_restore_backup: backup_valid,
        detected_state,
        backup_valid,
    })
}

fn pending_recovery_items(database_path: &Path) -> Result<Vec<EditRecoveryItem>, String> {
    let mut items = Vec::new();
    for transaction in audit::pending(database_path)? {
        let item = recovery_item(&transaction)?;
        if item.detected_state == "source_before" {
            audit::update_status(
                database_path,
                &transaction.id,
                "rolled_back",
                transaction.backup_path.as_deref(),
                now_millis(),
                Some("startup recovery detected the original pre-operation transcript"),
            )?;
            continue;
        }
        items.push(item);
    }
    Ok(items)
}

pub fn session_has_pending_recovery(identity: &SessionIdentity) -> Result<bool, String> {
    let source_key = normalize_path_key(&identity.source_path);
    Ok(pending_recovery_items(&claudeix_database_path()?)?
        .into_iter()
        .any(|item| {
            item.session_id == identity.session_id
                && normalize_path_key(Path::new(&item.source_path)) == source_key
        }))
}

fn resolve_recovery(
    database_path: &Path,
    backup_root: &Path,
    request: &EditRecoveryResolveRequest,
) -> Result<EditRecoveryResolveResult, String> {
    let mut transaction = audit::get(database_path, &request.transaction_id)?
        .ok_or_else(|| "edit recovery transaction not found".to_string())?;
    if matches!(
        transaction.status.as_str(),
        "committed" | "rolled_back" | "dismissed"
    ) {
        return Err("edit recovery transaction is already resolved".to_string());
    }
    let item = recovery_item(&transaction)?;
    let identity = recovery_identity(&transaction)?;
    match request.action.as_str() {
        "keep_current" => {
            if !item.can_keep_current {
                return Err(
                    "current transcript does not match the prepared post-operation hash"
                        .to_string(),
                );
            }
            let detail = validate_detail(&identity)?;
            if detail.summary.session_id != transaction.session_id {
                return Err("current transcript session id mismatch".to_string());
            }
            let backup_path = transaction
                .backup_path
                .as_deref()
                .ok_or_else(|| "recovery transaction has no recorded backup path".to_string())?;
            transaction.updated_at = now_millis();
            audit::commit(database_path, &transaction, backup_path)?;
            Ok(EditRecoveryResolveResult {
                transaction_id: transaction.id,
                status: "committed".to_string(),
                source_path: display_path(&identity.source_path),
            })
        }
        "restore_backup" => {
            if !item.can_restore_backup {
                return Err("the automatic pre-operation backup is unavailable or invalid".to_string());
            }
            let backup_path = PathBuf::from(
                transaction
                    .backup_path
                    .as_ref()
                    .ok_or_else(|| "recovery transaction has no backup path".to_string())?,
            );
            let backup_bytes = fs::read(&backup_path).map_err(|error| {
                format!("cannot read recovery backup {}: {error}", backup_path.display())
            })?;
            if sha256(&backup_bytes) != transaction.source_hash_before {
                return Err("recovery backup hash mismatch".to_string());
            }
            if identity.source_path.is_file() {
                let current_bytes = fs::read(&identity.source_path).map_err(|error| {
                    format!(
                        "cannot read current transcript before recovery {}: {error}",
                        identity.source_path.display()
                    )
                })?;
                create_snapshot(
                    backup_root,
                    &identity,
                    &current_bytes,
                    "before_recovery",
                    None,
                    None,
                    None,
                )?;
            }
            let temp = temp_path_for(&identity.source_path)?;
            write_file_synced(&temp, &backup_bytes, true)?;
            validate_candidate(&temp, &identity)?;
            atomic_replace(&temp, &identity.source_path)?;
            validate_detail(&identity)?;
            audit::update_status(
                database_path,
                &transaction.id,
                "rolled_back",
                Some(&display_path(&backup_path)),
                now_millis(),
                Some("resolved by startup recovery using the pre-operation backup"),
            )?;
            Ok(EditRecoveryResolveResult {
                transaction_id: transaction.id,
                status: "rolled_back".to_string(),
                source_path: display_path(&identity.source_path),
            })
        }
        _ => Err("recovery action must be keep_current or restore_backup".to_string()),
    }
}

fn apply_edit(
    identity: &SessionIdentity,
    request: &TranscriptEditRequest,
    backup_root: &Path,
    database_path: &Path,
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
    let mut transaction = new_transaction_record(
        identity,
        "edit",
        sha256(&source_bytes),
        sha256(&edited_bytes),
        Some(request.target.clone()),
        Some(old_text.clone()),
        Some(request.new_text.clone()),
    );
    audit::begin(database_path, &transaction)?;

    let mut rollback_snapshot: Option<BackupSnapshot> = None;
    let mut source_replaced = false;
    let result = (|| -> Result<TranscriptEditResult, String> {
        let snapshot = create_snapshot(
            backup_root,
            identity,
            &source_bytes,
            "before_edit",
            Some(request.target.clone()),
            Some(sha256(old_text.as_bytes())),
            Some(sha256(request.new_text.as_bytes())),
        )?;
        rollback_snapshot = Some(snapshot.clone());
        transaction.backup_path = Some(snapshot.backup_path.clone());
        transaction.updated_at = now_millis();
        audit::update_status(
            database_path,
            &transaction.id,
            "backed_up",
            Some(&snapshot.backup_path),
            transaction.updated_at,
            None,
        )?;

        let temp = temp_path_for(&identity.source_path)?;
        write_file_synced(&temp, &edited_bytes, true)?;
        validate_candidate(&temp, identity)?;
        let recheck = validate_detail(identity)?;
        if recheck.file_fingerprint != request.expected_file_fingerprint {
            return Err(
                "session changed while the edit was being prepared; source file was not replaced"
                    .to_string(),
            );
        }
        atomic_replace(&temp, &identity.source_path)?;
        source_replaced = true;
        transaction.updated_at = now_millis();
        audit::update_status(
            database_path,
            &transaction.id,
            "replaced",
            None,
            transaction.updated_at,
            None,
        )?;

        let detail = validate_detail(identity)?;
        transaction.updated_at = now_millis();
        audit::update_status(
            database_path,
            &transaction.id,
            "verified",
            None,
            transaction.updated_at,
            None,
        )?;
        audit::commit(database_path, &transaction, &snapshot.backup_path)?;
        Ok(TranscriptEditResult { detail, snapshot })
    })();

    match result {
        Ok(result) => Ok(result),
        Err(error) if source_replaced => {
            let Some(snapshot) = rollback_snapshot.as_ref() else {
                let _ = audit::update_status(
                    database_path,
                    &transaction.id,
                    "replaced",
                    None,
                    now_millis(),
                    Some(&error),
                );
                return Err(format!(
                    "edit failed after replacing the transcript and no rollback snapshot is available: {error}"
                ));
            };
            match rollback_to_snapshot(identity, snapshot) {
                Ok(_) => {
                    let _ = audit::update_status(
                        database_path,
                        &transaction.id,
                        "rolled_back",
                        Some(&snapshot.backup_path),
                        now_millis(),
                        Some(&error),
                    );
                    Err(format!(
                        "edit failed, but Claudeix restored the original transcript from the automatic snapshot: {error}"
                    ))
                }
                Err(rollback_error) => {
                    let combined = format!(
                        "edit failed after replacing the transcript: {error}; automatic rollback also failed: {rollback_error}"
                    );
                    let _ = audit::update_status(
                        database_path,
                        &transaction.id,
                        "replaced",
                        Some(&snapshot.backup_path),
                        now_millis(),
                        Some(&combined),
                    );
                    Err(combined)
                }
            }
        }
        Err(error) => {
            let _ = audit::update_status(
                database_path,
                &transaction.id,
                "rolled_back",
                rollback_snapshot
                    .as_ref()
                    .map(|snapshot| snapshot.backup_path.as_str()),
                now_millis(),
                Some(&error),
            );
            Err(error)
        }
    }
}

fn restore_snapshot(
    identity: &SessionIdentity,
    request: &TranscriptRestoreRequest,
    backup_root: &Path,
    database_path: &Path,
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
    let mut transaction = new_transaction_record(
        identity,
        "snapshot_restore",
        sha256(&current_bytes),
        sha256(&backup_bytes),
        None,
        None,
        None,
    );
    audit::begin(database_path, &transaction)?;

    let mut rollback_snapshot: Option<BackupSnapshot> = None;
    let mut source_replaced = false;
    let result = (|| -> Result<TranscriptRestoreResult, String> {
        let current_snapshot = create_snapshot(
            backup_root,
            identity,
            &current_bytes,
            "before_restore",
            None,
            None,
            None,
        )?;
        rollback_snapshot = Some(current_snapshot.clone());
        transaction.backup_path = Some(current_snapshot.backup_path.clone());
        transaction.updated_at = now_millis();
        audit::update_status(
            database_path,
            &transaction.id,
            "backed_up",
            Some(&current_snapshot.backup_path),
            transaction.updated_at,
            None,
        )?;

        let recheck = validate_detail(identity)?;
        if recheck.file_fingerprint != request.expected_file_fingerprint {
            return Err(
                "session changed while restore was being prepared; source file was not replaced"
                    .to_string(),
            );
        }
        atomic_replace(&validation_temp, &identity.source_path)?;
        source_replaced = true;
        transaction.updated_at = now_millis();
        audit::update_status(
            database_path,
            &transaction.id,
            "replaced",
            None,
            transaction.updated_at,
            None,
        )?;

        let detail = validate_detail(identity)?;
        transaction.updated_at = now_millis();
        audit::update_status(
            database_path,
            &transaction.id,
            "verified",
            None,
            transaction.updated_at,
            None,
        )?;
        audit::commit(
            database_path,
            &transaction,
            &current_snapshot.backup_path,
        )?;
        Ok(TranscriptRestoreResult {
            detail,
            current_snapshot,
            restored_snapshot_id: target.id,
        })
    })();

    match result {
        Ok(result) => Ok(result),
        Err(error) if source_replaced => {
            let Some(snapshot) = rollback_snapshot.as_ref() else {
                let _ = audit::update_status(
                    database_path,
                    &transaction.id,
                    "replaced",
                    None,
                    now_millis(),
                    Some(&error),
                );
                return Err(format!(
                    "snapshot restore failed after replacing the transcript and no rollback snapshot is available: {error}"
                ));
            };
            match rollback_to_snapshot(identity, snapshot) {
                Ok(_) => {
                    let _ = audit::update_status(
                        database_path,
                        &transaction.id,
                        "rolled_back",
                        Some(&snapshot.backup_path),
                        now_millis(),
                        Some(&error),
                    );
                    Err(format!(
                        "snapshot restore failed, but Claudeix restored the previous transcript from the automatic snapshot: {error}"
                    ))
                }
                Err(rollback_error) => {
                    let combined = format!(
                        "snapshot restore failed after replacing the transcript: {error}; automatic rollback also failed: {rollback_error}"
                    );
                    let _ = audit::update_status(
                        database_path,
                        &transaction.id,
                        "replaced",
                        Some(&snapshot.backup_path),
                        now_millis(),
                        Some(&combined),
                    );
                    Err(combined)
                }
            }
        }
        Err(error) => {
            let _ = audit::update_status(
                database_path,
                &transaction.id,
                "rolled_back",
                rollback_snapshot
                    .as_ref()
                    .map(|snapshot| snapshot.backup_path.as_str()),
                now_millis(),
                Some(&error),
            );
            Err(error)
        }
    }
}

#[tauri::command]
pub fn transcript_edit(
    editor: State<'_, TranscriptEditor>,
    runtime: State<'_, RuntimeRegistry>,
    trash: State<'_, TrashService>,
    catalog: State<'_, SessionCatalog>,
    request: TranscriptEditRequest,
) -> Result<TranscriptEditResult, String> {
    if trash.is_locked(&request.session_key) {
        return Err("this Claude session is currently being moved to the recycle bin".to_string());
    }
    let _lease = editor.acquire(&request.session_key)?;
    if trash.is_locked(&request.session_key) {
        return Err("this Claude session started a recycle-bin transaction while the edit was being prepared".to_string());
    }
    let identity = identity_from_catalog(&catalog, &request.session_key)?;
    if session_has_pending_recovery(&identity)? {
        return Err(
            "this Claude session has an unfinished edit/restore transaction; resolve recovery first"
                .to_string(),
        );
    }
    ensure_editable_runtime_state(&runtime, &identity, &request.session_key)?;
    apply_edit(
        &identity,
        &request,
        &claudeix_backups_dir()?,
        &claudeix_database_path()?,
    )
}

#[tauri::command]
pub fn session_rename(
    editor: State<'_, TranscriptEditor>,
    runtime: State<'_, RuntimeRegistry>,
    trash: State<'_, TrashService>,
    catalog: State<'_, SessionCatalog>,
    request: SessionRenameRequest,
) -> Result<SessionRenameResult, String> {
    if trash.is_locked(&request.session_key) {
        return Err("this Claude session is currently being moved to the recycle bin".to_string());
    }
    let _lease = editor.acquire(&request.session_key)?;
    if trash.is_locked(&request.session_key) {
        return Err("this Claude session started a recycle-bin transaction while rename was being prepared".to_string());
    }
    let identity = identity_from_catalog(&catalog, &request.session_key)?;
    if session_has_pending_recovery(&identity)? {
        return Err(
            "this Claude session has an unfinished edit/restore transaction; resolve recovery first"
                .to_string(),
        );
    }
    ensure_editable_runtime_state(&runtime, &identity, &request.session_key)?;
    apply_rename(
        &identity,
        &request,
        &claudeix_backups_dir()?,
        &claudeix_database_path()?,
    )
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
pub fn transcript_audit_list(
    catalog: State<'_, SessionCatalog>,
    session_key: String,
) -> Result<Vec<TranscriptAuditEntry>, String> {
    let identity = identity_from_catalog(&catalog, &session_key)?;
    audit::audit_for_session(
        &claudeix_database_path()?,
        &identity.session_id,
        &display_path(&identity.source_path),
    )
}

#[tauri::command]
pub fn transcript_recovery_list() -> Result<Vec<EditRecoveryItem>, String> {
    pending_recovery_items(&claudeix_database_path()?)
}

#[tauri::command]
pub fn transcript_recovery_resolve(
    editor: State<'_, TranscriptEditor>,
    runtime: State<'_, RuntimeRegistry>,
    trash: State<'_, TrashService>,
    request: EditRecoveryResolveRequest,
) -> Result<EditRecoveryResolveResult, String> {
    let database = claudeix_database_path()?;
    let transaction = audit::get(&database, &request.transaction_id)?
        .ok_or_else(|| "edit recovery transaction not found".to_string())?;
    let identity = recovery_identity(&transaction)?;
    let session_key = identity.key();
    if trash.is_locked(&session_key) {
        return Err("this Claude session is currently being moved to the recycle bin".to_string());
    }
    let _lease = editor.acquire(&session_key)?;
    if trash.is_locked(&session_key) {
        return Err(
            "this Claude session started a recycle-bin transaction while recovery was being prepared"
                .to_string(),
        );
    }
    ensure_editable_runtime_state(&runtime, &identity, &session_key)?;
    resolve_recovery(&database, &claudeix_backups_dir()?, &request)
}

#[tauri::command]
pub fn transcript_snapshot_restore(
    editor: State<'_, TranscriptEditor>,
    runtime: State<'_, RuntimeRegistry>,
    trash: State<'_, TrashService>,
    catalog: State<'_, SessionCatalog>,
    request: TranscriptRestoreRequest,
) -> Result<TranscriptRestoreResult, String> {
    if trash.is_locked(&request.session_key) {
        return Err("this Claude session is currently being moved to the recycle bin".to_string());
    }
    let _lease = editor.acquire(&request.session_key)?;
    if trash.is_locked(&request.session_key) {
        return Err("this Claude session started a recycle-bin transaction while restore was being prepared".to_string());
    }
    let identity = identity_from_catalog(&catalog, &request.session_key)?;
    if session_has_pending_recovery(&identity)? {
        return Err(
            "this Claude session has an unfinished edit/restore transaction; resolve recovery first"
                .to_string(),
        );
    }
    ensure_editable_runtime_state(&runtime, &identity, &request.session_key)?;
    restore_snapshot(
        &identity,
        &request,
        &claudeix_backups_dir()?,
        &claudeix_database_path()?,
    )
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
        let database = root.join("claudeix.db");
        let result = apply_edit(&identity, &request, &backup_root, &database).unwrap();
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
            &database,
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
        let error = apply_edit(
            &identity,
            &request,
            &root.join("backups"),
            &root.join("claudeix.db"),
        )
        .unwrap_err();
        assert!(error.contains("target record changed"));
        assert_eq!(fs::read(&identity.source_path).unwrap(), before);
    }

    #[test]
    fn assistant_text_requires_advanced_mode() {
        let (root, identity, mut request) = fixture();
        let detail = load_session(&identity).unwrap();
        request.target = detail.records[1].editable_targets[0].clone();
        request.expected_file_fingerprint = detail.file_fingerprint;
        let database = root.join("claudeix.db");
        let error = apply_edit(&identity, &request, &root.join("backups"), &database).unwrap_err();
        assert!(error.contains("not editable"));

        let detail = load_session(&identity).unwrap();
        request.expected_file_fingerprint = detail.file_fingerprint;
        request.allow_assistant_text = true;
        request.new_text = "edited reply".to_string();
        let result = apply_edit(&identity, &request, &root.join("backups"), &database).unwrap();
        assert_eq!(result.detail.records[1].visible_text(), "edited reply");
        let source = fs::read_to_string(&identity.source_path).unwrap();
        assert!(source.contains("edited reply"));
        assert!(source.contains("\"uuid\":\"assistant-1\""));
    }

    #[test]
    fn rename_appends_native_custom_title_and_preserves_existing_bytes() {
        let (root, identity, request) = fixture();
        let before = fs::read(&identity.source_path).unwrap();
        let detail = load_session(&identity).unwrap();
        let rename = SessionRenameRequest {
            session_key: identity.key(),
            expected_file_fingerprint: detail.file_fingerprint,
            title: "  Native title  ".to_string(),
        };
        let database = root.join("claudeix.db");
        let result = apply_rename(&identity, &rename, &root.join("backups"), &database).unwrap();
        assert_eq!(result.detail.summary.title, "Native title");
        let after = fs::read(&identity.source_path).unwrap();
        assert!(after.starts_with(&before));
        let appended = std::str::from_utf8(&after[before.len()..]).unwrap();
        let value: Value = serde_json::from_str(appended.trim_end()).unwrap();
        assert_eq!(value.get("type").and_then(Value::as_str), Some("custom-title"));
        assert_eq!(value.get("customTitle").and_then(Value::as_str), Some("Native title"));
        assert_eq!(value.get("sessionId").and_then(Value::as_str), Some("session-1"));
        let audit = audit::audit_for_session(
            &database,
            &identity.session_id,
            &display_path(&identity.source_path),
        )
        .unwrap();
        assert_eq!(audit[0].operation, "rename");
        assert_eq!(audit[0].new_content.as_deref(), Some("Native title"));
        assert_eq!(result.snapshot.reason, "before_rename");
        let _ = request;
    }

    #[test]
    fn blank_or_stale_rename_never_touches_source() {
        let (root, identity, _request) = fixture();
        let before = fs::read(&identity.source_path).unwrap();
        let detail = load_session(&identity).unwrap();
        let blank = SessionRenameRequest {
            session_key: identity.key(),
            expected_file_fingerprint: detail.file_fingerprint.clone(),
            title: "   ".to_string(),
        };
        assert!(apply_rename(
            &identity,
            &blank,
            &root.join("backups"),
            &root.join("claudeix.db")
        )
        .unwrap_err()
        .contains("non-empty"));
        assert_eq!(fs::read(&identity.source_path).unwrap(), before);

        let stale = SessionRenameRequest {
            session_key: identity.key(),
            expected_file_fingerprint: "stale".to_string(),
            title: "new title".to_string(),
        };
        assert!(apply_rename(
            &identity,
            &stale,
            &root.join("backups"),
            &root.join("claudeix.db")
        )
        .unwrap_err()
        .contains("changed on disk"));
        assert_eq!(fs::read(&identity.source_path).unwrap(), before);
    }

    fn simulate_crash_after_replace(
        root: &Path,
        identity: &SessionIdentity,
        request: &TranscriptEditRequest,
    ) -> (PathBuf, EditTransactionRecord, BackupSnapshot, Vec<u8>, Vec<u8>) {
        let backup_root = root.join("backups");
        let database = root.join("claudeix.db");
        let before = fs::read(&identity.source_path).unwrap();
        let (after, old_text) = build_edited_bytes(
            &before,
            &request.target,
            &request.new_text,
            request.allow_assistant_text,
        )
        .unwrap();
        let mut transaction = new_transaction_record(
            identity,
            "edit",
            sha256(&before),
            sha256(&after),
            Some(request.target.clone()),
            Some(old_text),
            Some(request.new_text.clone()),
        );
        audit::begin(&database, &transaction).unwrap();
        let snapshot = create_snapshot(
            &backup_root,
            identity,
            &before,
            "before_edit",
            Some(request.target.clone()),
            None,
            None,
        )
        .unwrap();
        transaction.backup_path = Some(snapshot.backup_path.clone());
        audit::update_status(
            &database,
            &transaction.id,
            "backed_up",
            Some(&snapshot.backup_path),
            now_millis(),
            None,
        )
        .unwrap();
        fs::write(&identity.source_path, &after).unwrap();
        audit::update_status(
            &database,
            &transaction.id,
            "replaced",
            None,
            now_millis(),
            None,
        )
        .unwrap();
        (database, transaction, snapshot, before, after)
    }

    #[test]
    fn startup_recovery_can_keep_replaced_transcript_and_commit_audit() {
        let (root, identity, request) = fixture();
        let (database, transaction, _snapshot, _before, after) =
            simulate_crash_after_replace(&root, &identity, &request);
        let items = pending_recovery_items(&database).unwrap();
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].detected_state, "source_after");
        assert!(items[0].can_keep_current);
        assert!(items[0].can_restore_backup);

        let result = resolve_recovery(
            &database,
            &root.join("backups"),
            &EditRecoveryResolveRequest {
                transaction_id: transaction.id.clone(),
                action: "keep_current".to_string(),
            },
        )
        .unwrap();
        assert_eq!(result.status, "committed");
        assert_eq!(fs::read(&identity.source_path).unwrap(), after);
        assert!(pending_recovery_items(&database).unwrap().is_empty());
        assert_eq!(
            audit::audit_for_session(
                &database,
                &identity.session_id,
                &display_path(&identity.source_path),
            )
            .unwrap()
            .len(),
            1
        );
    }

    #[test]
    fn startup_recovery_can_restore_pre_operation_backup() {
        let (root, identity, request) = fixture();
        let (database, transaction, _snapshot, before, _after) =
            simulate_crash_after_replace(&root, &identity, &request);
        let result = resolve_recovery(
            &database,
            &root.join("backups"),
            &EditRecoveryResolveRequest {
                transaction_id: transaction.id,
                action: "restore_backup".to_string(),
            },
        )
        .unwrap();
        assert_eq!(result.status, "rolled_back");
        assert_eq!(fs::read(&identity.source_path).unwrap(), before);
        assert!(pending_recovery_items(&database).unwrap().is_empty());
        let manifest = load_manifest(&root.join("backups"), &identity.session_id).unwrap();
        assert!(manifest
            .snapshots
            .iter()
            .any(|snapshot| snapshot.reason == "before_recovery"));
    }
}
