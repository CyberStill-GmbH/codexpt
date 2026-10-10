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

## Test data and privacy

Use synthetic tasks and evidence only. Tests must not call external model services or use real provider credentials. Temporary DB files must be removed by test fixtures. Do not store outputs from unrelated user conversations in test data.

## Experiment quality

The 12-task comparison is exploratory, counterbalanced, and not statistically conclusive. Record unavailable token metrics as unavailable; never estimate them as measurements. Keep task sets and scoring rubric fixed before each run.
