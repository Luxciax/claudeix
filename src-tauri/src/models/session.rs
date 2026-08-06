use crate::app_paths::{display_path, normalize_path_key};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};

use super::transcript::TranscriptRecord;

#[derive(Debug, Clone)]
pub struct SessionIdentity {
    pub config_dir: PathBuf,
    pub workspace_root: PathBuf,
    pub session_id: String,
    pub source_path: PathBuf,
}

impl SessionIdentity {
    pub fn key(&self) -> String {
        let mut hasher = Sha256::new();
        for value in [
            normalize_path_key(&self.config_dir),
            normalize_path_key(&self.workspace_root),
            self.session_id.clone(),
            normalize_path_key(&self.source_path),
        ] {
            hasher.update(value.as_bytes());
            hasher.update([0]);
        }
        format!("{:x}", hasher.finalize())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionSummary {
    pub key: String,
    pub session_id: String,
    pub title: String,
    pub preview: String,
    pub workspace_root: String,
    pub source_path: String,
    pub created_at: i64,
    pub updated_at: i64,
    pub message_count: usize,
    pub model: Option<String>,
    pub archived: bool,
    pub favorite: bool,
    pub trashed: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ParseDiagnostic {
    pub line_index: usize,
    pub message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionDetail {
    pub summary: SessionSummary,
    pub records: Vec<TranscriptRecord>,
    pub diagnostics: Vec<ParseDiagnostic>,
    pub file_fingerprint: String,
    pub has_branches: bool,
}

impl SessionSummary {
    pub fn from_identity(
        identity: &SessionIdentity,
        title: String,
        preview: String,
        created_at: i64,
        updated_at: i64,
        message_count: usize,
        model: Option<String>,
    ) -> Self {
        Self {
            key: identity.key(),
            session_id: identity.session_id.clone(),
            title,
            preview,
            workspace_root: display_path(&identity.workspace_root),
            source_path: display_path(&identity.source_path),
            created_at,
            updated_at,
            message_count,
            model,
            archived: false,
            favorite: false,
            trashed: false,
        }
    }
}

pub fn path_matches(left: &Path, right: &Path) -> bool {
    normalize_path_key(left) == normalize_path_key(right)
}
