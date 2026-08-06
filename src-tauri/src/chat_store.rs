//! Chat-session persistence for the desktop app — SQLite at
//! `~/.pi/agent/desktop-chat.sqlite`, owned by the desktop app (never read by
//! the pi CLI). Message payloads are stored as an opaque JSON string so the
//! frontend owns the schema; Rust only indexes metadata for the session list.

use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::fs::{self, File};
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};
use tauri::State;

pub struct ChatDb(Mutex<Option<Connection>>);

impl Default for ChatDb {
    fn default() -> Self {
        Self(Mutex::new(None))
    }
}

fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

fn open_db() -> Result<Connection, String> {
    let path = crate::pi_settings::home_dir()?
        .join(".pi")
        .join("agent")
        .join("desktop-chat.sqlite");
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    }
    let conn = Connection::open(&path).map_err(|e| e.to_string())?;
    let _ = conn.pragma_update(None, "journal_mode", "WAL");
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS chat_sessions (
           id TEXT PRIMARY KEY,
           name TEXT NOT NULL DEFAULT '',
           session_path TEXT NOT NULL DEFAULT '',
           preview TEXT NOT NULL DEFAULT '',
           messages TEXT NOT NULL DEFAULT '[]',
           project_root TEXT NOT NULL DEFAULT '',
           created_at INTEGER NOT NULL,
           updated_at INTEGER NOT NULL
         );",
    )
    .map_err(|e| e.to_string())?;
    migrate_project_root(&conn)?;
    conn.execute_batch(
        "CREATE INDEX IF NOT EXISTS idx_chat_sessions_project
           ON chat_sessions (project_root, updated_at DESC);",
    )
    .map_err(|e| e.to_string())?;
    Ok(conn)
}

/// Databases written before sessions were scoped per project have no
/// `project_root` column. Add it, then backfill every existing row with the
/// last-opened project — those transcripts were, in practice, produced there,
/// and an empty key would hide them from every project's list.
fn migrate_project_root(conn: &Connection) -> Result<(), String> {
    let present: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM pragma_table_info('chat_sessions') WHERE name = 'project_root'",
            [],
            |r| r.get(0),
        )
        .map_err(|e| e.to_string())?;
    if present > 0 {
        return Ok(());
    }
    conn.execute_batch(
        "ALTER TABLE chat_sessions ADD COLUMN project_root TEXT NOT NULL DEFAULT '';",
    )
    .map_err(|e| e.to_string())?;
    if let Some(root) = crate::projects::last_project() {
        conn.execute(
            "UPDATE chat_sessions SET project_root = ?1 WHERE project_root = ''",
            params![crate::projects::project_key(&root)],
        )
        .map_err(|e| e.to_string())?;
    }
    Ok(())
}

/// Run `f` with the (lazily opened) connection.
fn with_db<T>(
    db: &State<'_, ChatDb>,
    f: impl FnOnce(&Connection) -> Result<T, String>,
) -> Result<T, String> {
    let mut guard = db.0.lock().map_err(|e| e.to_string())?;
    if guard.is_none() {
        *guard = Some(open_db()?);
    }
    f(guard.as_ref().expect("connection just opened"))
}

#[derive(Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct ChatSessionMeta {
    pub id: String,
    pub name: String,
    pub session_path: String,
    pub preview: String,
    /// Project root this conversation belongs to (canonical key).
    pub project_root: String,
    pub created_at: i64,
    pub updated_at: i64,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChatSessionSave {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub session_path: String,
    #[serde(default)]
    pub preview: String,
    #[serde(default)]
    pub project_root: String,
    /// serialized ChatMessage[] — opaque to Rust
    pub messages: String,
    pub created_at: i64,
}

fn query_sessions(conn: &Connection, key: &str) -> Result<Vec<ChatSessionMeta>, String> {
    let mut stmt = conn
        .prepare(
            "SELECT id, name, session_path, preview, project_root, created_at, updated_at
             FROM chat_sessions WHERE project_root = ?1 ORDER BY updated_at DESC",
        )
        .map_err(|e| e.to_string())?;
    let rows = stmt
        .query_map(params![key], |r| {
            Ok(ChatSessionMeta {
                id: r.get(0)?,
                name: r.get(1)?,
                session_path: r.get(2)?,
                preview: r.get(3)?,
                project_root: r.get(4)?,
                created_at: r.get(5)?,
                updated_at: r.get(6)?,
            })
        })
        .map_err(|e| e.to_string())?;
    rows.collect::<Result<Vec<_>, _>>().map_err(|e| e.to_string())
}

fn collect_jsonl_files(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect_jsonl_files(&path, out);
        } else if path.extension().and_then(|value| value.to_str()) == Some("jsonl") {
            out.push(path);
        }
    }
}

fn content_text(content: &Value) -> String {
    match content {
        Value::String(value) => value.clone(),
        Value::Array(items) => items
            .iter()
            .filter_map(|item| {
                let kind = item.get("type").and_then(Value::as_str).unwrap_or("");
                matches!(kind, "text" | "input_text" | "output_text")
                    .then(|| item.get("text").and_then(Value::as_str).map(str::to_string))
                    .flatten()
            })
            .collect::<Vec<_>>()
            .join("\n"),
        _ => String::new(),
    }
}

fn assistant_thinking(content: &Value) -> String {
    let Value::Array(items) = content else {
        return String::new();
    };
    items
        .iter()
        .filter_map(|item| {
            let kind = item.get("type").and_then(Value::as_str).unwrap_or("");
            if matches!(kind, "thinking" | "reasoning") {
                item.get("thinking")
                    .or_else(|| item.get("text"))
                    .and_then(Value::as_str)
                    .map(str::to_string)
            } else {
                None
            }
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn assistant_tools(content: &Value) -> Vec<Value> {
    let Value::Array(items) = content else {
        return Vec::new();
    };
    items
        .iter()
        .filter_map(|item| {
            let kind = item.get("type").and_then(Value::as_str).unwrap_or("");
            if !matches!(kind, "toolCall" | "tool_use") {
                return None;
            }
            Some(json!({
                "id": item.get("id").and_then(Value::as_str).unwrap_or("tool"),
                "name": item.get("name").and_then(Value::as_str).unwrap_or("tool"),
                "args": item.get("arguments").or_else(|| item.get("input")).cloned().unwrap_or(Value::Null),
                "status": "done"
            }))
        })
        .collect()
}

fn file_time_ms(path: &Path) -> i64 {
    fs::metadata(path)
        .and_then(|meta| meta.modified())
        .ok()
        .and_then(|time| time.duration_since(UNIX_EPOCH).ok())
        .map(|duration| duration.as_millis() as i64)
        .unwrap_or_else(now_ms)
}

struct NativeSession {
    id: String,
    name: String,
    path: String,
    preview: String,
    messages: String,
    created_at: i64,
    updated_at: i64,
}

fn parse_native_session(path: &Path, project_key: &str) -> Option<NativeSession> {
    let file = File::open(path).ok()?;
    let mut lines = BufReader::new(file).lines();
    let header: Value = serde_json::from_str(&lines.next()?.ok()?).ok()?;
    if header.get("type").and_then(Value::as_str) != Some("session") {
        return None;
    }
    let cwd = header.get("cwd").and_then(Value::as_str).unwrap_or("");
    if crate::projects::project_key(cwd) != project_key {
        return None;
    }

    let session_id = header
        .get("id")
        .and_then(Value::as_str)
        .map(str::to_string)
        .unwrap_or_else(|| path.to_string_lossy().to_string());
    let mut messages = Vec::new();
    let mut name = String::new();

    for line in lines.map_while(Result::ok) {
        let Ok(entry) = serde_json::from_str::<Value>(&line) else {
            continue;
        };
        if entry.get("type").and_then(Value::as_str) == Some("session_info") {
            if let Some(value) = entry.get("name").and_then(Value::as_str) {
                name = value.to_string();
            }
            continue;
        }
        if entry.get("type").and_then(Value::as_str) != Some("message") {
            continue;
        }
        let Some(message) = entry.get("message") else {
            continue;
        };
        let role = message.get("role").and_then(Value::as_str).unwrap_or("");
        if !matches!(role, "user" | "assistant") {
            continue;
        }
        let content = message.get("content");
        let text = content.map(content_text).unwrap_or_default();
        let thinking = if role == "assistant" {
            content.map(assistant_thinking).unwrap_or_default()
        } else {
            String::new()
        };
        let tools = if role == "assistant" {
            content.map(assistant_tools).unwrap_or_default()
        } else {
            Vec::new()
        };
        if text.is_empty() && thinking.is_empty() && tools.is_empty() {
            continue;
        }
        let id = entry
            .get("id")
            .and_then(Value::as_str)
            .map(str::to_string)
            .unwrap_or_else(|| format!("native-{}", messages.len() + 1));
        messages.push(json!({
            "id": id,
            "role": role,
            "text": text,
            "thinking": thinking,
            "tools": tools,
            "streaming": false
        }));
    }

    if name.is_empty() {
        name = messages
            .iter()
            .find(|message| message.get("role").and_then(Value::as_str) == Some("user"))
            .and_then(|message| message.get("text").and_then(Value::as_str))
            .unwrap_or("")
            .chars()
            .take(40)
            .collect();
    }
    let preview = messages
        .iter()
        .rev()
        .find_map(|message| message.get("text").and_then(Value::as_str))
        .unwrap_or("")
        .chars()
        .take(80)
        .collect();
    let updated_at = file_time_ms(path);
    Some(NativeSession {
        id: format!("pi-native:{session_id}"),
        name,
        path: path.to_string_lossy().to_string(),
        preview,
        messages: serde_json::to_string(&messages).ok()?,
        created_at: updated_at,
        updated_at,
    })
}

#[tauri::command]
pub fn chat_sessions_sync_native(
    db: State<'_, ChatDb>,
    project_root: String,
) -> Result<Vec<ChatSessionMeta>, String> {
    let key = crate::projects::project_key(&project_root);
    let session_dir = crate::pi_settings::home_dir()?
        .join(".pi")
        .join("agent")
        .join("sessions");
    let mut files = Vec::new();
    collect_jsonl_files(&session_dir, &mut files);
    let imported: Vec<_> = files
        .iter()
        .filter_map(|path| parse_native_session(path, &key))
        .collect();

    with_db(&db, |conn| {
        for session in imported {
            conn.execute(
                "INSERT OR IGNORE INTO chat_sessions
                 (id, name, session_path, preview, messages, project_root, created_at, updated_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
                params![
                    session.id,
                    session.name,
                    session.path,
                    session.preview,
                    session.messages,
                    key,
                    session.created_at,
                    session.updated_at
                ],
            )
            .map_err(|e| e.to_string())?;
        }
        query_sessions(conn, &key)
    })
}

/// Sessions belonging to one project, most recently updated first (metadata
/// only — no messages). Scoping is by project so opening a project shows just
/// its own history instead of every conversation on the machine.
#[tauri::command]
pub fn chat_sessions_list(
    db: State<'_, ChatDb>,
    project_root: String,
) -> Result<Vec<ChatSessionMeta>, String> {
    let key = crate::projects::project_key(&project_root);
    with_db(&db, |conn| query_sessions(conn, &key))
}

/// Message payload (JSON string) for one session, `None` if it doesn't exist.
#[tauri::command]
pub fn chat_session_load(db: State<'_, ChatDb>, id: String) -> Result<Option<String>, String> {
    with_db(&db, |conn| {
        conn.query_row(
            "SELECT messages FROM chat_sessions WHERE id = ?1",
            params![id],
            |r| r.get::<_, String>(0),
        )
        .optional()
        .map_err(|e| e.to_string())
    })
}

/// Upsert — `updated_at` is stamped server-side so ordering survives clock
/// weirdness in the webview.
#[tauri::command]
pub fn chat_session_save(db: State<'_, ChatDb>, session: ChatSessionSave) -> Result<(), String> {
    let key = crate::projects::project_key(&session.project_root);
    with_db(&db, |conn| {
        conn.execute(
            "INSERT INTO chat_sessions (id, name, session_path, preview, messages, project_root, created_at, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)
             ON CONFLICT(id) DO UPDATE SET
               name = ?2, session_path = ?3, preview = ?4, messages = ?5, project_root = ?6, updated_at = ?8",
            params![
                session.id,
                session.name,
                session.session_path,
                session.preview,
                session.messages,
                key,
                session.created_at,
                now_ms()
            ],
        )
        .map(|_| ())
        .map_err(|e| e.to_string())
    })
}

#[tauri::command]
pub fn chat_session_rename(db: State<'_, ChatDb>, id: String, name: String) -> Result<(), String> {
    with_db(&db, |conn| {
        conn.execute(
            "UPDATE chat_sessions SET name = ?2, updated_at = ?3 WHERE id = ?1",
            params![id, name, now_ms()],
        )
        .map(|_| ())
        .map_err(|e| e.to_string())
    })
}

#[tauri::command]
pub fn chat_session_delete(db: State<'_, ChatDb>, id: String) -> Result<(), String> {
    with_db(&db, |conn| {
        conn.execute("DELETE FROM chat_sessions WHERE id = ?1", params![id])
            .map(|_| ())
            .map_err(|e| e.to_string())
    })
}
