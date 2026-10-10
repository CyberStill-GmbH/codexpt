# Local operations

## Prerequisites

- Rust stable toolchain and Cargo.
- Windows/macOS/Linux with filesystem support for SQLite WAL.

## Run

```powershell
cargo run -- --help
cargo run -- init
```

CLI commands open `.codexpt/state.sqlite3`, perform schema migrations, execute one command, and exit without opening a port. The separate `serve` command starts the optional loopback dashboard described below.

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
# Local dashboard

Build the UI assets after changing frontend sources, then start Rust:

```powershell
npm --prefix web ci
npm --prefix web run build
cargo run -- serve
```

The service binds only to `127.0.0.1:4173`. Open that URL on the same machine. Use `--db <path>` before `serve` to choose a separate SQLite file; the default is `.codexpt/state.sqlite3`. Use `--port <port>` if 4173 is occupied. Stop the server with Ctrl+C. The application does not authenticate actors or accept remote connections; do not tunnel, proxy, or expose the port.

To run it against a disposable database for a manual browser walkthrough, choose a path outside the repository and remove it only after the process has stopped and you have confirmed it contains no needed data.
