#!/bin/sh
set -eu

: "${CODEXPT_OIDC_ISSUER:?Set the OIDC issuer URL}"
: "${CODEXPT_OIDC_CLIENT_ID:?Set the OIDC public client ID}"
: "${CODEXPT_OIDC_REDIRECT_URI:?Set the exact registered OIDC callback URL}"
: "${CODEXPT_LOGIN_SUCCESS_URL:?Set the HTTPS post-login URL}"
: "${CODEXPT_API_HOST:?Set the public host expected by the application}"
: "${CODEXPT_WEB_ORIGIN:?Set the exact HTTPS browser origin}"

if ! awk '$5 == "/data" { found = 1 } END { exit !found }' /proc/self/mountinfo; then
    echo "A persistent volume must be mounted at /data" >&2
    exit 1
fi

PORT="${PORT:-4173}"
CODEXPT_DB_PATH="${CODEXPT_DB_PATH:-/data/codexpt.sqlite3}"
CODEXPT_BIND_ADDR="${CODEXPT_BIND_ADDR:-0.0.0.0:${PORT}}"
export CODEXPT_BIND_ADDR

exec /usr/local/bin/codexpt --db "$CODEXPT_DB_PATH" serve --port "$PORT"
