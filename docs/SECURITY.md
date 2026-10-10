# Security notes and operating boundaries

**Scope:** local-first MVP, single OS user. This is a practical security baseline, not a certification or production multi-user threat model.

## Assets

- Task text, proposals, objections, human decisions, event history, evidence locators.
- Future provider credentials and local agent conversation references, if an approved adapter is added.

## Threats and controls

| Threat | Current control | Remaining limitation / follow-up |
|---|---|---|
| Duplicate/replayed messages | Stable message ID and content hash; exact retry has no second effect | Actor authenticity is not established |
| Stale or racing state change | Expected-state check in an immediate transaction | Single local SQLite DB only |
| Partial write after error | Related state and event append share a transaction | Crash recovery should remain in regression tests |
| Agent claims human approval | Hard-coded human actor label check | The CLI caller can claim the label; never treat this as real authentication |
| Malicious prompt/evidence text | Stored as data; no automatic execution or retrieval | Future UI/adapters need output encoding and SSRF-safe evidence handling |
| Local data disclosure | DB is ignored by Git and stored under workspace | No application-level encryption; rely on OS account/device disk encryption |
| Secret leakage | No secrets needed by v0.1; README warns against storing them | Future tokens must use OS credential storage, not SQLite or logs |
| Database corruption/upgrade error | Transactions and explicit schema versioning | Backups and restore drills required before any production use |
| Unbounded agent actions/cost | No autonomous adapters in v0.1 | Future orchestration must cap turns, time, tool permissions, and spend |

## Required rules

1. Keep the server bound to loopback for local use. Do not expose it on a LAN or public interface without authentication, authorization, CSRF protection, and a separate security review.
2. Never place provider credentials, AWS credentials, customer secrets, or sensitive incident data in task text, messages, test fixtures, screenshots, or Git.
3. Do not execute content authored by a participant. Any future tool call requires explicit capability allowlisting, sandboxing, limits, and human approval for consequential actions.
4. Keep human final approval explicit. A provider's output is a proposal, not authorization.
5. Treat the event log as auditable application history, not tamper-proof evidence. A local database administrator can edit it.
6. Review dependency changes and keep the lockfile committed.

## Incident response for this prototype

If the DB is exposed or a token was stored in it, stop the process, revoke/rotate that token at its issuer, preserve a copy for investigation if appropriate, remove the exposed credential from the DB/logs and Git history as necessary, then validate recovery from a known-good backup. Do not claim that deleting a local file revokes a credential.
