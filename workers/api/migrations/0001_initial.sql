CREATE TABLE identity_users (
  user_id TEXT PRIMARY KEY,
  issuer TEXT NOT NULL CHECK (length(issuer) BETWEEN 1 AND 2048),
  subject TEXT NOT NULL CHECK (length(subject) BETWEEN 1 AND 255),
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
  role_updated_at TEXT NOT NULL,
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
CREATE INDEX auth_sessions_user_active ON auth_sessions(user_id) WHERE revoked_at IS NULL;

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
  accepted_by TEXT REFERENCES identity_users(user_id),
  revoked_at TEXT
);
CREATE INDEX workspace_invites_pending ON workspace_invites(workspace_id, email_normalized)
  WHERE accepted_at IS NULL AND revoked_at IS NULL;

CREATE TRIGGER workspace_invite_accept_membership
AFTER UPDATE OF accepted_at ON workspace_invites
WHEN OLD.accepted_at IS NULL AND NEW.accepted_at IS NOT NULL AND NEW.accepted_by IS NOT NULL
BEGIN
  INSERT INTO workspace_memberships(workspace_id, user_id, role, created_at, role_updated_at)
  VALUES (NEW.workspace_id, NEW.accepted_by, NEW.role, NEW.accepted_at, NEW.accepted_at)
  ON CONFLICT(workspace_id, user_id) DO UPDATE
    SET role = excluded.role, revoked_at = NULL, created_at = excluded.created_at,
      role_updated_at = excluded.role_updated_at;
END;

CREATE TABLE identity_audit_events (
  event_id TEXT PRIMARY KEY,
  occurred_at TEXT NOT NULL,
  actor_user_id TEXT REFERENCES identity_users(user_id),
  actor_session_id TEXT REFERENCES auth_sessions(session_id),
  workspace_id TEXT REFERENCES workspaces(workspace_id),
  action TEXT NOT NULL,
  resource_type TEXT NOT NULL,
  resource_id TEXT NOT NULL,
  result TEXT NOT NULL CHECK (result IN ('allowed', 'denied')),
  reason_code TEXT,
  correlation_id TEXT NOT NULL,
  causation_id TEXT,
  details_json TEXT NOT NULL CHECK (json_valid(details_json))
);
CREATE INDEX identity_audit_workspace_order
  ON identity_audit_events(workspace_id, occurred_at, event_id);
CREATE INDEX identity_audit_session_order
  ON identity_audit_events(actor_session_id, occurred_at, event_id)
  WHERE actor_session_id IS NOT NULL;

CREATE TABLE tasks (
  task_id TEXT PRIMARY KEY,
  title TEXT NOT NULL,
  description TEXT NOT NULL,
  status TEXT NOT NULL CHECK (status IN ('open', 'proposed', 'under_review', 'resolved', 'approved')),
  created_at TEXT NOT NULL,
  updated_at TEXT NOT NULL,
  workspace_id TEXT NOT NULL REFERENCES workspaces(workspace_id),
  version INTEGER NOT NULL DEFAULT 1 CHECK (version > 0),
  last_mutation_id TEXT NOT NULL
);
CREATE INDEX tasks_workspace_order ON tasks(workspace_id, updated_at DESC, task_id ASC);

CREATE TABLE task_events (
  sequence INTEGER PRIMARY KEY AUTOINCREMENT,
  task_id TEXT NOT NULL REFERENCES tasks(task_id),
  event_type TEXT NOT NULL,
  actor TEXT NOT NULL,
  correlation_id TEXT NOT NULL,
  details_json TEXT NOT NULL CHECK (json_valid(details_json)),
  occurred_at TEXT NOT NULL
);
CREATE INDEX task_events_task_order ON task_events(task_id, sequence);

CREATE TABLE messages (
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
CREATE INDEX messages_task_order ON messages(task_id, created_at, message_id);

CREATE TABLE human_decisions (
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
CREATE UNIQUE INDEX one_pending_decision_per_task ON human_decisions(task_id) WHERE status = 'pending';

CREATE TABLE context_checkpoints (
  checkpoint_id TEXT PRIMARY KEY,
  task_id TEXT NOT NULL REFERENCES tasks(task_id),
  workspace_id TEXT NOT NULL REFERENCES workspaces(workspace_id),
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
CREATE INDEX context_checkpoints_task_order ON context_checkpoints(task_id, created_at DESC, checkpoint_id DESC);
CREATE UNIQUE INDEX one_ready_checkpoint_per_task ON context_checkpoints(task_id) WHERE status = 'ready';
