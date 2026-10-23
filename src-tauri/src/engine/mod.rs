use crate::models::transcript::TranscriptRecord;
use crate::models::transcript::EditTarget;
use serde::{Deserialize, Serialize};
use std::path::Path;
use std::path::PathBuf;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
#[allow(dead_code)] // Additional storage kinds are part of the adapter contract before those engines land.
pub enum NativeStorageKind {
    Jsonl,
    Json,
    Sqlite,
    DirectoryTree,
    Custom,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EngineCapabilities {
    pub read_sessions: bool,
    pub write_sessions: bool,
    pub edit_message: bool,
    pub delete_message: bool,
    pub truncate_session: bool,
    pub delete_session: bool,
    pub restore_session: bool,
    pub create_session: bool,
    pub resume_session: bool,
    pub stream: bool,
    pub interrupt: bool,
    pub permissions: bool,
    pub model_discovery: bool,
}

impl EngineCapabilities {
    pub const fn read_only() -> Self {
        Self {
            read_sessions: true,
            write_sessions: false,
            edit_message: false,
            delete_message: false,
            truncate_session: false,
            delete_session: false,
            restore_session: false,
            create_session: false,
            resume_session: false,
            stream: false,
            interrupt: false,
            permissions: false,
            model_discovery: false,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EngineDescriptor {
    /// Stable machine-facing engine id (for example `claude` or `codex`).
    pub id: String,
    /// Human-facing product name shown by the UI.
    pub display_name: String,
    /// Native persistence used by the agent. The core must not assume sessions are files.
    pub storage: NativeStorageKind,
    /// Capability-driven UI contract. Product code must not branch on engine ids.
    pub capabilities: EngineCapabilities,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct NativeSessionSummary {
    pub engine_id: String,
    pub session_key: String,
    pub native_session_id: String,
    pub title: String,
    pub preview: String,
    pub workspace_root: String,
    pub source_path: String,
    pub updated_at: i64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct NativeSessionDetail {
    pub summary: NativeSessionSummary,
    pub records: Vec<TranscriptRecord>,
    pub source_revision: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EngineEditRequest {
    pub engine_id: String,
    pub session_key: String,
    pub workspace_root: String,
    pub expected_source_revision: String,
    pub target: EditTarget,
    pub new_text: String,
}

#[derive(Debug, Clone)]
pub struct PreparedFileMutation {
    pub engine_id: String,
    pub session_key: String,
    pub native_session_id: String,
    pub workspace_root: PathBuf,
    pub source_path: PathBuf,
    pub target: Option<EditTarget>,
    pub source_bytes: Vec<u8>,
    pub candidate_bytes: Vec<u8>,
    pub old_content: Option<String>,
    pub new_content: Option<String>,
    pub operation: String,
}

pub trait EngineAdapter: Send + Sync {
    fn descriptor(&self) -> EngineDescriptor;

    fn scan_sessions(&self, workspace_root: &Path) -> Result<Vec<NativeSessionSummary>, String>;

    fn get_session_detail(
        &self,
        session_key: &str,
        workspace_root: &Path,
    ) -> Result<NativeSessionDetail, String>;

    fn external_writer_pid(
        &self,
        _session_key: &str,
        _workspace_root: &Path,
    ) -> Result<Option<u32>, String> {
        Ok(None)
    }

    fn prepare_file_edit(&self, _request: &EngineEditRequest) -> Result<PreparedFileMutation, String> {
        Err("this engine is not yet connected to the generic file mutation layer".to_string())
    }

    fn validate_file_candidate(
        &self,
        _mutation: &PreparedFileMutation,
        _candidate_path: &Path,
    ) -> Result<(), String> {
        Err("this engine does not expose generic file candidate validation".to_string())
    }
}

pub struct EngineRegistry {
    adapters: Vec<Box<dyn EngineAdapter>>,
}

impl Default for EngineRegistry {
    fn default() -> Self {
        Self {
            adapters: vec![
                Box::new(crate::engines::claude::ClaudeAdapter),
                Box::new(crate::engines::pi::PiAdapter::default()),
            ],
        }
    }
}

impl EngineRegistry {
    pub fn descriptors(&self) -> Vec<EngineDescriptor> {
        self.adapters
            .iter()
            .map(|adapter| adapter.descriptor())
            .collect()
    }

    pub fn scan_sessions(
        &self,
        engine_id: &str,
        workspace_root: &Path,
    ) -> Result<Vec<NativeSessionSummary>, String> {
        let adapter = self
            .adapters
            .iter()
            .find(|adapter| adapter.descriptor().id == engine_id)
            .ok_or_else(|| format!("unknown engine: {engine_id}"))?;
        adapter.scan_sessions(workspace_root)
    }

    pub fn get_session_detail(
        &self,
        engine_id: &str,
        session_key: &str,
        workspace_root: &Path,
    ) -> Result<NativeSessionDetail, String> {
        let adapter = self
            .adapters
            .iter()
            .find(|adapter| adapter.descriptor().id == engine_id)
            .ok_or_else(|| format!("unknown engine: {engine_id}"))?;
        adapter.get_session_detail(session_key, workspace_root)
    }

    pub fn external_writer_pid(
        &self,
        engine_id: &str,
        session_key: &str,
        workspace_root: &Path,
    ) -> Result<Option<u32>, String> {
        let adapter = self
            .adapters
            .iter()
            .find(|adapter| adapter.descriptor().id == engine_id)
            .ok_or_else(|| format!("unknown engine: {engine_id}"))?;
        adapter.external_writer_pid(session_key, workspace_root)
    }

    pub(crate) fn prepare_file_edit(
        &self,
        request: &EngineEditRequest,
    ) -> Result<PreparedFileMutation, String> {
        let adapter = self
            .adapters
            .iter()
            .find(|adapter| adapter.descriptor().id == request.engine_id)
            .ok_or_else(|| format!("unknown engine: {}", request.engine_id))?;
        adapter.prepare_file_edit(request)
    }

    pub(crate) fn validate_file_candidate(
        &self,
        engine_id: &str,
        mutation: &PreparedFileMutation,
        candidate_path: &Path,
    ) -> Result<(), String> {
        let adapter = self
            .adapters
            .iter()
            .find(|adapter| adapter.descriptor().id == engine_id)
            .ok_or_else(|| format!("unknown engine: {engine_id}"))?;
        adapter.validate_file_candidate(mutation, candidate_path)
    }
}

#[tauri::command]
pub fn engine_list(registry: tauri::State<'_, EngineRegistry>) -> Vec<EngineDescriptor> {
    registry.descriptors()
}

#[tauri::command]
pub fn engine_session_scan(
    engine_id: String,
    workspace_root: String,
    registry: tauri::State<'_, EngineRegistry>,
) -> Result<Vec<NativeSessionSummary>, String> {
    registry.scan_sessions(&engine_id, Path::new(&workspace_root))
}

#[tauri::command]
pub fn engine_session_get(
    engine_id: String,
    session_key: String,
    workspace_root: String,
    registry: tauri::State<'_, EngineRegistry>,
) -> Result<NativeSessionDetail, String> {
    registry.get_session_detail(&engine_id, &session_key, Path::new(&workspace_root))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn registry_exposes_claude_without_changing_existing_runtime() {
        let descriptors = EngineRegistry::default().descriptors();
        assert_eq!(descriptors.len(), 2);
        let claude = &descriptors[0];
        assert_eq!(claude.id, "claude");
        assert_eq!(claude.storage, NativeStorageKind::Jsonl);
        assert!(claude.capabilities.read_sessions);
        assert!(claude.capabilities.write_sessions);
        assert!(claude.capabilities.edit_message);
        assert!(claude.capabilities.delete_session);
        assert!(claude.capabilities.restore_session);
        assert!(claude.capabilities.resume_session);
        // Claudeix does not yet expose semantic delete/truncate operations on transcript turns.
        assert!(!claude.capabilities.delete_message);
        assert!(!claude.capabilities.truncate_session);

        let pi = &descriptors[1];
        assert_eq!(pi.id, "pi");
        assert_eq!(pi.storage, NativeStorageKind::Jsonl);
        assert!(pi.capabilities.read_sessions);
        assert!(pi.capabilities.write_sessions);
        assert!(pi.capabilities.edit_message);
        assert!(pi.capabilities.create_session);
        assert!(pi.capabilities.resume_session);
        assert!(pi.capabilities.stream);
        assert!(pi.capabilities.interrupt);
        assert!(pi.capabilities.model_discovery);
        assert!(!pi.capabilities.delete_message);
        assert!(!pi.capabilities.truncate_session);
    }
}
