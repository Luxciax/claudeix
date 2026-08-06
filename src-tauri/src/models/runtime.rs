use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum RuntimeStatus {
    Starting,
    Running,
    Stopping,
    Completed,
    Failed,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RuntimeStartRequest {
    pub workspace_root: String,
    pub session_key: Option<String>,
    pub prompt: String,
    pub permission_mode: String,
    pub model: Option<String>,
    pub effort: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RuntimeSnapshot {
    pub runtime_id: String,
    pub session_key: Option<String>,
    pub session_id: Option<String>,
    pub workspace_root: String,
    pub status: RuntimeStatus,
    pub pid: u32,
    pub started_at: u64,
    pub exit_code: Option<i32>,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(tag = "type", rename_all = "snake_case", rename_all_fields = "camelCase")]
pub enum RuntimeEvent {
    RuntimeStarted {
        runtime_id: String,
        session_key: Option<String>,
        pid: u32,
    },
    SessionIdentified {
        runtime_id: String,
        session_id: String,
        session_key: Option<String>,
    },
    TextDelta {
        runtime_id: String,
        message_id: String,
        text: String,
    },
    ToolStarted {
        runtime_id: String,
        tool_use_id: String,
        name: String,
        input: serde_json::Value,
    },
    ToolFinished {
        runtime_id: String,
        tool_use_id: String,
        output: serde_json::Value,
        is_error: bool,
    },
    Usage {
        runtime_id: String,
        input_tokens: Option<u64>,
        output_tokens: Option<u64>,
        cost_usd: Option<f64>,
    },
    Stderr {
        runtime_id: String,
        text: String,
    },
    RuntimeCompleted {
        runtime_id: String,
        exit_code: Option<i32>,
        session_id: Option<String>,
        session_key: Option<String>,
    },
    RuntimeFailed {
        runtime_id: String,
        message: String,
        recoverable: bool,
        session_id: Option<String>,
        session_key: Option<String>,
    },
}
