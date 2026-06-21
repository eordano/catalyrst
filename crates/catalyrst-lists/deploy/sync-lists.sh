#!/usr/bin/env bash
# Refresh the catalyrst-lists master lists from the live upstream
# (dcl-lists.decentraland.org by default), upserting into the lists tables.
# Intended to run daily via a scheduler. The curated master is admin-managed
# upstream and not reconstructable locally, so we pull.
#
# Place this in <WORKSPACE>/scripts/ alongside the other sync scripts.
set -euo pipefail
HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=_lib.sh
source "$HERE/_lib.sh"

load_db_creds
set -a; source "$ENV_DIR/db.env"; set +a

UPSTREAM="${LISTS_UPSTREAM:-https://dcl-lists.decentraland.org}"
PSQL=(env "PGPASSWORD=$POSTGRES_PE_PASSWORD" psql -h "$PG_SOCK_DIR" -p "$PG_PORT"
      -U "$POSTGRES_PE_USER" -d "$POSTGRES_PE_DB" -v ON_ERROR_STOP=1 --no-psqlrc -q)

# Sync one list: pull JSON {data:[...]}, stage values via COPY (tab-delimited,
# jq-escaped), then upsert + prune in one transaction so a partial pull never
# leaves the table empty.
upsert_list() {
  local endpoint="$1" table="$2" col="$3"
  log "pulling $endpoint"
  local values
  values=$(curl -fsS -m 30 -X POST "$UPSTREAM/$endpoint" | jq -r '.data[]')
  {
    echo "BEGIN;"
    echo "CREATE TEMP TABLE _stage (v TEXT PRIMARY KEY) ON COMMIT DROP;"
    echo "COPY _stage (v) FROM STDIN;"
    # jq already gave us one value per line; values are coords / name strings
    # with no tabs/newlines, safe for COPY text format.
    printf '%s\n' "$values"
    echo "\\."
    echo "INSERT INTO $table ($col) SELECT v FROM _stage"
    echo "  ON CONFLICT ($col) DO UPDATE SET updated_at = now();"
    echo "DELETE FROM $table WHERE $col NOT IN (SELECT v FROM _stage);"
    echo "COMMIT;"
  } | "${PSQL[@]}"
  log "  $table now $(${PSQL[@]} -tAc "SELECT count(*) FROM $table") rows"
}

upsert_list pois          lists_poi         coord
upsert_list banned-names  lists_banned_name name

log "sync-lists complete."
