use std::{path::Path, time::Duration};

use chrono::{SecondsFormat, Utc};
use rusqlite::{
    Connection, OptionalExtension, Transaction, TransactionBehavior, backup::Backup, params,
};
use sha2::{Digest, Sha256};
use uuid::Uuid;

use crate::domain::{
    AppError, CheckpointContent, ContextCheckpoint, HumanDecision, Message, MessageStatus, Task,
    TaskEvent, TaskStatus,
};

const CURRENT_SCHEMA_VERSION: i64 = 5;

pub struct Store {
    connection: Connection,
    workspace_id: Option<String>,
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
        Ok(Self {
            connection,
            workspace_id: None,
        })
    }

    /// Verify that the task/review database can answer a minimal query.
    pub fn ping(&self) -> Result<(), AppError> {
        self.connection
            .query_row("SELECT COUNT(*) FROM tasks", [], |row| row.get::<_, i64>(0))?;
        Ok(())
    }

    /// Return SQLite's full integrity-check result for operational smoke tests.
    pub fn integrity_check(&self) -> Result<String, AppError> {
        let mut statement = self.connection.prepare("PRAGMA integrity_check")?;
        let rows = statement.query_map([], |row| row.get::<_, String>(0))?;
        let messages = rows.collect::<Result<Vec<_>, _>>()?;
        Ok(messages.join("\n"))
    }

    /// Create a consistent online SQLite backup without copying a live WAL database.
    pub fn backup_to(&self, destination: impl AsRef<Path>) -> Result<(), AppError> {
        let destination = destination.as_ref();
        if destination.exists() {
            return Err(AppError::InvalidInput(
                "backup destination already exists".into(),
            ));
        }
        if let Some(parent) = destination
            .parent()
            .filter(|parent| !parent.as_os_str().is_empty())
        {
            std::fs::create_dir_all(parent).map_err(|error| {
                AppError::InvalidInput(format!("cannot create backup directory: {error}"))
            })?;
        }
        let mut backup = Connection::open(destination)?;
        {
            let operation = Backup::new(&self.connection, &mut backup)?;
            operation.run_to_completion(128, Duration::from_millis(10), None)?;
        }
        let integrity: String = backup.query_row("PRAGMA integrity_check", [], |row| row.get(0))?;
        if integrity != "ok" {
            let _ = std::fs::remove_file(destination);
            return Err(AppError::InvalidInput(
                "SQLite backup integrity check failed".into(),
            ));
        }
        drop(backup);
        Ok(())
    }

    /// Restore a consistent backup to a new path. Existing destinations are never overwritten.
    pub fn restore_from_backup(
        source: impl AsRef<Path>,
        destination: impl AsRef<Path>,
    ) -> Result<(), AppError> {
        let source = source.as_ref();
        let destination = destination.as_ref();
        if !source.is_file() {
            return Err(AppError::InvalidInput(
                "backup source does not exist".into(),
            ));
        }
        if destination.exists()
            || sidecar_path(destination, "-wal").exists()
            || sidecar_path(destination, "-shm").exists()
        {
            return Err(AppError::InvalidInput(
                "restore destination or SQLite sidecar already exists; preserve it and choose a new path".into(),
            ));
        }
        let parent = destination
            .parent()
            .filter(|parent| !parent.as_os_str().is_empty());
        if let Some(parent) = parent {
            std::fs::create_dir_all(parent).map_err(|error| {
                AppError::InvalidInput(format!("cannot create restore directory: {error}"))
            })?;
        }
        let temporary = destination.with_extension(format!("restore-{}.tmp", Uuid::new_v4()));
        let result = (|| {
            let source_connection = Connection::open_with_flags(
                source,
                rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY
                    | rusqlite::OpenFlags::SQLITE_OPEN_NO_MUTEX,
            )?;
            let mut restored = Connection::open(&temporary)?;
            {
                let operation = Backup::new(&source_connection, &mut restored)?;
                operation.run_to_completion(128, Duration::from_millis(10), None)?;
            }
            let integrity: String =
                restored.query_row("PRAGMA integrity_check", [], |row| row.get(0))?;
            if integrity != "ok" {
                return Err(AppError::InvalidInput(
                    "restored SQLite database failed integrity_check".into(),
                ));
            }
            drop(restored);
            drop(source_connection);
            std::fs::rename(&temporary, destination).map_err(|error| {
                AppError::InvalidInput(format!("cannot publish restored database: {error}"))
            })?;
            Ok(())
        })();
        if result.is_err() {
            let _ = std::fs::remove_file(&temporary);
        }
        result
    }

    pub fn set_workspace_context(&mut self, workspace_id: Option<&str>) {
        self.workspace_id = workspace_id.map(str::to_owned);
    }

    pub fn create_task(
        &mut self,
        title: &str,
        description: &str,
        actor: &str,
    ) -> Result<Task, AppError> {
        self.create_task_inner(title, description, actor, None)
    }

    pub(crate) fn create_task_verified_human(
        &mut self,
        title: &str,
        description: &str,
        user_id: &str,
        session_id: &str,
    ) -> Result<Task, AppError> {
        let actor = format!("human:{user_id}");
        self.create_task_inner(title, description, &actor, Some((user_id, session_id)))
    }

    fn create_task_inner(
        &mut self,
        title: &str,
        description: &str,
        actor: &str,
        verified_human: Option<(&str, &str)>,
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
            "INSERT INTO tasks (task_id, title, description, status, created_at, updated_at, workspace_id)
             VALUES (?1, ?2, ?3, 'open', ?4, ?4, ?5)",
            params![task_id, title.trim(), description, now, self.workspace_id.as_deref()],
        )?;
        let mut details = serde_json::json!({ "status": "open" });
        if let Some((user_id, session_id)) = verified_human {
            details["verified_human"] = serde_json::json!({
                "user_id": user_id,
                "session_id": session_id,
            });
        }
        append_event(
            &transaction,
            &task_id,
            "task_created",
            actor,
            &task_id,
            details,
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
                        t.created_at, t.updated_at, t.version
                 FROM tasks t WHERE t.task_id = ?1 AND (?2 IS NULL OR t.workspace_id = ?2)",
                params![task_id, self.workspace_id.as_deref()],
                row_to_task,
            )
            .optional()
            .map_err(AppError::from)
    }

    pub fn list_tasks(&self) -> Result<Vec<Task>, AppError> {
        let mut statement = self.connection.prepare(
            "SELECT t.task_id, t.title, t.description, t.status,
                    EXISTS(SELECT 1 FROM human_decisions d WHERE d.task_id = t.task_id AND d.status = 'pending'),
                    t.created_at, t.updated_at, t.version
             FROM tasks t WHERE (?1 IS NULL OR t.workspace_id = ?1)
             ORDER BY t.updated_at DESC, t.task_id ASC",
        )?;
        let rows = statement.query_map([self.workspace_id.as_deref()], row_to_task)?;
        rows.collect::<Result<Vec<_>, _>>().map_err(AppError::from)
    }

    pub fn get_pending_decision(&self, task_id: &str) -> Result<Option<HumanDecision>, AppError> {
        self.connection
            .query_row(
                "SELECT decision_id, task_id, question, requested_by, status, created_at
                 FROM human_decisions WHERE task_id = ?1 AND status = 'pending'
                   AND EXISTS (SELECT 1 FROM tasks t WHERE t.task_id = human_decisions.task_id
                               AND (?2 IS NULL OR t.workspace_id = ?2))",
                params![task_id, self.workspace_id.as_deref()],
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
        self.transition_task_inner(task_id, expected, next, actor, correlation_id, None)
    }

    #[allow(clippy::too_many_arguments)] // The authenticated principal and transition evidence are recorded together.
    pub(crate) fn transition_task_verified_human(
        &mut self,
        task_id: &str,
        expected: TaskStatus,
        next: TaskStatus,
        user_id: &str,
        session_id: &str,
        expected_version: i64,
        correlation_id: &str,
    ) -> Result<Task, AppError> {
        let actor = format!("human:{user_id}");
        self.transition_task_inner(
            task_id,
            expected,
            next,
            &actor,
            correlation_id,
            Some((user_id, session_id, expected_version)),
        )
    }

    fn transition_task_inner(
        &mut self,
        task_id: &str,
        expected: TaskStatus,
        next: TaskStatus,
        actor: &str,
        correlation_id: &str,
        verified_human: Option<(&str, &str, i64)>,
    ) -> Result<Task, AppError> {
        self.ensure_task_in_scope(task_id)?;
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
        if next == TaskStatus::Approved && actor != "human:cesar" && verified_human.is_none() {
            return Err(AppError::HumanApprovalRequired);
        }

        let transaction = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let current: Option<(String, i64)> = transaction
            .query_row(
                "SELECT status, version FROM tasks WHERE task_id = ?1",
                [task_id],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .optional()?;
        let Some((current, current_version)) = current else {
            return Err(AppError::TaskNotFound(task_id.into()));
        };
        if let Some((_, _, expected_version)) = verified_human
            && current_version != expected_version
        {
            return Err(AppError::StaleState {
                expected: format!("version {expected_version}"),
                actual: format!("version {current_version}"),
            });
        }
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

        let approval_evidence = if next == TaskStatus::Approved && verified_human.is_some() {
            let mut statement = transaction.prepare(
                "SELECT evidence_refs_json FROM messages WHERE task_id = ?1 ORDER BY created_at, message_id",
            )?;
            let rows = statement.query_map([task_id], |row| row.get::<_, String>(0))?;
            let mut evidence = Vec::new();
            for row in rows {
                evidence.extend(serde_json::from_str::<Vec<crate::domain::EvidenceRef>>(
                    &row?,
                )?);
            }
            if evidence.is_empty() {
                return Err(AppError::InvalidInput(
                    "approval requires at least one evidence reference".into(),
                ));
            }
            evidence
        } else {
            Vec::new()
        };

        let timestamp = now();
        transaction.execute(
            "UPDATE tasks SET status = ?2, updated_at = ?3, version = version + 1 WHERE task_id = ?1 AND status = ?4 AND version = ?5",
            params![task_id, next.as_str(), timestamp, expected.as_str(), current_version],
        )?;
        if transaction.changes() != 1 {
            return Err(AppError::StaleState {
                expected: expected.to_string(),
                actual: "changed by another writer".into(),
            });
        }
        let mut details = serde_json::json!({ "from": expected, "to": next, "expected_version": current_version, "resulting_version": current_version + 1 });
        if let Some((user_id, session_id, expected_version)) = verified_human {
            details["verified_human"] = serde_json::json!({
                "user_id": user_id,
                "session_id": session_id,
            });
            if next == TaskStatus::Approved {
                details["workspace_id"] = serde_json::json!(self.workspace_id);
                details["resource_id"] = serde_json::json!(task_id);
                details["expected_version"] = serde_json::json!(expected_version);
                details["evidence_refs"] = serde_json::to_value(approval_evidence)?;
                details["approved_at"] = serde_json::json!(timestamp);
            }
        }
        append_event(
            &transaction,
            task_id,
            "task_transitioned",
            actor,
            correlation_id,
            details,
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
        self.request_human_decision_inner(
            task_id,
            decision_id,
            question,
            actor,
            correlation_id,
            None,
        )
    }

    pub(crate) fn request_human_decision_verified_human(
        &mut self,
        task_id: &str,
        decision_id: &str,
        question: &str,
        user_id: &str,
        session_id: &str,
        correlation_id: &str,
    ) -> Result<(), AppError> {
        let actor = format!("human:{user_id}");
        self.request_human_decision_inner(
            task_id,
            decision_id,
            question,
            &actor,
            correlation_id,
            Some((user_id, session_id)),
        )
    }

    fn request_human_decision_inner(
        &mut self,
        task_id: &str,
        decision_id: &str,
        question: &str,
        actor: &str,
        correlation_id: &str,
        verified_human: Option<(&str, &str)>,
    ) -> Result<(), AppError> {
        self.ensure_task_in_scope(task_id)?;
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
            "UPDATE tasks SET updated_at = ?2, version = version + 1 WHERE task_id = ?1",
            params![task_id, timestamp],
        )?;
        let mut details = serde_json::json!({
            "decision_id": decision_id,
            "state_preserved": status,
            "question": question.trim()
        });
        if let Some((user_id, session_id)) = verified_human {
            details["verified_human"] = serde_json::json!({
                "user_id": user_id,
                "session_id": session_id,
            });
        }
        append_event(
            &transaction,
            task_id,
            "human_decision_requested",
            actor,
            correlation_id,
            details,
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
        self.answer_human_decision_inner(task_id, decision_id, answer, actor, correlation_id, None)
    }

    pub(crate) fn answer_human_decision_verified_human(
        &mut self,
        task_id: &str,
        decision_id: &str,
        answer: &str,
        user_id: &str,
        session_id: &str,
        correlation_id: &str,
    ) -> Result<(), AppError> {
        let actor = format!("human:{user_id}");
        self.answer_human_decision_inner(
            task_id,
            decision_id,
            answer,
            &actor,
            correlation_id,
            Some((user_id, session_id)),
        )
    }

    fn answer_human_decision_inner(
        &mut self,
        task_id: &str,
        decision_id: &str,
        answer: &str,
        actor: &str,
        correlation_id: &str,
        verified_human: Option<(&str, &str)>,
    ) -> Result<(), AppError> {
        self.ensure_task_in_scope(task_id)?;
        if actor != "human:cesar" && verified_human.is_none() {
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
            "UPDATE tasks SET updated_at = ?2, version = version + 1 WHERE task_id = ?1",
            params![task_id, timestamp],
        )?;
        let mut details =
            serde_json::json!({ "decision_id": decision_id, "answer": answer.trim() });
        if let Some((user_id, session_id)) = verified_human {
            details["verified_human"] = serde_json::json!({
                "user_id": user_id,
                "session_id": session_id,
            });
        }
        append_event(
            &transaction,
            task_id,
            "human_decision_answered",
            actor,
            correlation_id,
            details,
        )?;
        transaction.commit()?;
        Ok(())
    }

    pub fn insert_message(&mut self, message: &Message) -> Result<bool, AppError> {
        self.insert_message_inner(message, None)
    }

    pub(crate) fn insert_message_verified_human(
        &mut self,
        message: &Message,
        user_id: &str,
        session_id: &str,
    ) -> Result<bool, AppError> {
        self.insert_message_inner(message, Some((user_id, session_id)))
    }

    fn insert_message_inner(
        &mut self,
        message: &Message,
        verified_human: Option<(&str, &str)>,
    ) -> Result<bool, AppError> {
        self.ensure_task_in_scope(&message.task_id)?;
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
            "UPDATE tasks SET updated_at = ?2, version = version + 1 WHERE task_id = ?1",
            params![message.task_id, message.created_at],
        )?;
        let mut details = serde_json::json!({
            "message_id": message.message_id,
            "recipient": message.recipient
        });
        if let Some((user_id, session_id)) = verified_human {
            details["verified_human"] = serde_json::json!({
                "user_id": user_id,
                "session_id": session_id,
            });
        }
        append_event(
            &transaction,
            &message.task_id,
            "message_recorded",
            &message.sender,
            &message.correlation_id,
            details,
        )?;
        transaction.commit()?;
        Ok(true)
    }

    pub fn list_messages(&self, task_id: &str) -> Result<Vec<Message>, AppError> {
        self.ensure_task_in_scope(task_id)?;
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
        self.ensure_task_in_scope(task_id)?;
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

    pub fn create_context_checkpoint(
        &mut self,
        content: CheckpointContent,
        created_by: &str,
    ) -> Result<ContextCheckpoint, AppError> {
        validate_checkpoint(&content, created_by)?;
        self.ensure_task_in_scope(&content.task_id)?;
        let content_json = serde_json::to_string(&content)?;
        let content_sha256 = format!("{:x}", Sha256::digest(content_json.as_bytes()));
        let transaction = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)?;

        let existing = transaction
            .query_row(
                "SELECT content_json, content_sha256, status, created_by, created_at, resumed_by, resumed_at
                 FROM context_checkpoints WHERE checkpoint_id = ?1
                   AND (?2 IS NULL OR workspace_id = ?2)",
                params![content.checkpoint_id, self.workspace_id.as_deref()],
                |row| {
                    Ok((
                        row.get::<_, String>(0)?, row.get::<_, String>(1)?, row.get::<_, String>(2)?,
                        row.get::<_, String>(3)?, row.get::<_, String>(4)?, row.get::<_, Option<String>>(5)?,
                        row.get::<_, Option<String>>(6)?,
                    ))
                },
            )
            .optional()?;
        if let Some(existing) = existing {
            if existing.0 != content_json || existing.3 != created_by {
                return Err(AppError::IdempotencyConflict(content.checkpoint_id));
            }
            transaction.commit()?;
            return checkpoint_from_row(&content.task_id, existing);
        }

        let pending: bool = transaction.query_row(
            "SELECT EXISTS(SELECT 1 FROM context_checkpoints WHERE task_id = ?1 AND status = 'ready'
             AND (?2 IS NULL OR workspace_id = ?2))",
            params![content.task_id, self.workspace_id.as_deref()],
            |row| row.get(0),
        )?;
        if pending {
            return Err(AppError::InvalidInput(
                "this task already has a ready checkpoint; resume it before creating another"
                    .into(),
            ));
        }

        let task_version: i64 = transaction.query_row(
            "SELECT version FROM tasks WHERE task_id = ?1 AND (?2 IS NULL OR workspace_id = ?2)",
            params![content.task_id, self.workspace_id.as_deref()],
            |row| row.get(0),
        )?;
        let created_at = now();
        transaction.execute(
            "INSERT INTO context_checkpoints
             (checkpoint_id, task_id, workspace_id, task_version, content_json, content_sha256,
              status, created_by, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, 'ready', ?7, ?8)",
            params![
                content.checkpoint_id,
                content.task_id,
                self.workspace_id.as_deref(),
                task_version,
                content_json,
                content_sha256,
                created_by,
                created_at
            ],
        )?;
        append_event(
            &transaction,
            &content.task_id,
            "context_checkpoint_ready",
            created_by,
            &content.checkpoint_id,
            serde_json::json!({ "checkpoint_id": content.checkpoint_id, "sha256": content_sha256 }),
        )?;
        transaction.commit()?;
        Ok(ContextCheckpoint {
            content,
            content_sha256,
            status: "ready".into(),
            created_by: created_by.to_owned(),
            created_at,
            resumed_by: None,
            resumed_at: None,
        })
    }

    pub fn list_context_checkpoints(
        &self,
        task_id: &str,
    ) -> Result<Vec<ContextCheckpoint>, AppError> {
        self.ensure_task_in_scope(task_id)?;
        let mut statement = self.connection.prepare(
            "SELECT content_json, content_sha256, status, created_by, created_at, resumed_by, resumed_at
             FROM context_checkpoints WHERE task_id = ?1 AND (?2 IS NULL OR workspace_id = ?2)
             ORDER BY created_at DESC, checkpoint_id DESC",
        )?;
        let rows = statement.query_map(params![task_id, self.workspace_id.as_deref()], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, String>(3)?,
                row.get::<_, String>(4)?,
                row.get::<_, Option<String>>(5)?,
                row.get::<_, Option<String>>(6)?,
            ))
        })?;
        rows.map(|row| checkpoint_from_row(task_id, row?)).collect()
    }

    pub fn acknowledge_context_checkpoint(
        &mut self,
        task_id: &str,
        checkpoint_id: &str,
        resumed_by: &str,
    ) -> Result<ContextCheckpoint, AppError> {
        validate_length("resumed_by", resumed_by, 120)?;
        self.ensure_task_in_scope(task_id)?;
        let transaction = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let existing = transaction.query_row(
            "SELECT content_json, content_sha256, status, created_by, created_at, resumed_by, resumed_at
             FROM context_checkpoints WHERE checkpoint_id = ?1 AND task_id = ?2
               AND (?3 IS NULL OR workspace_id = ?3)",
            params![checkpoint_id, task_id, self.workspace_id.as_deref()],
            |row| {
                Ok((
                    row.get::<_, String>(0)?, row.get::<_, String>(1)?, row.get::<_, String>(2)?,
                    row.get::<_, String>(3)?, row.get::<_, String>(4)?, row.get::<_, Option<String>>(5)?,
                    row.get::<_, Option<String>>(6)?,
                ))
            },
        ).optional()?.ok_or_else(|| AppError::TaskNotFound(checkpoint_id.to_owned()))?;
        let checkpoint = checkpoint_from_row(task_id, existing)?;
        if checkpoint.status == "resumed" {
            if checkpoint.resumed_by.as_deref() == Some(resumed_by) {
                transaction.commit()?;
                return Ok(checkpoint);
            }
            return Err(AppError::IdempotencyConflict(checkpoint_id.to_owned()));
        }
        let resumed_at = now();
        transaction.execute(
            "UPDATE context_checkpoints SET status = 'resumed', resumed_by = ?1, resumed_at = ?2
             WHERE checkpoint_id = ?3 AND task_id = ?4 AND status = 'ready'
               AND (?5 IS NULL OR workspace_id = ?5)",
            params![
                resumed_by,
                resumed_at,
                checkpoint_id,
                task_id,
                self.workspace_id.as_deref()
            ],
        )?;
        append_event(
            &transaction,
            task_id,
            "context_checkpoint_resumed",
            resumed_by,
            checkpoint_id,
            serde_json::json!({ "checkpoint_id": checkpoint_id }),
        )?;
        transaction.commit()?;
        let mut checkpoint = checkpoint;
        checkpoint.status = "resumed".into();
        checkpoint.resumed_by = Some(resumed_by.to_owned());
        checkpoint.resumed_at = Some(resumed_at);
        Ok(checkpoint)
    }

    fn ensure_task_in_scope(&self, task_id: &str) -> Result<(), AppError> {
        if self.get_task(task_id)?.is_some() {
            Ok(())
        } else {
            Err(AppError::TaskNotFound(task_id.into()))
        }
    }
}

fn sidecar_path(database: &Path, suffix: &str) -> std::path::PathBuf {
    let mut value = database.as_os_str().to_owned();
    value.push(suffix);
    value.into()
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
        version: row.get(7)?,
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
    if version < 2 {
        let transaction = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
        transaction.execute_batch(
            "ALTER TABLE tasks ADD COLUMN workspace_id TEXT;
             CREATE INDEX tasks_workspace_order ON tasks(workspace_id, updated_at DESC, task_id ASC);",
        )?;
        transaction.pragma_update(None, "user_version", 2)?;
        transaction.commit()?;
    }
    if version < 3 {
        let transaction = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
        transaction.execute_batch(
            "ALTER TABLE tasks ADD COLUMN version INTEGER NOT NULL DEFAULT 1 CHECK (version > 0);",
        )?;
        transaction.pragma_update(None, "user_version", 3)?;
        transaction.commit()?;
    }
    if version < 4 {
        let transaction = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
        transaction.execute_batch(
            "CREATE INDEX IF NOT EXISTS task_events_task_order ON task_events(task_id, sequence);",
        )?;
        transaction.pragma_update(None, "user_version", 4)?;
        transaction.commit()?;
    }
    if version < 5 {
        let transaction = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
        transaction.execute_batch(
            "CREATE TABLE context_checkpoints (
                checkpoint_id TEXT PRIMARY KEY,
                task_id TEXT NOT NULL REFERENCES tasks(task_id),
                workspace_id TEXT,
                task_version INTEGER NOT NULL CHECK (task_version > 0),
                content_json TEXT NOT NULL CHECK (json_valid(content_json)),
                content_sha256 TEXT NOT NULL CHECK (length(content_sha256) = 64),
                status TEXT NOT NULL CHECK (status IN ('ready', 'resumed')),
                created_by TEXT NOT NULL,
                created_at TEXT NOT NULL,
                resumed_by TEXT,
                resumed_at TEXT,
                CHECK ((status = 'ready' AND resumed_by IS NULL AND resumed_at IS NULL)
                    OR (status = 'resumed' AND resumed_by IS NOT NULL AND resumed_at IS NOT NULL))
             );
             CREATE INDEX context_checkpoints_task_order
                ON context_checkpoints(task_id, created_at DESC, checkpoint_id DESC);
             CREATE UNIQUE INDEX one_ready_checkpoint_per_task
                ON context_checkpoints(task_id) WHERE status = 'ready';",
        )?;
        transaction.pragma_update(None, "user_version", 5)?;
        transaction.commit()?;
    }
    Ok(())
}

fn validate_checkpoint(content: &CheckpointContent, actor: &str) -> Result<(), AppError> {
    if content.schema != crate::domain::CONTEXT_CHECKPOINT_SCHEMA
        || content.version != crate::domain::CONTEXT_CHECKPOINT_VERSION
    {
        return Err(AppError::InvalidInput(
            "unsupported context checkpoint schema".into(),
        ));
    }
    Uuid::parse_str(&content.checkpoint_id)
        .map_err(|_| AppError::InvalidInput("checkpoint_id must be a UUID".into()))?;
    if actor.trim().is_empty() || actor.chars().count() > 120 {
        return Err(AppError::InvalidInput("checkpoint actor is invalid".into()));
    }
    if !matches!(content.source.as_str(), "chatgpt_web" | "codex" | "unknown") {
        return Err(AppError::InvalidInput(
            "unsupported checkpoint source".into(),
        ));
    }
    if content.source_completeness != "user_curated_partial" {
        return Err(AppError::InvalidInput(
            "unsupported checkpoint completeness label".into(),
        ));
    }
    for (name, value, limit) in [
        ("goal", content.goal.as_str(), 2_000),
        ("decisions", content.decisions.as_str(), 2_500),
        ("completed", content.completed.as_str(), 1_500),
        ("blocked", content.blocked.as_str(), 1_500),
        ("next_actions", content.next_actions.as_str(), 1_500),
        ("constraints", content.constraints.as_str(), 1_500),
    ] {
        if value.chars().count() > limit {
            return Err(AppError::InvalidInput(format!(
                "{name} exceeds {limit} characters"
            )));
        }
    }
    if content.goal.trim().is_empty() {
        return Err(AppError::InvalidInput("goal must not be empty".into()));
    }
    if content.artifacts.len() > 12 {
        return Err(AppError::InvalidInput(
            "a checkpoint can contain at most 12 references".into(),
        ));
    }
    for artifact in &content.artifacts {
        if artifact.label.trim().is_empty() || artifact.locator.trim().is_empty() {
            return Err(AppError::InvalidInput(
                "artifact label and locator must not be empty".into(),
            ));
        }
        validate_length("artifact label", &artifact.label, 120)?;
        validate_length("artifact locator", &artifact.locator, 2_048)?;
    }
    let serialized = serde_json::to_string(content)?;
    if serialized.len() > 24 * 1024 {
        return Err(AppError::InvalidInput(
            "checkpoint package exceeds 24 KiB".into(),
        ));
    }
    Ok(())
}

fn checkpoint_from_row(
    task_id: &str,
    row: (
        String,
        String,
        String,
        String,
        String,
        Option<String>,
        Option<String>,
    ),
) -> Result<ContextCheckpoint, AppError> {
    if format!("{:x}", Sha256::digest(row.0.as_bytes())) != row.1 {
        return Err(AppError::InvalidInput(
            "checkpoint integrity hash does not match".into(),
        ));
    }
    let content: CheckpointContent = serde_json::from_str(&row.0)?;
    if content.task_id != task_id {
        return Err(AppError::InvalidInput(
            "checkpoint task reference is inconsistent".into(),
        ));
    }
    validate_checkpoint(&content, &row.3)?;
    Ok(ContextCheckpoint {
        content,
        content_sha256: row.1,
        status: row.2,
        created_by: row.3,
        created_at: row.4,
        resumed_by: row.5,
        resumed_at: row.6,
    })
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

    fn checkpoint_content(task_id: &str, checkpoint_id: &str) -> CheckpointContent {
        CheckpointContent {
            schema: crate::domain::CONTEXT_CHECKPOINT_SCHEMA.into(),
            version: crate::domain::CONTEXT_CHECKPOINT_VERSION.into(),
            checkpoint_id: checkpoint_id.into(),
            task_id: task_id.into(),
            source: "chatgpt_web".into(),
            source_completeness: "user_curated_partial".into(),
            goal: "Retomar la revisión sin perder las decisiones".into(),
            decisions: "Usar el checkpoint revisado".into(),
            completed: "Contrato implementado".into(),
            blocked: String::new(),
            next_actions: "Ejecutar las pruebas".into(),
            constraints: "No copiar secretos".into(),
            artifacts: vec![],
            context_capacity: "unknown; no ChatGPT web context telemetry is available".into(),
        }
    }

    #[test]
    fn checkpoint_is_immutable_idempotent_and_requires_explicit_resume() {
        let path = db_path();
        let mut store = Store::open(&path).unwrap();
        let task = store
            .create_task("review", "context", "human:cesar")
            .unwrap();
        let content = checkpoint_content(&task.task_id, "6fb8907e-7f4a-427c-835b-d4d39c833ffe");
        let first = store
            .create_context_checkpoint(content.clone(), "human:owner-1")
            .unwrap();
        assert_eq!(first.status, "ready");
        assert_eq!(first.content_sha256.len(), 64);
        assert_eq!(
            store
                .create_context_checkpoint(content.clone(), "human:owner-1")
                .unwrap(),
            first
        );

        let mut changed = content.clone();
        changed.goal = "cambio incompatible".into();
        assert!(matches!(
            store.create_context_checkpoint(changed, "human:owner-1"),
            Err(AppError::IdempotencyConflict(_))
        ));
        let second = checkpoint_content(&task.task_id, "9aabca60-d8a3-428b-9dcf-2c0ce29fb603");
        assert!(matches!(
            store.create_context_checkpoint(second, "human:owner-1"),
            Err(AppError::InvalidInput(_))
        ));

        let resumed = store
            .acknowledge_context_checkpoint(
                &task.task_id,
                &first.content.checkpoint_id,
                "human:owner-1",
            )
            .unwrap();
        assert_eq!(resumed.status, "resumed");
        assert_eq!(
            store
                .acknowledge_context_checkpoint(
                    &task.task_id,
                    &first.content.checkpoint_id,
                    "human:owner-1",
                )
                .unwrap(),
            resumed
        );
        assert!(matches!(
            store.acknowledge_context_checkpoint(
                &task.task_id,
                &first.content.checkpoint_id,
                "human:other"
            ),
            Err(AppError::IdempotencyConflict(_))
        ));
        // Retrying the original create after its explicit resume returns the same
        // historical row; it must not reopen it or append another event.
        assert_eq!(
            store
                .create_context_checkpoint(content, "human:owner-1")
                .unwrap(),
            resumed
        );
        let second = checkpoint_content(&task.task_id, "f6b9087e-7f4a-427c-835b-d4d39c833ffe");
        assert_eq!(
            store
                .create_context_checkpoint(second, "human:owner-1")
                .unwrap()
                .status,
            "ready"
        );
        assert_eq!(
            store.list_context_checkpoints(&task.task_id).unwrap().len(),
            2
        );
        let event_types = store
            .list_events(&task.task_id)
            .unwrap()
            .into_iter()
            .map(|event| event.event_type)
            .collect::<Vec<_>>();
        assert!(event_types.contains(&"context_checkpoint_ready".into()));
        assert!(event_types.contains(&"context_checkpoint_resumed".into()));
        assert_eq!(
            event_types
                .iter()
                .filter(|event| event.as_str() == "context_checkpoint_ready")
                .count(),
            2
        );
        assert_eq!(
            event_types
                .iter()
                .filter(|event| event.as_str() == "context_checkpoint_resumed")
                .count(),
            1
        );
    }

    #[test]
    fn checkpoint_read_rejects_tampered_persisted_content() {
        let path = db_path();
        let mut store = Store::open(&path).unwrap();
        let task = store
            .create_task("review", "context", "human:cesar")
            .unwrap();
        let content = checkpoint_content(&task.task_id, "6fb8907e-7f4a-427c-835b-d4d39c833ffe");
        store
            .create_context_checkpoint(content, "human:owner-1")
            .unwrap();
        store
            .connection
            .execute(
                "UPDATE context_checkpoints SET content_json = replace(content_json, 'Usar el checkpoint revisado', 'Contenido alterado')",
                [],
            )
            .unwrap();

        assert!(matches!(
            store.list_context_checkpoints(&task.task_id),
            Err(AppError::InvalidInput(message)) if message.contains("integrity hash")
        ));
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
    fn online_backup_restores_tasks_messages_and_ordered_events() {
        let source = db_path();
        let backup_dir = tempfile::tempdir().unwrap();
        let backup = backup_dir.path().join("state.sqlite3");
        let restored = backup_dir.path().join("restored.sqlite3");
        let mut store = Store::open(&source).unwrap();
        let journal_mode: String = store
            .connection
            .query_row("PRAGMA journal_mode", [], |row| row.get(0))
            .unwrap();
        assert_eq!(journal_mode.to_ascii_lowercase(), "wal");
        let task = store
            .create_task("recover me", "restore exercise", "human:local")
            .unwrap();
        store
            .transition_task(
                &task.task_id,
                TaskStatus::Open,
                TaskStatus::Proposed,
                "agent:codex",
                "restore-1",
            )
            .unwrap();
        store
            .insert_message(&new_message(
                &task.task_id,
                "restore-message",
                "durable evidence",
            ))
            .unwrap();

        store.backup_to(&backup).unwrap();
        Store::restore_from_backup(&backup, &restored).unwrap();
        let restored_store = Store::open(&restored).unwrap();
        assert_eq!(
            restored_store
                .get_task(&task.task_id)
                .unwrap()
                .unwrap()
                .status,
            TaskStatus::Proposed
        );
        assert_eq!(
            restored_store.list_messages(&task.task_id).unwrap()[0].message_id,
            "restore-message"
        );
        let events = restored_store.list_events(&task.task_id).unwrap();
        assert_eq!(events.len(), 3);
        assert!(
            events
                .windows(2)
                .all(|pair| pair[0].sequence < pair[1].sequence)
        );
        assert!(Store::restore_from_backup(&backup, &restored).is_err());
    }

    #[tokio::test]
    async fn online_backup_restores_identity_session_invite_and_task_history() {
        let source = db_path();
        let backup_dir = tempfile::tempdir().unwrap();
        let backup = backup_dir.path().join("hosted-state.sqlite3");
        let restored = backup_dir.path().join("hosted-restored.sqlite3");

        let mut store = Store::open(&source).unwrap();
        let journal_mode: String = store
            .connection
            .query_row("PRAGMA journal_mode", [], |row| row.get(0))
            .unwrap();
        assert_eq!(journal_mode.to_ascii_lowercase(), "wal");
        let task = store
            .create_task("recover hosted state", "identity and audit", "human:legacy")
            .unwrap();
        store
            .transition_task(
                &task.task_id,
                TaskStatus::Open,
                TaskStatus::Proposed,
                "agent:codex",
                "recovery-correlation",
            )
            .unwrap();

        let identities = crate::identity::IdentityStore::connect_sqlite(&source, 1)
            .await
            .unwrap();
        let (owner_id, workspace) = identities
            .bootstrap_first_owner(
                "https://issuer.example.test",
                "owner-subject",
                "owner@example.test",
                "Recovery workspace",
            )
            .await
            .unwrap();
        let owner_session = identities.create_session(&owner_id).await.unwrap();
        identities
            .create_auth_transaction("restore-state", "restore-nonce", "restore-verifier")
            .await
            .unwrap();
        let invitation = identities
            .create_invite(
                &owner_id,
                Some(&owner_session.session_id),
                &workspace.workspace_id,
                "editor@example.test",
                crate::identity::InviteRole::Editor,
            )
            .await
            .unwrap();
        let editor_id = identities
            .authenticate_identity(
                "https://issuer.example.test",
                "editor-subject",
                Some("editor@example.test"),
                true,
            )
            .await
            .unwrap();

        store.backup_to(&backup).unwrap();
        Store::restore_from_backup(&backup, &restored).unwrap();

        let restored_store = Store::open(&restored).unwrap();
        assert_eq!(
            restored_store
                .get_task(&task.task_id)
                .unwrap()
                .unwrap()
                .status,
            TaskStatus::Proposed
        );
        assert_eq!(restored_store.list_events(&task.task_id).unwrap().len(), 2);
        let restored_identities = crate::identity::IdentityStore::connect_sqlite(&restored, 1)
            .await
            .unwrap();
        let membership = restored_identities
            .membership(&owner_id, &workspace.workspace_id)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(membership.role, crate::identity::WorkspaceRole::Owner);
        let invalidated = restored_identities
            .invalidate_auth_state_after_restore()
            .await
            .unwrap();
        assert_eq!(invalidated.sessions_revoked, 1);
        assert_eq!(invalidated.login_transactions_consumed, 1);
        assert_eq!(invalidated.pending_invites_revoked, 1);
        assert!(
            restored_identities
                .resolve_session(&owner_session.raw_token)
                .await
                .unwrap()
                .is_none()
        );
        assert!(
            restored_identities
                .consume_auth_transaction("restore-state")
                .await
                .unwrap()
                .is_none()
        );
        assert!(matches!(
            restored_identities
                .accept_invite(&invitation.raw_token, &editor_id, None)
                .await,
            Err(crate::identity::IdentityError::NotFound)
        ));
        let recovery_events: i64 = Connection::open(&restored)
            .unwrap()
            .query_row(
                "SELECT COUNT(*) FROM identity_audit_events
                 WHERE action = 'recovery.auth_state_invalidated' AND reason_code = 'database_restore'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(recovery_events, 1);
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
        assert_eq!(version, 5);
    }

    #[test]
    fn task_event_history_uses_workspace_task_order_index() {
        let path = db_path();
        let store = Store::open(&path).unwrap();
        let plan: String = store
            .connection
            .query_row(
                "EXPLAIN QUERY PLAN
                 SELECT sequence, task_id, event_type, actor, correlation_id, details_json, occurred_at
                 FROM task_events WHERE task_id = 'task-1' ORDER BY sequence ASC",
                [],
                |row| row.get(3),
            )
            .unwrap();

        assert!(
            plan.contains("task_events_task_order"),
            "query plan: {plan}"
        );
    }

    #[test]
    fn workspace_scope_hides_other_workspaces_and_legacy_local_records() {
        let path = db_path();
        let mut store = Store::open(&path).unwrap();
        let legacy = store.create_task("legacy", "old", "human:cesar").unwrap();
        store.set_workspace_context(Some("workspace-a"));
        let task_a = store.create_task("A", "private A", "human:user-a").unwrap();
        store.set_workspace_context(Some("workspace-b"));
        let task_b = store.create_task("B", "private B", "human:user-b").unwrap();
        assert_eq!(store.list_tasks().unwrap().len(), 1);
        assert!(store.get_task(&task_a.task_id).unwrap().is_none());
        assert!(matches!(
            store.transition_task(
                &task_a.task_id,
                TaskStatus::Open,
                TaskStatus::Proposed,
                "human:user-b",
                "scope-test"
            ),
            Err(AppError::TaskNotFound(_))
        ));
        store.set_workspace_context(Some("workspace-a"));
        assert_eq!(store.list_tasks().unwrap()[0].task_id, task_a.task_id);
        assert!(store.get_task(&legacy.task_id).unwrap().is_none());
        store.set_workspace_context(Some("workspace-b"));
        assert_eq!(store.list_tasks().unwrap()[0].task_id, task_b.task_id);
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
