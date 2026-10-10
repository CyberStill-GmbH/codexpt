#!/bin/sh
set -eu

case "${CODEXPT_OIDC_LOOPBACK_TEST:-false}" in
    false) ;;
    true)
        echo "OIDC loopback test mode is disabled in release containers" >&2
        exit 1
        ;;
    *)
        echo "CODEXPT_OIDC_LOOPBACK_TEST must be the literal false in release containers" >&2
        exit 1
        ;;
esac

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
case "$CODEXPT_DB_PATH" in
    /data/*) relative_db_path=${CODEXPT_DB_PATH#/data/} ;;
    *)
        echo "CODEXPT_DB_PATH must be a normalized path below /data" >&2
        exit 1
        ;;
esac
case "$relative_db_path" in
    ''|/*|*//*|.|./*|*/./*|*/.|..|../*|*/../*|*/..|*/)
        echo "CODEXPT_DB_PATH must be a normalized path below /data" >&2
        exit 1
        ;;
esac
if [ -L "$CODEXPT_DB_PATH" ] || [ -d "$CODEXPT_DB_PATH" ]; then
    echo "CODEXPT_DB_PATH must be a regular database file below /data" >&2
    exit 1
fi
database_parent=$(dirname -- "$CODEXPT_DB_PATH")
canonical_data=$(realpath -e -- /data)
existing_parent=$database_parent
while [ ! -e "$existing_parent" ] && [ ! -L "$existing_parent" ]; do
    next_parent=$(dirname -- "$existing_parent")
    if [ "$next_parent" = "$existing_parent" ]; then
        echo "CODEXPT_DB_PATH has no existing parent under /data" >&2
        exit 1
    fi
    existing_parent=$next_parent
done
if ! canonical_existing_parent=$(realpath -e -- "$existing_parent"); then
    echo "CODEXPT_DB_PATH has an invalid existing parent" >&2
    exit 1
fi
case "$canonical_existing_parent" in
    "$canonical_data"|"$canonical_data"/*) ;;
    *)
        echo "CODEXPT_DB_PATH resolves outside the mounted /data volume" >&2
        exit 1
        ;;
esac
mkdir -p -- "$database_parent"
canonical_parent=$(realpath -e -- "$database_parent")
case "$canonical_parent" in
    "$canonical_data"|"$canonical_data"/*) ;;
    *)
        echo "CODEXPT_DB_PATH resolves outside the mounted /data volume" >&2
        exit 1
        ;;
esac
CODEXPT_BIND_ADDR="${CODEXPT_BIND_ADDR:-0.0.0.0:${PORT}}"
export CODEXPT_BIND_ADDR

exec /usr/local/bin/codexpt --db "$CODEXPT_DB_PATH" serve --port "$PORT"
