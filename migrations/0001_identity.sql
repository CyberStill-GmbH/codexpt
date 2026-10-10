CREATE TABLE identity_users (
    user_id TEXT PRIMARY KEY,
    issuer TEXT NOT NULL CHECK (length(issuer) > 0),
    subject TEXT NOT NULL CHECK (length(subject) > 0),
    email TEXT,
    email_verified INTEGER NOT NULL DEFAULT 0 CHECK (email_verified IN (0, 1)),
    disabled_at TEXT,
    created_at TEXT NOT NULL,
    UNIQUE (issuer, subject)
);

CREATE TABLE identity_bootstrap (
    singleton INTEGER PRIMARY KEY CHECK (singleton = 1),
    completed_at TEXT NOT NULL
);

CREATE TABLE workspaces (
    workspace_id TEXT PRIMARY KEY,
    name TEXT NOT NULL CHECK (length(trim(name)) BETWEEN 1 AND 120),
    created_at TEXT NOT NULL
);

CREATE TABLE workspace_memberships (
    workspace_id TEXT NOT NULL REFERENCES workspaces(workspace_id),
    user_id TEXT NOT NULL REFERENCES identity_users(user_id),
    role TEXT NOT NULL CHECK (role IN ('OWNER', 'EDITOR', 'VIEWER')),
    created_at TEXT NOT NULL,
    revoked_at TEXT,
    PRIMARY KEY (workspace_id, user_id)
);
CREATE INDEX workspace_memberships_user_active
    ON workspace_memberships(user_id, workspace_id) WHERE revoked_at IS NULL;

CREATE TABLE auth_sessions (
    session_id TEXT PRIMARY KEY,
    user_id TEXT NOT NULL REFERENCES identity_users(user_id),
    token_hash TEXT NOT NULL UNIQUE,
    created_at TEXT NOT NULL,
    last_seen_at TEXT NOT NULL,
    idle_expires_at TEXT NOT NULL,
    absolute_expires_at TEXT NOT NULL,
    revoked_at TEXT
);
CREATE INDEX auth_sessions_user_active
    ON auth_sessions(user_id) WHERE revoked_at IS NULL;

CREATE TABLE auth_transactions (
    state_hash TEXT PRIMARY KEY,
    nonce TEXT NOT NULL,
    pkce_verifier TEXT NOT NULL,
    created_at TEXT NOT NULL,
    expires_at TEXT NOT NULL,
    consumed_at TEXT
);
CREATE INDEX auth_transactions_expiry ON auth_transactions(expires_at);

CREATE TABLE workspace_invites (
    invite_id TEXT PRIMARY KEY,
    workspace_id TEXT NOT NULL REFERENCES workspaces(workspace_id),
    email_normalized TEXT NOT NULL,
    role TEXT NOT NULL CHECK (role IN ('EDITOR', 'VIEWER')),
    token_hash TEXT NOT NULL UNIQUE,
    invited_by TEXT NOT NULL REFERENCES identity_users(user_id),
    created_at TEXT NOT NULL,
    expires_at TEXT NOT NULL,
    accepted_at TEXT,
    revoked_at TEXT
);
CREATE INDEX workspace_invites_pending
    ON workspace_invites(workspace_id, email_normalized)
    WHERE accepted_at IS NULL AND revoked_at IS NULL;

CREATE TABLE identity_audit_events (
    event_id TEXT PRIMARY KEY,
    occurred_at TEXT NOT NULL,
    actor_user_id TEXT REFERENCES identity_users(user_id),
    workspace_id TEXT REFERENCES workspaces(workspace_id),
    action TEXT NOT NULL,
    resource_type TEXT NOT NULL,
    resource_id TEXT NOT NULL,
    result TEXT NOT NULL CHECK (result IN ('allowed', 'denied')),
    reason_code TEXT,
    correlation_id TEXT NOT NULL,
    causation_id TEXT,
    details_json TEXT NOT NULL
);
CREATE INDEX identity_audit_workspace_order
    ON identity_audit_events(workspace_id, occurred_at, event_id);
