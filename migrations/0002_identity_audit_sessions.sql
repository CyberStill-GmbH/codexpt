ALTER TABLE identity_audit_events
    ADD COLUMN actor_session_id TEXT REFERENCES auth_sessions(session_id);

CREATE INDEX identity_audit_session_order
    ON identity_audit_events(actor_session_id, occurred_at, event_id)
    WHERE actor_session_id IS NOT NULL;
