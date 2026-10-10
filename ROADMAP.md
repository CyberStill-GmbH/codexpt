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
- [x] README, C4, ADRs, security, operations, and verification records reflect implemented features and unauthenticated local-only limits.
- [x] `v0.1.0-local-mvp` release reference created after tests and a clean working tree.

## Identity v1 implementation (in progress, not release-ready)

The delegated review approved Cognito as the invite-only OIDC candidate, CodeXPT-owned workspace RBAC, SQLx + SQLite for a one-instance beta, server-side sessions (30-minute idle/12-hour absolute), and distinct human/agent/provider identities. The branch implements OIDC state/nonce/PKCE, hashed sessions, Owner/Editor/Viewer membership, one-use invites, workspace task isolation, approval evidence/version binding, and the Owner invite flow. A controlled OIDC provider callback test, authorization/session tests, SQLite backup/restore (including sessions, invite, and task history), Rust API/storage tests, frontend tests, and local browser tests pass. Cognito itself, full hosted-browser authorization, cross-domain E2E, production DNS, and the full auth UI acceptance pass remain. Terraform for a single EC2 + EBS + SQLite + Caddy + S3-backup beta has been added and validated, but it has not been planned against AWS or applied. The required immutable ARM64 release artifact and verified registrar access are still missing. Do not enable public traffic or deploy until the release gates pass, a concrete costed plan is reviewed, and the DNS prerequisites are met. Agent integrations remain separate.

## Explicitly out of current scope

- Direct automation of a ChatGPT browser conversation as a product adapter.
- An unauthenticated web server or public deployment.
- Paid API, hosted database, AWS/Azure/Vercel, or additional subscription spend.
- Automatic repository writes, infrastructure changes, or arbitrary shell execution by agents.
- Public self-registration, AWS deployment, agent execution, and compliance claims.

## Release gates for any next phase

1. Approve identity/session contracts and the OIDC provider choice before handling credentials.
2. Tests cover login/logout, expiry/revocation, CSRF, IDOR, cross-workspace isolation, privilege escalation, replay, and provenance.
3. Keep registration invite-only until access control and recovery are reviewed.
4. Design cloud infrastructure and cost bounds separately; no AWS apply until a concrete Terraform plan is reviewed.
5. Run the 12-task experiment with a frozen rubric before claiming coordination benefit.

## Hosted release gates

- [ ] Finish Identity v1 acceptance in `docs/identity-v1/AUTHORIZATION-TEST-PLAN.md`, including Cognito behavior and hosted-browser security tests.
- [x] Use one SQLite database file for tasks and identity for the approved single-instance beta; PostgreSQL and multi-instance operation remain deferred.
- [ ] Produce a reviewed immutable Linux ARM64 artifact, then generate a deployable Terraform plan with current costs; review the plan before applying.
- [ ] Resolve registrar access, audit current DNS/DNSSEC records, select one DNS authority, and prepare the rollback before changing delegation.
- [ ] Verify `codexpt.jappi.net.pe` + `api.codexpt.jappi.net.pe` DNS and callback URI, then deploy and have the delegated reviewer inspect the real login and workspace.
