use std::{path::Path, time::Duration};

use chrono::{SecondsFormat, Utc};
use rusqlite::{Connection, OptionalExtension, Transaction, TransactionBehavior, params};
use sha2::{Digest, Sha256};
use uuid::Uuid;

use crate::domain::{AppError, HumanDecision, Message, MessageStatus, Task, TaskEvent, TaskStatus};

const CURRENT_SCHEMA_VERSION: i64 = 1;

pub struct Store {
    connection: Connection,
}

impl Store {
    pub fn open(path: impl AsRef<Path>) -> Result<Self, AppError> {
        if let Some(parent) = path.as_ref().parent()
            && !parent.as_os_str().is_empty()
        {
            std::fs::create_dir_all(parent).map_err(|error| {
                AppError::InvalidInput(format!("cannot create database directory: {error}"))
            })?;
        }
        let mut connection = Connection::open(path)?;
        connection.busy_timeout(Duration::from_secs(5))?;
        connection.execute_batch("PRAGMA foreign_keys = ON; PRAGMA journal_mode = WAL;")?;
        migrate(&mut connection)?;
        Ok(Self { connection })
    }

    pub fn create_task(
        &mut self,
        title: &str,
        description: &str,
        actor: &str,
    ) -> Result<Task, AppError> {
        if title.trim().is_empty() || actor.trim().is_empty() {
            return Err(AppError::InvalidInput(
                "title and actor must not be empty".into(),
            ));
        }
        validate_length("title", title, 160)?;
        validate_length("description", description, 4_000)?;
        validate_length("actor", actor, 120)?;
        let task_id = Uuid::new_v4().to_string();
        let now = now();
        let transaction = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        transaction.execute(
            "INSERT INTO tasks (task_id, title, description, status, created_at, updated_at)
             VALUES (?1, ?2, ?3, 'open', ?4, ?4)",
            params![task_id, title.trim(), description, now],
        )?;
        append_event(
            &transaction,
            &task_id,
            "task_created",
            actor,
            &task_id,
            serde_json::json!({ "status": "open" }),
        )?;
        transaction.commit()?;
        self.get_task(&task_id)?
            .ok_or_else(|| AppError::TaskNotFound(task_id))
    }

    pub fn get_task(&self, task_id: &str) -> Result<Option<Task>, AppError> {
        self.connection
            .query_row(
                "SELECT t.task_id, t.title, t.description, t.status,
                        EXISTS(SELECT 1 FROM human_decisions d WHERE d.task_id = t.task_id AND d.status = 'pending'),
                        t.created_at, t.updated_at
                 FROM tasks t WHERE t.task_id = ?1",
                [task_id],
                row_to_task,
            )
            .optional()
            .map_err(AppError::from)
    }

    pub fn list_tasks(&self) -> Result<Vec<Task>, AppError> {
        let mut statement = self.connection.prepare(
            "SELECT t.task_id, t.title, t.description, t.status,
                    EXISTS(SELECT 1 FROM human_decisions d WHERE d.task_id = t.task_id AND d.status = 'pending'),
                    t.created_at, t.updated_at
             FROM tasks t ORDER BY t.updated_at DESC, t.task_id ASC",
        )?;
        let rows = statement.query_map([], row_to_task)?;
        rows.collect::<Result<Vec<_>, _>>().map_err(AppError::from)
    }

    pub fn get_pending_decision(&self, task_id: &str) -> Result<Option<HumanDecision>, AppError> {
        self.connection
            .query_row(
                "SELECT decision_id, task_id, question, requested_by, status, created_at
                 FROM human_decisions WHERE task_id = ?1 AND status = 'pending'",
                [task_id],
                |row| {
                    Ok(HumanDecision {
                        decision_id: row.get(0)?,
                        task_id: row.get(1)?,
                        question: row.get(2)?,
                        requested_by: row.get(3)?,
                        status: row.get(4)?,
                        created_at: row.get(5)?,
                    })
                },
            )
            .optional()
            .map_err(AppError::from)
    }

    pub fn transition_task(
        &mut self,
        task_id: &str,
        expected: TaskStatus,
        next: TaskStatus,
        actor: &str,
        correlation_id: &str,
    ) -> Result<Task, AppError> {
        if !expected.can_transition_to(next) {
            return Err(AppError::InvalidTransition {
                from: expected.to_string(),
                to: next.to_string(),
            });
        }
        if actor.trim().is_empty() || correlation_id.trim().is_empty() {
            return Err(AppError::InvalidInput(
                "actor and correlation_id must not be empty".into(),
            ));
        }
        validate_length("actor", actor, 120)?;
        validate_length("correlation_id", correlation_id, 128)?;
        if next == TaskStatus::Approved && actor != "human:cesar" {
            return Err(AppError::HumanApprovalRequired);
        }

        let transaction = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let current: Option<String> = transaction
            .query_row(
                "SELECT status FROM tasks WHERE task_id = ?1",
                [task_id],
                |row| row.get(0),
            )
            .optional()?;
        let Some(current) = current else {
            return Err(AppError::TaskNotFound(task_id.into()));
        };
        if current != expected.as_str() {
            return Err(AppError::StaleState {
                expected: expected.to_string(),
                actual: current,
            });
        }
        let pending: bool = transaction.query_row(
            "SELECT EXISTS(SELECT 1 FROM human_decisions WHERE task_id = ?1 AND status = 'pending')",
            [task_id],
            |row| row.get(0),
        )?;
        if pending {
            return Err(AppError::PendingDecision);
        }

        let timestamp = now();
        transaction.execute(
            "UPDATE tasks SET status = ?2, updated_at = ?3 WHERE task_id = ?1 AND status = ?4",
            params![task_id, next.as_str(), timestamp, expected.as_str()],
        )?;
        if transaction.changes() != 1 {
            return Err(AppError::StaleState {
                expected: expected.to_string(),
                actual: "changed by another writer".into(),
            });
        }
        append_event(
            &transaction,
            task_id,
            "task_transitioned",
            actor,
            correlation_id,
            serde_json::json!({ "from": expected, "to": next }),
        )?;
        transaction.commit()?;
        self.get_task(task_id)?
            .ok_or_else(|| AppError::TaskNotFound(task_id.into()))
    }

    pub fn request_human_decision(
        &mut self,
        task_id: &str,
        decision_id: &str,
        question: &str,
        actor: &str,
        correlation_id: &str,
    ) -> Result<(), AppError> {
        if decision_id.trim().is_empty() || question.trim().is_empty() || actor.trim().is_empty() {
            return Err(AppError::InvalidInput(
                "decision_id, question and actor must not be empty".into(),
            ));
        }
        validate_length("decision_id", decision_id, 128)?;
        validate_length("question", question, 2_000)?;
        validate_length("actor", actor, 120)?;
        validate_length("correlation_id", correlation_id, 128)?;
        let transaction = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let status: Option<String> = transaction
            .query_row(
                "SELECT status FROM tasks WHERE task_id = ?1",
                [task_id],
                |row| row.get(0),
            )
            .optional()?;
        let Some(status) = status else {
            return Err(AppError::TaskNotFound(task_id.into()));
        };
        if status == TaskStatus::Open.as_str() || status == TaskStatus::Approved.as_str() {
            return Err(AppError::InvalidInput(
                "human decisions can only be requested after a proposal exists and before approval"
                    .into(),
            ));
        }
        let timestamp = now();
        transaction.execute(
            "INSERT INTO human_decisions (decision_id, task_id, state_at_request, question, requested_by, status, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5, 'pending', ?6)",
            params![decision_id, task_id, status, question.trim(), actor, timestamp],
        )?;
        transaction.execute(
            "UPDATE tasks SET updated_at = ?2 WHERE task_id = ?1",
            params![task_id, timestamp],
        )?;
        append_event(
            &transaction,
            task_id,
            "human_decision_requested",
            actor,
            correlation_id,
            serde_json::json!({ "decision_id": decision_id, "state_preserved": status, "question": question.trim() }),
        )?;
        transaction.commit()?;
        Ok(())
    }

    pub fn answer_human_decision(
        &mut self,
        task_id: &str,
        decision_id: &str,
        answer: &str,
        actor: &str,
        correlation_id: &str,
    ) -> Result<(), AppError> {
        if actor != "human:cesar" {
            return Err(AppError::HumanApprovalRequired);
        }
        if answer.trim().is_empty() {
            return Err(AppError::InvalidInput("answer must not be empty".into()));
        }
        validate_length("answer", answer, 4_000)?;
        validate_length("correlation_id", correlation_id, 128)?;
        let transaction = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let current: Option<String> = transaction
            .query_row(
                "SELECT status FROM human_decisions WHERE decision_id = ?1 AND task_id = ?2",
                params![decision_id, task_id],
                |row| row.get(0),
            )
            .optional()?;
        match current.as_deref() {
            None | Some("answered") => {
                return Err(AppError::DecisionNotPending(decision_id.into()));
            }
            Some("pending") => {}
            Some(_) => return Err(AppError::DecisionNotPending(decision_id.into())),
        }
        let timestamp = now();
        transaction.execute(
            "UPDATE human_decisions SET status = 'answered', answer = ?3, answered_by = ?4, answered_at = ?5
             WHERE decision_id = ?1 AND task_id = ?2 AND status = 'pending'",
            params![decision_id, task_id, answer.trim(), actor, timestamp],
        )?;
        transaction.execute(
            "UPDATE tasks SET updated_at = ?2 WHERE task_id = ?1",
            params![task_id, timestamp],
        )?;
        append_event(
            &transaction,
            task_id,
            "human_decision_answered",
            actor,
            correlation_id,
            serde_json::json!({ "decision_id": decision_id, "answer": answer.trim() }),
        )?;
        transaction.commit()?;
        Ok(())
    }

    pub fn insert_message(&mut self, message: &Message) -> Result<bool, AppError> {
        message.validate()?;
        let transaction = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)?;

        let existing: Option<String> = transaction
            .query_row(
                "SELECT payload_hash FROM messages WHERE message_id = ?1",
                [&message.message_id],
                |row| row.get(0),
            )
            .optional()?;
        // `created_at` is assigned at the boundary and may differ when a caller retries
        // the same idempotency key. Hash only the caller-supplied message content.
        let mut canonical_message = message.clone();
        canonical_message.created_at.clear();
        let payload = serde_json::to_vec(&canonical_message)?;
        let payload_hash = format!("{:x}", Sha256::digest(payload));
        if let Some(existing_hash) = existing {
            if existing_hash == payload_hash {
                transaction.rollback()?;
                return Ok(false);
            }
            return Err(AppError::IdempotencyConflict(message.message_id.clone()));
        }

        let task_exists: bool = transaction.query_row(
            "SELECT EXISTS(SELECT 1 FROM tasks WHERE task_id = ?1)",
            [&message.task_id],
            |row| row.get(0),
        )?;
        if !task_exists {
            return Err(AppError::TaskNotFound(message.task_id.clone()));
        }
        if let Some(parent_id) = &message.parent_id {
            let parent_task: Option<String> = transaction
                .query_row(
                    "SELECT task_id FROM messages WHERE message_id = ?1",
                    [parent_id],
                    |row| row.get(0),
                )
                .optional()?;
            match parent_task {
                None => return Err(AppError::MessageNotFound(parent_id.clone())),
                Some(parent_task) if parent_task != message.task_id => {
                    return Err(AppError::InvalidInput(
                        "parent message belongs to a different task".into(),
                    ));
                }
                _ => {}
            }
        }
        let evidence_json = serde_json::to_string(&message.evidence_refs)?;
        transaction.execute(
            "INSERT INTO messages (message_id, schema_version, task_id, sender, recipient, correlation_id,
                                  parent_id, status, body, evidence_refs_json, created_at, payload_hash)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, 'recorded', ?8, ?9, ?10, ?11)",
            params![
                message.message_id,
                message.schema_version,
                message.task_id,
                message.sender,
                message.recipient,
                message.correlation_id,
                message.parent_id,
                message.body,
                evidence_json,
                message.created_at,
                payload_hash,
            ],
        )?;
        transaction.execute(
            "UPDATE tasks SET updated_at = ?2 WHERE task_id = ?1",
            params![message.task_id, message.created_at],
        )?;
        append_event(
            &transaction,
            &message.task_id,
            "message_recorded",
            &message.sender,
            &message.correlation_id,
            serde_json::json!({ "message_id": message.message_id, "recipient": message.recipient }),
        )?;
        transaction.commit()?;
        Ok(true)
    }

    pub fn list_messages(&self, task_id: &str) -> Result<Vec<Message>, AppError> {
        let mut statement = self.connection.prepare(
            "SELECT schema_version, message_id, task_id, sender, recipient, correlation_id, parent_id,
                    status, body, evidence_refs_json, created_at
             FROM messages WHERE task_id = ?1 ORDER BY created_at ASC, message_id ASC",
        )?;
        let rows = statement.query_map([task_id], |row| {
            let status: String = row.get(7)?;
            let evidence_json: String = row.get(9)?;
            let evidence_refs = serde_json::from_str(&evidence_json).map_err(|error| {
                rusqlite::Error::FromSqlConversionFailure(
                    9,
                    rusqlite::types::Type::Text,
                    Box::new(error),
                )
            })?;
            let message_status = match status.as_str() {
                "recorded" => MessageStatus::Recorded,
                _ => {
                    return Err(rusqlite::Error::InvalidColumnType(
                        7,
                        "status".into(),
                        rusqlite::types::Type::Text,
                    ));
                }
            };
            Ok(Message {
                schema_version: row.get(0)?,
                message_id: row.get(1)?,
                task_id: row.get(2)?,
                sender: row.get(3)?,
                recipient: row.get(4)?,
                correlation_id: row.get(5)?,
                parent_id: row.get(6)?,
                status: message_status,
                body: row.get(8)?,
                evidence_refs,
                created_at: row.get(10)?,
            })
        })?;
        rows.collect::<Result<Vec<_>, _>>().map_err(AppError::from)
    }

    pub fn list_events(&self, task_id: &str) -> Result<Vec<TaskEvent>, AppError> {
        let mut statement = self.connection.prepare(
            "SELECT sequence, task_id, event_type, actor, correlation_id, details_json, occurred_at
             FROM task_events WHERE task_id = ?1 ORDER BY sequence ASC",
        )?;
        let rows = statement.query_map([task_id], |row| {
            let details: String = row.get(5)?;
            let details = serde_json::from_str(&details).map_err(|error| {
                rusqlite::Error::FromSqlConversionFailure(
                    5,
                    rusqlite::types::Type::Text,
                    Box::new(error),
                )
            })?;
            Ok(TaskEvent {
                sequence: row.get(0)?,
                task_id: row.get(1)?,
                event_type: row.get(2)?,
                actor: row.get(3)?,
                correlation_id: row.get(4)?,
                details,
                occurred_at: row.get(6)?,
            })
        })?;
        rows.collect::<Result<Vec<_>, _>>().map_err(AppError::from)
    }
}

fn row_to_task(row: &rusqlite::Row<'_>) -> rusqlite::Result<Task> {
    let status: String = row.get(3)?;
    let status = status.parse().map_err(|_| {
        rusqlite::Error::InvalidColumnType(3, "status".into(), rusqlite::types::Type::Text)
    })?;
    Ok(Task {
        task_id: row.get(0)?,
        title: row.get(1)?,
        description: row.get(2)?,
        status,
        pending_decision: row.get(4)?,
        created_at: row.get(5)?,
        updated_at: row.get(6)?,
    })
}

fn append_event(
    transaction: &Transaction<'_>,
    task_id: &str,
    event_type: &str,
    actor: &str,
    correlation_id: &str,
    details: serde_json::Value,
) -> Result<(), AppError> {
    transaction.execute(
        "INSERT INTO task_events (task_id, event_type, actor, correlation_id, details_json, occurred_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
        params![task_id, event_type, actor, correlation_id, serde_json::to_string(&details)?, now()],
    )?;
    Ok(())
}

fn now() -> String {
    Utc::now().to_rfc3339_opts(SecondsFormat::Millis, true)
}

fn validate_length(name: &str, value: &str, limit: usize) -> Result<(), AppError> {
    if value.chars().count() > limit {
        return Err(AppError::InvalidInput(format!(
            "{name} exceeds {limit} characters"
        )));
    }
    Ok(())
}

fn migrate(connection: &mut Connection) -> Result<(), AppError> {
    let version: i64 = connection.query_row("PRAGMA user_version", [], |row| row.get(0))?;
    if version > CURRENT_SCHEMA_VERSION {
        return Err(AppError::InvalidInput(format!(
            "database schema version {version} is newer than this binary supports ({CURRENT_SCHEMA_VERSION})"
        )));
    }

    if version < 1 {
        let transaction = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
        transaction.execute_batch(SCHEMA)?;
        transaction.pragma_update(None, "user_version", 1)?;
        transaction.commit()?;
    }
    Ok(())
}

const SCHEMA: &str = r#"
CREATE TABLE IF NOT EXISTS tasks (
    task_id TEXT PRIMARY KEY,
    title TEXT NOT NULL,
    description TEXT NOT NULL,
    status TEXT NOT NULL CHECK (status IN ('open', 'proposed', 'under_review', 'resolved', 'approved')),
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL
);
CREATE TABLE IF NOT EXISTS task_events (
    sequence INTEGER PRIMARY KEY AUTOINCREMENT,
    task_id TEXT NOT NULL REFERENCES tasks(task_id),
    event_type TEXT NOT NULL,
    actor TEXT NOT NULL,
    correlation_id TEXT NOT NULL,
    details_json TEXT NOT NULL CHECK (json_valid(details_json)),
    occurred_at TEXT NOT NULL
);
CREATE TABLE IF NOT EXISTS messages (
    message_id TEXT PRIMARY KEY,
    schema_version INTEGER NOT NULL,
    task_id TEXT NOT NULL REFERENCES tasks(task_id),
    sender TEXT NOT NULL,
    recipient TEXT NOT NULL,
    correlation_id TEXT NOT NULL,
    parent_id TEXT REFERENCES messages(message_id),
    status TEXT NOT NULL CHECK (status = 'recorded'),
    body TEXT NOT NULL,
    evidence_refs_json TEXT NOT NULL CHECK (json_valid(evidence_refs_json)),
    created_at TEXT NOT NULL,
    payload_hash TEXT NOT NULL
);
CREATE INDEX IF NOT EXISTS messages_task_order ON messages(task_id, created_at, message_id);
CREATE TABLE IF NOT EXISTS human_decisions (
    decision_id TEXT PRIMARY KEY,
    task_id TEXT NOT NULL REFERENCES tasks(task_id),
    state_at_request TEXT NOT NULL,
    question TEXT NOT NULL,
    requested_by TEXT NOT NULL,
    status TEXT NOT NULL CHECK (status IN ('pending', 'answered')),
    answer TEXT,
    answered_by TEXT,
    created_at TEXT NOT NULL,
    answered_at TEXT
);
CREATE UNIQUE INDEX IF NOT EXISTS one_pending_decision_per_task
    ON human_decisions(task_id) WHERE status = 'pending';
"#;

#[cfg(test)]
mod tests {
    use std::{
        sync::{Arc, Barrier},
        thread,
    };

    use super::*;
    use crate::domain::{EvidenceRef, MESSAGE_SCHEMA_VERSION};

    fn db_path() -> tempfile::TempPath {
        let file = tempfile::NamedTempFile::new().unwrap();
        file.into_temp_path()
    }

    fn new_message(task_id: &str, message_id: &str, body: &str) -> Message {
        Message {
            schema_version: MESSAGE_SCHEMA_VERSION,
            message_id: message_id.into(),
            task_id: task_id.into(),
            sender: "agent:codex".into(),
            recipient: "agent:chatgpt".into(),
            correlation_id: "corr-1".into(),
            parent_id: None,
            status: MessageStatus::Recorded,
            body: body.into(),
            evidence_refs: vec![EvidenceRef {
                label: "spec".into(),
                locator: "docs/spec.md#phase-1".into(),
                sha256: "a".repeat(64),
            }],
            created_at: "2026-10-10T00:00:00.000Z".into(),
        }
    }

    #[test]
    fn duplicate_message_is_idempotent_and_conflicting_reuse_is_rejected() {
        let path = db_path();
        let mut store = Store::open(&path).unwrap();
        let task = store.create_task("spec", "desc", "human:cesar").unwrap();
        let message = new_message(&task.task_id, "message-1", "proposal");
        assert!(store.insert_message(&message).unwrap());
        assert!(!store.insert_message(&message).unwrap());
        let mut retry_with_new_timestamp = message.clone();
        retry_with_new_timestamp.created_at = "2026-10-10T00:01:00.000Z".into();
        assert!(!store.insert_message(&retry_with_new_timestamp).unwrap());
        let conflicting = new_message(&task.task_id, "message-1", "different payload");
        assert!(matches!(
            store.insert_message(&conflicting),
            Err(AppError::IdempotencyConflict(_))
        ));
        assert_eq!(store.list_messages(&task.task_id).unwrap().len(), 1);
    }

    #[test]
    fn invalid_transition_is_rejected_and_expected_state_prevents_stale_write() {
        let path = db_path();
        let mut store = Store::open(&path).unwrap();
        let task = store.create_task("spec", "desc", "human:cesar").unwrap();
        assert!(matches!(
            store.transition_task(
                &task.task_id,
                TaskStatus::Open,
                TaskStatus::Approved,
                "human:cesar",
                "c1"
            ),
            Err(AppError::InvalidTransition { .. })
        ));
        store
            .transition_task(
                &task.task_id,
                TaskStatus::Open,
                TaskStatus::Proposed,
                "human:cesar",
                "c2",
            )
            .unwrap();
        assert!(matches!(
            store.transition_task(
                &task.task_id,
                TaskStatus::Open,
                TaskStatus::Proposed,
                "human:cesar",
                "c3"
            ),
            Err(AppError::StaleState { .. })
        ));
    }

    #[test]
    fn pending_human_decision_preserves_task_state_until_answered() {
        let path = db_path();
        let mut store = Store::open(&path).unwrap();
        let task = store.create_task("spec", "desc", "human:cesar").unwrap();
        store
            .transition_task(
                &task.task_id,
                TaskStatus::Open,
                TaskStatus::Proposed,
                "agent:codex",
                "c1",
            )
            .unwrap();
        store
            .request_human_decision(
                &task.task_id,
                "decision-1",
                "Which option?",
                "agent:codex",
                "c2",
            )
            .unwrap();
        let waiting = store.get_task(&task.task_id).unwrap().unwrap();
        assert_eq!(waiting.status, TaskStatus::Proposed);
        assert!(waiting.pending_decision);
        assert!(matches!(
            store.transition_task(
                &task.task_id,
                TaskStatus::Proposed,
                TaskStatus::UnderReview,
                "agent:chatgpt",
                "c3"
            ),
            Err(AppError::PendingDecision)
        ));
        store
            .answer_human_decision(&task.task_id, "decision-1", "Option A", "human:cesar", "c4")
            .unwrap();
        let resumed = store.get_task(&task.task_id).unwrap().unwrap();
        assert_eq!(resumed.status, TaskStatus::Proposed);
        assert!(!resumed.pending_decision);
    }

    #[test]
    fn non_human_cannot_approve() {
        let path = db_path();
        let mut store = Store::open(&path).unwrap();
        let task = store.create_task("spec", "desc", "human:cesar").unwrap();
        store
            .transition_task(
                &task.task_id,
                TaskStatus::Open,
                TaskStatus::Proposed,
                "agent:codex",
                "c1",
            )
            .unwrap();
        store
            .transition_task(
                &task.task_id,
                TaskStatus::Proposed,
                TaskStatus::UnderReview,
                "agent:chatgpt",
                "c2",
            )
            .unwrap();
        store
            .transition_task(
                &task.task_id,
                TaskStatus::UnderReview,
                TaskStatus::Resolved,
                "agent:chatgpt",
                "c3",
            )
            .unwrap();
        assert!(matches!(
            store.transition_task(
                &task.task_id,
                TaskStatus::Resolved,
                TaskStatus::Approved,
                "agent:codex",
                "c4"
            ),
            Err(AppError::HumanApprovalRequired)
        ));
    }

    #[test]
    fn caller_can_spoof_human_actor_label_so_approval_is_not_authenticated() {
        let path = db_path();
        let mut store = Store::open(&path).unwrap();
        let task = store
            .create_task("identity boundary", "synthetic", "human:cesar")
            .unwrap();
        store
            .transition_task(
                &task.task_id,
                TaskStatus::Open,
                TaskStatus::Proposed,
                "agent:codex",
                "c1",
            )
            .unwrap();
        store
            .transition_task(
                &task.task_id,
                TaskStatus::Proposed,
                TaskStatus::UnderReview,
                "agent:chatgpt",
                "c2",
            )
            .unwrap();
        store
            .transition_task(
                &task.task_id,
                TaskStatus::UnderReview,
                TaskStatus::Resolved,
                "agent:chatgpt",
                "c3",
            )
            .unwrap();

        // This expected limitation proves that the actor string is a claim, not an identity.
        store
            .transition_task(
                &task.task_id,
                TaskStatus::Resolved,
                TaskStatus::Approved,
                "human:cesar",
                "c4",
            )
            .unwrap();

        assert_eq!(
            store.get_task(&task.task_id).unwrap().unwrap().status,
            TaskStatus::Approved
        );
        let approval = store
            .list_events(&task.task_id)
            .unwrap()
            .into_iter()
            .find(|event| {
                event.event_type == "task_transitioned" && event.details["to"] == "approved"
            })
            .unwrap();
        assert_eq!(approval.actor, "human:cesar");
    }

    #[test]
    fn dropped_transaction_rolls_back_all_writes() {
        let path = db_path();
        let mut store = Store::open(&path).unwrap();
        let transaction = store
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .unwrap();
        transaction.execute(
            "INSERT INTO tasks (task_id, title, description, status, created_at, updated_at) VALUES ('rollback', 'x', '', 'open', 't', 't')",
            [],
        ).unwrap();
        append_event(
            &transaction,
            "rollback",
            "task_created",
            "human:cesar",
            "c1",
            serde_json::json!({}),
        )
        .unwrap();
        drop(transaction);
        assert!(store.get_task("rollback").unwrap().is_none());
        let count: i64 = store
            .connection
            .query_row(
                "SELECT COUNT(*) FROM task_events WHERE task_id = 'rollback'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(count, 0);
    }

    #[test]
    fn concurrent_stale_transitions_have_one_winner_and_no_lost_update() {
        let path = db_path();
        let mut store = Store::open(&path).unwrap();
        let task = store
            .create_task("concurrency", "desc", "human:cesar")
            .unwrap();
        drop(store);

        let barrier = Arc::new(Barrier::new(3));
        let mut joins = Vec::new();
        for (actor, correlation) in [("agent:codex", "c1"), ("agent:chatgpt", "c2")] {
            let path = path.to_path_buf();
            let task_id = task.task_id.clone();
            let barrier = Arc::clone(&barrier);
            joins.push(thread::spawn(move || {
                let mut store = Store::open(path).unwrap();
                barrier.wait();
                store.transition_task(
                    &task_id,
                    TaskStatus::Open,
                    TaskStatus::Proposed,
                    actor,
                    correlation,
                )
            }));
        }
        barrier.wait();
        let results: Vec<_> = joins.into_iter().map(|join| join.join().unwrap()).collect();
        assert_eq!(results.iter().filter(|result| result.is_ok()).count(), 1);
        assert_eq!(
            results
                .iter()
                .filter(|result| matches!(result, Err(AppError::StaleState { .. })))
                .count(),
            1
        );

        let store = Store::open(&path).unwrap();
        let task_after = store.get_task(&task.task_id).unwrap().unwrap();
        assert_eq!(task_after.status, TaskStatus::Proposed);
        let events = store.list_events(&task.task_id).unwrap();
        assert_eq!(
            events
                .iter()
                .filter(|event| event.event_type == "task_transitioned")
                .count(),
            1
        );
    }

    #[test]
    fn restart_recovers_state_and_event_order_is_deterministic() {
        let path = db_path();
        let mut store = Store::open(&path).unwrap();
        let task = store.create_task("restart", "desc", "human:cesar").unwrap();
        store
            .transition_task(
                &task.task_id,
                TaskStatus::Open,
                TaskStatus::Proposed,
                "agent:codex",
                "c1",
            )
            .unwrap();
        store
            .insert_message(&new_message(&task.task_id, "message-a", "first"))
            .unwrap();
        store
            .insert_message(&new_message(&task.task_id, "message-b", "second"))
            .unwrap();
        drop(store);

        let store = Store::open(&path).unwrap();
        assert_eq!(
            store.get_task(&task.task_id).unwrap().unwrap().status,
            TaskStatus::Proposed
        );
        let events = store.list_events(&task.task_id).unwrap();
        assert!(
            events
                .windows(2)
                .all(|pair| pair[0].sequence < pair[1].sequence)
        );
        let messages = store.list_messages(&task.task_id).unwrap();
        assert_eq!(messages.len(), 2);
        assert_eq!(messages[0].message_id, "message-a");
        assert_eq!(messages[1].message_id, "message-b");
    }

    #[test]
    fn message_requires_valid_evidence_hash() {
        let path = db_path();
        let mut store = Store::open(&path).unwrap();
        let task = store
            .create_task("evidence", "desc", "human:cesar")
            .unwrap();
        let mut message = new_message(&task.task_id, "message-1", "proposal");
        message.evidence_refs[0].sha256 = "not-a-hash".into();
        assert!(matches!(
            store.insert_message(&message),
            Err(AppError::InvalidInput(_))
        ));
    }

    #[test]
    fn migration_preserves_existing_unversioned_database_and_sets_version() {
        let path = db_path();
        {
            let connection = Connection::open(&path).unwrap();
            connection.execute_batch(SCHEMA).unwrap();
            connection.execute(
                "INSERT INTO tasks (task_id, title, description, status, created_at, updated_at)
                 VALUES ('legacy-task', 'legacy', '', 'open', 't', 't')",
                [],
            ).unwrap();
        }

        let store = Store::open(&path).unwrap();
        assert_eq!(
            store.get_task("legacy-task").unwrap().unwrap().title,
            "legacy"
        );
        let version: i64 = store
            .connection
            .query_row("PRAGMA user_version", [], |row| row.get(0))
            .unwrap();
        assert_eq!(version, 1);
    }

    #[test]
    fn migration_rolls_back_schema_and_version_when_sql_fails() {
        let path = db_path();
        {
            let connection = Connection::open(&path).unwrap();
            // This incompatible object makes index creation fail after earlier DDL ran.
            connection
                .execute_batch("CREATE VIEW messages AS SELECT 1 AS message_id;")
                .unwrap();
        }

        assert!(Store::open(&path).is_err());
        let connection = Connection::open(&path).unwrap();
        let version: i64 = connection
            .query_row("PRAGMA user_version", [], |row| row.get(0))
            .unwrap();
        assert_eq!(version, 0);
        let tasks_exists: bool = connection.query_row(
            "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type = 'table' AND name = 'tasks')",
            [],
            |row| row.get(0),
        ).unwrap();
        assert!(!tasks_exists);
    }

    #[test]
    fn database_from_newer_binary_is_rejected_without_modification() {
        let path = db_path();
        {
            let connection = Connection::open(&path).unwrap();
            connection
                .pragma_update(None, "user_version", CURRENT_SCHEMA_VERSION + 1)
                .unwrap();
        }
        let error = match Store::open(&path) {
            Ok(_) => panic!("newer schema should be rejected"),
            Err(error) => error,
        };
        assert!(
            matches!(error, AppError::InvalidInput(message) if message.contains("newer than this binary supports"))
        );
        let connection = Connection::open(&path).unwrap();
        let version: i64 = connection
            .query_row("PRAGMA user_version", [], |row| row.get(0))
            .unwrap();
        assert_eq!(version, CURRENT_SCHEMA_VERSION + 1);
    }
}
