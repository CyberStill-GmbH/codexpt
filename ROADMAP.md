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

## Next phase: Identity v1

Start with design, contracts, an ADR, a threat model, and authorization tests. The approved direction is Rust + SQLx, verified identity, secure sessions, and workspace RBAC (`OWNER`, `EDITOR`, `VIEWER`). Keep local SQLite use working and evaluate a managed OIDC provider using current capabilities, terms, and costs. Do not enable public self-registration or deploy to AWS in this phase. Agent integrations remain separate.

## Explicitly out of current scope

- Direct automation of a ChatGPT browser conversation as a product adapter.
- An unauthenticated web server or public deployment.
- Paid API, hosted database, AWS/Azure/Vercel, or additional subscription spend.
- Automatic repository writes, infrastructure changes, or arbitrary shell execution by agents.
- Public self-registration, AWS deployment, multi-tenant use, and compliance claims.

## Release gates for any next phase

1. Approve identity/session contracts and the OIDC provider choice before handling credentials.
2. Tests cover login/logout, expiry/revocation, CSRF, IDOR, cross-workspace isolation, privilege escalation, replay, and provenance.
3. Keep registration invite-only until access control and recovery are reviewed.
4. Design cloud infrastructure and cost bounds separately; no AWS apply until a concrete Terraform plan is reviewed.
5. Run the 12-task experiment with a frozen rubric before claiming coordination benefit.
