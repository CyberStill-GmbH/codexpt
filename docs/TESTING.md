# Verification strategy

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

## Test data and privacy

Use synthetic tasks and evidence only. Tests must not call external model services or use real provider credentials. Temporary DB files must be removed by test fixtures. Do not store outputs from unrelated user conversations in test data.

## Experiment quality

The 12-task comparison is exploratory, counterbalanced, and not statistically conclusive. Record unavailable token metrics as unavailable; never estimate them as measurements. Keep task sets and scoring rubric fixed before each run.
