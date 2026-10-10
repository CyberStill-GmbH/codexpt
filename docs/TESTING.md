# Verification strategy

## Required gates

Run before each commit and release candidate:

```powershell
cargo fmt --check
cargo test --all-targets
cargo clippy --all-targets -- -D warnings
```

## Invariant coverage

- Protocol: allowed/forbidden transitions and explicit human approval.
- Persistence: atomic writes, foreign keys, pending-decision preservation, restart recovery, deterministic event ordering.
- Idempotency: same ID and same content is a no-op; same ID and different content is rejected.
- Concurrency: two writers cannot both advance from the same expected state.
- Migration: fresh DB, existing v0.1 DB, transactional failure, and rejection of a newer schema.
- CLI smoke: help/init/task create/show/message send/list/events, using a temporary database.
- Adapter contracts (future): provenance, cancellation, timeouts, maximum turns, redaction, and no implicit tool execution.

## Test data and privacy

Use synthetic tasks and evidence only. Tests must not call external model services or use real provider credentials. Temporary DB files must be removed by test fixtures. Do not store outputs from unrelated user conversations in test data.

## Experiment quality

The 12-task comparison is exploratory, counterbalanced, and not statistically conclusive. Record unavailable token metrics as unavailable; never estimate them as measurements. Keep task sets and scoring rubric fixed before each run.
