#!/usr/bin/env bash
set -euo pipefail

if [ $# -eq 0 ]; then
  echo "usage: $0 <command> [args...]" >&2
  exit 2
fi

if [ -n "${SITES_E2E_PG_URL:-}" ]; then
  exec "$@"
fi

resolve_bindir() {
  local found real
  found=$(command -v initdb 2>/dev/null) || return 1
  real=$(readlink -f "$found")
  dirname "$real"
}

BINDIR=$(resolve_bindir) || {
  echo "[e2e-pg] no initdb on PATH -- running without a cluster" >&2
  exec "$@"
}
if [ ! -x "$BINDIR/pg_ctl" ]; then
  echo "[e2e-pg] $BINDIR/pg_ctl missing -- running without a cluster" >&2
  exec "$@"
fi

BASE=$(mktemp -d "${TMPDIR:-/tmp}/sites-e2e-pg.XXXXXX")
SOCK="$BASE/sock"
DATA="$BASE/data"
mkdir -p "$SOCK"

cleanup() {
  "$BINDIR/pg_ctl" -D "$DATA" -m immediate stop >/dev/null 2>&1 || true
  rm -rf "$BASE"
}
trap cleanup EXIT INT TERM

if ! "$BINDIR/initdb" -D "$DATA" -A trust -U postgres --no-sync \
    >"$BASE/initdb.log" 2>&1; then
  echo "[e2e-pg] initdb failed (see below) -- running without a cluster" >&2
  tail -5 "$BASE/initdb.log" >&2 || true
  cleanup
  trap - EXIT INT TERM
  exec "$@"
fi

if ! "$BINDIR/pg_ctl" -D "$DATA" \
    -o "-k $SOCK -c listen_addresses='' -c fsync=off" \
    -w -t 60 start >"$BASE/pg_ctl.log" 2>&1; then
  echo "[e2e-pg] pg_ctl start failed (see below) -- running without a cluster" >&2
  tail -5 "$BASE/pg_ctl.log" >&2 || true
  cleanup
  trap - EXIT INT TERM
  exec "$@"
fi

export SITES_E2E_PG_URL="postgresql://postgres@localhost/postgres?host=$SOCK"
echo "[e2e-pg] throwaway cluster up at $SOCK (removed on exit)" >&2

status=0
"$@" || status=$?
exit $status
