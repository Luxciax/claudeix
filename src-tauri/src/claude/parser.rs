use crate::app_paths::canonical_or_original;
use crate::models::session::{ParseDiagnostic, SessionDetail, SessionIdentity, SessionSummary};
use crate::models::transcript::{ContentBlock, EditTarget, TranscriptRecord};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::collections::{HashMap, HashSet};
use std::fs::{self, File};
use std::io::{BufRead, BufReader};
use std::path::Path;
use std::time::UNIX_EPOCH;

fn sha256(value: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(value);
    format!("{:x}", hasher.finalize())
}

fn value_string(value: &Value, names: &[&str]) -> Option<String> {
    names
        .iter()
        .find_map(|name| value.get(*name).and_then(Value::as_str))
        .map(ToOwned::to_owned)
}

fn private_event(text: &str) -> Option<ContentBlock> {
    const TAGS: &[&str] = &[
        "system-reminder",
        "ide_opened_file",
        "ide_selection",
        "task-notification",
        "user-prompt-submit-hook",
        "persisted-output",
        "tool_use_error",
        "local-command-stdout",
    ];
    let trimmed = text.trim();
    for tag in TAGS {
        let open = format!("<{tag}>");
        let close = format!("</{tag}>");
        if trimmed.starts_with(&open) && trimmed.ends_with(&close) {
            let body = trimmed[open.len()..trimmed.len() - close.len()]
                .trim()
                .to_string();
            return Some(ContentBlock::SystemEvent {
                name: (*tag).to_string(),
                text: body,
            });
        }
    }
    None
}

fn parse_string_content(
    text: &str,
    role: Option<&str>,
    line_index: usize,
    uuid: &Option<String>,
    raw_hash: &str,
    targets: &mut Vec<EditTarget>,
) -> ContentBlock {
    if let Some(event) = private_event(text) {
        return event;
    }
    let editable = role == Some("user");
    if editable {
        targets.push(EditTarget {
            line_index,
            record_uuid: uuid.clone(),
            content_index: None,
            field: "content".into(),
            expected_record_hash: raw_hash.to_string(),
        });
    }
    ContentBlock::Text {
        text: text.to_string(),
        editable,
    }
}

fn parse_content_array(
    items: &[Value],
    role: Option<&str>,
    line_index: usize,
    uuid: &Option<String>,
    raw_hash: &str,
    targets: &mut Vec<EditTarget>,
) -> Vec<ContentBlock> {
    items
        .iter()
        .enumerate()
        .map(|(content_index, item)| {
            let kind = item.get("type").and_then(Value::as_str).unwrap_or("unknown");
            match kind {
                "text" => {
                    let text = item.get("text").and_then(Value::as_str).unwrap_or("");
                    if let Some(event) = private_event(text) {
                        return event;
                    }
                    let editable = matches!(role, Some("user" | "assistant"));
                    if editable {
                        targets.push(EditTarget {
                            line_index,
                            record_uuid: uuid.clone(),
                            content_index: Some(content_index),
                            field: "text".into(),
                            expected_record_hash: raw_hash.to_string(),
                        });
                    }
                    ContentBlock::Text {
                        text: text.to_string(),
                        editable,
                    }
                }
                "thinking" => ContentBlock::Thinking {
                    text: item
                        .get("thinking")
                        .or_else(|| item.get("text"))
                        .and_then(Value::as_str)
                        .unwrap_or("")
                        .to_string(),
                    editable: false,
                    redacted: false,
                },
                "redacted_thinking" => ContentBlock::Thinking {
                    text: "[redacted thinking]".into(),
                    editable: false,
                    redacted: true,
                },
                "tool_use" => ContentBlock::ToolUse {
                    id: item
                        .get("id")
                        .and_then(Value::as_str)
                        .unwrap_or("")
                        .to_string(),
                    name: item
                        .get("name")
                        .and_then(Value::as_str)
                        .unwrap_or("Unknown tool")
                        .to_string(),
                    input: item.get("input").cloned().unwrap_or(Value::Null),
                },
                "tool_result" => ContentBlock::ToolResult {
                    tool_use_id: item
                        .get("tool_use_id")
                        .and_then(Value::as_str)
                        .unwrap_or("")
                        .to_string(),
                    output: item.get("content").cloned().unwrap_or(Value::Null),
                    is_error: item
                        .get("is_error")
                        .and_then(Value::as_bool)
                        .unwrap_or(false),
                },
                "image" => ContentBlock::Image {
                    media_type: item
                        .get("source")
                        .and_then(|source| source.get("media_type"))
                        .and_then(Value::as_str)
                        .map(ToOwned::to_owned),
                },
                _ => ContentBlock::Unknown {
                    kind: kind.to_string(),
                    raw: item.clone(),
                },
            }
        })
        .collect()
}

fn parse_record(line_index: usize, raw: &str, value: Value) -> TranscriptRecord {
    let raw_hash = sha256(raw.as_bytes());
    let raw_type = value
        .get("type")
        .and_then(Value::as_str)
        .unwrap_or("unknown")
        .to_string();
    let uuid = value_string(&value, &["uuid", "id"]);
    let parent_uuid = value_string(&value, &["parentUuid", "parent_uuid"]);
    let timestamp = value_string(&value, &["timestamp", "createdAt", "created_at"]);
    let message = value.get("message").filter(|value| value.is_object());
    let role = message
        .and_then(|message| message.get("role"))
        .and_then(Value::as_str)
        .map(ToOwned::to_owned)
        .or_else(|| match raw_type.as_str() {
            "user" | "human" => Some("user".into()),
            "assistant" => Some("assistant".into()),
            _ => None,
        });
    let content = message
        .and_then(|message| message.get("content"))
        .or_else(|| value.get("content"));
    let mut editable_targets = Vec::new();
    let blocks = match content {
        Some(Value::String(text)) => vec![parse_string_content(
            text,
            role.as_deref(),
            line_index,
            &uuid,
            &raw_hash,
            &mut editable_targets,
        )],
        Some(Value::Array(items)) => parse_content_array(
            items,
            role.as_deref(),
            line_index,
            &uuid,
            &raw_hash,
            &mut editable_targets,
        ),
        _ => Vec::new(),
    };

    TranscriptRecord {
        line_index,
        raw_type,
        uuid,
        parent_uuid,
        timestamp,
        role,
        blocks,
        editable_targets,
        raw_hash,
    }
}

fn metadata_times(path: &Path) -> (i64, i64) {
    let Ok(metadata) = fs::metadata(path) else {
        return (0, 0);
    };
    let created = metadata
        .created()
        .or_else(|_| metadata.modified())
        .ok()
        .and_then(|time| time.duration_since(UNIX_EPOCH).ok())
        .map(|duration| duration.as_millis() as i64)
        .unwrap_or(0);
    let updated = metadata
        .modified()
        .ok()
        .and_then(|time| time.duration_since(UNIX_EPOCH).ok())
        .map(|duration| duration.as_millis() as i64)
        .unwrap_or(created);
    (created, updated)
}

fn first_string(value: &Value, names: &[&str]) -> Option<String> {
    names
        .iter()
        .find_map(|name| value.get(*name).and_then(Value::as_str))
        .map(ToOwned::to_owned)
}

pub fn parse_session_file(
    path: &Path,
    config_dir: &Path,
    expected_workspace: Option<&Path>,
) -> Result<SessionDetail, String> {
    let file = File::open(path)
        .map_err(|error| format!("cannot open Claude session {}: {error}", path.display()))?;
    let mut records = Vec::new();
    let mut diagnostics = Vec::new();
    let mut session_id = String::new();
    let mut workspace_root = None;
    let mut model = None;
    let mut file_hasher = Sha256::new();

    for (line_index, line) in BufReader::new(file).lines().enumerate() {
        let raw = match line {
            Ok(line) => line,
            Err(error) => {
                diagnostics.push(ParseDiagnostic {
                    line_index,
                    message: format!("cannot read line: {error}"),
                });
                continue;
            }
        };
        file_hasher.update(raw.as_bytes());
        file_hasher.update(b"\n");
        if raw.trim().is_empty() {
            continue;
        }
        let value = match serde_json::from_str::<Value>(&raw) {
            Ok(value) => value,
            Err(error) => {
                diagnostics.push(ParseDiagnostic {
                    line_index,
                    message: format!("invalid JSON: {error}"),
                });
                continue;
            }
        };
        if session_id.is_empty() {
            session_id = first_string(&value, &["sessionId", "session_id"]).unwrap_or_default();
        }
        if workspace_root.is_none() {
            workspace_root = first_string(&value, &["cwd", "workspaceRoot", "workspace_root"])
                .map(std::path::PathBuf::from);
        }
        if model.is_none() {
            model = value
                .get("message")
                .and_then(|message| message.get("model"))
                .and_then(Value::as_str)
                .map(ToOwned::to_owned);
        }
        records.push(parse_record(line_index, &raw, value));
    }

    if session_id.is_empty() {
        session_id = path
            .file_stem()
            .and_then(|value| value.to_str())
            .unwrap_or("unknown-session")
            .to_string();
    }
    let workspace_root = workspace_root.unwrap_or_else(|| {
        expected_workspace
            .map(ToOwned::to_owned)
            .unwrap_or_default()
    });
    if let Some(expected) = expected_workspace {
        if !crate::models::session::path_matches(&workspace_root, expected) {
            return Err("session belongs to a different workspace".into());
        }
    }

    let source_path = canonical_or_original(path);
    let identity = SessionIdentity {
        config_dir: canonical_or_original(config_dir),
        workspace_root: canonical_or_original(&workspace_root),
        session_id,
        source_path,
    };
    let visible: Vec<_> = records
        .iter()
        .filter(|record| record.is_visible_message())
        .collect();
    let title = visible
        .iter()
        .find(|record| record.role.as_deref() == Some("user"))
        .map(|record| record.visible_text())
        .filter(|text| !text.is_empty())
        .map(|text| text.chars().take(60).collect())
        .unwrap_or_else(|| "未命名会话".into());
    let preview = visible
        .last()
        .map(|record| record.visible_text())
        .filter(|text| !text.is_empty())
        .map(|text| text.chars().take(140).collect())
        .unwrap_or_default();
    let (created_at, updated_at) = metadata_times(path);
    let summary = SessionSummary::from_identity(
        &identity,
        title,
        preview,
        created_at,
        updated_at,
        visible.len(),
        model,
    );

    let uuids: HashSet<_> = records.iter().filter_map(|record| record.uuid.clone()).collect();
    let mut child_counts: HashMap<String, usize> = HashMap::new();
    for parent in records.iter().filter_map(|record| record.parent_uuid.as_ref()) {
        if uuids.contains(parent) {
            *child_counts.entry(parent.clone()).or_default() += 1;
        }
    }
    let has_branches = child_counts.values().any(|count| *count > 1);

    Ok(SessionDetail {
        summary,
        records,
        diagnostics,
        file_fingerprint: format!("{:x}", file_hasher.finalize()),
        has_branches,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    fn fixture(lines: &[&str]) -> (std::path::PathBuf, std::path::PathBuf) {
        let root = std::env::temp_dir().join(format!(
            "claudeix-parser-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir_all(&root).unwrap();
        let path = root.join("session.jsonl");
        let mut file = File::create(&path).unwrap();
        for line in lines {
            writeln!(file, "{line}").unwrap();
        }
        (root, path)
    }

    #[test]
    fn parses_text_tools_and_bad_lines() {
        let (root, path) = fixture(&[
            r#"{"type":"user","sessionId":"s1","cwd":"C:/work","uuid":"u1","message":{"role":"user","content":"hello"}}"#,
            r#"{"type":"assistant","sessionId":"s1","cwd":"C:/work","uuid":"a1","parentUuid":"u1","message":{"role":"assistant","model":"claude-test","content":[{"type":"text","text":"working"},{"type":"tool_use","id":"tool1","name":"Read","input":{"file_path":"a.txt"}}]}}"#,
            "not-json",
            r#"{"type":"user","sessionId":"s1","cwd":"C:/work","uuid":"u2","parentUuid":"a1","message":{"role":"user","content":[{"type":"tool_result","tool_use_id":"tool1","content":"done"}]}}"#,
        ]);
        let detail = parse_session_file(&path, &root, Some(Path::new("C:/work"))).unwrap();
        assert_eq!(detail.summary.session_id, "s1");
        assert_eq!(detail.summary.title, "hello");
        assert_eq!(detail.summary.model.as_deref(), Some("claude-test"));
        assert_eq!(detail.diagnostics.len(), 1);
        assert!(matches!(detail.records[1].blocks[1], ContentBlock::ToolUse { .. }));
        assert!(matches!(detail.records[2].blocks[0], ContentBlock::ToolResult { .. }));
        assert_eq!(detail.records[0].editable_targets.len(), 1);
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn keeps_unknown_and_thinking_blocks() {
        let (root, path) = fixture(&[
            r#"{"type":"assistant","sessionId":"s2","cwd":"C:/work","message":{"role":"assistant","content":[{"type":"thinking","thinking":"secret"},{"type":"future_block","value":1}]}}"#,
        ]);
        let detail = parse_session_file(&path, &root, Some(Path::new("C:/work"))).unwrap();
        assert!(matches!(detail.records[0].blocks[0], ContentBlock::Thinking { .. }));
        assert!(matches!(detail.records[0].blocks[1], ContentBlock::Unknown { .. }));
        let _ = fs::remove_dir_all(root);
    }
}
