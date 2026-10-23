pub mod runtime;

use crate::app_paths::{display_path, home_dir, normalize_path_key};
use crate::engine::{
    EngineAdapter, EngineCapabilities, EngineDescriptor, EngineEditRequest, NativeSessionDetail,
    NativeSessionSummary, NativeStorageKind, PreparedFileMutation,
};
use crate::models::transcript::{ContentBlock, EditTarget, TranscriptRecord};
use crate::mutation::revision;
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::collections::{HashMap, HashSet};
use std::fs::{self, File};
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::time::UNIX_EPOCH;
use sysinfo::System;

pub struct PiAdapter {
    sessions_root: PathBuf,
}

impl Default for PiAdapter {
    fn default() -> Self {
        let sessions_root = std::env::var_os("PI_CODING_AGENT_SESSION_DIR")
            .map(PathBuf::from)
            .or_else(|| {
                std::env::var_os("PI_CODING_AGENT_DIR")
                    .map(PathBuf::from)
                    .map(|home| home.join("sessions"))
            })
            .or_else(|| home_dir().ok().map(|home| home.join(".pi/agent/sessions")))
            .unwrap_or_else(|| PathBuf::from(".pi/agent/sessions"));
        Self { sessions_root }
    }
}

#[derive(Default)]
struct PiSummary {
    session_id: String,
    cwd: String,
    title: String,
    preview: String,
}

struct PiRow {
    line_index: usize,
    raw: String,
    value: Value,
}

impl PiAdapter {
    #[cfg(test)]
    pub(crate) fn with_sessions_root(sessions_root: PathBuf) -> Self {
        Self { sessions_root }
    }

    fn collect_jsonl(dir: &Path, out: &mut Vec<PathBuf>) {
        let Ok(entries) = fs::read_dir(dir) else {
            return;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                Self::collect_jsonl(&path, out);
            } else if path.extension().and_then(|ext| ext.to_str()) == Some("jsonl") {
                out.push(path);
            }
        }
    }

    fn read_summary(path: &Path) -> Option<PiSummary> {
        let file = File::open(path).ok()?;
        let mut summary = PiSummary::default();

        for raw in BufReader::new(file).lines().map_while(Result::ok) {
            let value = serde_json::from_str::<Value>(&raw).ok()?;
            match value.get("type").and_then(Value::as_str) {
                Some("session") => {
                    if summary.session_id.is_empty() {
                        summary.session_id = value
                            .get("id")
                            .and_then(Value::as_str)
                            .unwrap_or_default()
                            .to_string();
                    }
                    if summary.cwd.is_empty() {
                        summary.cwd = value
                            .get("cwd")
                            .and_then(Value::as_str)
                            .unwrap_or_default()
                            .to_string();
                    }
                }
                Some("session_info") => {
                    if let Some(name) = value.get("name").and_then(Value::as_str) {
                        if !name.trim().is_empty() {
                            summary.title = name.trim().to_string();
                        }
                    }
                }
                Some("message") if summary.preview.is_empty() => {
                    if let Some(message) = value.get("message") {
                        let role = message
                            .get("role")
                            .and_then(Value::as_str)
                            .unwrap_or_default();
                        if matches!(role, "user" | "assistant") {
                            summary.preview = message_text(message)
                                .trim()
                                .chars()
                                .take(200)
                                .collect();
                        }
                    }
                }
                _ => {}
            }
        }

        if summary.session_id.is_empty() {
            summary.session_id = path
                .file_stem()
                .and_then(|stem| stem.to_str())
                .unwrap_or("unknown")
                .to_string();
        }
        if summary.title.is_empty() {
            summary.title = if summary.preview.is_empty() {
                summary.session_id.clone()
            } else {
                summary.preview.chars().take(60).collect()
            };
        }
        Some(summary)
    }

    fn resolve_session_path(&self, session_key: &str) -> Result<PathBuf, String> {
        let root = self
            .sessions_root
            .canonicalize()
            .map_err(|error| format!("cannot resolve Pi sessions root: {error}"))?;
        let path = PathBuf::from(session_key)
            .canonicalize()
            .map_err(|error| format!("cannot resolve Pi session: {error}"))?;
        if !path.starts_with(&root) {
            return Err("Pi session path is outside the configured sessions root".to_string());
        }
        if path.extension().and_then(|ext| ext.to_str()) != Some("jsonl") {
            return Err("Pi session is not a JSONL file".to_string());
        }
        Ok(path)
    }

    fn read_rows(path: &Path) -> Result<Vec<PiRow>, String> {
        let file = File::open(path)
            .map_err(|error| format!("cannot open Pi session {}: {error}", path.display()))?;
        let mut rows = Vec::new();
        for (line_index, line) in BufReader::new(file).lines().enumerate() {
            let raw = line.map_err(|error| format!("cannot read Pi session line: {error}"))?;
            if raw.trim().is_empty() {
                continue;
            }
            let value = serde_json::from_str::<Value>(&raw)
                .map_err(|error| format!("cannot parse Pi session line {line_index}: {error}"))?;
            rows.push(PiRow {
                line_index,
                raw,
                value,
            });
        }
        Ok(rows)
    }

    fn current_chain(rows: &[PiRow]) -> Vec<usize> {
        let mut by_id = HashMap::new();
        for (index, row) in rows.iter().enumerate() {
            if row.value.get("type").and_then(Value::as_str) == Some("session") {
                continue;
            }
            if let Some(id) = row.value.get("id").and_then(Value::as_str) {
                by_id.insert(id.to_string(), index);
            }
        }

        let Some(mut current_id) = rows.iter().rev().find_map(|row| {
            row.value
                .get("id")
                .and_then(Value::as_str)
                .map(str::to_string)
        }) else {
            return Vec::new();
        };

        let mut chain = Vec::new();
        let mut seen = HashSet::new();
        while seen.insert(current_id.clone()) {
            let Some(index) = by_id.get(&current_id).copied() else {
                break;
            };
            chain.push(index);
            let Some(parent_id) = rows[index]
                .value
                .get("parentId")
                .and_then(Value::as_str)
            else {
                break;
            };
            current_id = parent_id.to_string();
        }
        chain.reverse();
        chain
    }

    fn native_summary_for_path(path: &Path, summary: PiSummary) -> NativeSessionSummary {
        let updated_at = fs::metadata(path)
            .and_then(|metadata| metadata.modified())
            .ok()
            .and_then(|time| time.duration_since(UNIX_EPOCH).ok())
            .map(|duration| duration.as_millis() as i64)
            .unwrap_or(0);
        let source_path = display_path(path);
        NativeSessionSummary {
            engine_id: "pi".to_string(),
            session_key: source_path.clone(),
            native_session_id: summary.session_id,
            title: summary.title,
            preview: summary.preview,
            workspace_root: summary.cwd,
            source_path,
            updated_at,
        }
    }

    fn build_edited_bytes(
        source_bytes: &[u8],
        target: &EditTarget,
        new_text: &str,
    ) -> Result<(Vec<u8>, String), String> {
        let text = String::from_utf8(source_bytes.to_vec())
            .map_err(|_| "Pi transcript is not valid UTF-8".to_string())?;
        let mut lines = split_lines_preserving_newlines(&text);
        let line = lines
            .get_mut(target.line_index)
            .ok_or_else(|| "target Pi JSONL line no longer exists".to_string())?;
        let (body, ending) = line_body_and_ending(line);
        if revision(body.as_bytes()) != target.expected_record_hash {
            return Err("target Pi record changed; reload the session before editing".to_string());
        }
        let mut value = serde_json::from_str::<Value>(body)
            .map_err(|error| format!("target Pi JSONL line is invalid: {error}"))?;
        if value.get("type").and_then(Value::as_str) != Some("message") {
            return Err("target Pi record is not a message".to_string());
        }
        if let Some(expected_id) = target.record_uuid.as_deref() {
            if value.get("id").and_then(Value::as_str) != Some(expected_id) {
                return Err("target Pi message id changed; reload the session".to_string());
            }
        }
        let message = value
            .get_mut("message")
            .and_then(Value::as_object_mut)
            .ok_or_else(|| "target Pi record has no message object".to_string())?;
        let role = message
            .get("role")
            .and_then(Value::as_str)
            .unwrap_or_default();
        if !matches!(role, "user" | "assistant") {
            return Err(format!("Pi message role is not editable: {role}"));
        }
        let content = message
            .get_mut("content")
            .ok_or_else(|| "target Pi message has no content".to_string())?;
        let old_text = match (target.field.as_str(), target.content_index) {
            ("content", None) => {
                let old = content
                    .as_str()
                    .ok_or_else(|| "target Pi content is no longer a string".to_string())?
                    .to_string();
                *content = Value::String(new_text.to_string());
                old
            }
            ("text", Some(content_index)) => {
                let item = content
                    .as_array_mut()
                    .and_then(|items| items.get_mut(content_index))
                    .ok_or_else(|| "target Pi content block no longer exists".to_string())?;
                if item.get("type").and_then(Value::as_str) != Some("text") {
                    return Err("only Pi text content blocks are editable".to_string());
                }
                let text = item
                    .get_mut("text")
                    .ok_or_else(|| "target Pi text field no longer exists".to_string())?;
                let old = text
                    .as_str()
                    .ok_or_else(|| "target Pi text field is no longer a string".to_string())?
                    .to_string();
                *text = Value::String(new_text.to_string());
                old
            }
            _ => return Err("unsupported Pi edit target".to_string()),
        };
        let replacement = serde_json::to_string(&value).map_err(|error| error.to_string())?;
        *line = format!("{replacement}{ending}");
        Ok((lines.concat().into_bytes(), old_text))
    }

    fn external_writer_pid(
        session_id: &str,
        source_path: &Path,
        workspace_root: &Path,
    ) -> Option<u32> {
        let current_pid = std::process::id();
        let source_key = normalize_process_path(source_path);
        let workspace_key = normalize_process_path(workspace_root);
        let session_id = session_id.to_ascii_lowercase();
        let system = System::new_all();

        for (pid, process) in system.processes() {
            let pid = pid.as_u32();
            if pid == current_pid {
                continue;
            }
            let command_line = process
                .cmd()
                .iter()
                .map(String::as_str)
                .collect::<Vec<_>>()
                .join(" ");
            let command_key = command_line.replace('\\', "/").to_ascii_lowercase();
            let name = process.name().to_ascii_lowercase();
            let executable = process
                .exe()
                .map(normalize_process_path)
                .unwrap_or_default();
            let looks_like_pi = name == "pi"
                || name == "pi.exe"
                || command_key.contains("pi-coding-agent")
                || command_key.contains("/pi.cmd")
                || executable.ends_with("/pi.exe");
            if !looks_like_pi {
                continue;
            }

            let exact_session = command_key.contains(&source_key)
                || (!session_id.is_empty() && command_key.contains(&session_id));
            let same_workspace = process
                .cwd()
                .map(normalize_process_path)
                .map(|cwd| cwd == workspace_key)
                .unwrap_or(false);
            if exact_session || same_workspace {
                return Some(pid);
            }
        }
        None
    }
}

impl EngineAdapter for PiAdapter {
    fn descriptor(&self) -> EngineDescriptor {
        EngineDescriptor {
            id: "pi".to_string(),
            display_name: "Pi".to_string(),
            storage: NativeStorageKind::Jsonl,
            capabilities: EngineCapabilities {
                read_sessions: true,
                write_sessions: true,
                edit_message: true,
                delete_session: true,
                restore_session: true,
                create_session: true,
                resume_session: true,
                stream: true,
                interrupt: true,
                model_discovery: true,
                ..EngineCapabilities::read_only()
            },
        }
    }

    fn scan_sessions(&self, workspace_root: &Path) -> Result<Vec<NativeSessionSummary>, String> {
        if !self.sessions_root.is_dir() {
            return Ok(Vec::new());
        }

        let expected_workspace = normalize_path_key(workspace_root);
        let mut paths = Vec::new();
        Self::collect_jsonl(&self.sessions_root, &mut paths);
        let mut sessions = Vec::new();

        for path in paths {
            let Some(summary) = Self::read_summary(&path) else {
                continue;
            };
            if !summary.cwd.is_empty()
                && normalize_path_key(Path::new(&summary.cwd)) != expected_workspace
            {
                continue;
            }

            sessions.push(Self::native_summary_for_path(&path, summary));
        }

        sessions.sort_by(|left, right| right.updated_at.cmp(&left.updated_at));
        Ok(sessions)
    }

    fn get_session_detail(
        &self,
        session_key: &str,
        workspace_root: &Path,
    ) -> Result<NativeSessionDetail, String> {
        let path = self.resolve_session_path(session_key)?;
        let summary = Self::read_summary(&path)
            .ok_or_else(|| format!("cannot summarize Pi session: {}", path.display()))?;
        if !summary.cwd.is_empty()
            && normalize_path_key(Path::new(&summary.cwd)) != normalize_path_key(workspace_root)
        {
            return Err("Pi session belongs to a different workspace".to_string());
        }
        let rows = Self::read_rows(&path)?;
        let records = Self::current_chain(&rows)
            .into_iter()
            .filter_map(|index| pi_row_to_record(&rows[index]))
            .collect();
        let source_bytes = fs::read(&path)
            .map_err(|error| format!("cannot read Pi session revision: {error}"))?;
        Ok(NativeSessionDetail {
            summary: Self::native_summary_for_path(&path, summary),
            records,
            source_revision: revision(&source_bytes),
        })
    }

    fn external_writer_pid(
        &self,
        session_key: &str,
        workspace_root: &Path,
    ) -> Result<Option<u32>, String> {
        let path = self.resolve_session_path(session_key)?;
        let summary = Self::read_summary(&path)
            .ok_or_else(|| format!("cannot summarize Pi session: {}", path.display()))?;
        Ok(Self::external_writer_pid(
            &summary.session_id,
            &path,
            workspace_root,
        ))
    }

    fn prepare_file_edit(&self, request: &EngineEditRequest) -> Result<PreparedFileMutation, String> {
        if request.engine_id != "pi" {
            return Err("Pi adapter received a mutation for a different engine".to_string());
        }
        let workspace_root = PathBuf::from(&request.workspace_root);
        if !workspace_root.is_dir() {
            return Err(format!(
                "Pi session workspace no longer exists: {}",
                workspace_root.display()
            ));
        }
        let path = self.resolve_session_path(&request.session_key)?;
        let summary = Self::read_summary(&path)
            .ok_or_else(|| format!("cannot summarize Pi session: {}", path.display()))?;
        if !summary.cwd.is_empty()
            && normalize_path_key(Path::new(&summary.cwd)) != normalize_path_key(&workspace_root)
        {
            return Err("Pi session belongs to a different workspace".to_string());
        }
        if let Some(pid) = Self::external_writer_pid(&summary.session_id, &path, &workspace_root) {
            return Err(format!(
                "external Pi process {pid} may be writing this workspace/session; stop it before editing"
            ));
        }
        let source_bytes = fs::read(&path)
            .map_err(|error| format!("cannot read Pi session before edit: {error}"))?;
        if revision(&source_bytes) != request.expected_source_revision {
            return Err("Pi session changed on disk; reload it before editing".to_string());
        }
        let (candidate_bytes, old_text) =
            Self::build_edited_bytes(&source_bytes, &request.target, &request.new_text)?;
        Ok(PreparedFileMutation {
            engine_id: "pi".to_string(),
            session_key: request.session_key.clone(),
            native_session_id: summary.session_id,
            workspace_root,
            source_path: path,
            target: Some(request.target.clone()),
            source_bytes,
            candidate_bytes,
            old_content: Some(old_text),
            new_content: Some(request.new_text.clone()),
            operation: "edit_message".to_string(),
        })
    }

    fn validate_file_candidate(
        &self,
        mutation: &PreparedFileMutation,
        candidate_path: &Path,
    ) -> Result<(), String> {
        let summary = Self::read_summary(candidate_path)
            .ok_or_else(|| "cannot parse Pi mutation candidate".to_string())?;
        if summary.session_id != mutation.native_session_id {
            return Err("Pi mutation changed the native session id".to_string());
        }
        if !summary.cwd.is_empty()
            && normalize_path_key(Path::new(&summary.cwd))
                != normalize_path_key(&mutation.workspace_root)
        {
            return Err("Pi mutation changed the workspace identity".to_string());
        }
        let rows = Self::read_rows(candidate_path)?;
        if rows.is_empty() {
            return Err("Pi mutation candidate is empty".to_string());
        }
        let chain = Self::current_chain(&rows);
        if chain.is_empty() {
            return Err("Pi mutation candidate has no resolvable current branch".to_string());
        }
        if let Some(target) = mutation.target.as_ref() {
            let expected_id = target.record_uuid.as_deref();
            let target_still_exists = rows.iter().any(|row| {
                row.line_index == target.line_index
                    && expected_id
                        .map(|id| row.value.get("id").and_then(Value::as_str) == Some(id))
                        .unwrap_or(true)
            });
            if !target_still_exists {
                return Err("Pi mutation candidate lost the edited message identity".to_string());
            }
        }
        Ok(())
    }
}

fn message_text(message: &Value) -> String {
    match message.get("content") {
        Some(Value::String(text)) => text.clone(),
        Some(Value::Array(items)) => items
            .iter()
            .filter_map(|item| {
                if item.get("type").and_then(Value::as_str) == Some("text") {
                    item.get("text").and_then(Value::as_str).map(str::to_string)
                } else {
                    None
                }
            })
            .collect::<Vec<_>>()
            .join("\n"),
        _ => String::new(),
    }
}

fn pi_row_to_record(row: &PiRow) -> Option<TranscriptRecord> {
    let entry_type = row
        .value
        .get("type")
        .and_then(Value::as_str)
        .unwrap_or("unknown");
    let uuid = row
        .value
        .get("id")
        .and_then(Value::as_str)
        .map(str::to_string);
    let parent_uuid = row
        .value
        .get("parentId")
        .and_then(Value::as_str)
        .map(str::to_string);
    let timestamp = row
        .value
        .get("timestamp")
        .and_then(Value::as_str)
        .map(str::to_string);
    let mut role = None;
    let mut blocks = Vec::new();
    let mut editable_targets = Vec::new();

    if entry_type == "message" {
        let message = row.value.get("message")?;
        let native_role = message
            .get("role")
            .and_then(Value::as_str)
            .unwrap_or_default();
        match native_role {
            "user" | "assistant" => {
                role = Some(native_role.to_string());
                let (message_blocks, targets) =
                    pi_content_blocks(message, native_role, row.line_index, uuid.as_deref(), &row.raw);
                blocks.extend(message_blocks);
                editable_targets.extend(targets);
            }
            "toolResult" => {
                role = Some("assistant".to_string());
                blocks.push(ContentBlock::ToolResult {
                    tool_use_id: message
                        .get("toolCallId")
                        .and_then(Value::as_str)
                        .unwrap_or_default()
                        .to_string(),
                    output: message.get("content").cloned().unwrap_or(Value::Null),
                    is_error: message
                        .get("isError")
                        .and_then(Value::as_bool)
                        .unwrap_or(false),
                });
            }
            "bashExecution" => {
                role = Some("assistant".to_string());
                let command = message
                    .get("command")
                    .and_then(Value::as_str)
                    .unwrap_or_default();
                let output = message
                    .get("output")
                    .and_then(Value::as_str)
                    .unwrap_or_default();
                blocks.push(ContentBlock::SystemEvent {
                    name: "bash".to_string(),
                    text: format!("{command}\n{output}").trim().to_string(),
                });
            }
            _ => {
                blocks.push(ContentBlock::Unknown {
                    kind: format!("pi-message:{native_role}"),
                    raw: message.clone(),
                });
            }
        }
    } else if matches!(
        entry_type,
        "compaction" | "branch_summary" | "custom_message" | "session_info"
    ) {
        let text = row
            .value
            .get("summary")
            .or_else(|| row.value.get("content"))
            .or_else(|| row.value.get("name"))
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string();
        blocks.push(ContentBlock::SystemEvent {
            name: entry_type.to_string(),
            text,
        });
    } else {
        return None;
    }

    let raw_hash = format!("{:x}", Sha256::digest(row.raw.as_bytes()));
    Some(TranscriptRecord {
        line_index: row.line_index,
        raw_type: entry_type.to_string(),
        uuid,
        parent_uuid,
        timestamp,
        role,
        blocks,
        editable_targets,
        raw_hash,
    })
}

fn pi_content_blocks(
    message: &Value,
    role: &str,
    line_index: usize,
    record_uuid: Option<&str>,
    raw: &str,
) -> (Vec<ContentBlock>, Vec<EditTarget>) {
    let expected_record_hash = revision(raw.as_bytes());
    match message.get("content") {
        Some(Value::String(text)) => (
            vec![ContentBlock::Text {
                text: text.clone(),
                editable: true,
            }],
            vec![EditTarget {
                line_index,
                record_uuid: record_uuid.map(str::to_string),
                content_index: None,
                field: "content".to_string(),
                expected_record_hash,
            }],
        ),
        Some(Value::Array(items)) => {
            let mut blocks = Vec::new();
            let mut targets = Vec::new();
            for (content_index, item) in items.iter().enumerate() {
                match item.get("type").and_then(Value::as_str).unwrap_or("unknown") {
                    "text" => {
                        blocks.push(ContentBlock::Text {
                            text: item
                                .get("text")
                                .and_then(Value::as_str)
                                .unwrap_or_default()
                                .to_string(),
                            editable: true,
                        });
                        targets.push(EditTarget {
                            line_index,
                            record_uuid: record_uuid.map(str::to_string),
                            content_index: Some(content_index),
                            field: "text".to_string(),
                            expected_record_hash: expected_record_hash.clone(),
                        });
                    }
                    "thinking" if role == "assistant" => blocks.push(ContentBlock::Thinking {
                        text: item
                            .get("thinking")
                            .or_else(|| item.get("text"))
                            .and_then(Value::as_str)
                            .unwrap_or_default()
                            .to_string(),
                        editable: false,
                        redacted: false,
                    }),
                    "toolCall" if role == "assistant" => blocks.push(ContentBlock::ToolUse {
                        id: item
                            .get("id")
                            .and_then(Value::as_str)
                            .unwrap_or_default()
                            .to_string(),
                        name: item
                            .get("name")
                            .and_then(Value::as_str)
                            .unwrap_or("tool")
                            .to_string(),
                        input: item.get("arguments").cloned().unwrap_or(Value::Null),
                    }),
                    kind => blocks.push(ContentBlock::Unknown {
                        kind: format!("pi:{kind}"),
                        raw: item.clone(),
                    }),
                }
            }
            (blocks, targets)
        }
        _ => (Vec::new(), Vec::new()),
    }
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

fn normalize_process_path(path: &Path) -> String {
    path.to_string_lossy()
        .replace('\\', "/")
        .trim_end_matches('/')
        .to_ascii_lowercase()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scans_pi_session_without_enabling_source_writes() {
        let root = std::env::temp_dir().join(format!(
            "claudeix-pi-adapter-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .expect("clock")
                .as_nanos()
        ));
        let workspace = root.join("workspace");
        let sessions = root.join("sessions");
        let project = sessions.join("project-key");
        fs::create_dir_all(&workspace).expect("workspace");
        fs::create_dir_all(&project).expect("session dir");
        let session_path = project.join("demo.jsonl");
        fs::write(
            &session_path,
            format!(
                "{{\"type\":\"session\",\"id\":\"pi-s1\",\"cwd\":{}}}\n{{\"type\":\"session_info\",\"id\":\"n1\",\"parentId\":null,\"name\":\"Pi Demo\"}}\n{{\"type\":\"message\",\"id\":\"u1\",\"parentId\":\"n1\",\"message\":{{\"role\":\"user\",\"content\":[{{\"type\":\"text\",\"text\":\"hello pi\"}}]}}}}\n",
                serde_json::to_string(&display_path(&workspace)).expect("path json")
            ),
        )
        .expect("write fixture");

        let adapter = PiAdapter::with_sessions_root(sessions);
        let found = adapter.scan_sessions(&workspace).expect("scan");
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].native_session_id, "pi-s1");
        assert_eq!(found[0].title, "Pi Demo");
        assert_eq!(found[0].preview, "hello pi");
        assert!(adapter.descriptor().capabilities.write_sessions);
        assert!(adapter.descriptor().capabilities.edit_message);

        let detail = adapter
            .get_session_detail(&found[0].session_key, &workspace)
            .expect("detail");
        assert_eq!(detail.records.len(), 2);
        assert_eq!(detail.records[1].role.as_deref(), Some("user"));
        assert_eq!(detail.records[1].visible_text(), "hello pi");
        assert_eq!(detail.records[1].editable_targets.len(), 1);
        assert!(!detail.source_revision.is_empty());

        let request = EngineEditRequest {
            engine_id: "pi".to_string(),
            session_key: found[0].session_key.clone(),
            workspace_root: display_path(&workspace),
            expected_source_revision: detail.source_revision.clone(),
            target: detail.records[1].editable_targets[0].clone(),
            new_text: "edited pi".to_string(),
        };
        let mutation = adapter.prepare_file_edit(&request).expect("prepare edit");
        let candidate_path = project.join("candidate.jsonl");
        fs::write(&candidate_path, &mutation.candidate_bytes).expect("candidate");
        adapter
            .validate_file_candidate(&mutation, &candidate_path)
            .expect("validate candidate");
        assert!(String::from_utf8(mutation.candidate_bytes)
            .expect("utf8")
            .contains("edited pi"));

        fs::remove_dir_all(root).ok();
    }

    #[test]
    fn edited_fixture_is_accepted_by_real_pi_cli_when_requested() {
        if std::env::var("CLAUDEIX_TEST_PI_CLI").ok().as_deref() != Some("1") {
            return;
        }
        #[cfg(not(windows))]
        return;

        #[cfg(windows)]
        {
            use std::process::Command;

            let root = std::env::temp_dir().join(format!(
                "claudeix-pi-cli-{}-{}",
                std::process::id(),
                std::time::SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .expect("clock")
                    .as_nanos()
            ));
            let sessions = root.join("sessions");
            let project = sessions.join("project");
            fs::create_dir_all(&project).expect("project");
            let fixture = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .join("tests/fixtures/pi/v3-basic.jsonl");
            let session_path = project.join("v3-basic.jsonl");
            fs::copy(&fixture, &session_path).expect("copy fixture");

            let workspace = PathBuf::from(r"C:\");
            let adapter = PiAdapter::with_sessions_root(sessions);
            let found = adapter.scan_sessions(&workspace).expect("scan");
            let detail = adapter
                .get_session_detail(&found[0].session_key, &workspace)
                .expect("detail");
            let user = detail
                .records
                .iter()
                .find(|record| record.role.as_deref() == Some("user"))
                .expect("user record");
            let request = EngineEditRequest {
                engine_id: "pi".to_string(),
                session_key: found[0].session_key.clone(),
                workspace_root: display_path(&workspace),
                expected_source_revision: detail.source_revision,
                target: user.editable_targets[0].clone(),
                new_text: "edited fixture user message".to_string(),
            };
            let mutation = adapter.prepare_file_edit(&request).expect("prepare");
            fs::write(&session_path, &mutation.candidate_bytes).expect("write edited fixture");
            adapter
                .validate_file_candidate(&mutation, &session_path)
                .expect("adapter validation");

            let output = Command::new("pi")
                .args([
                    "--session",
                    session_path.to_string_lossy().as_ref(),
                    "--print",
                    "--mode",
                    "json",
                ])
                .env("PI_OFFLINE", "1")
                .output()
                .expect("launch pi");
            assert!(
                output.status.success(),
                "Pi rejected edited session: {}",
                String::from_utf8_lossy(&output.stderr)
            );
            assert!(String::from_utf8_lossy(&output.stdout).contains("claudeix-pi-fixture"));

            fs::remove_dir_all(root).ok();
        }
    }
}
