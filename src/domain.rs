use serde::{Deserialize, Serialize};
use thiserror::Error;

pub const MESSAGE_SCHEMA_VERSION: u16 = 1;

#[derive(Debug, Error)]
pub enum AppError {
    #[error("database error: {0}")]
    Database(#[from] rusqlite::Error),
    #[error("invalid input: {0}")]
    InvalidInput(String),
    #[error("task {0} was not found")]
    TaskNotFound(String),
    #[error("message {0} was not found")]
    MessageNotFound(String),
    #[error("decision {0} was not found or is no longer pending")]
    DecisionNotPending(String),
    #[error("stale task state: expected {expected}, found {actual}")]
    StaleState { expected: String, actual: String },
    #[error("transition from {from} to {to} is not allowed")]
    InvalidTransition { from: String, to: String },
    #[error("task has an unanswered human decision")]
    PendingDecision,
    #[error("message id {0} already exists with different content")]
    IdempotencyConflict(String),
    #[error("a human decision must be answered by human:cesar")]
    HumanApprovalRequired,
    #[error("serialization error: {0}")]
    Serialization(#[from] serde_json::Error),
    #[error("internal worker error: {0}")]
    Internal(String),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TaskStatus {
    Open,
    Proposed,
    UnderReview,
    Resolved,
    Approved,
}

impl TaskStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Open => "open",
            Self::Proposed => "proposed",
            Self::UnderReview => "under_review",
            Self::Resolved => "resolved",
            Self::Approved => "approved",
        }
    }

    pub fn can_transition_to(self, next: Self) -> bool {
        matches!(
            (self, next),
            (Self::Open, Self::Proposed)
                | (Self::Proposed, Self::UnderReview)
                | (Self::UnderReview, Self::Proposed)
                | (Self::UnderReview, Self::Resolved)
                | (Self::Resolved, Self::Approved)
        )
    }
}

impl std::fmt::Display for TaskStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

impl std::str::FromStr for TaskStatus {
    type Err = AppError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value {
            "open" => Ok(Self::Open),
            "proposed" => Ok(Self::Proposed),
            "under_review" => Ok(Self::UnderReview),
            "resolved" => Ok(Self::Resolved),
            "approved" => Ok(Self::Approved),
            _ => Err(AppError::InvalidInput(format!(
                "unknown task status: {value}"
            ))),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EvidenceRef {
    pub label: String,
    pub locator: String,
    pub sha256: String,
}

impl EvidenceRef {
    pub fn validate(&self) -> Result<(), AppError> {
        if self.label.trim().is_empty() || self.locator.trim().is_empty() {
            return Err(AppError::InvalidInput(
                "evidence label and locator must not be empty".into(),
            ));
        }
        if self.label.chars().count() > 120 || self.locator.chars().count() > 2048 {
            return Err(AppError::InvalidInput(
                "evidence label or locator exceeds its length limit".into(),
            ));
        }
        if self.sha256.len() != 64 || !self.sha256.bytes().all(|b| b.is_ascii_hexdigit()) {
            return Err(AppError::InvalidInput(
                "evidence sha256 must contain exactly 64 hexadecimal characters".into(),
            ));
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MessageStatus {
    Recorded,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Message {
    pub schema_version: u16,
    pub message_id: String,
    pub task_id: String,
    pub sender: String,
    pub recipient: String,
    pub correlation_id: String,
    pub parent_id: Option<String>,
    pub status: MessageStatus,
    pub body: String,
    pub evidence_refs: Vec<EvidenceRef>,
    pub created_at: String,
}

impl Message {
    pub fn validate(&self) -> Result<(), AppError> {
        if self.schema_version != MESSAGE_SCHEMA_VERSION {
            return Err(AppError::InvalidInput(format!(
                "unsupported message schema version {}; supported version is {}",
                self.schema_version, MESSAGE_SCHEMA_VERSION
            )));
        }
        for (name, value) in [
            ("message_id", self.message_id.as_str()),
            ("task_id", self.task_id.as_str()),
            ("sender", self.sender.as_str()),
            ("recipient", self.recipient.as_str()),
            ("correlation_id", self.correlation_id.as_str()),
            ("body", self.body.as_str()),
        ] {
            if value.trim().is_empty() {
                return Err(AppError::InvalidInput(format!("{name} must not be empty")));
            }
        }
        for (name, value, limit) in [
            ("message_id", self.message_id.as_str(), 128),
            ("task_id", self.task_id.as_str(), 128),
            ("sender", self.sender.as_str(), 120),
            ("recipient", self.recipient.as_str(), 120),
            ("correlation_id", self.correlation_id.as_str(), 128),
            ("body", self.body.as_str(), 12_000),
        ] {
            if value.chars().count() > limit {
                return Err(AppError::InvalidInput(format!(
                    "{name} exceeds {limit} characters"
                )));
            }
        }
        if self.evidence_refs.len() > 32 {
            return Err(AppError::InvalidInput(
                "a message can contain at most 32 evidence references".into(),
            ));
        }
        for evidence in &self.evidence_refs {
            evidence.validate()?;
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Task {
    pub task_id: String,
    pub title: String,
    pub description: String,
    pub status: TaskStatus,
    pub pending_decision: bool,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskEvent {
    pub sequence: i64,
    pub task_id: String,
    pub event_type: String,
    pub actor: String,
    pub correlation_id: String,
    pub details: serde_json::Value,
    pub occurred_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HumanDecision {
    pub decision_id: String,
    pub task_id: String,
    pub question: String,
    pub requested_by: String,
    pub status: String,
    pub created_at: String,
}
