# ADR 0002: Version SQLite schema with ordered migrations

- **Status:** Accepted
- **Date:** 2026-10-10
- **Decision makers:** César (technical director); implementation review delegated to ChatGPT session by César

## Context

The v0.1 store created tables with `CREATE TABLE IF NOT EXISTS` but did not record the database schema version. This makes future changes ambiguous and can leave an older database partly compatible with a newer binary.

## Decision

Use SQLite `PRAGMA user_version` as the schema version. Apply ordered, transactional migrations on open; set the version only after each migration succeeds. Reject a database whose schema version is newer than the binary supports. Never silently reset or recreate existing user data.

This implements decision `DEC-001` from the technical review: make schema evolution safe before adding complex session identity, while retaining the minimum domain rule that only the declared human actor may approve.

## Alternatives considered

- **Keep `CREATE TABLE IF NOT EXISTS` without a version:** rejected because schema drift cannot be detected and a new binary cannot know which upgrade path is required.
- **Add a migration ledger table:** valid, but adds bookkeeping the current single migration does not need. Reconsider if checksums, per-migration history, or more complex upgrade diagnostics become necessary.
- **Adopt an ORM/migration framework now:** rejected as extra runtime/dependency complexity for a small embedded schema. The SQL remains explicit and tested in source control.
- **Complex authentication now:** deferred. The prototype has one local OS user and no authenticated agent boundary; pretending actor labels are credentials would create false assurance.

## Consequences

- Existing unversioned v0.1 databases are interpreted as version 0 and migrated by creating the original tables, then setting version 1.
- New schema changes require a new migration and tests for upgrade, rollback, and unsupported future versions.
- Migration SQL remains in source control and executes atomically before the store is exposed.
- This is a local compatibility mechanism, not a backup. Users still need a backup before upgrades.

## Revisit when

Schema versioning needs branching migration histories, externally managed databases, or downgrade support. Downgrades are not supported by this ADR.

Revisit actor authorization when a real agent transport or additional human user crosses a trust boundary.
