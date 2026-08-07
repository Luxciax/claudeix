use crate::models::edit::TranscriptAuditEntry;
use crate::models::session::SessionSummary;
use crate::models::transcript::EditTarget;
use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::Path;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EditTransactionRecord {
    pub id: String,
    pub session_id: String,
    pub source_path: String,
    pub operation: String,
    pub status: String,
    pub source_hash_before: String,
    pub source_hash_after: String,
    pub backup_path: Option<String>,
    pub target: Option<EditTarget>,
    pub old_content: Option<String>,
    pub new_content: Option<String>,
    pub created_at: u64,
    pub updated_at: u64,
    pub error: Option<String>,
}

fn open(path: &Path) -> Result<Connection, String> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .map_err(|error| format!("cannot create Claudeix data directory {}: {error}", parent.display()))?;
    }
    let connection = Connection::open(path)
        .map_err(|error| format!("cannot open Claudeix database {}: {error}", path.display()))?;
    connection
        .execute_batch(
            "PRAGMA journal_mode=WAL;
             PRAGMA synchronous=FULL;
             PRAGMA foreign_keys=ON;
             CREATE TABLE IF NOT EXISTS edit_transactions(
               id TEXT PRIMARY KEY,
               session_id TEXT NOT NULL,
               source_path TEXT NOT NULL,
               operation TEXT NOT NULL,
               status TEXT NOT NULL,
               source_hash_before TEXT NOT NULL,
               source_hash_after TEXT NOT NULL,
               backup_path TEXT,
               target_json TEXT,
               old_content TEXT,
               new_content TEXT,
               created_at INTEGER NOT NULL,
               updated_at INTEGER NOT NULL,
               error TEXT
             );
             CREATE INDEX IF NOT EXISTS idx_edit_transactions_status ON edit_transactions(status);
             CREATE INDEX IF NOT EXISTS idx_edit_transactions_session ON edit_transactions(session_id, source_path);
             CREATE TABLE IF NOT EXISTS transcript_edits(
               id INTEGER PRIMARY KEY AUTOINCREMENT,
               transaction_id TEXT NOT NULL UNIQUE,
               session_id TEXT NOT NULL,
               source_path TEXT NOT NULL,
               operation TEXT NOT NULL,
               line_index INTEGER,
               content_index INTEGER,
               field_name TEXT,
               old_content TEXT,
               new_content TEXT,
               backup_path TEXT NOT NULL,
               source_hash_before TEXT NOT NULL,
               source_hash_after TEXT NOT NULL,
               created_at INTEGER NOT NULL
             );
             CREATE INDEX IF NOT EXISTS idx_transcript_edits_session ON transcript_edits(session_id, source_path, created_at DESC);",
        )
        .map_err(|error| format!("cannot initialize Claudeix audit database: {error}"))?;
    Ok(connection)
}

fn ensure_session_metadata_schema(connection: &Connection) -> Result<(), String> {
    connection
        .execute_batch(
            "CREATE TABLE IF NOT EXISTS session_metadata(
               session_id TEXT NOT NULL,
               source_path TEXT NOT NULL,
               archived INTEGER NOT NULL DEFAULT 0,
               favorite INTEGER NOT NULL DEFAULT 0,
               updated_at INTEGER NOT NULL,
               PRIMARY KEY(session_id, source_path)
             );",
        )
        .map_err(|error| format!("cannot initialize Claudeix session metadata: {error}"))
}

pub fn apply_session_metadata(path: &Path, summaries: &mut [SessionSummary]) -> Result<(), String> {
    let connection = open(path)?;
    ensure_session_metadata_schema(&connection)?;
    let mut statement = connection
        .prepare("SELECT session_id, source_path, archived, favorite FROM session_metadata")
        .map_err(|error| format!("cannot query Claudeix session metadata: {error}"))?;
    let rows = statement
        .query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, i64>(2)? != 0,
                row.get::<_, i64>(3)? != 0,
            ))
        })
        .map_err(|error| format!("cannot query Claudeix session metadata: {error}"))?;
    let metadata = rows
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| format!("cannot decode Claudeix session metadata: {error}"))?;
    for summary in summaries {
        if let Some((_, _, archived, favorite)) = metadata
            .iter()
            .find(|(session_id, source_path, _, _)| {
                session_id == &summary.session_id && source_path == &summary.source_path
            })
        {
            summary.archived = *archived;
            summary.favorite = *favorite;
        }
    }
    Ok(())
}

pub fn set_session_archived(
    path: &Path,
    session_id: &str,
    source_path: &str,
    archived: bool,
    updated_at: u64,
) -> Result<(), String> {
    let connection = open(path)?;
    ensure_session_metadata_schema(&connection)?;
    connection
        .execute(
            "INSERT INTO session_metadata(session_id, source_path, archived, favorite, updated_at)
             VALUES (?1, ?2, ?3, 0, ?4)
             ON CONFLICT(session_id, source_path)
             DO UPDATE SET archived=excluded.archived, updated_at=excluded.updated_at",
            params![session_id, source_path, archived as i64, updated_at as i64],
        )
        .map_err(|error| format!("cannot update Claudeix session archive state: {error}"))?;
    Ok(())
}

fn target_json(target: &Option<EditTarget>) -> Result<Option<String>, String> {
    target
        .as_ref()
        .map(serde_json::to_string)
        .transpose()
        .map_err(|error| error.to_string())
}

pub fn begin(path: &Path, record: &EditTransactionRecord) -> Result<(), String> {
    let connection = open(path)?;
    connection
        .execute(
            "INSERT INTO edit_transactions(
               id, session_id, source_path, operation, status,
               source_hash_before, source_hash_after, backup_path, target_json,
               old_content, new_content, created_at, updated_at, error
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14)",
            params![
                record.id,
                record.session_id,
                record.source_path,
                record.operation,
                record.status,
                record.source_hash_before,
                record.source_hash_after,
                record.backup_path,
                target_json(&record.target)?,
                record.old_content,
                record.new_content,
                record.created_at as i64,
                record.updated_at as i64,
                record.error,
            ],
        )
        .map_err(|error| format!("cannot create edit transaction: {error}"))?;
    Ok(())
}

pub fn update_status(
    path: &Path,
    transaction_id: &str,
    status: &str,
    backup_path: Option<&str>,
    updated_at: u64,
    error: Option<&str>,
) -> Result<(), String> {
    let connection = open(path)?;
    connection
        .execute(
            "UPDATE edit_transactions
             SET status=?2,
                 backup_path=COALESCE(?3, backup_path),
                 updated_at=?4,
                 error=?5
             WHERE id=?1",
            params![transaction_id, status, backup_path, updated_at as i64, error],
        )
        .map_err(|error| format!("cannot update edit transaction {transaction_id}: {error}"))?;
    Ok(())
}

pub fn commit(path: &Path, record: &EditTransactionRecord, backup_path: &str) -> Result<(), String> {
    let mut connection = open(path)?;
    let transaction = connection
        .transaction()
        .map_err(|error| format!("cannot start audit transaction: {error}"))?;
    let (line_index, content_index, field_name) = record
        .target
        .as_ref()
        .map(|target| {
            (
                Some(target.line_index as i64),
                target.content_index.map(|value| value as i64),
                Some(target.field.as_str()),
            )
        })
        .unwrap_or((None, None, None));
    transaction
        .execute(
            "INSERT OR IGNORE INTO transcript_edits(
               transaction_id, session_id, source_path, operation,
               line_index, content_index, field_name, old_content, new_content,
               backup_path, source_hash_before, source_hash_after, created_at
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)",
            params![
                record.id,
                record.session_id,
                record.source_path,
                record.operation,
                line_index,
                content_index,
                field_name,
                record.old_content,
                record.new_content,
                backup_path,
                record.source_hash_before,
                record.source_hash_after,
                record.created_at as i64,
            ],
        )
        .map_err(|error| format!("cannot append transcript audit record: {error}"))?;
    transaction
        .execute(
            "UPDATE edit_transactions SET status='committed', backup_path=?2, updated_at=?3, error=NULL WHERE id=?1",
            params![record.id, backup_path, record.updated_at as i64],
        )
        .map_err(|error| format!("cannot commit edit transaction state: {error}"))?;
    transaction
        .commit()
        .map_err(|error| format!("cannot commit Claudeix audit transaction: {error}"))?;
    Ok(())
}

fn row_to_transaction(row: &rusqlite::Row<'_>) -> rusqlite::Result<EditTransactionRecord> {
    let target_json: Option<String> = row.get(8)?;
    let target = target_json
        .as_deref()
        .and_then(|value| serde_json::from_str::<EditTarget>(value).ok());
    Ok(EditTransactionRecord {
        id: row.get(0)?,
        session_id: row.get(1)?,
        source_path: row.get(2)?,
        operation: row.get(3)?,
        status: row.get(4)?,
        source_hash_before: row.get(5)?,
        source_hash_after: row.get(6)?,
        backup_path: row.get(7)?,
        target,
        old_content: row.get(9)?,
        new_content: row.get(10)?,
        created_at: row.get::<_, i64>(11)? as u64,
        updated_at: row.get::<_, i64>(12)? as u64,
        error: row.get(13)?,
    })
}

pub fn get(path: &Path, transaction_id: &str) -> Result<Option<EditTransactionRecord>, String> {
    let connection = open(path)?;
    connection
        .query_row(
            "SELECT id, session_id, source_path, operation, status,
                    source_hash_before, source_hash_after, backup_path, target_json,
                    old_content, new_content, created_at, updated_at, error
             FROM edit_transactions WHERE id=?1",
            [transaction_id],
            row_to_transaction,
        )
        .optional()
        .map_err(|error| format!("cannot read edit transaction {transaction_id}: {error}"))
}

pub fn pending(path: &Path) -> Result<Vec<EditTransactionRecord>, String> {
    let connection = open(path)?;
    let mut statement = connection
        .prepare(
            "SELECT id, session_id, source_path, operation, status,
                    source_hash_before, source_hash_after, backup_path, target_json,
                    old_content, new_content, created_at, updated_at, error
             FROM edit_transactions
             WHERE status NOT IN ('committed', 'rolled_back', 'dismissed')
             ORDER BY created_at ASC",
        )
        .map_err(|error| format!("cannot query pending edit transactions: {error}"))?;
    let rows = statement
        .query_map([], row_to_transaction)
        .map_err(|error| format!("cannot query pending edit transactions: {error}"))?;
    rows.collect::<Result<Vec<_>, _>>()
        .map_err(|error| format!("cannot decode pending edit transaction: {error}"))
}

pub fn audit_for_session(
    path: &Path,
    session_id: &str,
    source_path: &str,
) -> Result<Vec<TranscriptAuditEntry>, String> {
    let connection = open(path)?;
    let mut statement = connection
        .prepare(
            "SELECT id, transaction_id, session_id, source_path, operation,
                    line_index, content_index, field_name, old_content, new_content,
                    backup_path, source_hash_before, source_hash_after, created_at
             FROM transcript_edits
             WHERE session_id=?1 AND source_path=?2
             ORDER BY created_at DESC",
        )
        .map_err(|error| format!("cannot query transcript audit records: {error}"))?;
    let rows = statement
        .query_map(params![session_id, source_path], |row| {
            Ok(TranscriptAuditEntry {
                id: row.get(0)?,
                transaction_id: row.get(1)?,
                session_id: row.get(2)?,
                source_path: row.get(3)?,
                operation: row.get(4)?,
                line_index: row.get::<_, Option<i64>>(5)?.map(|value| value as usize),
                content_index: row.get::<_, Option<i64>>(6)?.map(|value| value as usize),
                field_name: row.get(7)?,
                old_content: row.get(8)?,
                new_content: row.get(9)?,
                backup_path: row.get(10)?,
                source_hash_before: row.get(11)?,
                source_hash_after: row.get(12)?,
                created_at: row.get::<_, i64>(13)? as u64,
            })
        })
        .map_err(|error| format!("cannot query transcript audit records: {error}"))?;
    rows.collect::<Result<Vec<_>, _>>()
        .map_err(|error| format!("cannot decode transcript audit record: {error}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    #[test]
    fn persists_transaction_and_audit_atomically() {
        let root = std::env::temp_dir().join(format!(
            "claudeix-audit-test-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let db = root.join("claudeix.db");
        let mut record = EditTransactionRecord {
            id: "tx-1".to_string(),
            session_id: "session-1".to_string(),
            source_path: "C:/session.jsonl".to_string(),
            operation: "edit".to_string(),
            status: "prepared".to_string(),
            source_hash_before: "before".to_string(),
            source_hash_after: "after".to_string(),
            backup_path: None,
            target: None,
            old_content: Some("old".to_string()),
            new_content: Some("new".to_string()),
            created_at: 1,
            updated_at: 1,
            error: None,
        };
        begin(&db, &record).unwrap();
        update_status(&db, &record.id, "backed_up", Some("C:/backup.jsonl"), 2, None).unwrap();
        record.updated_at = 3;
        commit(&db, &record, "C:/backup.jsonl").unwrap();
        assert!(pending(&db).unwrap().is_empty());
        let audit = audit_for_session(&db, "session-1", "C:/session.jsonl").unwrap();
        assert_eq!(audit.len(), 1);
        assert_eq!(audit[0].old_content.as_deref(), Some("old"));
        assert_eq!(get(&db, "tx-1").unwrap().unwrap().status, "committed");
    }

    #[test]
    fn applies_archive_metadata_without_touching_native_session_fields() {
        let root = std::env::temp_dir().join(format!(
            "claudeix-metadata-test-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let db = root.join("claudeix.db");
        let mut summary = SessionSummary {
            key: "key".to_string(),
            session_id: "session-1".to_string(),
            title: "title".to_string(),
            preview: "preview".to_string(),
            workspace_root: "C:/work".to_string(),
            source_path: "C:/sessions/session-1.jsonl".to_string(),
            created_at: 0,
            updated_at: 0,
            message_count: 1,
            model: None,
            archived: false,
            favorite: false,
            trashed: false,
        };
        set_session_archived(
            &db,
            &summary.session_id,
            &summary.source_path,
            true,
            1,
        )
        .unwrap();
        apply_session_metadata(&db, std::slice::from_mut(&mut summary)).unwrap();
        assert!(summary.archived);
        assert!(!summary.favorite);
        set_session_archived(
            &db,
            &summary.session_id,
            &summary.source_path,
            false,
            2,
        )
        .unwrap();
        apply_session_metadata(&db, std::slice::from_mut(&mut summary)).unwrap();
        assert!(!summary.archived);
    }
}
