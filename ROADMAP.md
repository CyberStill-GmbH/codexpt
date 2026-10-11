# Roadmap

## MVP now

- [x] Local Rust + SQLite protocol core and CLI.
- [x] Human decision pause and explicit final approval rule.
- [x] Idempotency, transactional events, stale-state protection.
- [x] Versioned database migrations and migration regression tests.
- [x] C4, ADR, security, testing, and operations documentation.
- [x] Local Rust HTTP API, loopback browser dashboard, review/decision flow, responsive states, security boundaries, and API/UI smoke tests.
- [x] Repeatable CLI end-to-end smoke test and documented user journey.
- [x] Desktop, tablet, and mobile review of real-browser captures; Axe WCAG 2.1 A/AA found no violations in mobile or desktop runs.
- [x] Immutable, bounded context checkpoints with integrity digest, idempotent creation, one ready checkpoint per task, auditable explicit handoff receipt, and an honest unknown-capacity state for ChatGPT web.
- [x] README, C4, ADRs, security, operations, and verification records reflect implemented features and unauthenticated local-only limits.
- [x] `v0.1.0-local-mvp` release reference created after tests and a clean working tree.

## Identity v1 implementation (in progress, not release-ready)

The delegated review approved Cognito as the invite-only OIDC candidate, CodeXPT-owned workspace RBAC, SQLx + SQLite for a one-instance beta, server-side sessions (30-minute idle/12-hour absolute), and distinct human/agent/provider identities. The Rust branch implements OIDC state/nonce/PKCE, hashed sessions, Owner/Editor/Viewer membership, one-use invites, workspace task isolation, approval evidence/version binding, and the Owner invite flow. A controlled OIDC provider callback test, authorization/session tests, SQLite backup/restore (including sessions, invite, and task history), Rust API/storage tests, frontend tests, and local browser tests pass. Cognito itself and hosted-browser authorization remain unverified. AWS EC2 exceeds the current US$5/month hosting ceiling. ADR-0015 selects Cloudflare Workers Free + SQLite Durable Objects as a candidate only; the isolated checkpoint storage spike passes eight local tests, but the full backend, hosted identity, account quotas, and DNS are not validated. `codexpt.jappi.net.pe` currently returns `SERVFAIL`; no Cloudflare zone or CodeXPT service exists. Do not publish a beta until the gates below pass. Agent integrations remain separate.

## Explicitly out of current scope

- Direct automation of a ChatGPT browser conversation as a product adapter.
- An unauthenticated web server or public deployment.
- Paid model APIs, paid hosting, and additional subscriptions above the product's US$5/month ceiling.
- Automatic repository writes, infrastructure changes, or arbitrary shell execution by agents.
- Public self-registration, AWS deployment, agent execution, and compliance claims.

## Release gates for any next phase

1. Approve identity/session contracts and the OIDC provider choice before handling credentials.
2. Tests cover login/logout, expiry/revocation, CSRF, IDOR, cross-workspace isolation, privilege escalation, replay, and provenance.
3. Keep registration invite-only until access control and recovery are reviewed.
4. Complete and security-review the Cloudflare adapter locally; no hosted deployment until the US$5 cost and DNS gates pass.
5. Run the 12-task experiment with a frozen rubric before claiming coordination benefit.

## Hosted release gates

- [ ] Finish Identity v1 acceptance in `docs/identity-v1/AUTHORIZATION-TEST-PLAN.md`, including Cognito behavior and hosted-browser security tests.
- [x] Use one SQLite database file for tasks and identity for the approved single-instance beta; PostgreSQL and multi-instance operation remain deferred.
- [ ] Implement Cloudflare Worker parity for OIDC sessions, invitations, RBAC, tasks/events, checkpoints and agent inbox; retain the Rust implementation as reference until contract tests pass.
- [ ] Validate Free-plan quota/fail-closed behavior, recovery and account-wide cost for the complete app before creating hosted resources.
- [ ] Identify the authoritative registrar/DNS operator; inventory every `jappi.net.pe` record and a rollback plan before changing delegation.
- [ ] Verify `codexpt.jappi.net.pe` DNS, HTTPS, OIDC callbacks and hosted browser behavior before opening the invite-only beta.
- [ ] Add the hosted checkpoint routes to authorization, workspace-isolation, CSRF, concurrent-retry, retention and browser E2E coverage before enabling checkpoint export to invited users.
