# Verification strategy

## Current release-candidate snapshot (2026-10-10)

The latest local UI and backend changes were verified in the `C:\projects\codexpt` worktree. These checks do not certify live OIDC, hosted authorization, or a public deployment.

- Rust: 39 tests passed; `cargo fmt --check` and `cargo clippy --all-targets --all-features --locked -- -D warnings` passed.
- Web: `npm --prefix web run lint`, `npm --prefix web run test` (6 tests), and production build passed.
- Browser: `npm --prefix web run test:e2e` passed 12 Playwright cases. Member-management requests use synthetic API fixtures.
- Impeccable detector reported no findings for the changed dashboard files.
- `scripts/container-smoke.ps1` passed against the digest-pinned `codexpt:release-candidate` image (40,802,113 bytes). It verifies non-root UID 10001, read-only root filesystem, missing/read-only `/data` rejection, relative/out-of-volume/traversal/symlink-escape DB path rejection, task-store readiness, and embedded frontend HTTP 200.
- The smoke test commits 8 synthetic tasks before SIGTERM, then starts a concurrent writer. All 8 confirmed IDs and the Owner identity survive restart with no duplicate task IDs; SQLite `db-check` reports `ok`. The online backup restores all 14 tasks created by the confirmed baseline and concurrent writer, and the restored database also passes `db-check`. The script removes only its unique disposable containers and volumes; the built image remains cached locally.
- Rust readiness tests verify that unavailable identity or task storage returns HTTP 503. Container checks use a synthetic OIDC issuer and identity: they prove local packaging, storage, shutdown, and backup mechanics only, not live OIDC or hosted platform behavior.
- No Railway service, DNS, certificate, or cloud resource has been created or modified.

Remaining hosted release gates are tracked in [`deployment/PLAN.md`](deployment/PLAN.md) and [`identity-v1/AUTHORIZATION-TEST-PLAN.md`](identity-v1/AUTHORIZATION-TEST-PLAN.md). Railway is not linked in this CLI workspace, and its account billing/usage is unverified.

## Required gates

Run before each commit and release candidate:

```powershell
cargo fmt --check
cargo test --all-targets
cargo clippy --all-targets -- -D warnings
npm --prefix web ci
npm --prefix web test
npm --prefix web run build
npm --prefix web run test:e2e
./scripts/smoke.ps1
./scripts/container-smoke.ps1
```

## Invariant coverage

- Protocol: allowed/forbidden transitions and explicit human approval.
- Persistence: atomic writes, foreign keys, pending-decision preservation, restart recovery, deterministic event ordering.
- Idempotency: same ID and same content is a no-op; same ID and different content is rejected.
- Concurrency: two writers cannot both advance from the same expected state.
- Migration: fresh DB, existing v0.1 DB, transactional failure, and rejection of a newer schema.
- HTTP API: complete task/message/decision-pause/answer/resume flow, foreign Host/Origin rejection, body limits, and security headers.
- Dashboard: Vitest covers task creation and error states. Playwright exercises the persisted review flow at 390×844 and responsive layouts at 768×1024 and 1280×900. Axe checks WCAG 2.1 A/AA on mobile and desktop; E2E also checks keyboard focus, annotation order, horizontal overflow, and browser errors.
- CLI smoke: help/init/task create/show/message send/list/events, using a temporary database.
- Adapter contracts (future): provenance, cancellation, timeouts, maximum turns, redaction, and no implicit tool execution.

## MVP verification record

Verified on 2026-10-10 from the repository root before tagging `v0.1.0-local-mvp`:

- `cargo fmt --check`, `cargo test --all-targets` (18 passed), and `cargo clippy --all-targets -- -D warnings` passed.
- `npm --prefix web test` (2 passed), `npm --prefix web run lint`, and `npm --prefix web run build` passed.
- `npm --prefix web run test:e2e` passed 3 Playwright cases at 390×844, 768×1024, and 1280×900; Axe reported 0 WCAG 2.1 A/AA violations on mobile and desktop.
- `./scripts/smoke.ps1` passed against a temporary SQLite database. Playwright captures are regenerated under `web/test-results/` by the E2E command.
- The E2E server shut down after the run; ports 4173 and 4174 had no listening process.

Axe is an automated check, not a full accessibility certification. The application remains local-only and does not authenticate actors.

## Current local-first verification

Verified on 2026-10-10 from `C:\projects\codexpt` on `feature/local-dashboard`. This worktree also contains an unfinished hosted identity prototype; these results prove local behavior only and do not certify Cognito, hosted authorization, or deployment.

- `cargo fmt --check`, `cargo test` (32 passed), and `cargo clippy --all-targets --all-features -- -D warnings` passed.
- `npm --prefix web test -- --run` passed 3 tests; `npm --prefix web run lint` and `npm --prefix web run build` passed.
- `npm --prefix web run test:e2e` passed 3 Playwright cases: a complete human review flow on mobile, responsive tablet layout, and desktop accessibility. Axe reported no WCAG 2.1 A/AA violations in the audited mobile and desktop flows.
- The E2E runner starts Rust on loopback with a unique SQLite file and removes that database plus WAL sidecars after Playwright and its server exit. Cleanup was verified: the generated file was absent and port 4174 was closed.
- `scripts/smoke.ps1` passed using a disposable SQLite database: one synthetic review, one message, and eight audit events, including human approval.
- A manual local launch on `127.0.0.1:4173` returned `GET /api/health` = `{"mode":"local","status":"ok"}` and `GET /` = HTTP 200 with title `CodeXPT · cuaderno de revisión`. The browser showed the empty local workspace and its create-review action. The process was stopped after verification; ports 4173 and 4174 are closed.
- Terraform formatting and `terraform validate` passed. No plan, cloud resource creation, DNS mutation, paid API call, or external model integration was performed.

The local dashboard is usable for manually recording review conversations; agents still exchange content through the human director. Actor labels are not authenticated. Hosted identity and infra remain follow-on work with separate acceptance gates.

## Identity prototype and UX verification

Verified on 2026-10-10 from `C:\projects\codexpt` after the identity prototype and responsive-dialog updates:

- `cargo fmt --check`, `cargo test --all-targets` (32 passed), and `cargo clippy --all-targets --all-features -- -D warnings` passed.
- `npm --prefix web test` (3 passed), `npm --prefix web run lint`, and `npm --prefix web run build` passed.
- `npm --prefix web run test:e2e` passed all 5 Playwright cases, including touch search/filter controls and dialog focus/Escape behavior. The run took 18.1 seconds after build. Axe reported no WCAG 2.1 A/AA violations in the audited desktop and mobile flows.
- `scripts/smoke.ps1` passed with a disposable local SQLite database.
- `terraform fmt -check -recursive` and `terraform validate` passed. Validation does not mean the infrastructure is deployable; no cloud plan or apply was run.
- Production web bundle: JavaScript 259.60 kB (79.79 kB gzip), CSS 25.17 kB (5.93 kB gzip).

These checks do not close the remaining release gates in `docs/identity-v1/AUTHORIZATION-TEST-PLAN.md`: Cognito/live-domain browser behavior, full HTTP authorization matrix, production CSRF/CORS, and hosted end-to-end verification are still pending. The system has no integrated agent transport, and the 12-task comparison has not been conducted. See [`docs/VALIDATION-REPORT.md`](VALIDATION-REPORT.md) for measured metrics and scope limits.

## Screen-by-screen QA before visual review

Verified on 2026-10-10 before sending screenshots to the product reviewer:

- `npm --prefix web run test:e2e`: 8 Playwright cases passed. The suite now captures and checks login, invitation, empty workspace, member management, and review dashboard screens at mobile (390×844) and desktop (1280×900), plus tablet dashboard behavior (768×1024).
- Axe WCAG 2.1 A/AA reported zero violations for the tested routes and states, including the Owner member dialog. One real issue found during this pass was the mobile-only icon button for “Miembros” lacking an accessible name; it now has an explicit label. The login panel’s muted metadata and accent contrast were darkened, and its decorative watermark was removed to clear contrast violations.
- The hosted Owner/member/invite visual flow uses synthetic Playwright API responses. It checks the member list, the invitation role, and one-time token display, but it is not evidence of Cognito, CSRF, or production API integration.
- Login and invitation were tested against the local unconfigured identity state. Real provider sign-in, a live user account, invitation email delivery, and the deployed domain were not tested.
- Screenshots are regenerated under `web/test-results/` and contain synthetic test identities only.

The seven prior release gates and limits above still apply; this screen QA does not constitute deployment approval or hosted release certification.

## ChatGPT-auth follow-up and architecture review

Verified on 2026-10-10 after adding provider-specific OpenAI login presentation, correcting Terraform identity-provider configuration, and recording the agent-connection and hosting decisions:

- `cargo fmt --check`, `cargo test --all-targets` (33 passed), and `cargo clippy --all-targets --all-features -- -D warnings` passed.
- `npm --prefix web test -- --run` passed 5 tests; `npm --prefix web run lint` and `npm --prefix web run build` passed.
- `npm --prefix web run test:e2e` passed all 11 Playwright cases, including configured OpenAI OIDC copy, long-title overflow at 320–1280 px, accessible member dialog, and correct logo/favicon variants. Axe reported no violations in the flows covered by the suite.
- `scripts/smoke.ps1` passed using a disposable SQLite database: one synthetic review, one message and eight audit events.
- `terraform fmt -check -recursive` and `terraform validate` passed. No Terraform plan/apply, deployment, DNS change, cloud resource change, live OIDC login or API/model usage was performed.
- The first clippy run overlapped a Vite build that replaced tracked `web/dist` assets and caused RustEmbed to observe an incomplete directory. The serial rerun passed; the first E2E run also timed out during that overlap. The subsequent serial E2E run passed all 11 cases.

This verifies the current local branch only. No Codex runner, pairing endpoint, durable agent inbox, remote MCP server, OpenAI website client, or Workers/D1 adapter has been implemented or tested. Those remain separately gated work. ChatGPT's approved design decisions are recorded in [ADR-0006](adr/0006-local-agent-runner-inbox.md) and [ADR-0007](adr/0007-workers-d1-hosting-spike.md).

## Test data and privacy

Use synthetic tasks and evidence only. Tests must not call external model services or use real provider credentials. Temporary DB files must be removed by test fixtures. Do not store outputs from unrelated user conversations in test data.

## Experiment quality

The 12-task comparison is exploratory, counterbalanced, and not statistically conclusive. Record unavailable token metrics as unavailable; never estimate them as measurements. Keep task sets and scoring rubric fixed before each run.
