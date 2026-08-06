use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EditTarget {
    pub line_index: usize,
    pub record_uuid: Option<String>,
    pub content_index: Option<usize>,
    pub field: String,
    pub expected_record_hash: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ContentBlock {
    Text {
        text: String,
        editable: bool,
    },
    Thinking {
        text: String,
        editable: bool,
        redacted: bool,
    },
    ToolUse {
        id: String,
        name: String,
        input: Value,
    },
    ToolResult {
        tool_use_id: String,
        output: Value,
        is_error: bool,
    },
    Image {
        media_type: Option<String>,
    },
    SystemEvent {
        name: String,
        text: String,
    },
    Unknown {
        kind: String,
        raw: Value,
    },
}

impl ContentBlock {
    pub fn visible_text(&self) -> Option<&str> {
        match self {
            Self::Text { text, .. } => Some(text),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TranscriptRecord {
    pub line_index: usize,
    pub raw_type: String,
    pub uuid: Option<String>,
    pub parent_uuid: Option<String>,
    pub timestamp: Option<String>,
    pub role: Option<String>,
    pub blocks: Vec<ContentBlock>,
    pub editable_targets: Vec<EditTarget>,
    pub raw_hash: String,
}

impl TranscriptRecord {
    pub fn visible_text(&self) -> String {
        self.blocks
            .iter()
            .filter_map(ContentBlock::visible_text)
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .collect::<Vec<_>>()
            .join("\n")
    }

    pub fn is_visible_message(&self) -> bool {
        matches!(self.role.as_deref(), Some("user" | "assistant"))
            && (!self.visible_text().is_empty()
                || self.blocks.iter().any(|block| {
                    matches!(
                        block,
                        ContentBlock::ToolUse { .. }
                            | ContentBlock::ToolResult { .. }
                            | ContentBlock::Image { .. }
                    )
                }))
    }
}
