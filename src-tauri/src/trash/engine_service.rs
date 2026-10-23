use crate::app_paths::{claudeix_trash_dir, display_path};
use crate::engine::EngineRegistry;
use crate::engines::pi::runtime::PiRuntimeState;
use crate::mutation::{has_pending_recovery, MutationCoordinator};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::HashSet;
use std::fs::{self, File, OpenOptions};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};
use tauri::State;

#[derive(Default)]
pub struct EngineTrashService {
    active_sessions: Mutex<HashSet<String>>,
    active_entries: Mutex<HashSet<String>>,
}

struct Lease<'a> {
    set: &'a Mutex<HashSet<String>>,
    key: String,
}

impl Drop for Lease<'_> {
    fn drop(&mut self) {
        if let Ok(mut set) = self.set.lock() {
            set.remove(&self.key);
        }
    }
}

impl EngineTrashService {
    fn acquire_session(&self, engine_id: &str, session_key: &str) -> Result<Lease<'_>, String> {
        let key = format!("{engine_id}\0{session_key}");
        let mut set = self.active_sessions.lock().map_err(|error| error.to_string())?;
        if !set.insert(key.clone()) {
            return Err("this native session is already being recycled".to_string());
        }
        Ok(Lease {
            set: &self.active_sessions,
            key,
        })
    }

    fn acquire_entry(&self, trash_id: &str) -> Result<Lease<'_>, String> {
        let mut set = self.active_entries.lock().map_err(|error| error.to_string())?;
        if !set.insert(trash_id.to_string()) {
            return Err("this recycle-bin entry is already being changed".to_string());
        }
        Ok(Lease {
            set: &self.active_entries,
            key: trash_id.to_string(),
        })
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EngineTrashFilePreview {
    pub original_path: String,
    pub purpose: String,
    pub size: u64,
    pub sha256: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EngineTrashDeletePreview {
    pub engine_id: String,
    pub session_key: String,
    pub native_session_id: String,
    pub title: String,
    pub workspace_root: String,
    pub files: Vec<EngineTrashFilePreview>,
    pub total_files: usize,
    pub total_size: u64,
    pub preview_fingerprint: String,
    pub runtime_active: bool,
    pub external_runtime_active: bool,
    pub external_runtime_pid: Option<u32>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EngineTrashMoveRequest {
    pub engine_id: String,
    pub session_key: String,
    pub workspace_root: String,
    pub expected_preview_fingerprint: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct EngineTrashFileRecord {
    original_path: String,
    payload_relative_path: String,
    purpose: String,
    size: u64,
    sha256: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct EngineTrashManifest {
    version: u32,
    trash_id: String,
    engine_id: String,
    session_key: String,
    native_session_id: String,
    title: String,
    workspace_root: String,
    deleted_at: u64,
    restored_at: Option<u64>,
    status: String,
    files: Vec<EngineTrashFileRecord>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EngineTrashItemSummary {
    pub trash_id: String,
    pub engine_id: String,
    pub session_key: String,
    pub native_session_id: String,
    pub title: String,
    pub workspace_root: String,
    pub deleted_at: u64,
    pub restored_at: Option<u64>,
    pub status: String,
    pub total_files: usize,
    pub total_size: u64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EngineTrashRestoreFileState {
    pub original_path: String,
    pub payload_path: String,
    pub purpose: String,
    pub size: u64,
    pub sha256: String,
    pub state: String,
    pub current_sha256: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EngineTrashRestorePreview {
    pub trash_id: String,
    pub engine_id: String,
    pub native_session_id: String,
    pub title: String,
    pub files: Vec<EngineTrashRestoreFileState>,
    pub can_restore_safely: bool,
    pub has_conflicts: bool,
    pub corrupt: bool,
    pub preview_fingerprint: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EngineTrashRestoreRequest {
    pub trash_id: String,
    pub strategy: String,
    pub expected_preview_fingerprint: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EngineTrashRestoreResult {
    pub trash_id: String,
    pub status: String,
    pub exchange_trash_id: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EngineTrashPurgeFile {
    pub path: String,
    pub size: u64,
    pub purpose: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EngineTrashPurgePreview {
    pub trash_id: String,
    pub engine_id: String,
    pub native_session_id: String,
    pub title: String,
    pub files: Vec<EngineTrashPurgeFile>,
    pub total_files: usize,
    pub total_size: u64,
    pub confirmation_text: String,
    pub preview_fingerprint: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EngineTrashPurgeRequest {
    pub trash_id: String,
    pub confirmation_text: String,
    pub expected_preview_fingerprint: String,
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

fn engine_root_in(root: &Path, engine_id: &str) -> PathBuf {
    root.join(safe_component(engine_id))
}

fn sha256_file(path: &Path) -> Result<String, String> {
    let mut file = File::open(path)
        .map_err(|error| format!("cannot open {} for hashing: {error}", path.display()))?;
    let mut hasher = Sha256::new();
    let mut buffer = [0u8; 64 * 1024];
    loop {
        let read = file
            .read(&mut buffer)
            .map_err(|error| format!("cannot read {} for hashing: {error}", path.display()))?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
    }
    Ok(format!("{:x}", hasher.finalize()))
}

fn write_synced(path: &Path, bytes: &[u8]) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|error| error.to_string())?;
    }
    let mut file = OpenOptions::new()
        .create(true)
        .truncate(true)
        .write(true)
        .open(path)
        .map_err(|error| error.to_string())?;
    file.write_all(bytes)
        .and_then(|_| file.flush())
        .and_then(|_| file.sync_all())
        .map_err(|error| error.to_string())
}

fn write_manifest(dir: &Path, manifest: &EngineTrashManifest) -> Result<(), String> {
    let bytes = serde_json::to_vec_pretty(manifest).map_err(|error| error.to_string())?;
    write_synced(&dir.join("manifest.json"), &bytes)
}

fn read_manifest(dir: &Path) -> Result<EngineTrashManifest, String> {
    let path = dir.join("manifest.json");
    let bytes = fs::read(&path).map_err(|error| format!("cannot read {}: {error}", path.display()))?;
    serde_json::from_slice(&bytes)
        .map_err(|error| format!("invalid engine recycle manifest {}: {error}", path.display()))
}

fn preview_fingerprint(files: &[EngineTrashFilePreview]) -> String {
    let mut hasher = Sha256::new();
    for file in files {
        hasher.update(file.original_path.as_bytes());
        hasher.update([0]);
        hasher.update(file.size.to_le_bytes());
        hasher.update(file.sha256.as_bytes());
        hasher.update([0]);
    }
    format!("{:x}", hasher.finalize())
}

fn runtime_active(
    engine_id: &str,
    session_key: &str,
    workspace_root: &str,
    pi_runtime: &PiRuntimeState,
) -> bool {
    engine_id == "pi" && pi_runtime.conflicts_with(session_key, workspace_root)
}

fn build_delete_preview(
    engine_id: &str,
    session_key: &str,
    workspace_root: &Path,
    registry: &EngineRegistry,
    pi_runtime: &PiRuntimeState,
) -> Result<EngineTrashDeletePreview, String> {
    let descriptor = registry
        .descriptors()
        .into_iter()
        .find(|descriptor| descriptor.id == engine_id)
        .ok_or_else(|| format!("unknown engine: {engine_id}"))?;
    if !descriptor.capabilities.delete_session {
        return Err(format!("{} does not expose native session deletion", descriptor.display_name));
    }
    let detail = registry.get_session_detail(engine_id, session_key, workspace_root)?;
    let source = PathBuf::from(&detail.summary.source_path);
    if !source.is_file() {
        return Err(format!("native session source no longer exists: {}", source.display()));
    }
    let metadata = fs::metadata(&source).map_err(|error| error.to_string())?;
    let file = EngineTrashFilePreview {
        original_path: display_path(&source),
        purpose: "native_session".to_string(),
        size: metadata.len(),
        sha256: sha256_file(&source)?,
    };
    let files = vec![file];
    let managed_pid = if engine_id == "pi" { pi_runtime.pid() } else { None };
    let external_pid = registry
        .external_writer_pid(engine_id, session_key, workspace_root)?
        .filter(|pid| Some(*pid) != managed_pid);
    Ok(EngineTrashDeletePreview {
        engine_id: engine_id.to_string(),
        session_key: session_key.to_string(),
        native_session_id: detail.summary.native_session_id,
        title: detail.summary.title,
        workspace_root: display_path(workspace_root),
        total_files: files.len(),
        total_size: files.iter().map(|file| file.size).sum(),
        preview_fingerprint: preview_fingerprint(&files),
        runtime_active: runtime_active(
            engine_id,
            session_key,
            &display_path(workspace_root),
            pi_runtime,
        ),
        external_runtime_active: external_pid.is_some(),
        external_runtime_pid: external_pid,
        files,
    })
}

fn summary_from_manifest(manifest: &EngineTrashManifest) -> EngineTrashItemSummary {
    EngineTrashItemSummary {
        trash_id: manifest.trash_id.clone(),
        engine_id: manifest.engine_id.clone(),
        session_key: manifest.session_key.clone(),
        native_session_id: manifest.native_session_id.clone(),
        title: manifest.title.clone(),
        workspace_root: manifest.workspace_root.clone(),
        deleted_at: manifest.deleted_at,
        restored_at: manifest.restored_at,
        status: manifest.status.clone(),
        total_files: manifest.files.len(),
        total_size: manifest.files.iter().map(|file| file.size).sum(),
    }
}

fn move_preview_to_trash_in(
    trash_root: &Path,
    preview: &EngineTrashDeletePreview,
) -> Result<EngineTrashItemSummary, String> {
    let root = engine_root_in(trash_root, &preview.engine_id);
    fs::create_dir_all(&root).map_err(|error| error.to_string())?;
    let trash_id = unique_id(&format!("{}-{}", safe_component(&preview.engine_id), safe_component(&preview.native_session_id)));
    let staging = root.join(format!(".staging-{trash_id}"));
    let final_dir = root.join(&trash_id);
    fs::create_dir_all(staging.join("payload")).map_err(|error| error.to_string())?;

    let files: Vec<EngineTrashFileRecord> = preview
        .files
        .iter()
        .enumerate()
        .map(|(index, file)| EngineTrashFileRecord {
            original_path: file.original_path.clone(),
            payload_relative_path: format!("payload/{index}-session.jsonl"),
            purpose: file.purpose.clone(),
            size: file.size,
            sha256: file.sha256.clone(),
        })
        .collect();
    let mut manifest = EngineTrashManifest {
        version: 1,
        trash_id: trash_id.clone(),
        engine_id: preview.engine_id.clone(),
        session_key: preview.session_key.clone(),
        native_session_id: preview.native_session_id.clone(),
        title: preview.title.clone(),
        workspace_root: preview.workspace_root.clone(),
        deleted_at: now_millis(),
        restored_at: None,
        status: "moving".to_string(),
        files,
    };
    write_manifest(&staging, &manifest)?;

    let mut moved: Vec<(PathBuf, PathBuf)> = Vec::new();
    for record in &manifest.files {
        let original = PathBuf::from(&record.original_path);
        if !original.is_file() || sha256_file(&original)? != record.sha256 {
            for (payload, original) in moved.iter().rev() {
                let _ = fs::rename(payload, original);
            }
            let _ = fs::remove_dir_all(&staging);
            return Err("native session changed after delete preview; review the exact files again".to_string());
        }
        let payload = staging.join(&record.payload_relative_path);
        if let Some(parent) = payload.parent() {
            fs::create_dir_all(parent).map_err(|error| error.to_string())?;
        }
        fs::rename(&original, &payload).map_err(|error| {
            format!("cannot move native session {} to recycle bin: {error}", original.display())
        })?;
        moved.push((payload.clone(), original.clone()));
        if sha256_file(&payload)? != record.sha256 {
            for (payload, original) in moved.iter().rev() {
                if !original.exists() {
                    let _ = fs::rename(payload, original);
                }
            }
            let _ = fs::remove_dir_all(&staging);
            return Err("recycle-bin payload hash mismatch; move was rolled back".to_string());
        }
    }
    manifest.status = "trashed".to_string();
    write_manifest(&staging, &manifest)?;
    fs::rename(&staging, &final_dir).map_err(|error| {
        for (payload, original) in moved.iter().rev() {
            if !original.exists() {
                let _ = fs::rename(payload, original);
            }
        }
        format!("cannot commit recycle-bin transaction: {error}")
    })?;
    Ok(summary_from_manifest(&manifest))
}

fn move_preview_to_trash(preview: &EngineTrashDeletePreview) -> Result<EngineTrashItemSummary, String> {
    move_preview_to_trash_in(&claudeix_trash_dir()?, preview)
}

fn recover_staging_in(root: &Path) -> Result<Vec<String>, String> {
    if !root.is_dir() {
        return Ok(Vec::new());
    }
    let mut warnings = Vec::new();
    for engine in fs::read_dir(root).map_err(|error| error.to_string())? {
        let engine_dir = engine.map_err(|error| error.to_string())?.path();
        if !engine_dir.is_dir() {
            continue;
        }
        for entry in fs::read_dir(&engine_dir).map_err(|error| error.to_string())? {
            let staging = entry.map_err(|error| error.to_string())?.path();
            let Some(name) = staging.file_name().and_then(|name| name.to_str()) else {
                continue;
            };
            if !staging.is_dir() || !name.starts_with(".staging-") {
                continue;
            }
            let Ok(mut manifest) = read_manifest(&staging) else {
                warnings.push(format!("cannot inspect incomplete engine-trash transaction {name}"));
                continue;
            };
            let mut conflict = false;
            for record in &manifest.files {
                let payload = staging.join(&record.payload_relative_path);
                if !payload.is_file() {
                    continue;
                }
                let original = PathBuf::from(&record.original_path);
                if original.exists() {
                    if sha256_file(&original).map(|hash| hash != record.sha256).unwrap_or(true) {
                        conflict = true;
                    }
                    continue;
                }
                if let Some(parent) = original.parent() {
                    fs::create_dir_all(parent).map_err(|error| error.to_string())?;
                }
                fs::rename(&payload, &original).map_err(|error| {
                    format!(
                        "cannot roll back incomplete engine-trash move {} -> {}: {error}",
                        payload.display(),
                        original.display()
                    )
                })?;
            }

            manifest.status = if conflict {
                warnings.push(format!(
                    "incomplete engine-trash transaction {} has a recovery conflict",
                    manifest.trash_id
                ));
                "recovery_conflict".to_string()
            } else {
                "rolled_back".to_string()
            };
            write_manifest(&staging, &manifest)?;
            if conflict {
                let visible = engine_dir.join(&manifest.trash_id);
                if !visible.exists() {
                    fs::rename(&staging, &visible).map_err(|error| {
                        format!("cannot expose engine-trash recovery conflict: {error}")
                    })?;
                }
            } else {
                fs::remove_dir_all(&staging).map_err(|error| {
                    format!("cannot remove rolled-back engine-trash staging: {error}")
                })?;
            }
        }
    }
    Ok(warnings)
}

pub(crate) fn recover_engine_trash_staging() -> Result<Vec<String>, String> {
    recover_staging_in(&claudeix_trash_dir()?)
}

fn all_entry_dirs_in(root: &Path) -> Result<Vec<PathBuf>, String> {
    if !root.is_dir() {
        return Ok(Vec::new());
    }
    let mut dirs = Vec::new();
    for engine in fs::read_dir(root).map_err(|error| error.to_string())? {
        let engine = engine.map_err(|error| error.to_string())?.path();
        if !engine.is_dir() {
            continue;
        }
        for entry in fs::read_dir(engine).map_err(|error| error.to_string())? {
            let entry = entry.map_err(|error| error.to_string())?.path();
            if entry.is_dir()
                && !entry
                    .file_name()
                    .and_then(|name| name.to_str())
                    .is_some_and(|name| name.starts_with(".staging-"))
            {
                dirs.push(entry);
            }
        }
    }
    Ok(dirs)
}

fn all_entry_dirs() -> Result<Vec<PathBuf>, String> {
    let root = claudeix_trash_dir()?;
    let _ = recover_staging_in(&root)?;
    all_entry_dirs_in(&root)
}

fn find_entry_in(root: &Path, trash_id: &str) -> Result<PathBuf, String> {
    all_entry_dirs_in(root)?
        .into_iter()
        .find(|dir| dir.file_name().and_then(|name| name.to_str()) == Some(trash_id))
        .ok_or_else(|| format!("recycle-bin entry not found: {trash_id}"))
}

fn find_entry(trash_id: &str) -> Result<PathBuf, String> {
    find_entry_in(&claudeix_trash_dir()?, trash_id)
}

fn restore_preview_for(dir: &Path, manifest: &EngineTrashManifest) -> Result<EngineTrashRestorePreview, String> {
    let mut files = Vec::new();
    let mut corrupt = false;
    let mut conflicts = false;
    let mut hasher = Sha256::new();
    for record in &manifest.files {
        let payload = dir.join(&record.payload_relative_path);
        let original = PathBuf::from(&record.original_path);
        let payload_valid = payload.is_file()
            && sha256_file(&payload).map(|hash| hash == record.sha256).unwrap_or(false);
        let current_sha256 = if original.is_file() {
            Some(sha256_file(&original)?)
        } else {
            None
        };
        let state = if let Some(current) = current_sha256.as_deref() {
            if current == record.sha256 {
                if payload_valid { "same" } else { "already_restored" }
            } else if !payload_valid {
                corrupt = true;
                if payload.exists() { "corrupt_payload" } else { "missing_payload" }
            } else {
                conflicts = true;
                "different"
            }
        } else if !payload_valid {
            corrupt = true;
            if payload.exists() { "corrupt_payload" } else { "missing_payload" }
        } else {
            "free"
        };
        hasher.update(record.original_path.as_bytes());
        hasher.update(state.as_bytes());
        hasher.update(record.sha256.as_bytes());
        if let Some(current) = current_sha256.as_deref() {
            hasher.update(current.as_bytes());
        }
        files.push(EngineTrashRestoreFileState {
            original_path: record.original_path.clone(),
            payload_path: display_path(&payload),
            purpose: record.purpose.clone(),
            size: record.size,
            sha256: record.sha256.clone(),
            state: state.to_string(),
            current_sha256,
        });
    }
    Ok(EngineTrashRestorePreview {
        trash_id: manifest.trash_id.clone(),
        engine_id: manifest.engine_id.clone(),
        native_session_id: manifest.native_session_id.clone(),
        title: manifest.title.clone(),
        can_restore_safely: !corrupt && !conflicts,
        has_conflicts: conflicts,
        corrupt,
        preview_fingerprint: format!("{:x}", hasher.finalize()),
        files,
    })
}

fn create_exchange_entry_in(
    trash_root: &Path,
    manifest: &EngineTrashManifest,
    preview: &EngineTrashRestorePreview,
) -> Result<Option<String>, String> {
    let conflicting: Vec<_> = preview
        .files
        .iter()
        .filter(|file| file.state == "different")
        .collect();
    if conflicting.is_empty() {
        return Ok(None);
    }
    let files = conflicting
        .into_iter()
        .map(|file| EngineTrashFilePreview {
            original_path: file.original_path.clone(),
            purpose: "restore_conflict_current_version".to_string(),
            size: fs::metadata(&file.original_path).map(|metadata| metadata.len()).unwrap_or(0),
            sha256: file.current_sha256.clone().unwrap_or_default(),
        })
        .collect::<Vec<_>>();
    let exchange = EngineTrashDeletePreview {
        engine_id: manifest.engine_id.clone(),
        session_key: manifest.session_key.clone(),
        native_session_id: manifest.native_session_id.clone(),
        title: format!("{} · 恢复前当前版本", manifest.title),
        workspace_root: manifest.workspace_root.clone(),
        total_files: files.len(),
        total_size: files.iter().map(|file| file.size).sum(),
        preview_fingerprint: preview_fingerprint(&files),
        runtime_active: false,
        external_runtime_active: false,
        external_runtime_pid: None,
        files,
    };
    Ok(Some(move_preview_to_trash_in(trash_root, &exchange)?.trash_id))
}

fn restore_entry_in(
    trash_root: &Path,
    request: &EngineTrashRestoreRequest,
) -> Result<EngineTrashRestoreResult, String> {
    let dir = find_entry_in(trash_root, &request.trash_id)?;
    let mut manifest = read_manifest(&dir)?;
    let preview = restore_preview_for(&dir, &manifest)?;
    if preview.preview_fingerprint != request.expected_preview_fingerprint {
        return Err("restore destinations changed after preview; review them again".to_string());
    }
    if preview.corrupt {
        return Err("recycle-bin payload is incomplete or corrupt".to_string());
    }
    if preview.has_conflicts && request.strategy != "exchange" {
        return Err("restore destination contains different data; use exchange restore or cancel".to_string());
    }
    if !matches!(request.strategy.as_str(), "safe" | "exchange") {
        return Err("unknown restore strategy".to_string());
    }

    let exchange_id = if request.strategy == "exchange" {
        create_exchange_entry_in(trash_root, &manifest, &preview)?
    } else {
        None
    };

    for record in &manifest.files {
        let payload = dir.join(&record.payload_relative_path);
        let original = PathBuf::from(&record.original_path);
        if original.is_file() {
            if sha256_file(&original)? == record.sha256 {
                continue;
            }
            return Err(format!("restore destination still occupied: {}", original.display()));
        }
        if let Some(parent) = original.parent() {
            fs::create_dir_all(parent).map_err(|error| error.to_string())?;
        }
        fs::rename(&payload, &original)
            .map_err(|error| format!("cannot restore {}: {error}", original.display()))?;
        if sha256_file(&original)? != record.sha256 {
            return Err("restored native session hash mismatch".to_string());
        }
    }
    manifest.status = "restored".to_string();
    manifest.restored_at = Some(now_millis());
    write_manifest(&dir, &manifest)?;
    Ok(EngineTrashRestoreResult {
        trash_id: manifest.trash_id,
        status: manifest.status,
        exchange_trash_id: exchange_id,
    })
}

fn restore_entry(request: &EngineTrashRestoreRequest) -> Result<EngineTrashRestoreResult, String> {
    restore_entry_in(&claudeix_trash_dir()?, request)
}

fn collect_files(dir: &Path, out: &mut Vec<PathBuf>) -> Result<(), String> {
    for entry in fs::read_dir(dir).map_err(|error| error.to_string())? {
        let path = entry.map_err(|error| error.to_string())?.path();
        if path.is_dir() {
            collect_files(&path, out)?;
        } else if path.is_file() {
            out.push(path);
        }
    }
    Ok(())
}

fn purge_preview_for(dir: &Path, manifest: &EngineTrashManifest) -> Result<EngineTrashPurgePreview, String> {
    let mut paths = Vec::new();
    collect_files(dir, &mut paths)?;
    paths.sort();
    let mut hasher = Sha256::new();
    let mut files = Vec::new();
    for path in paths {
        let metadata = fs::metadata(&path).map_err(|error| error.to_string())?;
        let sha = sha256_file(&path)?;
        let display = display_path(&path);
        hasher.update(display.as_bytes());
        hasher.update(metadata.len().to_le_bytes());
        hasher.update(sha.as_bytes());
        files.push(EngineTrashPurgeFile {
            path: display,
            size: metadata.len(),
            purpose: if path.file_name().and_then(|name| name.to_str()) == Some("manifest.json") {
                "trash_manifest".to_string()
            } else {
                "trash_payload".to_string()
            },
        });
    }
    let short = manifest.native_session_id.chars().take(8).collect::<String>();
    Ok(EngineTrashPurgePreview {
        trash_id: manifest.trash_id.clone(),
        engine_id: manifest.engine_id.clone(),
        native_session_id: manifest.native_session_id.clone(),
        title: manifest.title.clone(),
        total_files: files.len(),
        total_size: files.iter().map(|file| file.size).sum(),
        confirmation_text: format!("永久删除 {} {short}", manifest.engine_id),
        preview_fingerprint: format!("{:x}", hasher.finalize()),
        files,
    })
}

#[tauri::command]
pub fn engine_trash_delete_preview(
    engine_id: String,
    session_key: String,
    workspace_root: String,
    registry: State<'_, EngineRegistry>,
    coordinator: State<'_, MutationCoordinator>,
    pi_runtime: State<'_, PiRuntimeState>,
) -> Result<EngineTrashDeletePreview, String> {
    if coordinator.is_locked(&engine_id, &session_key) {
        return Err("finish the native session edit/restore before recycling it".to_string());
    }
    if has_pending_recovery(&engine_id, &session_key)? {
        return Err("this native session has an unfinished mutation recovery; resolve it before deleting".to_string());
    }
    build_delete_preview(
        &engine_id,
        &session_key,
        Path::new(&workspace_root),
        &registry,
        &pi_runtime,
    )
}

#[tauri::command]
pub fn engine_trash_move(
    request: EngineTrashMoveRequest,
    service: State<'_, EngineTrashService>,
    registry: State<'_, EngineRegistry>,
    coordinator: State<'_, MutationCoordinator>,
    pi_runtime: State<'_, PiRuntimeState>,
) -> Result<EngineTrashItemSummary, String> {
    let _lease = service.acquire_session(&request.engine_id, &request.session_key)?;
    if coordinator.is_locked(&request.engine_id, &request.session_key) {
        return Err("finish the native session edit/restore before recycling it".to_string());
    }
    if has_pending_recovery(&request.engine_id, &request.session_key)? {
        return Err("this native session has an unfinished mutation recovery; resolve it before deleting".to_string());
    }
    let preview = build_delete_preview(
        &request.engine_id,
        &request.session_key,
        Path::new(&request.workspace_root),
        &registry,
        &pi_runtime,
    )?;
    if preview.preview_fingerprint != request.expected_preview_fingerprint {
        return Err("native session changed after delete preview; review the exact files again".to_string());
    }
    if preview.runtime_active {
        return Err("stop the managed Agent runtime before recycling this session".to_string());
    }
    if preview.external_runtime_active {
        return Err(format!(
            "external Agent process {:?} may still be writing this session/workspace; stop it first",
            preview.external_runtime_pid
        ));
    }
    move_preview_to_trash(&preview)
}

#[tauri::command]
pub fn engine_trash_list(engine_id: Option<String>) -> Result<Vec<EngineTrashItemSummary>, String> {
    let mut items = Vec::new();
    for dir in all_entry_dirs()? {
        let manifest = match read_manifest(&dir) {
            Ok(manifest) => manifest,
            Err(_) => continue,
        };
        if engine_id.as_deref().is_some_and(|engine| engine != manifest.engine_id) {
            continue;
        }
        items.push(summary_from_manifest(&manifest));
    }
    items.sort_by(|left, right| right.deleted_at.cmp(&left.deleted_at));
    Ok(items)
}

#[tauri::command]
pub fn engine_trash_restore_preview(trash_id: String) -> Result<EngineTrashRestorePreview, String> {
    let dir = find_entry(&trash_id)?;
    let manifest = read_manifest(&dir)?;
    restore_preview_for(&dir, &manifest)
}

#[tauri::command]
pub fn engine_trash_restore(
    request: EngineTrashRestoreRequest,
    service: State<'_, EngineTrashService>,
    registry: State<'_, EngineRegistry>,
    pi_runtime: State<'_, PiRuntimeState>,
) -> Result<EngineTrashRestoreResult, String> {
    let _lease = service.acquire_entry(&request.trash_id)?;
    let dir = find_entry(&request.trash_id)?;
    let manifest = read_manifest(&dir)?;
    if runtime_active(
        &manifest.engine_id,
        &manifest.session_key,
        &manifest.workspace_root,
        &pi_runtime,
    ) {
        return Err("stop the managed Agent runtime before restoring a native session".to_string());
    }
    let managed_pid = if manifest.engine_id == "pi" {
        pi_runtime.pid()
    } else {
        None
    };
    let external_pid = registry
        .external_writer_pid(
            &manifest.engine_id,
            &manifest.session_key,
            Path::new(&manifest.workspace_root),
        )?
        .filter(|pid| Some(*pid) != managed_pid);
    if let Some(pid) = external_pid {
        return Err(format!(
            "external Agent process {pid} may still be writing this workspace; stop it before restoring"
        ));
    }
    restore_entry(&request)
}

#[tauri::command]
pub fn engine_trash_purge_preview(trash_id: String) -> Result<EngineTrashPurgePreview, String> {
    let dir = find_entry(&trash_id)?;
    let manifest = read_manifest(&dir)?;
    purge_preview_for(&dir, &manifest)
}

#[tauri::command]
pub fn engine_trash_purge(
    request: EngineTrashPurgeRequest,
    service: State<'_, EngineTrashService>,
) -> Result<(), String> {
    let _lease = service.acquire_entry(&request.trash_id)?;
    let dir = find_entry(&request.trash_id)?;
    let manifest = read_manifest(&dir)?;
    let preview = purge_preview_for(&dir, &manifest)?;
    if preview.preview_fingerprint != request.expected_preview_fingerprint {
        return Err("recycle-bin contents changed after purge preview; review them again".to_string());
    }
    if preview.confirmation_text != request.confirmation_text {
        return Err(format!("confirmation text must exactly match: {}", preview.confirmation_text));
    }
    fs::remove_dir_all(&dir)
        .map_err(|error| format!("cannot permanently delete {}: {error}", dir.display()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn preview_fingerprint_changes_with_payload_hash() {
        let first = vec![EngineTrashFilePreview {
            original_path: "C:/x/session.jsonl".into(),
            purpose: "native_session".into(),
            size: 4,
            sha256: "aaaa".into(),
        }];
        let mut second = first.clone();
        second[0].sha256 = "bbbb".into();
        assert_ne!(preview_fingerprint(&first), preview_fingerprint(&second));
    }

    #[test]
    fn incomplete_staging_move_rolls_back_source() {
        let root = std::env::temp_dir().join(unique_id("engine-trash-recovery"));
        let engine_dir = root.join("pi");
        let trash_id = "pi-session-recovery";
        let staging = engine_dir.join(format!(".staging-{trash_id}"));
        let payload = staging.join("payload/0-session.jsonl");
        let original = root.join("sessions/session.jsonl");
        let bytes = b"{\"type\":\"session\",\"id\":\"s1\"}\n";
        fs::create_dir_all(payload.parent().unwrap()).unwrap();
        fs::write(&payload, bytes).unwrap();
        let hash = sha256_file(&payload).unwrap();
        let manifest = EngineTrashManifest {
            version: 1,
            trash_id: trash_id.into(),
            engine_id: "pi".into(),
            session_key: display_path(&original),
            native_session_id: "s1".into(),
            title: "Recovery".into(),
            workspace_root: display_path(&root),
            deleted_at: now_millis(),
            restored_at: None,
            status: "moving".into(),
            files: vec![EngineTrashFileRecord {
                original_path: display_path(&original),
                payload_relative_path: "payload/0-session.jsonl".into(),
                purpose: "native_session".into(),
                size: bytes.len() as u64,
                sha256: hash,
            }],
        };
        write_manifest(&staging, &manifest).unwrap();

        let warnings = recover_staging_in(&root).unwrap();
        assert!(warnings.is_empty());
        assert_eq!(fs::read(&original).unwrap(), bytes);
        assert!(!staging.exists());
        fs::remove_dir_all(root).ok();
    }

    #[test]
    fn native_session_move_and_restore_round_trip() {
        let root = std::env::temp_dir().join(unique_id("engine-trash-roundtrip"));
        let trash_root = root.join("trash");
        let workspace = root.join("workspace");
        let source = root.join("pi-sessions/project/session.jsonl");
        fs::create_dir_all(source.parent().unwrap()).unwrap();
        fs::create_dir_all(&workspace).unwrap();
        let bytes = b"{\"type\":\"session\",\"id\":\"pi-trash-s1\"}\n{\"type\":\"message\",\"id\":\"u1\",\"parentId\":null,\"message\":{\"role\":\"user\",\"content\":\"hello\"}}\n";
        fs::write(&source, bytes).unwrap();
        let file = EngineTrashFilePreview {
            original_path: display_path(&source),
            purpose: "native_session".into(),
            size: bytes.len() as u64,
            sha256: sha256_file(&source).unwrap(),
        };
        let preview = EngineTrashDeletePreview {
            engine_id: "pi".into(),
            session_key: display_path(&source),
            native_session_id: "pi-trash-s1".into(),
            title: "Pi Trash Test".into(),
            workspace_root: display_path(&workspace),
            total_files: 1,
            total_size: bytes.len() as u64,
            preview_fingerprint: preview_fingerprint(std::slice::from_ref(&file)),
            runtime_active: false,
            external_runtime_active: false,
            external_runtime_pid: None,
            files: vec![file],
        };

        let item = move_preview_to_trash_in(&trash_root, &preview).unwrap();
        assert!(!source.exists());
        let entry = find_entry_in(&trash_root, &item.trash_id).unwrap();
        let manifest = read_manifest(&entry).unwrap();
        let payload = entry.join(&manifest.files[0].payload_relative_path);
        assert_eq!(fs::read(&payload).unwrap(), bytes);

        let restore_preview = restore_preview_for(&entry, &manifest).unwrap();
        assert!(restore_preview.can_restore_safely);
        let result = restore_entry_in(
            &trash_root,
            &EngineTrashRestoreRequest {
                trash_id: item.trash_id,
                strategy: "safe".into(),
                expected_preview_fingerprint: restore_preview.preview_fingerprint,
            },
        )
        .unwrap();
        assert_eq!(result.status, "restored");
        assert_eq!(fs::read(&source).unwrap(), bytes);
        fs::remove_dir_all(root).ok();
    }

    #[test]
    fn exchange_restore_preserves_conflicting_current_version() {
        let root = std::env::temp_dir().join(unique_id("engine-trash-exchange"));
        let trash_root = root.join("trash");
        let workspace = root.join("workspace");
        let source = root.join("pi-sessions/project/session.jsonl");
        fs::create_dir_all(source.parent().unwrap()).unwrap();
        fs::create_dir_all(&workspace).unwrap();
        let old_bytes = b"old-native-session\n";
        let current_bytes = b"new-conflicting-session\n";
        fs::write(&source, old_bytes).unwrap();
        let file = EngineTrashFilePreview {
            original_path: display_path(&source),
            purpose: "native_session".into(),
            size: old_bytes.len() as u64,
            sha256: sha256_file(&source).unwrap(),
        };
        let item = move_preview_to_trash_in(
            &trash_root,
            &EngineTrashDeletePreview {
                engine_id: "pi".into(),
                session_key: display_path(&source),
                native_session_id: "pi-exchange-s1".into(),
                title: "Pi Exchange Test".into(),
                workspace_root: display_path(&workspace),
                total_files: 1,
                total_size: old_bytes.len() as u64,
                preview_fingerprint: preview_fingerprint(std::slice::from_ref(&file)),
                runtime_active: false,
                external_runtime_active: false,
                external_runtime_pid: None,
                files: vec![file],
            },
        )
        .unwrap();

        fs::write(&source, current_bytes).unwrap();
        let entry = find_entry_in(&trash_root, &item.trash_id).unwrap();
        let manifest = read_manifest(&entry).unwrap();
        let preview = restore_preview_for(&entry, &manifest).unwrap();
        assert!(preview.has_conflicts);

        let result = restore_entry_in(
            &trash_root,
            &EngineTrashRestoreRequest {
                trash_id: item.trash_id,
                strategy: "exchange".into(),
                expected_preview_fingerprint: preview.preview_fingerprint,
            },
        )
        .unwrap();
        assert_eq!(fs::read(&source).unwrap(), old_bytes);
        let exchange_id = result.exchange_trash_id.expect("exchange trash id");
        let exchange_dir = find_entry_in(&trash_root, &exchange_id).unwrap();
        let exchange_manifest = read_manifest(&exchange_dir).unwrap();
        let payload = exchange_dir.join(&exchange_manifest.files[0].payload_relative_path);
        assert_eq!(fs::read(payload).unwrap(), current_bytes);
        fs::remove_dir_all(root).ok();
    }
}
