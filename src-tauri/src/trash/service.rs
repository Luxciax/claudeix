use crate::app_paths::{
    canonical_or_original, claude_config_dir, claude_trash_dir, display_path, normalize_path_key,
};
use crate::claude::runtime_registry::RuntimeRegistry;
use crate::claude::scanner::SessionCatalog;
use crate::models::session::SessionIdentity;
use crate::models::trash::{
    TrashDeletePreview, TrashFilePreview, TrashFileRecord, TrashItemSummary, TrashManifest,
    TrashMoveRequest, TrashPurgeFile, TrashPurgePreview, TrashPurgeRequest, TrashRestoreFileState,
    TrashRestorePreview, TrashRestoreRequest, TrashRestoreResult,
};
use crate::transcript::editor::{session_has_pending_recovery, TranscriptEditor};
use sha2::{Digest, Sha256};
use std::collections::HashSet;
use std::fs::{self, File, OpenOptions};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};
use tauri::State;

#[derive(Default)]
pub struct TrashService {
    active_sessions: Mutex<HashSet<String>>,
    active_entries: Mutex<HashSet<String>>,
}

struct SessionLease<'a> {
    service: &'a TrashService,
    key: String,
}

impl Drop for SessionLease<'_> {
    fn drop(&mut self) {
        if let Ok(mut active) = self.service.active_sessions.lock() {
            active.remove(&self.key);
        }
    }
}

struct EntryLease<'a> {
    service: &'a TrashService,
    key: String,
}

impl Drop for EntryLease<'_> {
    fn drop(&mut self) {
        if let Ok(mut active) = self.service.active_entries.lock() {
            active.remove(&self.key);
        }
    }
}

impl TrashService {
    fn acquire_session(&self, session_key: &str) -> Result<SessionLease<'_>, String> {
        let mut active = self
            .active_sessions
            .lock()
            .map_err(|error| error.to_string())?;
        if !active.insert(session_key.to_string()) {
            return Err("this Claude session is already being moved to the recycle bin".to_string());
        }
        Ok(SessionLease {
            service: self,
            key: session_key.to_string(),
        })
    }

    fn acquire_entry(&self, trash_id: &str) -> Result<EntryLease<'_>, String> {
        let mut active = self
            .active_entries
            .lock()
            .map_err(|error| error.to_string())?;
        if !active.insert(trash_id.to_string()) {
            return Err("this recycle-bin entry is already being restored or purged".to_string());
        }
        Ok(EntryLease {
            service: self,
            key: trash_id.to_string(),
        })
    }

    pub fn is_locked(&self, session_key: &str) -> bool {
        self.active_sessions
            .lock()
            .map(|active| active.contains(session_key))
            .unwrap_or(true)
    }
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

fn write_file_synced(path: &Path, bytes: &[u8]) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .map_err(|error| format!("cannot create {}: {error}", parent.display()))?;
    }
    let mut file = OpenOptions::new()
        .create(true)
        .write(true)
        .truncate(true)
        .open(path)
        .map_err(|error| format!("cannot create {}: {error}", path.display()))?;
    file.write_all(bytes)
        .and_then(|_| file.flush())
        .and_then(|_| file.sync_all())
        .map_err(|error| format!("cannot persist {}: {error}", path.display()))
}

fn manifest_path(entry_dir: &Path) -> PathBuf {
    entry_dir.join("manifest.json")
}

fn write_manifest(entry_dir: &Path, manifest: &TrashManifest) -> Result<(), String> {
    let bytes = serde_json::to_vec_pretty(manifest).map_err(|error| error.to_string())?;
    write_file_synced(&manifest_path(entry_dir), &bytes)
}

fn read_manifest(entry_dir: &Path) -> Result<TrashManifest, String> {
    let path = manifest_path(entry_dir);
    let bytes = fs::read(&path)
        .map_err(|error| format!("cannot read recycle-bin manifest {}: {error}", path.display()))?;
    serde_json::from_slice(&bytes)
        .map_err(|error| format!("invalid recycle-bin manifest {}: {error}", path.display()))
}

fn collect_files_recursive(dir: &Path, out: &mut Vec<PathBuf>) -> Result<(), String> {
    let entries = fs::read_dir(dir)
        .map_err(|error| format!("cannot list {}: {error}", dir.display()))?;
    for entry in entries {
        let entry = entry.map_err(|error| error.to_string())?;
        let path = entry.path();
        if path.is_dir() {
            collect_files_recursive(&path, out)?;
        } else if path.is_file() {
            out.push(path);
        }
    }
    Ok(())
}

fn file_purpose(path: &Path, main_source: &Path) -> String {
    if normalize_path_key(path) == normalize_path_key(main_source) {
        return "main_transcript".to_string();
    }
    let is_agent = path
        .file_name()
        .and_then(|value| value.to_str())
        .map(|value| value.starts_with("agent-") && value.ends_with(".jsonl"))
        .unwrap_or(false)
        && path
            .components()
            .any(|component| component.as_os_str().to_string_lossy().eq_ignore_ascii_case("subagents"));
    if is_agent {
        "subagent_transcript".to_string()
    } else {
        "session_attachment".to_string()
    }
}

fn session_files(identity: &SessionIdentity) -> Result<Vec<PathBuf>, String> {
    if !identity.source_path.is_file() {
        return Err(format!(
            "Claude session source no longer exists: {}",
            identity.source_path.display()
        ));
    }
    let mut files = vec![canonical_or_original(&identity.source_path)];
    let parent = identity
        .source_path
        .parent()
        .ok_or_else(|| "Claude session source has no parent directory".to_string())?;
    let companion = parent.join(&identity.session_id);
    if companion.is_dir() {
        collect_files_recursive(&companion, &mut files)?;
    }
    files.sort_by_key(|path| normalize_path_key(path));
    files.dedup_by(|left, right| normalize_path_key(left) == normalize_path_key(right));

    let projects = canonical_or_original(&identity.config_dir.join("projects"));
    let projects_key = normalize_path_key(&projects);
    for path in &files {
        let key = normalize_path_key(path);
        if key != projects_key && !key.starts_with(&(projects_key.clone() + "/")) {
            return Err(format!(
                "refusing to recycle a file outside the Claude projects directory: {}",
                path.display()
            ));
        }
    }
    Ok(files)
}

fn file_preview(path: &Path, main_source: &Path) -> Result<TrashFilePreview, String> {
    let metadata = fs::metadata(path)
        .map_err(|error| format!("cannot inspect {}: {error}", path.display()))?;
    Ok(TrashFilePreview {
        original_path: display_path(path),
        purpose: file_purpose(path, main_source),
        size: metadata.len(),
        sha256: sha256_file(path)?,
    })
}

fn fingerprint_previews(files: &[TrashFilePreview]) -> String {
    let mut hasher = Sha256::new();
    for file in files {
        hasher.update(file.original_path.as_bytes());
        hasher.update([0]);
        hasher.update(file.purpose.as_bytes());
        hasher.update([0]);
        hasher.update(file.size.to_le_bytes());
        hasher.update(file.sha256.as_bytes());
        hasher.update([0]);
    }
    format!("{:x}", hasher.finalize())
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

fn build_delete_preview(
    identity: &SessionIdentity,
    session_key: &str,
    title: &str,
    runtime: &RuntimeRegistry,
) -> Result<TrashDeletePreview, String> {
    let paths = session_files(identity)?;
    let mut files = Vec::with_capacity(paths.len());
    for path in paths {
        files.push(file_preview(&path, &identity.source_path)?);
    }
    let total_size = files.iter().map(|file| file.size).sum();
    let external = runtime.probe_external_identity(identity);
    Ok(TrashDeletePreview {
        session_key: session_key.to_string(),
        session_id: identity.session_id.clone(),
        title: title.to_string(),
        workspace_root: display_path(&identity.workspace_root),
        config_dir: display_path(&identity.config_dir),
        total_files: files.len(),
        total_size,
        preview_fingerprint: fingerprint_previews(&files),
        runtime_active: runtime.session_active(session_key),
        external_runtime_active: external.active,
        external_runtime_pid: external.pid,
        files,
    })
}

fn relative_to_config(path: &Path, config_dir: &Path) -> Result<PathBuf, String> {
    let path = canonical_or_original(path);
    let config = canonical_or_original(config_dir);
    path.strip_prefix(&config)
        .map(Path::to_path_buf)
        .map_err(|_| format!("{} is outside {}", path.display(), config.display()))
}

fn records_from_preview(
    preview: &TrashDeletePreview,
    config_dir: &Path,
) -> Result<Vec<TrashFileRecord>, String> {
    preview
        .files
        .iter()
        .map(|file| {
            let original = PathBuf::from(&file.original_path);
            let relative = relative_to_config(&original, config_dir)?;
            Ok(TrashFileRecord {
                original_path: file.original_path.clone(),
                payload_relative_path: display_path(&Path::new("payload").join(relative)),
                purpose: file.purpose.clone(),
                size: file.size,
                sha256: file.sha256.clone(),
            })
        })
        .collect()
}

fn rollback_moved(moved: &[(PathBuf, PathBuf)]) -> Result<(), String> {
    let mut errors = Vec::new();
    for (payload, original) in moved.iter().rev() {
        if !payload.exists() {
            continue;
        }
        if original.exists() {
            errors.push(format!(
                "cannot roll back {} because the original path now exists",
                original.display()
            ));
            continue;
        }
        if let Some(parent) = original.parent() {
            if let Err(error) = fs::create_dir_all(parent) {
                errors.push(format!("cannot recreate {}: {error}", parent.display()));
                continue;
            }
        }
        if let Err(error) = fs::rename(payload, original) {
            errors.push(format!(
                "cannot roll back {} to {}: {error}",
                payload.display(),
                original.display()
            ));
        }
    }
    if errors.is_empty() {
        Ok(())
    } else {
        Err(errors.join("; "))
    }
}

fn recover_incomplete_staging(config_dir: &Path) -> Result<Vec<String>, String> {
    let root = claude_trash_dir(config_dir);
    if !root.is_dir() {
        return Ok(Vec::new());
    }
    let mut warnings = Vec::new();
    for entry in fs::read_dir(&root).map_err(|error| error.to_string())? {
        let entry = entry.map_err(|error| error.to_string())?;
        let dir = entry.path();
        let name = entry.file_name().to_string_lossy().to_string();
        if !dir.is_dir() || !name.starts_with(".staging-") {
            continue;
        }
        let Ok(mut manifest) = read_manifest(&dir) else {
            warnings.push(format!("cannot inspect incomplete recycle transaction {name}"));
            continue;
        };
        if manifest.status != "moving" && manifest.status != "restoring" {
            continue;
        }
        let mut conflict = false;
        for file in &manifest.files {
            let payload = dir.join(PathBuf::from(&file.payload_relative_path));
            let original = PathBuf::from(&file.original_path);
            if !payload.is_file() {
                continue;
            }
            if original.exists() {
                let same = sha256_file(&original)
                    .map(|hash| hash == file.sha256)
                    .unwrap_or(false);
                if !same {
                    conflict = true;
                }
                continue;
            }
            if let Some(parent) = original.parent() {
                fs::create_dir_all(parent).map_err(|error| {
                    format!("cannot recover staging parent {}: {error}", parent.display())
                })?;
            }
            fs::rename(&payload, &original).map_err(|error| {
                format!(
                    "cannot recover incomplete recycle transaction {} -> {}: {error}",
                    payload.display(),
                    original.display()
                )
            })?;
        }
        manifest.status = if conflict {
            warnings.push(format!(
                "incomplete recycle transaction {name} has a recovery conflict"
            ));
            "recovery_conflict".to_string()
        } else {
            "rolled_back".to_string()
        };
        write_manifest(&dir, &manifest)?;
        if conflict {
            let visible = root.join(&manifest.trash_id);
            if !visible.exists() {
                fs::rename(&dir, &visible).map_err(|error| {
                    format!(
                        "cannot expose recovery conflict {} as {}: {error}",
                        dir.display(),
                        visible.display()
                    )
                })?;
            }
        }
    }
    Ok(warnings)
}

fn manifest_to_summary(manifest: &TrashManifest) -> TrashItemSummary {
    TrashItemSummary {
        trash_id: manifest.trash_id.clone(),
        session_id: manifest.session_id.clone(),
        title: manifest.title.clone(),
        workspace_root: manifest.workspace_root.clone(),
        config_dir: manifest.config_dir.clone(),
        deleted_at: manifest.deleted_at,
        restored_at: manifest.restored_at,
        status: manifest.status.clone(),
        total_files: manifest.files.len(),
        total_size: manifest.files.iter().map(|file| file.size).sum(),
    }
}

fn load_final_manifest(config_dir: &Path, trash_id: &str) -> Result<(PathBuf, TrashManifest), String> {
    if trash_id.starts_with('.') || trash_id.contains('/') || trash_id.contains('\\') {
        return Err("invalid recycle-bin id".to_string());
    }
    let entry_dir = claude_trash_dir(config_dir).join(trash_id);
    if !entry_dir.is_dir() {
        return Err("recycle-bin entry not found".to_string());
    }
    let manifest = read_manifest(&entry_dir)?;
    if manifest.trash_id != trash_id {
        return Err("recycle-bin manifest id mismatch".to_string());
    }
    Ok((entry_dir, manifest))
}

fn move_session_to_trash(
    identity: &SessionIdentity,
    session_key: &str,
    title: &str,
    expected_preview_fingerprint: &str,
    runtime: &RuntimeRegistry,
) -> Result<TrashItemSummary, String> {
    let preview = build_delete_preview(identity, session_key, title, runtime)?;
    if preview.runtime_active {
        return Err("stop the Claudeix runtime before moving this session to the recycle bin".to_string());
    }
    if preview.external_runtime_active {
        return Err(format!(
            "external Claude Code process {} may be writing this session",
            preview
                .external_runtime_pid
                .map(|pid| pid.to_string())
                .unwrap_or_else(|| "unknown".to_string())
        ));
    }
    if preview.preview_fingerprint != expected_preview_fingerprint {
        return Err("session files changed after the delete preview; review the file list again".to_string());
    }

    let root = claude_trash_dir(&identity.config_dir);
    fs::create_dir_all(&root)
        .map_err(|error| format!("cannot create recycle bin {}: {error}", root.display()))?;
    let _ = recover_incomplete_staging(&identity.config_dir)?;

    let trash_id = unique_id("trash");
    let staging = root.join(format!(".staging-{trash_id}"));
    let final_dir = root.join(&trash_id);
    fs::create_dir_all(staging.join("payload"))
        .map_err(|error| format!("cannot create recycle staging directory: {error}"))?;
    let records = records_from_preview(&preview, &identity.config_dir)?;
    let mut manifest = TrashManifest {
        version: 1,
        trash_id: trash_id.clone(),
        session_id: identity.session_id.clone(),
        title: title.to_string(),
        workspace_root: display_path(&identity.workspace_root),
        config_dir: display_path(&identity.config_dir),
        source_path: display_path(&identity.source_path),
        deleted_at: now_millis(),
        restored_at: None,
        status: "moving".to_string(),
        files: records,
    };
    write_manifest(&staging, &manifest)?;

    let mut moved = Vec::new();
    for record in &manifest.files {
        let original = PathBuf::from(&record.original_path);
        if !original.is_file() {
            let rollback = rollback_moved(&moved);
            return Err(format!(
                "session file disappeared during recycle: {}{}",
                original.display(),
                rollback
                    .err()
                    .map(|error| format!("; rollback warning: {error}"))
                    .unwrap_or_default()
            ));
        }
        let current_hash = sha256_file(&original)?;
        let current_size = fs::metadata(&original).map_err(|error| error.to_string())?.len();
        if current_hash != record.sha256 || current_size != record.size {
            let _ = rollback_moved(&moved);
            return Err("session files changed while the recycle transaction was running".to_string());
        }
        let payload = staging.join(PathBuf::from(&record.payload_relative_path));
        if let Some(parent) = payload.parent() {
            fs::create_dir_all(parent)
                .map_err(|error| format!("cannot create {}: {error}", parent.display()))?;
        }
        if let Err(error) = fs::rename(&original, &payload) {
            let rollback = rollback_moved(&moved);
            return Err(format!(
                "cannot move {} to the recycle bin: {error}{}",
                original.display(),
                rollback
                    .err()
                    .map(|warning| format!("; rollback warning: {warning}"))
                    .unwrap_or_default()
            ));
        }
        moved.push((payload.clone(), original.clone()));
        if sha256_file(&payload)? != record.sha256 {
            let _ = rollback_moved(&moved);
            return Err("recycle payload hash mismatch; moved files were rolled back".to_string());
        }
    }

    manifest.status = "trashed".to_string();
    write_manifest(&staging, &manifest)?;
    if let Err(error) = fs::rename(&staging, &final_dir) {
        let rollback = rollback_moved(&moved);
        return Err(format!(
            "cannot commit recycle-bin transaction: {error}{}",
            rollback
                .err()
                .map(|warning| format!("; rollback warning: {warning}"))
                .unwrap_or_default()
        ));
    }
    Ok(manifest_to_summary(&manifest))
}

fn list_items_in(config_dir: &Path) -> Result<Vec<TrashItemSummary>, String> {
    let _ = recover_incomplete_staging(config_dir)?;
    let root = claude_trash_dir(config_dir);
    if !root.is_dir() {
        return Ok(Vec::new());
    }
    let mut items = Vec::new();
    for entry in fs::read_dir(&root).map_err(|error| error.to_string())? {
        let entry = entry.map_err(|error| error.to_string())?;
        let dir = entry.path();
        let name = entry.file_name().to_string_lossy().to_string();
        if !dir.is_dir() || name.starts_with('.') {
            continue;
        }
        if let Ok(manifest) = read_manifest(&dir) {
            items.push(manifest_to_summary(&manifest));
        }
    }
    items.sort_by(|left, right| right.deleted_at.cmp(&left.deleted_at));
    Ok(items)
}

fn restore_preview_in(config_dir: &Path, trash_id: &str) -> Result<TrashRestorePreview, String> {
    let (entry_dir, manifest) = load_final_manifest(config_dir, trash_id)?;
    let mut files = Vec::new();
    let mut corrupt = false;
    let mut conflicts = false;
    for record in &manifest.files {
        let original = PathBuf::from(&record.original_path);
        let payload = entry_dir.join(PathBuf::from(&record.payload_relative_path));
        let payload_hash = if payload.is_file() {
            Some(sha256_file(&payload)?)
        } else {
            None
        };
        let current_hash = if original.is_file() {
            Some(sha256_file(&original)?)
        } else {
            None
        };
        let state = match (&payload_hash, &current_hash) {
            (Some(payload_hash), _) if payload_hash != &record.sha256 => {
                corrupt = true;
                "corrupt_payload"
            }
            (Some(_), None) => "free",
            (Some(_), Some(current)) if current == &record.sha256 => "same",
            (Some(_), Some(_)) => {
                conflicts = true;
                "different"
            }
            (None, Some(current)) if current == &record.sha256 => "already_restored",
            (None, _) => {
                corrupt = true;
                "missing_payload"
            }
        };
        files.push(TrashRestoreFileState {
            original_path: record.original_path.clone(),
            payload_path: display_path(&payload),
            purpose: record.purpose.clone(),
            size: record.size,
            sha256: record.sha256.clone(),
            state: state.to_string(),
            current_sha256: current_hash,
        });
    }
    let mut hasher = Sha256::new();
    for file in &files {
        hasher.update(file.original_path.as_bytes());
        hasher.update(file.payload_path.as_bytes());
        hasher.update(file.state.as_bytes());
        hasher.update(file.sha256.as_bytes());
        if let Some(current) = &file.current_sha256 {
            hasher.update(current.as_bytes());
        }
        hasher.update([0]);
    }
    Ok(TrashRestorePreview {
        trash_id: trash_id.to_string(),
        session_id: manifest.session_id,
        title: manifest.title,
        files,
        can_restore_safely: !corrupt && !conflicts,
        has_conflicts: conflicts,
        corrupt,
        preview_fingerprint: format!("{:x}", hasher.finalize()),
    })
}

fn current_identity_from_manifest(manifest: &TrashManifest) -> SessionIdentity {
    SessionIdentity {
        config_dir: PathBuf::from(&manifest.config_dir),
        workspace_root: PathBuf::from(&manifest.workspace_root),
        session_id: manifest.session_id.clone(),
        source_path: PathBuf::from(&manifest.source_path),
    }
}

fn build_exchange_manifest(
    root: &Path,
    source_manifest: &TrashManifest,
    conflicts: &[TrashRestoreFileState],
) -> Result<(PathBuf, PathBuf, TrashManifest), String> {
    let exchange_id = unique_id("exchange");
    let staging = root.join(format!(".staging-{exchange_id}"));
    let final_dir = root.join(&exchange_id);
    fs::create_dir_all(staging.join("payload"))
        .map_err(|error| format!("cannot create exchange staging: {error}"))?;
    let config_dir = PathBuf::from(&source_manifest.config_dir);
    let mut records = Vec::new();
    for conflict in conflicts.iter().filter(|file| file.state == "different") {
        let original = PathBuf::from(&conflict.original_path);
        let relative = relative_to_config(&original, &config_dir)?;
        let metadata = fs::metadata(&original)
            .map_err(|error| format!("cannot inspect conflict {}: {error}", original.display()))?;
        records.push(TrashFileRecord {
            original_path: conflict.original_path.clone(),
            payload_relative_path: display_path(&Path::new("payload").join(relative)),
            purpose: "exchange_displaced_current_version".to_string(),
            size: metadata.len(),
            sha256: sha256_file(&original)?,
        });
    }
    let manifest = TrashManifest {
        version: 1,
        trash_id: exchange_id,
        session_id: source_manifest.session_id.clone(),
        title: format!("{} · 恢复冲突时保留的当前版本", source_manifest.title),
        workspace_root: source_manifest.workspace_root.clone(),
        config_dir: source_manifest.config_dir.clone(),
        source_path: source_manifest.source_path.clone(),
        deleted_at: now_millis(),
        restored_at: None,
        status: "moving".to_string(),
        files: records,
    };
    write_manifest(&staging, &manifest)?;
    Ok((staging, final_dir, manifest))
}

fn restore_entry(
    config_dir: &Path,
    request: &TrashRestoreRequest,
    runtime: &RuntimeRegistry,
) -> Result<TrashRestoreResult, String> {
    let preview = restore_preview_in(config_dir, &request.trash_id)?;
    if preview.preview_fingerprint != request.expected_preview_fingerprint {
        return Err("restore paths changed after preview; review the conflicts again".to_string());
    }
    if preview.corrupt {
        return Err("recycle-bin payload is incomplete or corrupt; automatic restore is blocked".to_string());
    }
    if request.strategy != "safe" && request.strategy != "exchange" {
        return Err("restore strategy must be safe or exchange".to_string());
    }
    if preview.has_conflicts && request.strategy == "safe" {
        return Err("restore has different-content conflicts; use exchange restore to preserve both versions".to_string());
    }

    let root = claude_trash_dir(config_dir);
    let (entry_dir, mut manifest) = load_final_manifest(config_dir, &request.trash_id)?;
    let identity = current_identity_from_manifest(&manifest);
    let external = runtime.probe_external_identity(&identity);
    if external.active {
        return Err(format!(
            "external Claude Code process {} may be using the restore destination",
            external
                .pid
                .map(|pid| pid.to_string())
                .unwrap_or_else(|| "unknown".to_string())
        ));
    }

    let mut exchange = None;
    let mut displaced = Vec::new();
    if preview.has_conflicts && request.strategy == "exchange" {
        let (staging, final_dir, exchange_manifest) =
            build_exchange_manifest(&root, &manifest, &preview.files)?;
        for record in &exchange_manifest.files {
            let original = PathBuf::from(&record.original_path);
            if sha256_file(&original)? != record.sha256 {
                let _ = rollback_moved(&displaced);
                return Err("a conflicting destination changed during exchange restore".to_string());
            }
            let payload = staging.join(PathBuf::from(&record.payload_relative_path));
            if let Some(parent) = payload.parent() {
                fs::create_dir_all(parent).map_err(|error| error.to_string())?;
            }
            if let Err(error) = fs::rename(&original, &payload) {
                let _ = rollback_moved(&displaced);
                return Err(format!("cannot preserve conflicting file {}: {error}", original.display()));
            }
            displaced.push((payload, original));
        }
        exchange = Some((staging, final_dir, exchange_manifest));
    }

    let mut restored = Vec::new();
    for state in &preview.files {
        if !matches!(state.state.as_str(), "free" | "different") {
            continue;
        }
        let record = manifest
            .files
            .iter()
            .find(|record| record.original_path == state.original_path)
            .ok_or_else(|| "restore manifest changed unexpectedly".to_string())?;
        let payload = entry_dir.join(PathBuf::from(&record.payload_relative_path));
        let original = PathBuf::from(&record.original_path);
        if original.exists() {
            let _ = rollback_moved(&restored);
            let _ = rollback_moved(&displaced);
            return Err(format!("restore destination still exists: {}", original.display()));
        }
        if let Some(parent) = original.parent() {
            fs::create_dir_all(parent).map_err(|error| error.to_string())?;
        }
        if let Err(error) = fs::rename(&payload, &original) {
            let _ = rollback_moved(&restored);
            let _ = rollback_moved(&displaced);
            return Err(format!("cannot restore {}: {error}", original.display()));
        }
        // rollback_moved expects (current_location, destination), which for a
        // failed restore means moving the restored original back to payload.
        restored.push((original.clone(), payload.clone()));
        if sha256_file(&original)? != record.sha256 {
            let _ = rollback_moved(&restored);
            let _ = rollback_moved(&displaced);
            return Err("restored file hash mismatch; restore was rolled back".to_string());
        }
    }

    let exchange_id = if let Some((staging, final_dir, mut exchange_manifest)) = exchange {
        exchange_manifest.status = "trashed".to_string();
        write_manifest(&staging, &exchange_manifest)?;
        if let Err(error) = fs::rename(&staging, &final_dir) {
            let _ = rollback_moved(&restored);
            let _ = rollback_moved(&displaced);
            return Err(format!("cannot commit exchange recycle entry: {error}"));
        }
        Some(exchange_manifest.trash_id)
    } else {
        None
    };

    manifest.status = "restored".to_string();
    manifest.restored_at = Some(now_millis());
    write_manifest(&entry_dir, &manifest)?;
    Ok(TrashRestoreResult {
        trash_id: manifest.trash_id,
        status: manifest.status,
        exchange_trash_id: exchange_id,
    })
}

fn collect_all_files(dir: &Path, out: &mut Vec<PathBuf>) -> Result<(), String> {
    for entry in fs::read_dir(dir).map_err(|error| error.to_string())? {
        let entry = entry.map_err(|error| error.to_string())?;
        let path = entry.path();
        if path.is_dir() {
            collect_all_files(&path, out)?;
        } else if path.is_file() {
            out.push(path);
        }
    }
    Ok(())
}

fn purge_preview_in(config_dir: &Path, trash_id: &str) -> Result<TrashPurgePreview, String> {
    let (entry_dir, manifest) = load_final_manifest(config_dir, trash_id)?;
    let mut paths = Vec::new();
    collect_all_files(&entry_dir, &mut paths)?;
    paths.sort_by_key(|path| normalize_path_key(path));
    let mut files = Vec::new();
    let mut hasher = Sha256::new();
    for path in paths {
        let metadata = fs::metadata(&path).map_err(|error| error.to_string())?;
        let hash = sha256_file(&path)?;
        let purpose = if path.file_name().and_then(|value| value.to_str()) == Some("manifest.json") {
            "trash_manifest"
        } else {
            "trash_payload"
        };
        let display = display_path(&path);
        hasher.update(display.as_bytes());
        hasher.update(metadata.len().to_le_bytes());
        hasher.update(hash.as_bytes());
        hasher.update([0]);
        files.push(TrashPurgeFile {
            path: display,
            size: metadata.len(),
            purpose: purpose.to_string(),
        });
    }
    let short_id = manifest.session_id.chars().take(8).collect::<String>();
    Ok(TrashPurgePreview {
        trash_id: trash_id.to_string(),
        session_id: manifest.session_id,
        title: manifest.title,
        total_files: files.len(),
        total_size: files.iter().map(|file| file.size).sum(),
        confirmation_text: format!("永久删除 {short_id}"),
        preview_fingerprint: format!("{:x}", hasher.finalize()),
        files,
    })
}

fn purge_entry(config_dir: &Path, request: &TrashPurgeRequest) -> Result<(), String> {
    let preview = purge_preview_in(config_dir, &request.trash_id)?;
    if preview.preview_fingerprint != request.expected_preview_fingerprint {
        return Err("recycle-bin contents changed after purge preview; review them again".to_string());
    }
    if request.confirmation_text != preview.confirmation_text {
        return Err(format!(
            "confirmation text must exactly match: {}",
            preview.confirmation_text
        ));
    }
    let entry_dir = claude_trash_dir(config_dir).join(&request.trash_id);
    fs::remove_dir_all(&entry_dir)
        .map_err(|error| format!("cannot permanently delete {}: {error}", entry_dir.display()))
}

#[tauri::command]
pub fn trash_delete_preview(
    runtime: State<'_, RuntimeRegistry>,
    catalog: State<'_, SessionCatalog>,
    session_key: String,
) -> Result<TrashDeletePreview, String> {
    let identity = identity_from_catalog(&catalog, &session_key)?;
    if session_has_pending_recovery(&identity)? {
        return Err(
            "this Claude session has an unfinished edit/restore transaction; resolve recovery before deleting it"
                .to_string(),
        );
    }
    let title = crate::claude::scanner::load_session(&identity)?.summary.title;
    build_delete_preview(&identity, &session_key, &title, &runtime)
}

#[tauri::command]
pub fn trash_move(
    service: State<'_, TrashService>,
    runtime: State<'_, RuntimeRegistry>,
    editor: State<'_, TranscriptEditor>,
    catalog: State<'_, SessionCatalog>,
    request: TrashMoveRequest,
) -> Result<TrashItemSummary, String> {
    let _lease = service.acquire_session(&request.session_key)?;
    if editor.is_locked(&request.session_key) {
        return Err("finish the transcript edit or restore before recycling this session".to_string());
    }
    let identity = identity_from_catalog(&catalog, &request.session_key)?;
    if session_has_pending_recovery(&identity)? {
        return Err(
            "this Claude session has an unfinished edit/restore transaction; resolve recovery before deleting it"
                .to_string(),
        );
    }
    let title = crate::claude::scanner::load_session(&identity)?.summary.title;
    let summary = move_session_to_trash(
        &identity,
        &request.session_key,
        &title,
        &request.expected_preview_fingerprint,
        &runtime,
    )?;
    if let Ok(mut catalog) = catalog.0.lock() {
        catalog.remove(&request.session_key);
    }
    Ok(summary)
}

#[tauri::command]
pub fn trash_list() -> Result<Vec<TrashItemSummary>, String> {
    list_items_in(&claude_config_dir()?)
}

#[tauri::command]
pub fn trash_restore_preview(trash_id: String) -> Result<TrashRestorePreview, String> {
    restore_preview_in(&claude_config_dir()?, &trash_id)
}

#[tauri::command]
pub fn trash_restore(
    service: State<'_, TrashService>,
    runtime: State<'_, RuntimeRegistry>,
    catalog: State<'_, SessionCatalog>,
    request: TrashRestoreRequest,
) -> Result<TrashRestoreResult, String> {
    let _lease = service.acquire_entry(&request.trash_id)?;
    let config_dir = claude_config_dir()?;
    let result = restore_entry(&config_dir, &request, &runtime)?;
    let (_, manifest) = load_final_manifest(&config_dir, &request.trash_id)?;
    let identity = current_identity_from_manifest(&manifest);
    if identity.source_path.is_file() {
        if let Ok(mut catalog) = catalog.0.lock() {
            catalog.insert(identity.key(), identity);
        }
    }
    Ok(result)
}

#[tauri::command]
pub fn trash_purge_preview(trash_id: String) -> Result<TrashPurgePreview, String> {
    purge_preview_in(&claude_config_dir()?, &trash_id)
}

#[tauri::command]
pub fn trash_purge(
    service: State<'_, TrashService>,
    request: TrashPurgeRequest,
) -> Result<(), String> {
    let _lease = service.acquire_entry(&request.trash_id)?;
    purge_entry(&claude_config_dir()?, &request)
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Fixture {
        root: PathBuf,
        identity: SessionIdentity,
        session_key: String,
        main_bytes: Vec<u8>,
        agent_bytes: Vec<u8>,
    }

    impl Fixture {
        fn new() -> Self {
            let root = std::env::temp_dir().join(unique_id("claudeix-trash-test"));
            let config = root.join(".claude");
            let project = config.join("projects").join("fixture");
            let workspace = root.join("workspace");
            let session_id = "session-trash-1";
            fs::create_dir_all(&project).unwrap();
            fs::create_dir_all(&workspace).unwrap();
            let main = project.join(format!("{session_id}.jsonl"));
            let agent = project
                .join(session_id)
                .join("subagents")
                .join("agent-test.jsonl");
            fs::create_dir_all(agent.parent().unwrap()).unwrap();
            let main_bytes = format!(
                "{{\"type\":\"user\",\"sessionId\":\"{session_id}\",\"cwd\":\"{}\",\"uuid\":\"u1\",\"message\":{{\"role\":\"user\",\"content\":\"hello\"}}}}\n",
                display_path(&workspace)
            )
            .into_bytes();
            let agent_bytes = b"{\"type\":\"assistant\",\"sessionId\":\"session-trash-1\",\"agentId\":\"agent-test\"}\n".to_vec();
            fs::write(&main, &main_bytes).unwrap();
            fs::write(&agent, &agent_bytes).unwrap();
            let identity = SessionIdentity {
                config_dir: config,
                workspace_root: workspace,
                session_id: session_id.to_string(),
                source_path: main,
            };
            let session_key = identity.key();
            Self {
                root,
                identity,
                session_key,
                main_bytes,
                agent_bytes,
            }
        }

        fn runtime() -> RuntimeRegistry {
            RuntimeRegistry::default()
        }
    }

    #[test]
    fn previews_moves_and_restores_main_and_subagent_files() {
        let fixture = Fixture::new();
        let runtime = Fixture::runtime();
        let preview = build_delete_preview(
            &fixture.identity,
            &fixture.session_key,
            "fixture",
            &runtime,
        )
        .unwrap();
        assert_eq!(preview.total_files, 2);
        assert!(preview
            .files
            .iter()
            .any(|file| file.purpose == "main_transcript"));
        assert!(preview
            .files
            .iter()
            .any(|file| file.purpose == "subagent_transcript"));
        let trashed = move_session_to_trash(
            &fixture.identity,
            &fixture.session_key,
            "fixture",
            &preview.preview_fingerprint,
            &runtime,
        )
        .unwrap();
        assert!(!fixture.identity.source_path.exists());
        let restore_preview =
            restore_preview_in(&fixture.identity.config_dir, &trashed.trash_id).unwrap();
        assert!(restore_preview.can_restore_safely);
        restore_entry(
            &fixture.identity.config_dir,
            &TrashRestoreRequest {
                trash_id: trashed.trash_id,
                strategy: "safe".to_string(),
                expected_preview_fingerprint: restore_preview.preview_fingerprint,
            },
            &runtime,
        )
        .unwrap();
        assert_eq!(fs::read(&fixture.identity.source_path).unwrap(), fixture.main_bytes);
        let agent = fixture
            .identity
            .source_path
            .parent()
            .unwrap()
            .join(&fixture.identity.session_id)
            .join("subagents")
            .join("agent-test.jsonl");
        assert_eq!(fs::read(agent).unwrap(), fixture.agent_bytes);
    }

    #[test]
    fn stale_delete_preview_does_not_move_changed_source() {
        let fixture = Fixture::new();
        let runtime = Fixture::runtime();
        let preview = build_delete_preview(
            &fixture.identity,
            &fixture.session_key,
            "fixture",
            &runtime,
        )
        .unwrap();
        fs::write(&fixture.identity.source_path, b"changed\n").unwrap();
        let error = move_session_to_trash(
            &fixture.identity,
            &fixture.session_key,
            "fixture",
            &preview.preview_fingerprint,
            &runtime,
        )
        .unwrap_err();
        assert!(error.contains("changed after the delete preview"));
        assert!(fixture.identity.source_path.exists());
    }

    #[test]
    fn incomplete_delete_staging_rolls_moved_files_back_on_recovery() {
        let fixture = Fixture::new();
        let runtime = Fixture::runtime();
        let preview = build_delete_preview(
            &fixture.identity,
            &fixture.session_key,
            "fixture",
            &runtime,
        )
        .unwrap();
        let root = claude_trash_dir(&fixture.identity.config_dir);
        fs::create_dir_all(&root).unwrap();
        let trash_id = unique_id("crash-fixture");
        let staging = root.join(format!(".staging-{trash_id}"));
        fs::create_dir_all(staging.join("payload")).unwrap();
        let records = records_from_preview(&preview, &fixture.identity.config_dir).unwrap();
        let manifest = TrashManifest {
            version: 1,
            trash_id,
            session_id: fixture.identity.session_id.clone(),
            title: "fixture".to_string(),
            workspace_root: display_path(&fixture.identity.workspace_root),
            config_dir: display_path(&fixture.identity.config_dir),
            source_path: display_path(&fixture.identity.source_path),
            deleted_at: now_millis(),
            restored_at: None,
            status: "moving".to_string(),
            files: records,
        };
        write_manifest(&staging, &manifest).unwrap();

        let main_record = manifest
            .files
            .iter()
            .find(|record| record.purpose == "main_transcript")
            .unwrap();
        let payload = staging.join(&main_record.payload_relative_path);
        fs::create_dir_all(payload.parent().unwrap()).unwrap();
        fs::rename(&fixture.identity.source_path, &payload).unwrap();
        assert!(!fixture.identity.source_path.exists());

        let warnings = recover_incomplete_staging(&fixture.identity.config_dir).unwrap();
        assert!(warnings.is_empty());
        assert_eq!(fs::read(&fixture.identity.source_path).unwrap(), fixture.main_bytes);
        let recovered_manifest = read_manifest(&staging).unwrap();
        assert_eq!(recovered_manifest.status, "rolled_back");
    }

    #[test]
    fn recovery_conflict_becomes_visible_trash_entry() {
        let fixture = Fixture::new();
        let runtime = Fixture::runtime();
        let preview = build_delete_preview(
            &fixture.identity,
            &fixture.session_key,
            "fixture",
            &runtime,
        )
        .unwrap();
        let root = claude_trash_dir(&fixture.identity.config_dir);
        fs::create_dir_all(&root).unwrap();
        let trash_id = unique_id("crash-conflict");
        let staging = root.join(format!(".staging-{trash_id}"));
        fs::create_dir_all(staging.join("payload")).unwrap();
        let records = records_from_preview(&preview, &fixture.identity.config_dir).unwrap();
        let manifest = TrashManifest {
            version: 1,
            trash_id: trash_id.clone(),
            session_id: fixture.identity.session_id.clone(),
            title: "fixture".to_string(),
            workspace_root: display_path(&fixture.identity.workspace_root),
            config_dir: display_path(&fixture.identity.config_dir),
            source_path: display_path(&fixture.identity.source_path),
            deleted_at: now_millis(),
            restored_at: None,
            status: "moving".to_string(),
            files: records,
        };
        write_manifest(&staging, &manifest).unwrap();
        let main_record = manifest
            .files
            .iter()
            .find(|record| record.purpose == "main_transcript")
            .unwrap();
        let payload = staging.join(&main_record.payload_relative_path);
        fs::create_dir_all(payload.parent().unwrap()).unwrap();
        fs::rename(&fixture.identity.source_path, &payload).unwrap();
        fs::write(&fixture.identity.source_path, b"new conflicting version\n").unwrap();

        let warnings = recover_incomplete_staging(&fixture.identity.config_dir).unwrap();
        assert_eq!(warnings.len(), 1);
        assert!(!staging.exists());
        let visible = root.join(&trash_id);
        assert!(visible.is_dir());
        let recovered_manifest = read_manifest(&visible).unwrap();
        assert_eq!(recovered_manifest.status, "recovery_conflict");
        assert_eq!(
            fs::read(visible.join(&main_record.payload_relative_path)).unwrap(),
            fixture.main_bytes
        );
    }

    #[test]
    fn exchange_restore_preserves_conflicting_current_version() {
        let fixture = Fixture::new();
        let runtime = Fixture::runtime();
        let preview = build_delete_preview(
            &fixture.identity,
            &fixture.session_key,
            "fixture",
            &runtime,
        )
        .unwrap();
        let trashed = move_session_to_trash(
            &fixture.identity,
            &fixture.session_key,
            "fixture",
            &preview.preview_fingerprint,
            &runtime,
        )
        .unwrap();
        let conflicting = b"new current version\n";
        fs::write(&fixture.identity.source_path, conflicting).unwrap();
        let restore_preview =
            restore_preview_in(&fixture.identity.config_dir, &trashed.trash_id).unwrap();
        assert!(restore_preview.has_conflicts);
        let safe_error = restore_entry(
            &fixture.identity.config_dir,
            &TrashRestoreRequest {
                trash_id: trashed.trash_id.clone(),
                strategy: "safe".to_string(),
                expected_preview_fingerprint: restore_preview.preview_fingerprint.clone(),
            },
            &runtime,
        )
        .unwrap_err();
        assert!(safe_error.contains("exchange restore"));

        let restored = restore_entry(
            &fixture.identity.config_dir,
            &TrashRestoreRequest {
                trash_id: trashed.trash_id,
                strategy: "exchange".to_string(),
                expected_preview_fingerprint: restore_preview.preview_fingerprint,
            },
            &runtime,
        )
        .unwrap();
        assert_eq!(fs::read(&fixture.identity.source_path).unwrap(), fixture.main_bytes);
        let exchange_id = restored.exchange_trash_id.expect("exchange entry");
        let (exchange_dir, exchange_manifest) =
            load_final_manifest(&fixture.identity.config_dir, &exchange_id).unwrap();
        let displaced = &exchange_manifest.files[0];
        assert_eq!(
            fs::read(exchange_dir.join(&displaced.payload_relative_path)).unwrap(),
            conflicting
        );
    }

    #[test]
    fn purge_requires_exact_confirmation_and_preview_fingerprint() {
        let fixture = Fixture::new();
        let runtime = Fixture::runtime();
        let preview = build_delete_preview(
            &fixture.identity,
            &fixture.session_key,
            "fixture",
            &runtime,
        )
        .unwrap();
        let trashed = move_session_to_trash(
            &fixture.identity,
            &fixture.session_key,
            "fixture",
            &preview.preview_fingerprint,
            &runtime,
        )
        .unwrap();
        let purge_preview = purge_preview_in(&fixture.identity.config_dir, &trashed.trash_id).unwrap();
        let wrong = purge_entry(
            &fixture.identity.config_dir,
            &TrashPurgeRequest {
                trash_id: trashed.trash_id.clone(),
                confirmation_text: "永久删除 错误".to_string(),
                expected_preview_fingerprint: purge_preview.preview_fingerprint.clone(),
            },
        )
        .unwrap_err();
        assert!(wrong.contains("confirmation text"));
        purge_entry(
            &fixture.identity.config_dir,
            &TrashPurgeRequest {
                trash_id: trashed.trash_id.clone(),
                confirmation_text: purge_preview.confirmation_text,
                expected_preview_fingerprint: purge_preview.preview_fingerprint,
            },
        )
        .unwrap();
        assert!(!claude_trash_dir(&fixture.identity.config_dir)
            .join(trashed.trash_id)
            .exists());
        assert!(fixture.root.exists());
    }
}
