use crate::models::runtime::RuntimeEvent;
use serde_json::Value;
use std::collections::HashSet;

pub struct RuntimeEventParser {
    runtime_id: String,
    session_id: Option<String>,
    emitted_text: bool,
    seen_tools: HashSet<String>,
}

impl RuntimeEventParser {
    pub fn new(runtime_id: impl Into<String>) -> Self {
        Self {
            runtime_id: runtime_id.into(),
            session_id: None,
            emitted_text: false,
            seen_tools: HashSet::new(),
        }
    }

    pub fn parse_line(&mut self, raw: &str, session_key: Option<&str>) -> Vec<RuntimeEvent> {
        let Ok(value) = serde_json::from_str::<Value>(raw) else {
            return Vec::new();
        };
        let mut events = Vec::new();
        if let Some(session_id) = find_session_id(&value) {
            if self.session_id.as_deref() != Some(session_id) {
                self.session_id = Some(session_id.to_string());
                events.push(RuntimeEvent::SessionIdentified {
                    runtime_id: self.runtime_id.clone(),
                    session_id: session_id.to_string(),
                    session_key: session_key.map(str::to_string),
                });
            }
        }

        if let Some(stream_event) = value.get("event") {
            self.parse_stream_event(stream_event, &mut events);
        }

        match value.get("type").and_then(Value::as_str).unwrap_or_default() {
            "control_request" => self.parse_control_request(&value, &mut events),
            "assistant" => self.parse_assistant_message(&value, &mut events),
            "user" => self.parse_user_message(&value, &mut events),
            "result" => self.parse_result(&value, &mut events),
            _ => {}
        }

        if let Some(usage) = find_usage(&value) {
            if usage.input_tokens.is_some() || usage.output_tokens.is_some() || usage.cost_usd.is_some() {
                events.push(RuntimeEvent::Usage {
                    runtime_id: self.runtime_id.clone(),
                    input_tokens: usage.input_tokens,
                    output_tokens: usage.output_tokens,
                    cost_usd: usage.cost_usd,
                });
            }
        }
        events
    }

    fn parse_control_request(&self, value: &Value, events: &mut Vec<RuntimeEvent>) {
        let Some(request) = value.get("request") else {
            return;
        };
        if request.get("subtype").and_then(Value::as_str) != Some("can_use_tool") {
            return;
        }
        let Some(request_id) = value.get("request_id").and_then(Value::as_str) else {
            return;
        };
        events.push(RuntimeEvent::PermissionRequested {
            runtime_id: self.runtime_id.clone(),
            request_id: request_id.to_string(),
            tool_use_id: request
                .get("tool_use_id")
                .and_then(Value::as_str)
                .map(str::to_string),
            tool: request
                .get("tool_name")
                .and_then(Value::as_str)
                .unwrap_or("Tool")
                .to_string(),
            input: request.get("input").cloned().unwrap_or(Value::Null),
            suggestions: request
                .get("permission_suggestions")
                .cloned()
                .unwrap_or(Value::Null),
            title: request
                .get("title")
                .and_then(Value::as_str)
                .map(str::to_string),
            description: request
                .get("description")
                .and_then(Value::as_str)
                .map(str::to_string),
            decision_reason: request
                .get("decision_reason")
                .and_then(Value::as_str)
                .map(str::to_string),
        });
    }

    fn parse_stream_event(&mut self, value: &Value, events: &mut Vec<RuntimeEvent>) {
        let event_type = value.get("type").and_then(Value::as_str).unwrap_or_default();
        if event_type != "content_block_delta" {
            return;
        }
        let Some(delta) = value.get("delta") else {
            return;
        };
        if let Some(text) = delta.get("text").and_then(Value::as_str) {
            if !text.is_empty() {
                self.emitted_text = true;
                events.push(RuntimeEvent::TextDelta {
                    runtime_id: self.runtime_id.clone(),
                    message_id: "assistant-current".to_string(),
                    text: text.to_string(),
                });
            }
        }
    }

    fn parse_assistant_message(&mut self, value: &Value, events: &mut Vec<RuntimeEvent>) {
        let content = value
            .get("message")
            .and_then(|message| message.get("content"))
            .and_then(Value::as_array);
        let Some(content) = content else {
            return;
        };
        for block in content {
            match block.get("type").and_then(Value::as_str).unwrap_or_default() {
                "text" if !self.emitted_text => {
                    if let Some(text) = block.get("text").and_then(Value::as_str) {
                        if !text.is_empty() {
                            self.emitted_text = true;
                            events.push(RuntimeEvent::TextDelta {
                                runtime_id: self.runtime_id.clone(),
                                message_id: "assistant-current".to_string(),
                                text: text.to_string(),
                            });
                        }
                    }
                }
                "tool_use" => {
                    let Some(id) = block.get("id").and_then(Value::as_str) else {
                        continue;
                    };
                    if !self.seen_tools.insert(id.to_string()) {
                        continue;
                    }
                    events.push(RuntimeEvent::ToolStarted {
                        runtime_id: self.runtime_id.clone(),
                        tool_use_id: id.to_string(),
                        name: block
                            .get("name")
                            .and_then(Value::as_str)
                            .unwrap_or("Tool")
                            .to_string(),
                        input: block.get("input").cloned().unwrap_or(Value::Null),
                    });
                }
                _ => {}
            }
        }
    }

    fn parse_user_message(&mut self, value: &Value, events: &mut Vec<RuntimeEvent>) {
        let content = value
            .get("message")
            .and_then(|message| message.get("content"))
            .and_then(Value::as_array);
        let Some(content) = content else {
            return;
        };
        for block in content {
            if block.get("type").and_then(Value::as_str) != Some("tool_result") {
                continue;
            }
            let Some(tool_use_id) = block.get("tool_use_id").and_then(Value::as_str) else {
                continue;
            };
            events.push(RuntimeEvent::ToolFinished {
                runtime_id: self.runtime_id.clone(),
                tool_use_id: tool_use_id.to_string(),
                output: block.get("content").cloned().unwrap_or(Value::Null),
                is_error: block.get("is_error").and_then(Value::as_bool).unwrap_or(false),
            });
        }
    }

    fn parse_result(&mut self, value: &Value, events: &mut Vec<RuntimeEvent>) {
        if self.emitted_text {
            return;
        }
        if let Some(text) = value.get("result").and_then(Value::as_str) {
            if !text.is_empty() {
                self.emitted_text = true;
                events.push(RuntimeEvent::TextDelta {
                    runtime_id: self.runtime_id.clone(),
                    message_id: "assistant-current".to_string(),
                    text: text.to_string(),
                });
            }
        }
    }
}

fn find_session_id(value: &Value) -> Option<&str> {
    value
        .get("session_id")
        .or_else(|| value.get("sessionId"))
        .and_then(Value::as_str)
        .or_else(|| {
            value
                .get("message")
                .and_then(|message| message.get("session_id"))
                .and_then(Value::as_str)
        })
}

struct Usage {
    input_tokens: Option<u64>,
    output_tokens: Option<u64>,
    cost_usd: Option<f64>,
}

fn find_usage(value: &Value) -> Option<Usage> {
    let usage = value
        .get("usage")
        .or_else(|| value.get("message").and_then(|message| message.get("usage")))
        .or_else(|| value.get("event").and_then(|event| event.get("usage")));
    let input_tokens = usage
        .and_then(|usage| usage.get("input_tokens"))
        .and_then(Value::as_u64);
    let output_tokens = usage
        .and_then(|usage| usage.get("output_tokens"))
        .and_then(Value::as_u64);
    let cost_usd = value
        .get("total_cost_usd")
        .or_else(|| value.get("cost_usd"))
        .and_then(Value::as_f64);
    if input_tokens.is_none() && output_tokens.is_none() && cost_usd.is_none() {
        None
    } else {
        Some(Usage {
            input_tokens,
            output_tokens,
            cost_usd,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_stream_text_session_and_usage_without_duplicate_final_text() {
        let mut parser = RuntimeEventParser::new("runtime-1");
        let first = parser.parse_line(
            r#"{"type":"system","subtype":"init","session_id":"session-1"}"#,
            Some("key-1"),
        );
        assert!(matches!(first[0], RuntimeEvent::SessionIdentified { .. }));

        let delta = parser.parse_line(
            r#"{"type":"stream_event","event":{"type":"content_block_delta","delta":{"type":"text_delta","text":"hello"}}}"#,
            Some("key-1"),
        );
        assert!(matches!(delta[0], RuntimeEvent::TextDelta { .. }));

        let final_message = parser.parse_line(
            r#"{"type":"assistant","message":{"content":[{"type":"text","text":"hello"}],"usage":{"input_tokens":5,"output_tokens":2}}}"#,
            Some("key-1"),
        );
        assert_eq!(final_message.len(), 1);
        assert!(matches!(final_message[0], RuntimeEvent::Usage { .. }));
    }

    #[test]
    fn parses_tool_start_and_result() {
        let mut parser = RuntimeEventParser::new("runtime-2");
        let start = parser.parse_line(
            r#"{"type":"assistant","message":{"content":[{"type":"tool_use","id":"tool-1","name":"Read","input":{"file_path":"a.txt"}}]}}"#,
            None,
        );
        assert!(matches!(start[0], RuntimeEvent::ToolStarted { .. }));
        let finish = parser.parse_line(
            r#"{"type":"user","message":{"content":[{"type":"tool_result","tool_use_id":"tool-1","content":"done"}]}}"#,
            None,
        );
        assert!(matches!(finish[0], RuntimeEvent::ToolFinished { .. }));
    }

    #[test]
    fn parses_permission_request() {
        let mut parser = RuntimeEventParser::new("runtime-3");
        let events = parser.parse_line(
            r#"{"type":"control_request","request_id":"permission-1","request":{"subtype":"can_use_tool","tool_name":"Bash","tool_use_id":"tool-1","input":{"command":"git status"},"permission_suggestions":[]}}"#,
            Some("key-1"),
        );
        assert!(matches!(
            events.first(),
            Some(RuntimeEvent::PermissionRequested { request_id, tool, .. })
                if request_id == "permission-1" && tool == "Bash"
        ));
    }
}
