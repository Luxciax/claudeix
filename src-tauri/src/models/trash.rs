use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TrashFilePreview {
    pub original_path: String,
    pub purpose: String,
    pub size: u64,
    pub sha256: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TrashDeletePreview {
    pub session_key: String,
    pub session_id: String,
    pub title: String,
    pub workspace_root: String,
    pub config_dir: String,
    pub files: Vec<TrashFilePreview>,
    pub total_files: usize,
    pub total_size: u64,
    pub preview_fingerprint: String,
    pub runtime_active: bool,
    pub external_runtime_active: bool,
    pub external_runtime_pid: Option<u32>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TrashMoveRequest {
    pub session_key: String,
    pub expected_preview_fingerprint: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TrashFileRecord {
    pub original_path: String,
    pub payload_relative_path: String,
    pub purpose: String,
    pub size: u64,
    pub sha256: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TrashManifest {
    pub version: u32,
    pub trash_id: String,
    pub session_id: String,
    pub title: String,
    pub workspace_root: String,
    pub config_dir: String,
    pub source_path: String,
    pub deleted_at: u64,
    pub restored_at: Option<u64>,
    pub status: String,
    pub files: Vec<TrashFileRecord>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TrashItemSummary {
    pub trash_id: String,
    pub session_id: String,
    pub title: String,
    pub workspace_root: String,
    pub config_dir: String,
    pub deleted_at: u64,
    pub restored_at: Option<u64>,
    pub status: String,
    pub total_files: usize,
    pub total_size: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TrashRestoreFileState {
    pub original_path: String,
    pub payload_path: String,
    pub purpose: String,
    pub size: u64,
    pub sha256: String,
    pub state: String,
    pub current_sha256: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TrashRestorePreview {
    pub trash_id: String,
    pub session_id: String,
    pub title: String,
    pub files: Vec<TrashRestoreFileState>,
    pub can_restore_safely: bool,
    pub has_conflicts: bool,
    pub corrupt: bool,
    pub preview_fingerprint: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TrashRestoreRequest {
    pub trash_id: String,
    pub strategy: String,
    pub expected_preview_fingerprint: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TrashRestoreResult {
    pub trash_id: String,
    pub status: String,
    pub exchange_trash_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TrashPurgeFile {
    pub path: String,
    pub size: u64,
    pub purpose: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TrashPurgePreview {
    pub trash_id: String,
    pub session_id: String,
    pub title: String,
    pub files: Vec<TrashPurgeFile>,
    pub total_files: usize,
    pub total_size: u64,
    pub confirmation_text: String,
    pub preview_fingerprint: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TrashPurgeRequest {
    pub trash_id: String,
    pub confirmation_text: String,
    pub expected_preview_fingerprint: String,
}
