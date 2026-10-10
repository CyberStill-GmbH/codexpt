# Local operations

## Prerequisites

- Rust stable toolchain and Cargo.
- Windows/macOS/Linux with filesystem support for SQLite WAL.

## Run

```powershell
cargo run -- --help
cargo run -- init
```

The app is a command-line process, not a background server. It opens `.codexpt/state.sqlite3`, performs schema migrations, executes one command, and exits. No network port is opened.

## Data and backup

- Keep `.codexpt/` local and out of Git.
- Before upgrades, make a consistent SQLite backup while no writes are running; do not copy the main DB alone while WAL is active.
- Verify backups by restoring to a temporary path and checking task and event counts.
- No automatic backup, encryption layer, or recovery-point objective is supplied in this MVP.

## Troubleshooting

- `database is locked`: ensure no stale writer remains; retry after it exits. The configured wait is five seconds.
- Unsupported schema version: use a compatible binary or export data; never lower `user_version` manually.
- Invalid transition: inspect the current task state and event history, then retry with the observed expected state.
- Data loss/corruption: stop writes and restore a verified backup; preserve the original file for diagnosis.

## Stop

The CLI exits after each operation. Stop a future local server with Ctrl+C and confirm its process has exited before shutting down the workstation.
