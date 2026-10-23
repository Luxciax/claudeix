use crate::app_paths::{claudeix_backups_dir, claudeix_database_path, display_path};
use crate::engine::{EngineEditRequest, EngineRegistry, NativeSessionDetail, PreparedFileMutation};
use crate::models::transcript::EditTarget;
use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::HashSet;
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};
use tauri::State;

#[derive(Default)]
pub struct MutationCoordinator {
    active: Mutex<HashSet<String>>,
}

struct MutationLease<'a> {
    coordinator: &'a MutationCoordinator,
    key: String,
}

impl Drop for MutationLease<'_> {
    fn drop(&mut self) {
        if let Ok(mut active) = self.coordinator.active.lock() {
            active.remove(&self.key);
        }
    }
}

impl MutationCoordinator {
    fn acquire(&self, engine_id: &str, session_key: &str) -> Result<MutationLease<'_>, String> {
        let key = format!("{engine_id}\0{session_key}");
        let mut active = self.active.lock().map_err(|error| error.to_string())?;
        if !active.insert(key.clone()) {
            return Err("this native session is already being mutated".to_string());
        }
        Ok(MutationLease {
            coordinator: self,
            key,
        })
    }

    pub(crate) fn is_locked(&self, engine_id: &str, session_key: &str) -> bool {
        let key = format!("{engine_id}\0{session_key}");
        self.active
            .lock()
            .map(|active| active.contains(&key))
            .unwrap_or(true)
    }
}

pub(crate) fn has_pending_recovery(engine_id: &str, session_key: &str) -> Result<bool, String> {
    let database_path = claudeix_database_path()?;
    Ok(pending_records(&database_path)?.iter().any(|record| {
        record.engine_id == engine_id && record.session_key == session_key
    }))
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EngineMutationSnapshot {
    pub transaction_id: String,
    pub engine_id: String,
    pub native_session_id: String,
    pub source_path: String,
    pub backup_path: String,
    pub created_at: u64,
    pub source_revision_before: String,
    pub source_revision_after: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EngineEditResult {
    pub detail: NativeSessionDetail,
    pub snapshot: EngineMutationSnapshot,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EngineRestoreRequest {
    pub engine_id: String,
    pub session_key: String,
    pub workspace_root: String,
    pub transaction_id: String,
    pub expected_source_revision: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EngineRestoreResult {
    pub detail: NativeSessionDetail,
    pub snapshot: EngineMutationSnapshot,
    pub restored_transaction_id: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EngineMutationRecoveryItem {
    pub transaction_id: String,
    pub engine_id: String,
    pub session_key: String,
    pub native_session_id: String,
    pub source_path: String,
    pub operation: String,
    pub status: String,
    pub backup_path: Option<String>,
    pub source_revision_before: String,
    pub source_revision_after: String,
    pub created_at: u64,
    pub detected_state: String,
    pub backup_valid: bool,
    pub can_keep_current: bool,
    pub can_restore_backup: bool,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EngineMutationRecoveryResolveRequest {
    pub transaction_id: String,
    pub action: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EngineMutationRecoveryResolveResult {
    pub transaction_id: String,
    pub status: String,
    pub source_path: String,
    pub restore_transaction_id: Option<String>,
}

#[derive(Debug, Clone)]
struct MutationRecord {
    id: String,
    engine_id: String,
    session_key: String,
    native_session_id: String,
    workspace_root: String,
    source_path: String,
    operation: String,
    status: String,
    source_revision_before: String,
    source_revision_after: String,
    backup_path: Option<String>,
    target: Option<EditTarget>,
    old_content: Option<String>,
    new_content: Option<String>,
    created_at: u64,
    updated_at: u64,
    error: Option<String>,
}

fn now_millis() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis() as u64)
        .unwrap_or(0)
}

fn unique_id(prefix: &str) -> String {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_nanos())
        .unwrap_or(0);
    format!("{prefix}-{nanos}-{}", std::process::id())
}

pub(crate) fn revision(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

fn safe_component(value: &str) -> String {
    value
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

fn open_database(path: &Path) -> Result<Connection, String> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .map_err(|error| format!("cannot create mutation database directory: {error}"))?;
    }
    let connection = Connection::open(path)
        .map_err(|error| format!("cannot open Claudeix mutation database: {error}"))?;
    connection
        .execute_batch(
            "PRAGMA journal_mode=WAL;
             PRAGMA synchronous=FULL;
             CREATE TABLE IF NOT EXISTS engine_mutation_transactions(
               id TEXT PRIMARY KEY,
               engine_id TEXT NOT NULL,
               session_key TEXT NOT NULL,
               native_session_id TEXT NOT NULL,
               workspace_root TEXT NOT NULL,
               source_path TEXT NOT NULL,
               operation TEXT NOT NULL,
               status TEXT NOT NULL,
               source_revision_before TEXT NOT NULL,
               source_revision_after TEXT NOT NULL,
               backup_path TEXT,
               target_json TEXT,
               old_content TEXT,
               new_content TEXT,
               created_at INTEGER NOT NULL,
               updated_at INTEGER NOT NULL,
               error TEXT
             );
             CREATE INDEX IF NOT EXISTS idx_engine_mutation_session
               ON engine_mutation_transactions(engine_id, session_key, created_at DESC);
             CREATE INDEX IF NOT EXISTS idx_engine_mutation_status
               ON engine_mutation_transactions(status);",
        )
        .map_err(|error| format!("cannot initialize engine mutation database: {error}"))?;
    Ok(connection)
}

fn insert_record(database_path: &Path, record: &MutationRecord) -> Result<(), String> {
    let connection = open_database(database_path)?;
    let target_json = record
        .target
        .as_ref()
        .map(serde_json::to_string)
        .transpose()
        .map_err(|error| error.to_string())?;
    connection
        .execute(
            "INSERT INTO engine_mutation_transactions(
               id, engine_id, session_key, native_session_id, workspace_root,
               source_path, operation, status, source_revision_before,
               source_revision_after, backup_path, target_json, old_content,
               new_content, created_at, updated_at, error
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17)",
            params![
                record.id,
                record.engine_id,
                record.session_key,
                record.native_session_id,
                record.workspace_root,
                record.source_path,
                record.operation,
                record.status,
                record.source_revision_before,
                record.source_revision_after,
                record.backup_path,
                target_json,
                record.old_content,
                record.new_content,
                record.created_at as i64,
                record.updated_at as i64,
                record.error,
            ],
        )
        .map_err(|error| format!("cannot create engine mutation transaction: {error}"))?;
    Ok(())
}

fn update_record(
    database_path: &Path,
    id: &str,
    status: &str,
    backup_path: Option<&str>,
    error: Option<&str>,
) -> Result<(), String> {
    let connection = open_database(database_path)?;
    connection
        .execute(
            "UPDATE engine_mutation_transactions
             SET status=?2, backup_path=COALESCE(?3, backup_path), updated_at=?4, error=?5
             WHERE id=?1",
            params![id, status, backup_path, now_millis() as i64, error],
        )
        .map_err(|error| format!("cannot update engine mutation transaction: {error}"))?;
    Ok(())
}

fn get_record(database_path: &Path, id: &str) -> Result<Option<MutationRecord>, String> {
    let connection = open_database(database_path)?;
    connection
        .query_row(
            "SELECT id, engine_id, session_key, native_session_id, workspace_root,
                    source_path, operation, status, source_revision_before,
                    source_revision_after, backup_path, target_json, old_content,
                    new_content, created_at, updated_at, error
             FROM engine_mutation_transactions WHERE id=?1",
            [id],
            |row| {
                let target_json: Option<String> = row.get(11)?;
                Ok(MutationRecord {
                    id: row.get(0)?,
                    engine_id: row.get(1)?,
                    session_key: row.get(2)?,
                    native_session_id: row.get(3)?,
                    workspace_root: row.get(4)?,
                    source_path: row.get(5)?,
                    operation: row.get(6)?,
                    status: row.get(7)?,
                    source_revision_before: row.get(8)?,
                    source_revision_after: row.get(9)?,
                    backup_path: row.get(10)?,
                    target: target_json
                        .as_deref()
                        .and_then(|value| serde_json::from_str(value).ok()),
                    old_content: row.get(12)?,
                    new_content: row.get(13)?,
                    created_at: row.get::<_, i64>(14)? as u64,
                    updated_at: row.get::<_, i64>(15)? as u64,
                    error: row.get(16)?,
                })
            },
        )
        .optional()
        .map_err(|error| format!("cannot read engine mutation transaction: {error}"))
}

fn pending_records(database_path: &Path) -> Result<Vec<MutationRecord>, String> {
    let connection = open_database(database_path)?;
    let mut statement = connection
        .prepare(
            "SELECT id, engine_id, session_key, native_session_id, workspace_root,
                    source_path, operation, status, source_revision_before,
                    source_revision_after, backup_path, target_json, old_content,
                    new_content, created_at, updated_at, error
             FROM engine_mutation_transactions
             WHERE status NOT IN ('committed', 'rolled_back', 'dismissed')
             ORDER BY created_at ASC",
        )
        .map_err(|error| format!("cannot query pending engine mutations: {error}"))?;
    let rows = statement
        .query_map([], |row| {
            let target_json: Option<String> = row.get(11)?;
            Ok(MutationRecord {
                id: row.get(0)?,
                engine_id: row.get(1)?,
                session_key: row.get(2)?,
                native_session_id: row.get(3)?,
                workspace_root: row.get(4)?,
                source_path: row.get(5)?,
                operation: row.get(6)?,
                status: row.get(7)?,
                source_revision_before: row.get(8)?,
                source_revision_after: row.get(9)?,
                backup_path: row.get(10)?,
                target: target_json
                    .as_deref()
                    .and_then(|value| serde_json::from_str(value).ok()),
                old_content: row.get(12)?,
                new_content: row.get(13)?,
                created_at: row.get::<_, i64>(14)? as u64,
                updated_at: row.get::<_, i64>(15)? as u64,
                error: row.get(16)?,
            })
        })
        .map_err(|error| format!("cannot query pending engine mutations: {error}"))?;
    rows.collect::<Result<Vec<_>, _>>()
        .map_err(|error| format!("cannot decode pending engine mutation: {error}"))
}

fn recovery_item(record: &MutationRecord) -> Result<EngineMutationRecoveryItem, String> {
    let source_path = PathBuf::from(&record.source_path);
    let current_revision = if source_path.is_file() {
        Some(revision(&fs::read(&source_path).map_err(|error| {
            format!("cannot read engine mutation source during recovery: {error}")
        })?))
    } else {
        None
    };
    let backup_valid = record
        .backup_path
        .as_ref()
        .map(PathBuf::from)
        .filter(|path| path.is_file())
        .map(|path| {
            fs::read(path)
                .map(|bytes| revision(&bytes) == record.source_revision_before)
                .unwrap_or(false)
        })
        .unwrap_or(false);
    let detected_state = match current_revision.as_deref() {
        None => "source_missing",
        Some(value) if value == record.source_revision_before => "source_before",
        Some(value) if value == record.source_revision_after => "source_after",
        Some(_) => "source_unknown",
    }
    .to_string();
    let can_keep_current = current_revision.is_some();
    let can_restore_backup = backup_valid && current_revision.is_some();
    Ok(EngineMutationRecoveryItem {
        transaction_id: record.id.clone(),
        engine_id: record.engine_id.clone(),
        session_key: record.session_key.clone(),
        native_session_id: record.native_session_id.clone(),
        source_path: record.source_path.clone(),
        operation: record.operation.clone(),
        status: record.status.clone(),
        backup_path: record.backup_path.clone(),
        source_revision_before: record.source_revision_before.clone(),
        source_revision_after: record.source_revision_after.clone(),
        created_at: record.created_at,
        detected_state,
        backup_valid,
        can_keep_current,
        can_restore_backup,
    })
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
        .unwrap_or("session");
    Ok(parent.join(format!(".{file_name}.claudeix-{}.tmp", unique_id("mutation"))))
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
    fs::rename(source, destination)
        .map_err(|error| format!("cannot atomically replace {}: {error}", destination.display()))
}

fn backup_path_for(
    backup_root: &Path,
    mutation: &PreparedFileMutation,
    transaction_id: &str,
) -> Result<PathBuf, String> {
    let root = backup_root
        .join(&mutation.engine_id)
        .join(safe_component(&mutation.native_session_id));
    fs::create_dir_all(&root)
        .map_err(|error| format!("cannot create engine backup directory {}: {error}", root.display()))?;
    let extension = mutation
        .source_path
        .extension()
        .and_then(|value| value.to_str())
        .unwrap_or("bak");
    Ok(root.join(format!("{transaction_id}-before.{extension}")))
}

fn execute_file_mutation(
    registry: &EngineRegistry,
    mutation: &PreparedFileMutation,
) -> Result<EngineMutationSnapshot, String> {
    let backup_root = claudeix_backups_dir()?;
    let database_path = claudeix_database_path()?;
    execute_file_mutation_at(registry, mutation, &backup_root, &database_path)
}

fn execute_file_mutation_at(
    registry: &EngineRegistry,
    mutation: &PreparedFileMutation,
    backup_root: &Path,
    database_path: &Path,
) -> Result<EngineMutationSnapshot, String> {
    let current_bytes = fs::read(&mutation.source_path)
        .map_err(|error| format!("cannot read native session before mutation: {error}"))?;
    if revision(&current_bytes) != revision(&mutation.source_bytes) {
        return Err("native session changed while the mutation was being prepared; reload it".to_string());
    }

    let before = revision(&current_bytes);
    let after = revision(&mutation.candidate_bytes);
    let created_at = now_millis();
    let transaction_id = unique_id(&format!("{}-{}", mutation.engine_id, mutation.operation));
    let mut record = MutationRecord {
        id: transaction_id.clone(),
        engine_id: mutation.engine_id.clone(),
        session_key: mutation.session_key.clone(),
        native_session_id: mutation.native_session_id.clone(),
        workspace_root: display_path(&mutation.workspace_root),
        source_path: display_path(&mutation.source_path),
        operation: mutation.operation.clone(),
        status: "prepared".to_string(),
        source_revision_before: before.clone(),
        source_revision_after: after.clone(),
        backup_path: None,
        target: mutation.target.clone(),
        old_content: mutation.old_content.clone(),
        new_content: mutation.new_content.clone(),
        created_at,
        updated_at: created_at,
        error: None,
    };
    insert_record(&database_path, &record)?;

    let backup_path = backup_path_for(backup_root, mutation, &transaction_id)?;
    write_file_synced(&backup_path, &current_bytes, true)?;
    record.backup_path = Some(display_path(&backup_path));
    update_record(
        &database_path,
        &transaction_id,
        "backed_up",
        record.backup_path.as_deref(),
        None,
    )?;

    let rechecked = fs::read(&mutation.source_path)
        .map_err(|error| format!("cannot re-read native session before replace: {error}"))?;
    if revision(&rechecked) != before {
        let error = "native session changed after backup; mutation was cancelled";
        let _ = update_record(
            &database_path,
            &transaction_id,
            "rolled_back",
            record.backup_path.as_deref(),
            Some(error),
        );
        return Err(error.to_string());
    }

    let temp = temp_path_for(&mutation.source_path)?;
    write_file_synced(&temp, &mutation.candidate_bytes, true)?;
    if let Err(error) = registry.validate_file_candidate(&mutation.engine_id, mutation, &temp) {
        let _ = fs::remove_file(&temp);
        let _ = update_record(
            &database_path,
            &transaction_id,
            "rolled_back",
            record.backup_path.as_deref(),
            Some(&error),
        );
        return Err(error);
    }

    let mut source_touched = false;
    let result = (|| -> Result<(), String> {
        atomic_replace(&temp, &mutation.source_path)?;
        source_touched = true;
        update_record(
            &database_path,
            &transaction_id,
            "replaced",
            record.backup_path.as_deref(),
            None,
        )?;
        registry.validate_file_candidate(&mutation.engine_id, mutation, &mutation.source_path)?;
        let committed = fs::read(&mutation.source_path)
            .map_err(|error| format!("cannot verify native session after replace: {error}"))?;
        if revision(&committed) != after {
            return Err("native session revision does not match the prepared mutation".to_string());
        }
        update_record(
            &database_path,
            &transaction_id,
            "committed",
            record.backup_path.as_deref(),
            None,
        )?;
        Ok(())
    })();

    if let Err(error) = result {
        if source_touched {
            let rollback_temp = temp_path_for(&mutation.source_path)?;
            write_file_synced(&rollback_temp, &current_bytes, true)?;
            registry.validate_file_candidate(&mutation.engine_id, mutation, &rollback_temp)?;
            match atomic_replace(&rollback_temp, &mutation.source_path) {
                Ok(()) => {
                    let _ = update_record(
                        &database_path,
                        &transaction_id,
                        "rolled_back",
                        record.backup_path.as_deref(),
                        Some(&error),
                    );
                    return Err(format!(
                        "mutation failed, but Claudeix restored the original native session: {error}"
                    ));
                }
                Err(rollback_error) => {
                    let combined = format!(
                        "mutation failed after replacing the native session: {error}; rollback also failed: {rollback_error}"
                    );
                    let _ = update_record(
                        &database_path,
                        &transaction_id,
                        "rollback_failed",
                        record.backup_path.as_deref(),
                        Some(&combined),
                    );
                    return Err(combined);
                }
            }
        }
        let _ = update_record(
            &database_path,
            &transaction_id,
            "rolled_back",
            record.backup_path.as_deref(),
            Some(&error),
        );
        return Err(error);
    }

    Ok(EngineMutationSnapshot {
        transaction_id,
        engine_id: mutation.engine_id.clone(),
        native_session_id: mutation.native_session_id.clone(),
        source_path: display_path(&mutation.source_path),
        backup_path: display_path(&backup_path),
        created_at,
        source_revision_before: before,
        source_revision_after: after,
    })
}

#[tauri::command]
pub fn engine_edit_message(
    request: EngineEditRequest,
    registry: State<'_, EngineRegistry>,
    coordinator: State<'_, MutationCoordinator>,
) -> Result<EngineEditResult, String> {
    let _lease = coordinator.acquire(&request.engine_id, &request.session_key)?;
    let mutation = registry.prepare_file_edit(&request)?;
    if revision(&mutation.source_bytes) != request.expected_source_revision {
        return Err("native session changed on disk; reload it before editing".to_string());
    }
    let snapshot = execute_file_mutation(&registry, &mutation)?;
    let detail = registry.get_session_detail(
        &request.engine_id,
        &request.session_key,
        Path::new(&request.workspace_root),
    )?;
    Ok(EngineEditResult { detail, snapshot })
}

#[tauri::command]
pub fn engine_restore_mutation(
    request: EngineRestoreRequest,
    registry: State<'_, EngineRegistry>,
    coordinator: State<'_, MutationCoordinator>,
) -> Result<EngineRestoreResult, String> {
    let _lease = coordinator.acquire(&request.engine_id, &request.session_key)?;
    let database_path = claudeix_database_path()?;
    let original = get_record(&database_path, &request.transaction_id)?
        .ok_or_else(|| "mutation snapshot was not found".to_string())?;
    if original.engine_id != request.engine_id || original.session_key != request.session_key {
        return Err("mutation snapshot belongs to a different native session".to_string());
    }
    let backup_path = original
        .backup_path
        .as_ref()
        .map(PathBuf::from)
        .ok_or_else(|| "mutation snapshot has no backup file".to_string())?;
    let backup_bytes = fs::read(&backup_path)
        .map_err(|error| format!("cannot read mutation snapshot: {error}"))?;
    if revision(&backup_bytes) != original.source_revision_before {
        return Err("mutation snapshot hash mismatch".to_string());
    }
    let source_path = PathBuf::from(&original.source_path);
    let current_bytes = fs::read(&source_path)
        .map_err(|error| format!("cannot read current native session: {error}"))?;
    if revision(&current_bytes) != request.expected_source_revision {
        return Err("native session changed on disk; reload it before restoring".to_string());
    }

    let mutation = PreparedFileMutation {
        engine_id: request.engine_id.clone(),
        session_key: request.session_key.clone(),
        native_session_id: original.native_session_id,
        workspace_root: PathBuf::from(&request.workspace_root),
        source_path,
        target: original.target,
        source_bytes: current_bytes,
        candidate_bytes: backup_bytes,
        old_content: original.new_content,
        new_content: original.old_content,
        operation: "restore".to_string(),
    };
    registry.validate_file_candidate(&request.engine_id, &mutation, &backup_path)?;
    let snapshot = execute_file_mutation(&registry, &mutation)?;
    let detail = registry.get_session_detail(
        &request.engine_id,
        &request.session_key,
        Path::new(&request.workspace_root),
    )?;
    Ok(EngineRestoreResult {
        detail,
        snapshot,
        restored_transaction_id: request.transaction_id,
    })
}

#[tauri::command]
pub fn engine_mutation_recovery_list() -> Result<Vec<EngineMutationRecoveryItem>, String> {
    let database_path = claudeix_database_path()?;
    pending_records(&database_path)?
        .iter()
        .map(recovery_item)
        .collect()
}

#[tauri::command]
pub fn engine_mutation_recovery_resolve(
    request: EngineMutationRecoveryResolveRequest,
    registry: State<'_, EngineRegistry>,
    coordinator: State<'_, MutationCoordinator>,
) -> Result<EngineMutationRecoveryResolveResult, String> {
    let database_path = claudeix_database_path()?;
    let record = get_record(&database_path, &request.transaction_id)?
        .ok_or_else(|| "engine mutation recovery item was not found".to_string())?;
    if matches!(record.status.as_str(), "committed" | "rolled_back" | "dismissed") {
        return Ok(EngineMutationRecoveryResolveResult {
            transaction_id: record.id,
            status: record.status,
            source_path: record.source_path,
            restore_transaction_id: None,
        });
    }
    let _lease = coordinator.acquire(&record.engine_id, &record.session_key)?;
    let item = recovery_item(&record)?;
    let source_path = PathBuf::from(&record.source_path);
    if !source_path.is_file() {
        return Err("native session source is missing; automatic recovery cannot safely recreate it yet".to_string());
    }
    let current_bytes = fs::read(&source_path)
        .map_err(|error| format!("cannot read native session during recovery: {error}"))?;
    let validation_mutation = PreparedFileMutation {
        engine_id: record.engine_id.clone(),
        session_key: record.session_key.clone(),
        native_session_id: record.native_session_id.clone(),
        workspace_root: PathBuf::from(&record.workspace_root),
        source_path: source_path.clone(),
        target: record.target.clone(),
        source_bytes: current_bytes.clone(),
        candidate_bytes: current_bytes.clone(),
        old_content: record.old_content.clone(),
        new_content: record.new_content.clone(),
        operation: "recovery_validate".to_string(),
    };

    match request.action.as_str() {
        "keep_current" => {
            if !item.can_keep_current {
                return Err("current native session cannot be kept because the source is unavailable".to_string());
            }
            registry.validate_file_candidate(
                &record.engine_id,
                &validation_mutation,
                &source_path,
            )?;
            let current_revision = revision(&current_bytes);
            let status = if current_revision == record.source_revision_before {
                "rolled_back"
            } else if current_revision == record.source_revision_after {
                "committed"
            } else {
                "dismissed"
            };
            update_record(
                &database_path,
                &record.id,
                status,
                record.backup_path.as_deref(),
                None,
            )?;
            Ok(EngineMutationRecoveryResolveResult {
                transaction_id: record.id,
                status: status.to_string(),
                source_path: record.source_path,
                restore_transaction_id: None,
            })
        }
        "restore_backup" => {
            if !item.can_restore_backup {
                return Err("a verified mutation backup is not available for recovery".to_string());
            }
            let backup_path = record
                .backup_path
                .as_ref()
                .map(PathBuf::from)
                .ok_or_else(|| "engine mutation recovery item has no backup".to_string())?;
            let backup_bytes = fs::read(&backup_path)
                .map_err(|error| format!("cannot read engine mutation recovery backup: {error}"))?;
            if revision(&backup_bytes) != record.source_revision_before {
                return Err("engine mutation recovery backup hash mismatch".to_string());
            }
            let restore_mutation = PreparedFileMutation {
                engine_id: record.engine_id.clone(),
                session_key: record.session_key.clone(),
                native_session_id: record.native_session_id.clone(),
                workspace_root: PathBuf::from(&record.workspace_root),
                source_path: source_path.clone(),
                target: record.target.clone(),
                source_bytes: current_bytes,
                candidate_bytes: backup_bytes,
                old_content: record.new_content.clone(),
                new_content: record.old_content.clone(),
                operation: "recovery_restore".to_string(),
            };
            registry.validate_file_candidate(
                &record.engine_id,
                &restore_mutation,
                &backup_path,
            )?;
            let restore_snapshot = execute_file_mutation(&registry, &restore_mutation)?;
            update_record(
                &database_path,
                &record.id,
                "rolled_back",
                record.backup_path.as_deref(),
                None,
            )?;
            Ok(EngineMutationRecoveryResolveResult {
                transaction_id: record.id,
                status: "rolled_back".to_string(),
                source_path: record.source_path,
                restore_transaction_id: Some(restore_snapshot.transaction_id),
            })
        }
        _ => Err("unsupported engine mutation recovery action".to_string()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::EngineAdapter;
    use crate::engines::pi::PiAdapter;

    #[test]
    fn revision_is_stable_sha256() {
        assert_eq!(
            revision(b"abc"),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
    }

    #[test]
    fn pi_file_transaction_snapshots_commits_and_restores() {
        let root = std::env::temp_dir().join(format!(
            "claudeix-mutation-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .expect("clock")
                .as_nanos()
        ));
        let workspace = root.join("workspace");
        let sessions_root = root.join("sessions");
        let project = sessions_root.join("project-key");
        let backup_root = root.join("backups");
        let database_path = root.join("audit.db");
        fs::create_dir_all(&workspace).expect("workspace");
        fs::create_dir_all(&project).expect("project");
        let source_path = project.join("demo.jsonl");
        let original = format!(
            "{{\"type\":\"session\",\"id\":\"pi-s1\",\"cwd\":{}}}\n{{\"type\":\"session_info\",\"id\":\"n1\",\"parentId\":null,\"name\":\"Pi Demo\"}}\n{{\"type\":\"message\",\"id\":\"u1\",\"parentId\":\"n1\",\"message\":{{\"role\":\"user\",\"content\":[{{\"type\":\"text\",\"text\":\"before\"}}]}}}}\n",
            serde_json::to_string(&display_path(&workspace)).expect("workspace json")
        );
        fs::write(&source_path, original.as_bytes()).expect("fixture");

        let adapter = PiAdapter::with_sessions_root(sessions_root);
        let found = adapter.scan_sessions(&workspace).expect("scan");
        let detail = adapter
            .get_session_detail(&found[0].session_key, &workspace)
            .expect("detail");
        let request = EngineEditRequest {
            engine_id: "pi".to_string(),
            session_key: found[0].session_key.clone(),
            workspace_root: display_path(&workspace),
            expected_source_revision: detail.source_revision.clone(),
            target: detail.records[1].editable_targets[0].clone(),
            new_text: "after".to_string(),
        };
        let mutation = adapter.prepare_file_edit(&request).expect("prepare");
        let registry = EngineRegistry::default();
        let snapshot = execute_file_mutation_at(
            &registry,
            &mutation,
            &backup_root,
            &database_path,
        )
        .expect("commit mutation");

        let edited = fs::read_to_string(&source_path).expect("edited source");
        assert!(edited.contains("\"text\":\"after\""));
        assert_eq!(
            fs::read_to_string(&snapshot.backup_path).expect("backup"),
            original
        );
        let committed = get_record(&database_path, &snapshot.transaction_id)
            .expect("audit")
            .expect("transaction");
        assert_eq!(committed.status, "committed");

        let current_bytes = fs::read(&source_path).expect("current");
        let backup_bytes = fs::read(&snapshot.backup_path).expect("backup bytes");
        let restore = PreparedFileMutation {
            engine_id: "pi".to_string(),
            session_key: mutation.session_key.clone(),
            native_session_id: mutation.native_session_id.clone(),
            workspace_root: workspace.clone(),
            source_path: source_path.clone(),
            target: mutation.target.clone(),
            source_bytes: current_bytes,
            candidate_bytes: backup_bytes,
            old_content: Some("after".to_string()),
            new_content: Some("before".to_string()),
            operation: "restore".to_string(),
        };
        let restore_snapshot = execute_file_mutation_at(
            &registry,
            &restore,
            &backup_root,
            &database_path,
        )
        .expect("restore mutation");
        assert_eq!(fs::read_to_string(&source_path).expect("restored"), original);
        let restore_record = get_record(&database_path, &restore_snapshot.transaction_id)
            .expect("restore audit")
            .expect("restore transaction");
        assert_eq!(restore_record.status, "committed");

        fs::remove_dir_all(root).ok();
    }

    #[test]
    fn recovery_detects_replaced_source_and_verified_backup() {
        let root = std::env::temp_dir().join(format!(
            "claudeix-recovery-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .expect("clock")
                .as_nanos()
        ));
        fs::create_dir_all(&root).expect("root");
        let source_path = root.join("session.jsonl");
        let backup_path = root.join("before.jsonl");
        let database_path = root.join("audit.db");
        let before = b"{\"type\":\"session\",\"id\":\"s1\"}\n";
        let after = b"{\"type\":\"session\",\"id\":\"s1\"}\n{\"type\":\"session_info\",\"id\":\"n1\"}\n";
        fs::write(&source_path, after).expect("source");
        fs::write(&backup_path, before).expect("backup");
        let record = MutationRecord {
            id: "recovery-test".to_string(),
            engine_id: "pi".to_string(),
            session_key: display_path(&source_path),
            native_session_id: "s1".to_string(),
            workspace_root: "C:/".to_string(),
            source_path: display_path(&source_path),
            operation: "edit_message".to_string(),
            status: "replaced".to_string(),
            source_revision_before: revision(before),
            source_revision_after: revision(after),
            backup_path: Some(display_path(&backup_path)),
            target: None,
            old_content: None,
            new_content: None,
            created_at: now_millis(),
            updated_at: now_millis(),
            error: None,
        };
        insert_record(&database_path, &record).expect("insert");

        let pending = pending_records(&database_path).expect("pending");
        assert_eq!(pending.len(), 1);
        let item = recovery_item(&pending[0]).expect("recovery item");
        assert_eq!(item.detected_state, "source_after");
        assert!(item.backup_valid);
        assert!(item.can_keep_current);
        assert!(item.can_restore_backup);

        fs::remove_dir_all(root).ok();
    }
}
