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
- Create a consistent online backup through SQLite's backup API; never copy only the main DB while WAL is active:

  ```powershell
  cargo run -- --db .codexpt/state.sqlite3 backup --output .codexpt/backups/state-2026-10-10.sqlite3
  ```

- Exercise a restore to a new path before upgrades or after recovery. The destination must not exist, which preserves the damaged/original file:

  ```powershell
  cargo run -- --db .codexpt/restore-check.sqlite3 restore --backup .codexpt/backups/state-2026-10-10.sqlite3
  cargo run -- --db .codexpt/restore-check.sqlite3 task list
  ```

- The backup and restore commands run `PRAGMA integrity_check`. Restore also validates the database in a temporary file before atomically publishing it at a new destination.
- Restore also revokes every restored active session, consumes pending OIDC login transactions, and revokes pending invitation tokens in one transaction; it writes an `identity_audit_events` recovery record. Users must authenticate again and Owners must reissue any pending invites after a recovery. This prevents a backup from resurrecting sessions, login transactions, or invite tokens that were invalidated after the backup was taken.
- The restore test opens the database in WAL mode, writes task history and identity state, makes an online backup, restores it, verifies the workspace/history data, and checks that restored sessions/invites/login transactions are invalidated and logged.
- A hosted schedule must upload these generated files to a private, versioned S3 bucket and alert on command/upload failure. EBS snapshots alone are not the documented SQLite recovery path.
- Local MVP backups are operator-triggered; cloud automation and a recovery-point objective are not supplied until the Terraform beta is reviewed and applied.

### Hosted restore procedure (after the beta is approved)

1. Open an SSM Session Manager shell and stop `codexpt.service` and `codexpt-backup.timer`.
2. Download the selected S3 backup to `/var/lib/codexpt/recovery/backup.sqlite3` using the instance role. Record its S3 key and verify the expected restore point.
3. Move the current `state.sqlite3` and any `-wal`/`-shm` sidecars into a timestamped recovery directory; never overwrite or delete the original files.
4. Run `codexpt --db /var/lib/codexpt/state.sqlite3 restore --backup /var/lib/codexpt/recovery/backup.sqlite3` as the `codexpt` service user. The destination must be absent. Restore validates integrity, applies the current identity migrations, invalidates restored sessions and pending credentials, then logs the recovery.
5. Start the service and backup timer, check `/api/health` and service logs, confirm the intended workspace and task history, then create fresh invitations for users whose invite was pending at the restore point.
6. Keep the pre-restore database and downloaded backup until the recovered service passes its checks. Reconcile work created after the backup from external evidence because a restore can roll back recent task changes.

## Troubleshooting

- `database is locked`: ensure no stale writer remains; retry after it exits. The configured wait is five seconds.
- Unsupported schema version: use a compatible binary or export data; never lower `user_version` manually.
- Invalid transition: inspect the current task state and event history, then retry with the observed expected state.
- Data loss/corruption: stop writes and restore a verified backup; preserve the original file for diagnosis.

## Stop

The CLI exits after each operation. Stop a future local server with Ctrl+C and confirm its process has exited before shutting down the workstation.

## Identity v1 prototype status

The branch contains a hosted identity prototype; it is not deploy-ready. Login stays visibly disabled unless `CODEXPT_OIDC_ISSUER`, `CODEXPT_OIDC_CLIENT_ID`, `CODEXPT_OIDC_REDIRECT_URI`, and `CODEXPT_LOGIN_SUCCESS_URL` are valid. When OIDC is enabled, `CODEXPT_BIND_ADDR`, `CODEXPT_API_HOST`, and `CODEXPT_WEB_ORIGIN` are also mandatory; production origins must use HTTPS. Terraform defaults to a Cognito public Authorization Code + PKCE client. Selecting OpenAI requires a client ID issued/authorized by OpenAI and the exact production callback registered there; no provider secret is configured. Keep the local server bound to loopback. Never test hosted settings against production data until the authorization acceptance plan passes.

The one-time `identity bootstrap-owner` command binds the first workspace Owner to the exact verified issuer/subject obtained from the OIDC provider. Record the command through the approved operator process; never place credentials, cookies, tokens, or invite codes in shell history, tickets, or Git. Invite tokens are displayed once and must be delivered only to the matching verified email address.
# Local dashboard

Build the UI assets after changing frontend sources, then start Rust:

```powershell
npm --prefix web ci
npm --prefix web run build
cargo run -- serve
```

The service binds only to `127.0.0.1:4173`. Open that URL on the same machine. Use `--db <path>` before `serve` to choose a separate SQLite file; the default is `.codexpt/state.sqlite3`. Use `--port <port>` if 4173 is occupied. Stop the server with Ctrl+C. The application does not authenticate actors or accept remote connections; do not tunnel, proxy, or expose the port.

To run it against a disposable database for a manual browser walkthrough, choose a path outside the repository and remove it only after the process has stopped and you have confirmed it contains no needed data.
