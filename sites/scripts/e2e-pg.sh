#!/usr/bin/env bash
# Provision a throwaway Postgres cluster in a temp dir, export SITES_E2E_PG_URL
# at it, run the given command, then stop and delete the cluster. globalSetup's
# "provided" path does the rest (scratch database, schema, seed, drop).
#
#   scripts/e2e-pg.sh vitest run --config vitest.e2e.config.ts
#
# A server the operator already pointed the suite at wins: with SITES_E2E_PG_URL
# set, the command runs untouched. With no usable initdb the command also runs
# untouched, and globalSetup degrades exactly as before. The cluster listens on
# a unix socket only -- no TCP port, nothing shared, nothing near a real database.
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
  # NixOS trap: the profile wrapper (/run/current-system/sw/bin/initdb) fails
  # its own relocation check. The store path it links to works, and on any
  # ordinary install readlink -f is the identity.
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

# `localhost` keeps the string parseable by WHATWG `new URL` (globalSetup edits
# its pathname); pg-connection-string gives the `?host=` socket dir precedence,
# so nothing ever dials TCP.
export SITES_E2E_PG_URL="postgresql://postgres@localhost/postgres?host=$SOCK"
echo "[e2e-pg] throwaway cluster up at $SOCK (removed on exit)" >&2

status=0
"$@" || status=$?
exit $status
