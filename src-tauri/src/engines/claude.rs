use crate::engine::{
    EngineAdapter, EngineCapabilities, EngineDescriptor, NativeSessionDetail, NativeSessionSummary,
    NativeStorageKind,
};
use std::path::Path;

/// Thin adapter seam around the already-working Claudeix implementation.
///
/// This intentionally does not duplicate the current scanner/parser/editor/runtime.
/// Those modules remain the production implementation while the multi-engine core is
/// introduced incrementally. Later changes can move them behind this adapter without
/// changing the Pi-derived frontend surface.
pub struct ClaudeAdapter;

impl EngineAdapter for ClaudeAdapter {
    fn descriptor(&self) -> EngineDescriptor {
        EngineDescriptor {
            id: "claude".to_string(),
            display_name: "Claude Code".to_string(),
            storage: NativeStorageKind::Jsonl,
            capabilities: EngineCapabilities {
                read_sessions: true,
                write_sessions: true,
                edit_message: true,
                delete_message: false,
                truncate_session: false,
                delete_session: true,
                restore_session: true,
                create_session: true,
                resume_session: true,
                stream: true,
                interrupt: true,
                permissions: true,
                model_discovery: false,
            },
        }
    }

    fn scan_sessions(&self, workspace_root: &Path) -> Result<Vec<NativeSessionSummary>, String> {
        crate::claude::scanner::scan_workspace(workspace_root).map(|sessions| {
            sessions
                .into_iter()
                .map(|(summary, _identity)| native_summary(summary))
                .collect()
        })
    }

    fn get_session_detail(
        &self,
        session_key: &str,
        workspace_root: &Path,
    ) -> Result<NativeSessionDetail, String> {
        let sessions = crate::claude::scanner::scan_workspace(workspace_root)?;
        let (_summary, identity) = sessions
            .into_iter()
            .find(|(summary, _)| summary.key == session_key)
            .ok_or_else(|| format!("Claude session not found: {session_key}"))?;
        let detail = crate::claude::scanner::load_session(&identity)?;
        Ok(NativeSessionDetail {
            summary: native_summary(detail.summary),
            records: detail.records,
            source_revision: detail.file_fingerprint,
        })
    }
}

fn native_summary(summary: crate::models::session::SessionSummary) -> NativeSessionSummary {
    NativeSessionSummary {
        engine_id: "claude".to_string(),
        session_key: summary.key,
        native_session_id: summary.session_id,
        title: summary.title,
        preview: summary.preview,
        workspace_root: summary.workspace_root,
        source_path: summary.source_path,
        updated_at: summary.updated_at,
    }
}
