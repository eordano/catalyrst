#!/usr/bin/env bash
# Local twin of CI's sqlx-prepare-check job: the committed offline query cache
# (crates/catalyrst-server/.sqlx) must match the query! macros + migrated
# schema, or every offline build breaks. Spins up a throwaway postgres that
# listens ONLY on a unix socket in its own mktemp dir -- no TCP port, so it can
# never collide with the 5xxx listeners or a concurrent guard run -- applies
# the server migrations in lexical order, then re-prepares and diffs.
# --regen swaps the diff for a rewrite of .sqlx/ (stage the result).
set -euo pipefail

SITES="$(cd "$(dirname "$0")/.." && pwd)"
ROOT="$(cd "$SITES/../.." && pwd)"
SERVER="$ROOT/catalyrst/crates/catalyrst-server"

mode=check
[[ "${1:-}" == "--regen" ]] && mode=regen

ver="$(sqlx --version 2>/dev/null || true)"
if [[ ! $ver =~ ^sqlx-cli\ 0\.9\. ]]; then
  echo "sqlx-guard: need sqlx-cli 0.9.x to match Cargo.lock sqlx 0.9.0 (got: ${ver:-not on PATH})." >&2
  echo "sqlx-guard: fix: run inside the catalyrst devshell -- nix develop <repo>/catalyrst#ci -- which builds sqlx-cli 0.9.0." >&2
  exit 1
fi
if ! command -v initdb >/dev/null 2>&1; then
  echo "sqlx-guard: initdb not on PATH -- run inside the catalyrst devshell (nix develop <repo>/catalyrst#ci)." >&2
  exit 1
fi

TMP="$(mktemp -d)"
started=0
cleanup() {
  if [[ $started -eq 1 ]]; then
    pg_ctl -D "$TMP/db" -m immediate stop >/dev/null 2>&1 || true
  fi
  rm -rf "$TMP"
}
trap cleanup EXIT

initdb -D "$TMP/db" -U postgres -A trust >"$TMP/initdb.log" 2>&1
pg_ctl -D "$TMP/db" -l "$TMP/pg.log" -w \
  -o "-c listen_addresses='' -c unix_socket_directories='$TMP'" start >/dev/null
started=1

createdb -h "$TMP" -U postgres sqlx_guard
for f in "$SERVER"/migrations/*.sql; do
  psql -h "$TMP" -U postgres -d sqlx_guard -v ON_ERROR_STOP=1 -q -f "$f"
done

export DATABASE_URL="postgres:///sqlx_guard?host=$TMP&user=postgres"

if [[ $mode == regen ]]; then
  (cd "$SERVER" && cargo sqlx prepare -- --all-targets)
  echo "sqlx-guard: regenerated $SERVER/.sqlx -- stage the changed files with your SQL edit." >&2
  exit 0
fi

if ! (cd "$SERVER" && cargo sqlx prepare --check -- --all-targets); then
  cat >&2 <<'EOF'

sqlx-guard: OFFLINE QUERY CACHE DRIFT -- crates/catalyrst-server/.sqlx no longer
matches the query! macros + migrated schema. Offline builds and CI's
sqlx-prepare-check job will fail until it is regenerated.

  Fix: catalyrst/sites/scripts/sqlx-guard.sh --regen
       (runs `cargo sqlx prepare -- --all-targets` from crates/catalyrst-server
       against a fresh migrated scratch DB), then stage the changed .sqlx/ files.
EOF
  exit 1
fi
echo "sqlx-guard: .sqlx offline cache matches the schema and queries." >&2
