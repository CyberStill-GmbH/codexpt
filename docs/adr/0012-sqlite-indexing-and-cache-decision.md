# ADR-0012: Query-driven SQLite indexing; no Redis in the beta

- **Status:** Accepted for the one-instance invite-only beta
- **Date:** 2026-10-10
- **Decision owner:** César; technical review delegated to Codex and ChatGPT

## Context

The beta candidate uses one local SQLite file, one Rust/Axum process, rusqlite for task records, and SQLx for identity/session records. There is no measured request volume or profile showing a cache bottleneck. Session membership/role checks must observe revocation on subsequent requests, making a separately invalidated authorization cache a security risk.

## Decision

1. Do not add Redis for this beta. It would add a billable service, network dependency, credentials, monitoring and outage behavior without removing SQLite's single-writer limit or a measured hot query. It must not become the authority for roles or sessions.
2. Keep relational identity data normalized around users, workspaces, memberships, sessions, transactions, invitations and audit events. Avoid duplicate role or identity fields. Keep versioned task/message event details and evidence references as validated JSON payloads because they are event/document payloads, not independent entities to join; normalize them only if a concrete query or integrity rule requires it.
3. Add indexes from observed access paths rather than a blanket “normalize three times” or index-everything exercise. `messages(task_id, created_at, message_id)` and the partial unique pending-decision index already match message history and pending-decision queries. Task event history filters by `task_id` and orders by `sequence`, so schema version 4 adds `task_events(task_id, sequence)`.
4. Reconsider Redis or a different database only after collecting request volume, endpoint p50/p95/p99, SQLite busy/locked errors and wait time, database size, CPU/RAM, and query plans under a representative workload. A candidate cache must define bounded TTL, invalidation on role/session changes, fail-closed behavior and a cost comparison; authorization remains backed by SQL storage.

## Verification

- Migration from existing schema versions is transactional and preserves existing rows.
- `EXPLAIN QUERY PLAN` for task event history must use `task_events_task_order`.
- Existing role/session revocation and task history contract tests remain green.
- No Redis service or second database is provisioned.

## Consequences

- The beta keeps a small operational surface and a single source of truth for authorization.
- Event history lookup avoids scanning all task events as the overall log grows.
- SQLite still serializes writes; this index does not make the application horizontally scalable or prove an application-level performance target. Profile a representative hosted workload before making latency or scale claims.
