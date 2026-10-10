use std::path::Path;
use std::str::FromStr;
use std::time::Duration;

use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use chrono::{SecondsFormat, Utc};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use sqlx::{AnyPool, Row, any::AnyPoolOptions, any::install_default_drivers, migrate::Migrator};
use thiserror::Error;
use uuid::Uuid;

static MIGRATOR: Migrator = sqlx::migrate!("./migrations");

const IDLE_SESSION_SECONDS: i64 = 30 * 60;
const ABSOLUTE_SESSION_SECONDS: i64 = 12 * 60 * 60;
const INVITE_TTL_SECONDS: i64 = 7 * 24 * 60 * 60;

#[derive(Debug, Error)]
pub enum IdentityError {
    #[error("identity database operation failed: {0}")]
    Database(#[from] sqlx::Error),
    #[error("identity database migration failed: {0}")]
    Migration(#[from] sqlx::migrate::MigrateError),
    #[error("invalid identity input: {0}")]
    Invalid(String),
    #[error("identity or resource was not found")]
    NotFound,
    #[error("identity operation is not authorized")]
    Forbidden,
    #[error("identity state conflicts with this operation")]
    Conflict,
    #[error("secure random number generation failed")]
    Random,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum WorkspaceRole {
    Owner,
    Editor,
    Viewer,
}

impl WorkspaceRole {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Owner => "OWNER",
            Self::Editor => "EDITOR",
            Self::Viewer => "VIEWER",
        }
    }

    fn parse(value: &str) -> Result<Self, IdentityError> {
        match value {
            "OWNER" => Ok(Self::Owner),
            "EDITOR" => Ok(Self::Editor),
            "VIEWER" => Ok(Self::Viewer),
            _ => Err(IdentityError::Invalid("unknown workspace role".into())),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum InviteRole {
    Editor,
    Viewer,
}

impl InviteRole {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Editor => "EDITOR",
            Self::Viewer => "VIEWER",
        }
    }

    pub fn parse(value: &str) -> Result<Self, IdentityError> {
        match value {
            "EDITOR" => Ok(Self::Editor),
            "VIEWER" => Ok(Self::Viewer),
            _ => Err(IdentityError::Invalid(
                "invites support EDITOR or VIEWER".into(),
            )),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Workspace {
    pub workspace_id: String,
    pub name: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Membership {
    pub workspace_id: String,
    pub user_id: String,
    pub role: WorkspaceRole,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AuthenticatedPrincipal {
    pub user_id: String,
    pub session_id: String,
    pub issuer: String,
    pub subject: String,
    pub email: Option<String>,
    pub authenticated_at: String,
}

#[derive(Debug)]
pub struct SessionCookie {
    pub raw_token: String,
    pub csrf_token: String,
    pub session_id: String,
    pub principal: AuthenticatedPrincipal,
}

#[derive(Debug, Serialize)]
pub struct InviteToken {
    pub invite_id: String,
    pub raw_token: String,
    pub expires_at: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct WorkspaceMember {
    pub user_id: String,
    pub email: Option<String>,
    pub role: WorkspaceRole,
}

#[derive(Debug, Clone)]
pub struct AuthTransaction {
    pub nonce: String,
    pub pkce_verifier: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RecoveryInvalidation {
    pub sessions_revoked: u64,
    pub login_transactions_consumed: u64,
    pub pending_invites_revoked: u64,
}

#[derive(Clone)]
pub struct IdentityStore {
    pool: AnyPool,
}

impl IdentityStore {
    pub async fn connect(database_url: &str, max_connections: u32) -> Result<Self, IdentityError> {
        install_default_drivers();
        let pool = AnyPoolOptions::new()
            .max_connections(max_connections.max(1))
            .acquire_timeout(Duration::from_secs(5))
            .connect(database_url)
            .await?;
        MIGRATOR.run(&pool).await?;
        Ok(Self { pool })
    }

    pub async fn connect_sqlite(
        path: impl AsRef<Path>,
        max_connections: u32,
    ) -> Result<Self, IdentityError> {
        install_default_drivers();
        let database_path = path.as_ref().to_string_lossy().replace('\\', "/");
        let database_url = format!("sqlite:{database_path}?mode=rwc");
        let any_options = sqlx::any::AnyConnectOptions::from_str(&database_url)?;
        let pool = AnyPoolOptions::new()
            .max_connections(max_connections.max(1))
            .acquire_timeout(Duration::from_secs(5))
            .connect_with(any_options)
            .await?;
        MIGRATOR.run(&pool).await?;
        Ok(Self { pool })
    }

    pub async fn ping(&self) -> Result<(), IdentityError> {
        sqlx::query("SELECT 1").execute(&self.pool).await?;
        Ok(())
    }

    #[cfg(test)]
    pub(crate) async fn close_for_test(&self) {
        self.pool.close().await;
    }

    pub async fn provision_identity(
        &self,
        issuer: &str,
        subject: &str,
        email: Option<&str>,
        email_verified: bool,
    ) -> Result<String, IdentityError> {
        validate_identity_claim(issuer, "issuer")?;
        validate_identity_claim(subject, "subject")?;
        if email.is_some_and(|value| value.len() > 320 || value.trim().is_empty()) {
            return Err(IdentityError::Invalid("email is invalid".into()));
        }
        let user_id = Uuid::new_v4().to_string();
        sqlx::query(
            "INSERT INTO identity_users (user_id, issuer, subject, email, email_verified, created_at)
             VALUES ($1, $2, $3, $4, $5, $6)",
        )
        .bind(&user_id)
        .bind(issuer)
        .bind(subject)
        .bind(email.map(str::trim))
        .bind(i64::from(email_verified))
        .bind(now())
        .execute(&self.pool)
        .await
        .map_err(map_identity_unique)?;
        Ok(user_id)
    }

    /// Creates the first verified Owner and workspace exactly once via an explicit operator action.
    pub async fn bootstrap_first_owner(
        &self,
        issuer: &str,
        subject: &str,
        email: &str,
        workspace_name: &str,
    ) -> Result<(String, Workspace), IdentityError> {
        validate_identity_claim(issuer, "issuer")?;
        validate_identity_claim(subject, "subject")?;
        let normalized_email = normalize_email(email)?;
        if workspace_name.trim().is_empty() || workspace_name.chars().count() > 120 {
            return Err(IdentityError::Invalid(
                "workspace name must be 1–120 characters".into(),
            ));
        }
        let timestamp = now();
        let user_id = Uuid::new_v4().to_string();
        let workspace = Workspace {
            workspace_id: Uuid::new_v4().to_string(),
            name: workspace_name.trim().to_owned(),
        };
        let mut transaction = self.pool.begin().await?;
        let reservation = sqlx::query(
            "INSERT INTO identity_bootstrap (singleton, completed_at) VALUES (1, $1)
             ON CONFLICT (singleton) DO NOTHING",
        )
        .bind(&timestamp)
        .execute(&mut *transaction)
        .await?;
        if reservation.rows_affected() != 1 {
            return Err(IdentityError::Conflict);
        }
        sqlx::query(
            "INSERT INTO identity_users (user_id, issuer, subject, email, email_verified, created_at)
             VALUES ($1, $2, $3, $4, 1, $5)",
        )
        .bind(&user_id)
        .bind(issuer)
        .bind(subject)
        .bind(normalized_email)
        .bind(&timestamp)
        .execute(&mut *transaction)
        .await
        .map_err(map_identity_unique)?;
        sqlx::query("INSERT INTO workspaces (workspace_id, name, created_at) VALUES ($1, $2, $3)")
            .bind(&workspace.workspace_id)
            .bind(&workspace.name)
            .bind(&timestamp)
            .execute(&mut *transaction)
            .await?;
        sqlx::query(
            "INSERT INTO workspace_memberships (workspace_id, user_id, role, created_at)
             VALUES ($1, $2, 'OWNER', $3)",
        )
        .bind(&workspace.workspace_id)
        .bind(&user_id)
        .bind(&timestamp)
        .execute(&mut *transaction)
        .await?;
        audit(
            &mut transaction,
            &user_id,
            None,
            Some(&workspace.workspace_id),
            "identity.bootstrap.owner",
            "workspace_membership",
            &user_id,
            &timestamp,
        )
        .await?;
        transaction.commit().await?;
        Ok((user_id, workspace))
    }

    pub async fn authenticate_identity(
        &self,
        issuer: &str,
        subject: &str,
        email: Option<&str>,
        email_verified: bool,
    ) -> Result<String, IdentityError> {
        validate_identity_claim(issuer, "issuer")?;
        validate_identity_claim(subject, "subject")?;
        if email.is_some_and(|value| value.len() > 320 || value.trim().is_empty()) {
            return Err(IdentityError::Invalid("email is invalid".into()));
        }
        let mut transaction = self.pool.begin().await?;
        let existing = sqlx::query(
            "SELECT user_id, disabled_at FROM identity_users WHERE issuer = $1 AND subject = $2",
        )
        .bind(issuer)
        .bind(subject)
        .fetch_optional(&mut *transaction)
        .await?;
        let user_id = if let Some(row) = existing {
            if row.try_get::<Option<String>, _>("disabled_at")?.is_some() {
                return Err(IdentityError::Forbidden);
            }
            let user_id: String = row.try_get("user_id")?;
            sqlx::query(
                "UPDATE identity_users SET email = $1, email_verified = $2 WHERE user_id = $3",
            )
            .bind(email.map(str::trim))
            .bind(i64::from(email_verified))
            .bind(&user_id)
            .execute(&mut *transaction)
            .await?;
            user_id
        } else {
            if !email_verified {
                return Err(IdentityError::Forbidden);
            }
            let normalized_email = normalize_email(email.ok_or(IdentityError::Forbidden)?)?;
            let pending_invite: i64 = sqlx::query_scalar(
                "SELECT count(*) FROM workspace_invites
                 WHERE email_normalized = $1 AND accepted_at IS NULL AND revoked_at IS NULL AND expires_at > $2",
            )
            .bind(&normalized_email)
            .bind(now())
            .fetch_one(&mut *transaction)
            .await?;
            if pending_invite == 0 {
                return Err(IdentityError::Forbidden);
            }
            let user_id = Uuid::new_v4().to_string();
            sqlx::query(
                "INSERT INTO identity_users (user_id, issuer, subject, email, email_verified, created_at)
                 VALUES ($1, $2, $3, $4, $5, $6)",
            )
            .bind(&user_id)
            .bind(issuer)
            .bind(subject)
            .bind(email.map(str::trim))
            .bind(i64::from(email_verified))
            .bind(now())
            .execute(&mut *transaction)
            .await
            .map_err(map_identity_unique)?;
            user_id
        };
        transaction.commit().await?;
        Ok(user_id)
    }

    pub async fn create_auth_transaction(
        &self,
        state: &str,
        nonce: &str,
        pkce_verifier: &str,
    ) -> Result<(), IdentityError> {
        let timestamp = Utc::now();
        let created_at = format_timestamp(timestamp);
        sqlx::query(
            "DELETE FROM auth_transactions WHERE expires_at <= $1 OR consumed_at IS NOT NULL",
        )
        .bind(&created_at)
        .execute(&self.pool)
        .await?;
        sqlx::query(
            "INSERT INTO auth_transactions (state_hash, nonce, pkce_verifier, created_at, expires_at)
             VALUES ($1, $2, $3, $4, $5)",
        )
        .bind(hash_token(state))
        .bind(nonce)
        .bind(pkce_verifier)
        .bind(created_at)
        .bind(format_timestamp(timestamp + chrono::Duration::minutes(10)))
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    pub async fn consume_auth_transaction(
        &self,
        state: &str,
    ) -> Result<Option<AuthTransaction>, IdentityError> {
        if state.is_empty() || state.len() > 256 {
            return Ok(None);
        }
        let timestamp = now();
        let mut transaction = self.pool.begin().await?;
        let row = sqlx::query(
            "UPDATE auth_transactions SET consumed_at = $1
             WHERE state_hash = $2 AND consumed_at IS NULL AND expires_at > $1
             RETURNING nonce, pkce_verifier",
        )
        .bind(timestamp)
        .bind(hash_token(state))
        .fetch_optional(&mut *transaction)
        .await?;
        transaction.commit().await?;
        row.map(|row| {
            Ok(AuthTransaction {
                nonce: row.try_get("nonce")?,
                pkce_verifier: row.try_get("pkce_verifier")?,
            })
        })
        .transpose()
    }

    pub async fn create_workspace_for_owner(
        &self,
        owner_user_id: &str,
        name: &str,
    ) -> Result<Workspace, IdentityError> {
        if name.trim().is_empty() || name.chars().count() > 120 {
            return Err(IdentityError::Invalid(
                "workspace name must be 1–120 characters".into(),
            ));
        }
        let workspace = Workspace {
            workspace_id: Uuid::new_v4().to_string(),
            name: name.trim().to_owned(),
        };
        let timestamp = now();
        let mut transaction = self.pool.begin().await?;
        let active_user: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM identity_users WHERE user_id = $1 AND disabled_at IS NULL",
        )
        .bind(owner_user_id)
        .fetch_one(&mut *transaction)
        .await?;
        if active_user != 1 {
            return Err(IdentityError::NotFound);
        }
        sqlx::query("INSERT INTO workspaces (workspace_id, name, created_at) VALUES ($1, $2, $3)")
            .bind(&workspace.workspace_id)
            .bind(&workspace.name)
            .bind(&timestamp)
            .execute(&mut *transaction)
            .await?;
        sqlx::query(
            "INSERT INTO workspace_memberships (workspace_id, user_id, role, created_at)
             VALUES ($1, $2, 'OWNER', $3)",
        )
        .bind(&workspace.workspace_id)
        .bind(owner_user_id)
        .bind(timestamp)
        .execute(&mut *transaction)
        .await?;
        transaction.commit().await?;
        Ok(workspace)
    }

    pub async fn list_workspaces_for_user(
        &self,
        user_id: &str,
    ) -> Result<Vec<(Workspace, WorkspaceRole)>, IdentityError> {
        let rows = sqlx::query(
            "SELECT w.workspace_id, w.name, m.role
             FROM workspaces w
             JOIN workspace_memberships m ON m.workspace_id = w.workspace_id
             JOIN identity_users u ON u.user_id = m.user_id
             WHERE m.user_id = $1 AND m.revoked_at IS NULL AND u.disabled_at IS NULL
             ORDER BY w.created_at, w.workspace_id",
        )
        .bind(user_id)
        .fetch_all(&self.pool)
        .await?;
        rows.iter()
            .map(|row| {
                Ok((
                    Workspace {
                        workspace_id: row.try_get("workspace_id")?,
                        name: row.try_get("name")?,
                    },
                    WorkspaceRole::parse(&row.try_get::<String, _>("role")?)?,
                ))
            })
            .collect()
    }

    pub async fn membership(
        &self,
        user_id: &str,
        workspace_id: &str,
    ) -> Result<Option<Membership>, IdentityError> {
        let row = sqlx::query(
            "SELECT workspace_id, user_id, role FROM workspace_memberships
             WHERE user_id = $1 AND workspace_id = $2 AND revoked_at IS NULL",
        )
        .bind(user_id)
        .bind(workspace_id)
        .fetch_optional(&self.pool)
        .await?;
        row.map(|row| {
            Ok(Membership {
                workspace_id: row.try_get("workspace_id")?,
                user_id: row.try_get("user_id")?,
                role: WorkspaceRole::parse(&row.try_get::<String, _>("role")?)?,
            })
        })
        .transpose()
    }

    pub async fn list_members(
        &self,
        workspace_id: &str,
    ) -> Result<Vec<WorkspaceMember>, IdentityError> {
        let rows = sqlx::query(
            "SELECT m.user_id, u.email, m.role FROM workspace_memberships m
             JOIN identity_users u ON u.user_id = m.user_id
             WHERE m.workspace_id = $1 AND m.revoked_at IS NULL AND u.disabled_at IS NULL
             ORDER BY CASE m.role WHEN 'OWNER' THEN 0 WHEN 'EDITOR' THEN 1 ELSE 2 END, m.user_id",
        )
        .bind(workspace_id)
        .fetch_all(&self.pool)
        .await?;
        rows.iter()
            .map(|row| {
                Ok(WorkspaceMember {
                    user_id: row.try_get("user_id")?,
                    email: row.try_get("email")?,
                    role: WorkspaceRole::parse(&row.try_get::<String, _>("role")?)?,
                })
            })
            .collect()
    }

    pub async fn create_session(&self, user_id: &str) -> Result<SessionCookie, IdentityError> {
        let user = sqlx::query(
            "SELECT issuer, subject, email FROM identity_users
             WHERE user_id = $1 AND disabled_at IS NULL",
        )
        .bind(user_id)
        .fetch_optional(&self.pool)
        .await?
        .ok_or(IdentityError::NotFound)?;
        let session_id = Uuid::new_v4().to_string();
        let raw_token = random_token()?;
        let timestamp = Utc::now();
        let created_at = format_timestamp(timestamp);
        let idle_expires_at =
            format_timestamp(timestamp + chrono::Duration::seconds(IDLE_SESSION_SECONDS));
        let absolute_expires_at =
            format_timestamp(timestamp + chrono::Duration::seconds(ABSOLUTE_SESSION_SECONDS));
        let mut transaction = self.pool.begin().await?;
        sqlx::query(
            "INSERT INTO auth_sessions
                (session_id, user_id, token_hash, created_at, last_seen_at, idle_expires_at, absolute_expires_at)
             VALUES ($1, $2, $3, $4, $4, $5, $6)",
        )
        .bind(&session_id)
        .bind(user_id)
        .bind(hash_token(&raw_token))
        .bind(&created_at)
        .bind(&idle_expires_at)
        .bind(&absolute_expires_at)
        .execute(&mut *transaction)
        .await?;
        audit(
            &mut transaction,
            user_id,
            Some(&session_id),
            None,
            "auth.login",
            "auth_session",
            &session_id,
            &created_at,
        )
        .await?;
        transaction.commit().await?;
        let csrf_token = derive_csrf_token(&raw_token);
        Ok(SessionCookie {
            raw_token,
            csrf_token,
            session_id: session_id.clone(),
            principal: AuthenticatedPrincipal {
                user_id: user_id.to_owned(),
                session_id,
                issuer: user.try_get("issuer")?,
                subject: user.try_get("subject")?,
                email: user.try_get("email")?,
                authenticated_at: created_at,
            },
        })
    }

    pub async fn resolve_session(
        &self,
        raw_token: &str,
    ) -> Result<Option<AuthenticatedPrincipal>, IdentityError> {
        if raw_token.len() > 128 || raw_token.is_empty() {
            return Ok(None);
        }
        let timestamp = now();
        let row = sqlx::query(
            "SELECT s.session_id, s.user_id, s.created_at, s.absolute_expires_at,
                    u.issuer, u.subject, u.email
             FROM auth_sessions s JOIN identity_users u ON u.user_id = s.user_id
             WHERE s.token_hash = $1 AND s.revoked_at IS NULL AND u.disabled_at IS NULL
               AND s.idle_expires_at > $2 AND s.absolute_expires_at > $2",
        )
        .bind(hash_token(raw_token))
        .bind(&timestamp)
        .fetch_optional(&self.pool)
        .await?;
        let Some(row) = row else {
            self.revoke_expired_session(raw_token, &timestamp).await?;
            return Ok(None);
        };
        let session_id: String = row.try_get("session_id")?;
        let absolute_expires_at: String = row.try_get("absolute_expires_at")?;
        let next_idle_expiry =
            format_timestamp(Utc::now() + chrono::Duration::seconds(IDLE_SESSION_SECONDS))
                .min(absolute_expires_at);
        let updated = sqlx::query(
            "UPDATE auth_sessions SET last_seen_at = $1, idle_expires_at = $2
             WHERE session_id = $3 AND revoked_at IS NULL",
        )
        .bind(&timestamp)
        .bind(next_idle_expiry)
        .bind(&session_id)
        .execute(&self.pool)
        .await?;
        if updated.rows_affected() != 1 {
            return Ok(None);
        }
        Ok(Some(AuthenticatedPrincipal {
            user_id: row.try_get("user_id")?,
            session_id,
            issuer: row.try_get("issuer")?,
            subject: row.try_get("subject")?,
            email: row.try_get("email")?,
            authenticated_at: row.try_get("created_at")?,
        }))
    }

    pub async fn revoke_session(&self, raw_token: &str) -> Result<(), IdentityError> {
        let timestamp = now();
        let mut transaction = self.pool.begin().await?;
        let session = sqlx::query(
            "SELECT session_id, user_id FROM auth_sessions
             WHERE token_hash = $1 AND revoked_at IS NULL",
        )
        .bind(hash_token(raw_token))
        .fetch_optional(&mut *transaction)
        .await?;
        if let Some(session) = session {
            let session_id: String = session.try_get("session_id")?;
            let user_id: String = session.try_get("user_id")?;
            sqlx::query("UPDATE auth_sessions SET revoked_at = $1 WHERE session_id = $2")
                .bind(&timestamp)
                .bind(&session_id)
                .execute(&mut *transaction)
                .await?;
            audit(
                &mut transaction,
                &user_id,
                Some(&session_id),
                None,
                "auth.logout",
                "auth_session",
                &session_id,
                &timestamp,
            )
            .await?;
        }
        transaction.commit().await?;
        Ok(())
    }

    /// Invalidates credentials that could have been resurrected by restoring an older snapshot.
    pub async fn invalidate_auth_state_after_restore(
        &self,
    ) -> Result<RecoveryInvalidation, IdentityError> {
        let timestamp = now();
        let recovery_id = Uuid::new_v4().to_string();
        let mut transaction = self.pool.begin().await?;
        let sessions =
            sqlx::query("UPDATE auth_sessions SET revoked_at = $1 WHERE revoked_at IS NULL")
                .bind(&timestamp)
                .execute(&mut *transaction)
                .await?
                .rows_affected();
        let login_transactions =
            sqlx::query("UPDATE auth_transactions SET consumed_at = $1 WHERE consumed_at IS NULL")
                .bind(&timestamp)
                .execute(&mut *transaction)
                .await?
                .rows_affected();
        let invites = sqlx::query(
            "UPDATE workspace_invites SET revoked_at = $1
             WHERE accepted_at IS NULL AND revoked_at IS NULL",
        )
        .bind(&timestamp)
        .execute(&mut *transaction)
        .await?
        .rows_affected();
        let invalidated = RecoveryInvalidation {
            sessions_revoked: sessions,
            login_transactions_consumed: login_transactions,
            pending_invites_revoked: invites,
        };
        sqlx::query(
            "INSERT INTO identity_audit_events
             (event_id, occurred_at, actor_user_id, workspace_id, action, resource_type,
              resource_id, result, reason_code, correlation_id, causation_id, details_json)
             VALUES ($1, $2, NULL, NULL, 'recovery.auth_state_invalidated', 'recovery',
                     $1, 'allowed', 'database_restore', $1, NULL, $3)",
        )
        .bind(&recovery_id)
        .bind(&timestamp)
        .bind(
            serde_json::json!({
                "sessions_revoked": invalidated.sessions_revoked,
                "login_transactions_consumed": invalidated.login_transactions_consumed,
                "pending_invites_revoked": invalidated.pending_invites_revoked,
            })
            .to_string(),
        )
        .execute(&mut *transaction)
        .await?;
        transaction.commit().await?;
        Ok(invalidated)
    }

    pub async fn create_invite(
        &self,
        actor_user_id: &str,
        actor_session_id: Option<&str>,
        workspace_id: &str,
        email: &str,
        role: InviteRole,
    ) -> Result<InviteToken, IdentityError> {
        let email_normalized = normalize_email(email)?;
        let invite_id = Uuid::new_v4().to_string();
        let raw_token = random_token()?;
        let timestamp = Utc::now();
        let created_at = format_timestamp(timestamp);
        let expires_at =
            format_timestamp(timestamp + chrono::Duration::seconds(INVITE_TTL_SECONDS));
        let mut transaction = self.pool.begin().await?;
        require_owner(&mut transaction, actor_user_id, workspace_id).await?;
        sqlx::query(
            "INSERT INTO workspace_invites
             (invite_id, workspace_id, email_normalized, role, token_hash, invited_by, created_at, expires_at)
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8)",
        )
        .bind(&invite_id)
        .bind(workspace_id)
        .bind(&email_normalized)
        .bind(role.as_str())
        .bind(hash_token(&raw_token))
        .bind(actor_user_id)
        .bind(&created_at)
        .bind(&expires_at)
        .execute(&mut *transaction)
        .await?;
        audit(
            &mut transaction,
            actor_user_id,
            actor_session_id,
            Some(workspace_id),
            "member.invite",
            "invite",
            &invite_id,
            &created_at,
        )
        .await?;
        transaction.commit().await?;
        Ok(InviteToken {
            invite_id,
            raw_token,
            expires_at,
        })
    }

    pub async fn accept_invite(
        &self,
        raw_token: &str,
        accepting_user_id: &str,
        actor_session_id: Option<&str>,
    ) -> Result<Membership, IdentityError> {
        let timestamp = now();
        let mut transaction = self.pool.begin().await?;
        let user = sqlx::query(
            "SELECT email, email_verified, disabled_at FROM identity_users WHERE user_id = $1",
        )
        .bind(accepting_user_id)
        .fetch_optional(&mut *transaction)
        .await?
        .ok_or(IdentityError::NotFound)?;
        if user.try_get::<Option<String>, _>("disabled_at")?.is_some()
            || user.try_get::<i64, _>("email_verified")? != 1
        {
            return Err(IdentityError::Forbidden);
        }
        let email: Option<String> = user.try_get("email")?;
        let email = email.ok_or(IdentityError::Forbidden)?;
        let invite = sqlx::query(
            "SELECT invite_id, workspace_id, email_normalized, role FROM workspace_invites
             WHERE token_hash = $1 AND accepted_at IS NULL AND revoked_at IS NULL AND expires_at > $2",
        )
        .bind(hash_token(raw_token))
        .bind(&timestamp)
        .fetch_optional(&mut *transaction)
        .await?
        .ok_or(IdentityError::NotFound)?;
        let normalized = normalize_email(&email)?;
        if invite.try_get::<String, _>("email_normalized")? != normalized {
            return Err(IdentityError::Forbidden);
        }
        let invite_id: String = invite.try_get("invite_id")?;
        let workspace_id: String = invite.try_get("workspace_id")?;
        lock_workspace(&mut transaction, &workspace_id).await?;
        let role = WorkspaceRole::parse(&invite.try_get::<String, _>("role")?)?;
        let consumed = sqlx::query(
            "UPDATE workspace_invites SET accepted_at = $1
             WHERE invite_id = $2 AND accepted_at IS NULL AND revoked_at IS NULL AND expires_at > $1",
        )
        .bind(&timestamp)
        .bind(&invite_id)
        .execute(&mut *transaction)
        .await?;
        if consumed.rows_affected() != 1 {
            return Err(IdentityError::Conflict);
        }
        sqlx::query(
            "INSERT INTO workspace_memberships (workspace_id, user_id, role, created_at)
             VALUES ($1, $2, $3, $4)
             ON CONFLICT (workspace_id, user_id) DO UPDATE SET role = excluded.role, revoked_at = NULL, created_at = excluded.created_at",
        )
        .bind(&workspace_id)
        .bind(accepting_user_id)
        .bind(role.as_str())
        .bind(&timestamp)
        .execute(&mut *transaction)
        .await
        .map_err(map_identity_unique)?;
        audit(
            &mut transaction,
            accepting_user_id,
            actor_session_id,
            Some(&workspace_id),
            "member.invite.accept",
            "workspace_membership",
            accepting_user_id,
            &timestamp,
        )
        .await?;
        transaction.commit().await?;
        Ok(Membership {
            workspace_id,
            user_id: accepting_user_id.to_owned(),
            role,
        })
    }

    pub async fn change_membership_role(
        &self,
        actor_user_id: &str,
        actor_session_id: Option<&str>,
        workspace_id: &str,
        target_user_id: &str,
        new_role: WorkspaceRole,
    ) -> Result<(), IdentityError> {
        let timestamp = now();
        let mut transaction = self.pool.begin().await?;
        require_owner(&mut transaction, actor_user_id, workspace_id).await?;
        let current = sqlx::query(
            "SELECT role FROM workspace_memberships WHERE workspace_id = $1 AND user_id = $2 AND revoked_at IS NULL",
        )
        .bind(workspace_id)
        .bind(target_user_id)
        .fetch_optional(&mut *transaction)
        .await?
        .ok_or(IdentityError::NotFound)?;
        let current_role = WorkspaceRole::parse(&current.try_get::<String, _>("role")?)?;
        if current_role == WorkspaceRole::Owner && new_role != WorkspaceRole::Owner {
            ensure_another_owner(&mut transaction, workspace_id, target_user_id).await?;
        }
        sqlx::query(
            "UPDATE workspace_memberships SET role = $1
             WHERE workspace_id = $2 AND user_id = $3 AND revoked_at IS NULL",
        )
        .bind(new_role.as_str())
        .bind(workspace_id)
        .bind(target_user_id)
        .execute(&mut *transaction)
        .await?;
        if new_role != current_role {
            revoke_user_sessions(&mut transaction, target_user_id, &timestamp).await?;
        }
        audit(
            &mut transaction,
            actor_user_id,
            actor_session_id,
            Some(workspace_id),
            "member.role.change",
            "workspace_membership",
            target_user_id,
            &timestamp,
        )
        .await?;
        transaction.commit().await?;
        Ok(())
    }

    pub async fn revoke_membership(
        &self,
        actor_user_id: &str,
        actor_session_id: Option<&str>,
        workspace_id: &str,
        target_user_id: &str,
    ) -> Result<(), IdentityError> {
        let timestamp = now();
        let mut transaction = self.pool.begin().await?;
        require_owner(&mut transaction, actor_user_id, workspace_id).await?;
        let current = sqlx::query(
            "SELECT role FROM workspace_memberships WHERE workspace_id = $1 AND user_id = $2 AND revoked_at IS NULL",
        )
        .bind(workspace_id)
        .bind(target_user_id)
        .fetch_optional(&mut *transaction)
        .await?
        .ok_or(IdentityError::NotFound)?;
        let role = WorkspaceRole::parse(&current.try_get::<String, _>("role")?)?;
        if role == WorkspaceRole::Owner {
            ensure_another_owner(&mut transaction, workspace_id, target_user_id).await?;
        }
        sqlx::query(
            "UPDATE workspace_memberships SET revoked_at = $1
             WHERE workspace_id = $2 AND user_id = $3 AND revoked_at IS NULL",
        )
        .bind(&timestamp)
        .bind(workspace_id)
        .bind(target_user_id)
        .execute(&mut *transaction)
        .await?;
        revoke_user_sessions(&mut transaction, target_user_id, &timestamp).await?;
        audit(
            &mut transaction,
            actor_user_id,
            actor_session_id,
            Some(workspace_id),
            "member.revoke",
            "workspace_membership",
            target_user_id,
            &timestamp,
        )
        .await?;
        transaction.commit().await?;
        Ok(())
    }

    pub async fn transfer_ownership(
        &self,
        current_owner_user_id: &str,
        actor_session_id: Option<&str>,
        workspace_id: &str,
        new_owner_user_id: &str,
    ) -> Result<(), IdentityError> {
        if current_owner_user_id == new_owner_user_id {
            return Err(IdentityError::Invalid(
                "ownership transfer target must differ".into(),
            ));
        }
        let timestamp = now();
        let mut transaction = self.pool.begin().await?;
        require_owner(&mut transaction, current_owner_user_id, workspace_id).await?;
        let target = sqlx::query(
            "SELECT role FROM workspace_memberships WHERE workspace_id = $1 AND user_id = $2 AND revoked_at IS NULL",
        )
        .bind(workspace_id)
        .bind(new_owner_user_id)
        .fetch_optional(&mut *transaction)
        .await?
        .ok_or(IdentityError::NotFound)?;
        if target.try_get::<Option<String>, _>("role")?.is_none() {
            return Err(IdentityError::NotFound);
        }
        sqlx::query(
            "UPDATE workspace_memberships SET role = 'OWNER'
             WHERE workspace_id = $1 AND user_id = $2 AND revoked_at IS NULL",
        )
        .bind(workspace_id)
        .bind(new_owner_user_id)
        .execute(&mut *transaction)
        .await?;
        sqlx::query(
            "UPDATE workspace_memberships SET role = 'EDITOR'
             WHERE workspace_id = $1 AND user_id = $2 AND revoked_at IS NULL AND role = 'OWNER'",
        )
        .bind(workspace_id)
        .bind(current_owner_user_id)
        .execute(&mut *transaction)
        .await?;
        revoke_user_sessions(&mut transaction, current_owner_user_id, &timestamp).await?;
        revoke_user_sessions(&mut transaction, new_owner_user_id, &timestamp).await?;
        audit(
            &mut transaction,
            current_owner_user_id,
            actor_session_id,
            Some(workspace_id),
            "ownership.transfer",
            "workspace_membership",
            new_owner_user_id,
            &timestamp,
        )
        .await?;
        transaction.commit().await?;
        Ok(())
    }
}

async fn require_owner(
    transaction: &mut sqlx::Transaction<'_, sqlx::Any>,
    user_id: &str,
    workspace_id: &str,
) -> Result<(), IdentityError> {
    lock_workspace(transaction, workspace_id).await?;
    let role: Option<String> = sqlx::query_scalar(
        "SELECT m.role FROM workspace_memberships m
         JOIN identity_users u ON u.user_id = m.user_id
         WHERE m.user_id = $1 AND m.workspace_id = $2 AND m.revoked_at IS NULL AND u.disabled_at IS NULL",
    )
    .bind(user_id)
    .bind(workspace_id)
    .fetch_optional(&mut **transaction)
    .await?;
    if role.as_deref() == Some("OWNER") {
        Ok(())
    } else {
        Err(IdentityError::Forbidden)
    }
}

async fn lock_workspace(
    transaction: &mut sqlx::Transaction<'_, sqlx::Any>,
    workspace_id: &str,
) -> Result<(), IdentityError> {
    // Taking a write lock on the workspace row serializes ownership and membership
    // changes across concurrent requests, including on PostgreSQL.
    let result = sqlx::query("UPDATE workspaces SET name = name WHERE workspace_id = $1")
        .bind(workspace_id)
        .execute(&mut **transaction)
        .await?;
    if result.rows_affected() == 1 {
        Ok(())
    } else {
        Err(IdentityError::NotFound)
    }
}

async fn ensure_another_owner(
    transaction: &mut sqlx::Transaction<'_, sqlx::Any>,
    workspace_id: &str,
    excluded_user_id: &str,
) -> Result<(), IdentityError> {
    let count: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM workspace_memberships m
         JOIN identity_users u ON u.user_id = m.user_id
         WHERE m.workspace_id = $1 AND m.user_id <> $2 AND m.role = 'OWNER'
           AND m.revoked_at IS NULL AND u.disabled_at IS NULL",
    )
    .bind(workspace_id)
    .bind(excluded_user_id)
    .fetch_one(&mut **transaction)
    .await?;
    if count > 0 {
        Ok(())
    } else {
        Err(IdentityError::Conflict)
    }
}

async fn revoke_user_sessions(
    transaction: &mut sqlx::Transaction<'_, sqlx::Any>,
    user_id: &str,
    timestamp: &str,
) -> Result<(), IdentityError> {
    sqlx::query(
        "UPDATE auth_sessions SET revoked_at = $1
         WHERE user_id = $2 AND revoked_at IS NULL",
    )
    .bind(timestamp)
    .bind(user_id)
    .execute(&mut **transaction)
    .await?;
    Ok(())
}

#[allow(clippy::too_many_arguments)] // Keep audit-row fields explicit at this single insert boundary.
async fn audit(
    transaction: &mut sqlx::Transaction<'_, sqlx::Any>,
    actor_user_id: &str,
    actor_session_id: Option<&str>,
    workspace_id: Option<&str>,
    action: &str,
    resource_type: &str,
    resource_id: &str,
    timestamp: &str,
) -> Result<(), IdentityError> {
    sqlx::query(
        "INSERT INTO identity_audit_events
          (event_id, occurred_at, actor_user_id, actor_session_id, workspace_id, action, resource_type, resource_id,
           result, correlation_id, details_json)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, 'allowed', $9, '{}')",
    )
    .bind(Uuid::new_v4().to_string())
    .bind(timestamp)
    .bind(actor_user_id)
    .bind(actor_session_id)
    .bind(workspace_id)
    .bind(action)
    .bind(resource_type)
    .bind(resource_id)
    .bind(Uuid::new_v4().to_string())
    .execute(&mut **transaction)
    .await?;
    Ok(())
}

impl IdentityStore {
    async fn revoke_expired_session(
        &self,
        raw_token: &str,
        timestamp: &str,
    ) -> Result<(), IdentityError> {
        sqlx::query(
            "UPDATE auth_sessions SET revoked_at = $1
             WHERE token_hash = $2 AND revoked_at IS NULL
               AND (idle_expires_at <= $1 OR absolute_expires_at <= $1)",
        )
        .bind(timestamp)
        .bind(hash_token(raw_token))
        .execute(&self.pool)
        .await?;
        Ok(())
    }
}

fn validate_identity_claim(value: &str, name: &str) -> Result<(), IdentityError> {
    if value.trim().is_empty() || value.len() > 2048 || value.chars().any(char::is_control) {
        return Err(IdentityError::Invalid(format!("{name} is invalid")));
    }
    Ok(())
}

fn normalize_email(value: &str) -> Result<String, IdentityError> {
    let normalized = value.trim().to_ascii_lowercase();
    if normalized.len() > 320
        || !normalized.contains('@')
        || normalized.starts_with('@')
        || normalized.ends_with('@')
        || normalized.chars().any(char::is_control)
    {
        return Err(IdentityError::Invalid("email is invalid".into()));
    }
    Ok(normalized)
}

fn random_token() -> Result<String, IdentityError> {
    let mut bytes = [0_u8; 32];
    getrandom::fill(&mut bytes).map_err(|_| IdentityError::Random)?;
    Ok(URL_SAFE_NO_PAD.encode(bytes))
}

fn hash_token(value: &str) -> String {
    format!("{:x}", Sha256::digest(value.as_bytes()))
}

pub(crate) fn derive_csrf_token(session_token: &str) -> String {
    let mut digest = Sha256::new();
    digest.update(b"codexpt-csrf-v1:");
    digest.update(session_token.as_bytes());
    URL_SAFE_NO_PAD.encode(digest.finalize())
}

fn now() -> String {
    format_timestamp(Utc::now())
}

fn format_timestamp(value: chrono::DateTime<Utc>) -> String {
    value.to_rfc3339_opts(SecondsFormat::Millis, true)
}

fn map_identity_unique(error: sqlx::Error) -> IdentityError {
    match &error {
        sqlx::Error::Database(db_error) if db_error.is_unique_violation() => {
            IdentityError::Conflict
        }
        _ => IdentityError::Database(error),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    async fn store() -> IdentityStore {
        IdentityStore::connect("sqlite::memory:", 1).await.unwrap()
    }

    #[tokio::test]
    async fn first_owner_bootstrap_is_explicit_and_only_succeeds_once() {
        let store = store().await;
        let (owner, workspace) = store
            .bootstrap_first_owner(
                "https://issuer.example",
                "first-owner",
                "OWNER@example.test",
                "CodeXPT",
            )
            .await
            .unwrap();
        assert_eq!(workspace.name, "CodeXPT");
        assert_eq!(
            store
                .membership(&owner, &workspace.workspace_id)
                .await
                .unwrap()
                .unwrap()
                .role,
            WorkspaceRole::Owner
        );
        assert!(matches!(
            store
                .bootstrap_first_owner(
                    "https://issuer.example",
                    "second-owner",
                    "second@example.test",
                    "Another workspace",
                )
                .await,
            Err(IdentityError::Conflict)
        ));
        assert!(matches!(
            store
                .authenticate_identity(
                    "https://issuer.example",
                    "uninvited",
                    Some("uninvited@example.test"),
                    true,
                )
                .await,
            Err(IdentityError::Forbidden)
        ));
    }

    async fn provision(store: &IdentityStore, subject: &str, email: &str) -> String {
        store
            .provision_identity("https://issuer.example", subject, Some(email), true)
            .await
            .unwrap()
    }

    #[tokio::test]
    async fn workspace_roles_are_scoped_and_owner_transfer_is_atomic() {
        let store = store().await;
        let owner = provision(&store, "owner", "owner@example.test").await;
        let editor = provision(&store, "editor", "editor@example.test").await;
        let workspace = store
            .create_workspace_for_owner(&owner, "Alpha")
            .await
            .unwrap();
        let second = store
            .create_workspace_for_owner(&editor, "Beta")
            .await
            .unwrap();
        assert_eq!(
            store
                .membership(&owner, &workspace.workspace_id)
                .await
                .unwrap()
                .unwrap()
                .role,
            WorkspaceRole::Owner
        );
        assert!(
            store
                .membership(&owner, &second.workspace_id)
                .await
                .unwrap()
                .is_none()
        );
        let invite = store
            .create_invite(
                &owner,
                None,
                &workspace.workspace_id,
                "editor@example.test",
                InviteRole::Editor,
            )
            .await
            .unwrap();
        store
            .accept_invite(&invite.raw_token, &editor, None)
            .await
            .unwrap();
        let owner_session = store.create_session(&owner).await.unwrap();
        let editor_session = store.create_session(&editor).await.unwrap();
        store
            .transfer_ownership(&owner, None, &workspace.workspace_id, &editor)
            .await
            .unwrap();
        assert!(
            store
                .resolve_session(&owner_session.raw_token)
                .await
                .unwrap()
                .is_none()
        );
        assert!(
            store
                .resolve_session(&editor_session.raw_token)
                .await
                .unwrap()
                .is_none()
        );
        assert_eq!(
            store
                .membership(&editor, &workspace.workspace_id)
                .await
                .unwrap()
                .unwrap()
                .role,
            WorkspaceRole::Owner
        );
        assert_eq!(
            store
                .membership(&owner, &workspace.workspace_id)
                .await
                .unwrap()
                .unwrap()
                .role,
            WorkspaceRole::Editor
        );
    }

    #[tokio::test]
    async fn session_is_opaque_revocable_and_expires_server_side() {
        let store = store().await;
        let user_id = provision(&store, "alice", "alice@example.test").await;
        let session = store.create_session(&user_id).await.unwrap();
        assert_eq!(session.raw_token.len(), 43);
        assert_ne!(hash_token(&session.raw_token), session.raw_token);
        let resolved = store
            .resolve_session(&session.raw_token)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(resolved.user_id, user_id);
        assert_eq!(derive_csrf_token(&session.raw_token), session.csrf_token);
        sqlx::query("UPDATE auth_sessions SET idle_expires_at = '2000-01-01T00:00:00.000Z' WHERE session_id = $1")
            .bind(&session.session_id)
            .execute(&store.pool)
            .await
            .unwrap();
        assert!(
            store
                .resolve_session(&session.raw_token)
                .await
                .unwrap()
                .is_none()
        );

        let fresh_session = store.create_session(&user_id).await.unwrap();
        store
            .revoke_session(&fresh_session.raw_token)
            .await
            .unwrap();
        assert!(
            store
                .resolve_session(&fresh_session.raw_token)
                .await
                .unwrap()
                .is_none()
        );
    }

    #[tokio::test]
    async fn identity_audit_links_login_logout_and_owner_actions_to_sessions() {
        let store = store().await;
        let owner = provision(&store, "audited-owner", "owner@example.test").await;
        let workspace = store
            .create_workspace_for_owner(&owner, "Audited")
            .await
            .unwrap();
        let session = store.create_session(&owner).await.unwrap();

        let login_session: Option<String> = sqlx::query_scalar(
            "SELECT actor_session_id FROM identity_audit_events
             WHERE action = 'auth.login' AND resource_id = $1",
        )
        .bind(&session.session_id)
        .fetch_one(&store.pool)
        .await
        .unwrap();
        assert_eq!(login_session.as_deref(), Some(session.session_id.as_str()));

        store
            .create_invite(
                &owner,
                Some(&session.session_id),
                &workspace.workspace_id,
                "editor@example.test",
                InviteRole::Editor,
            )
            .await
            .unwrap();
        let member_action_session: Option<String> = sqlx::query_scalar(
            "SELECT actor_session_id FROM identity_audit_events
             WHERE action = 'member.invite' AND actor_user_id = $1",
        )
        .bind(&owner)
        .fetch_one(&store.pool)
        .await
        .unwrap();
        assert_eq!(
            member_action_session.as_deref(),
            Some(session.session_id.as_str())
        );

        store.revoke_session(&session.raw_token).await.unwrap();
        let logout_session: Option<String> = sqlx::query_scalar(
            "SELECT actor_session_id FROM identity_audit_events
             WHERE action = 'auth.logout' AND actor_user_id = $1",
        )
        .bind(&owner)
        .fetch_one(&store.pool)
        .await
        .unwrap();
        assert_eq!(logout_session.as_deref(), Some(session.session_id.as_str()));
    }

    #[tokio::test]
    async fn oidc_identity_is_stable_and_auth_transaction_is_single_use() {
        let store = store().await;
        let owner = provision(&store, "owner", "owner@example.test").await;
        let workspace = store
            .create_workspace_for_owner(&owner, "Invited workspace")
            .await
            .unwrap();
        assert!(matches!(
            store
                .authenticate_identity(
                    "https://issuer.example",
                    "uninvited-sub",
                    Some("stranger@example.test"),
                    true,
                )
                .await,
            Err(IdentityError::Forbidden)
        ));
        store
            .create_invite(
                &owner,
                None,
                &workspace.workspace_id,
                "a@example.test",
                InviteRole::Editor,
            )
            .await
            .unwrap();
        let first = store
            .authenticate_identity(
                "https://issuer.example",
                "stable-sub",
                Some("a@example.test"),
                true,
            )
            .await
            .unwrap();
        let second = store
            .authenticate_identity(
                "https://issuer.example",
                "stable-sub",
                Some("new@example.test"),
                true,
            )
            .await
            .unwrap();
        assert_eq!(first, second);

        store
            .create_auth_transaction("opaque-state", "nonce", "pkce-verifier")
            .await
            .unwrap();
        let flow = store
            .consume_auth_transaction("opaque-state")
            .await
            .unwrap()
            .unwrap();
        assert_eq!(flow.nonce, "nonce");
        assert_eq!(flow.pkce_verifier, "pkce-verifier");
        assert!(
            store
                .consume_auth_transaction("opaque-state")
                .await
                .unwrap()
                .is_none()
        );
        assert!(
            store
                .consume_auth_transaction("unknown-state")
                .await
                .unwrap()
                .is_none()
        );
    }

    #[tokio::test]
    async fn invitations_require_owner_and_verified_matching_email_and_are_single_use() {
        let store = store().await;
        let owner = provision(&store, "owner", "owner@example.test").await;
        let wrong_user = provision(&store, "wrong", "wrong@example.test").await;
        let invitee = provision(&store, "invitee", "invitee@example.test").await;
        let workspace = store
            .create_workspace_for_owner(&owner, "Alpha")
            .await
            .unwrap();
        let invite = store
            .create_invite(
                &owner,
                None,
                &workspace.workspace_id,
                "Invitee@example.test",
                InviteRole::Editor,
            )
            .await
            .unwrap();
        assert!(
            store
                .accept_invite(&invite.raw_token, &wrong_user, None)
                .await
                .is_err()
        );
        assert_eq!(
            store
                .accept_invite(&invite.raw_token, &invitee, None)
                .await
                .unwrap()
                .role,
            WorkspaceRole::Editor
        );
        assert!(
            store
                .accept_invite(&invite.raw_token, &invitee, None)
                .await
                .is_err()
        );
    }

    #[tokio::test]
    async fn last_owner_cannot_be_removed_or_downgraded() {
        let store = store().await;
        let owner = provision(&store, "owner", "owner@example.test").await;
        let workspace = store
            .create_workspace_for_owner(&owner, "Alpha")
            .await
            .unwrap();
        assert_eq!(
            store
                .revoke_membership(&owner, None, &workspace.workspace_id, &owner)
                .await
                .unwrap_err()
                .to_string(),
            "identity state conflicts with this operation"
        );
        assert_eq!(
            store
                .change_membership_role(
                    &owner,
                    None,
                    &workspace.workspace_id,
                    &owner,
                    WorkspaceRole::Editor
                )
                .await
                .unwrap_err()
                .to_string(),
            "identity state conflicts with this operation"
        );
    }
}
