use super::session::SessionDetail;
use super::transcript::EditTarget;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TranscriptEditRequest {
    pub session_key: String,
    pub target: EditTarget,
    pub expected_file_fingerprint: String,
    pub new_text: String,
    #[serde(default)]
    pub allow_assistant_text: bool,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TranscriptRestoreRequest {
    pub session_key: String,
    pub snapshot_id: String,
    pub expected_file_fingerprint: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BackupSnapshot {
    pub id: String,
    pub session_id: String,
    pub source_path: String,
    pub backup_path: String,
    pub created_at: u64,
    pub source_hash: String,
    pub file_size: u64,
    pub reason: String,
    pub target: Option<EditTarget>,
    pub old_text_hash: Option<String>,
    pub new_text_hash: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TranscriptEditResult {
    pub detail: SessionDetail,
    pub snapshot: BackupSnapshot,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TranscriptRestoreResult {
    pub detail: SessionDetail,
    pub current_snapshot: BackupSnapshot,
    pub restored_snapshot_id: String,
}
