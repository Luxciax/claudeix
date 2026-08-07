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

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TranscriptAuditEntry {
    pub id: i64,
    pub transaction_id: String,
    pub session_id: String,
    pub source_path: String,
    pub operation: String,
    pub line_index: Option<usize>,
    pub content_index: Option<usize>,
    pub field_name: Option<String>,
    pub old_content: Option<String>,
    pub new_content: Option<String>,
    pub backup_path: String,
    pub source_hash_before: String,
    pub source_hash_after: String,
    pub created_at: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EditRecoveryItem {
    pub transaction_id: String,
    pub session_id: String,
    pub source_path: String,
    pub operation: String,
    pub status: String,
    pub backup_path: Option<String>,
    pub source_hash_before: String,
    pub source_hash_after: String,
    pub created_at: u64,
    pub detected_state: String,
    pub backup_valid: bool,
    pub can_keep_current: bool,
    pub can_restore_backup: bool,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EditRecoveryResolveRequest {
    pub transaction_id: String,
    pub action: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EditRecoveryResolveResult {
    pub transaction_id: String,
    pub status: String,
    pub source_path: String,
}
