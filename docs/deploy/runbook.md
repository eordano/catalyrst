# Bundle-stack bring-up runbook

> Status: rewritten 2026-07-04 from the previous runbook (re-verified
> 2026-07-03) + current code. This is the **bundle** deployment style: one TLS
> host, six bundle binaries + the content core, all loopback, path-routed by
> a reverse proxy ([nginx-catalyrst-bundles.conf](./nginx-catalyrst-bundles.conf)).
> Alternatives: the NixOS module or per-service standalone units — see
> [architecture.md §5](../architecture.md).

| Process | Default port | Members |
|---|---|---|
| catalyrst-live | 5141 (`CATALYRST_PORT`) | content, lambdas, about |
| catalyrst-explore | 5143 | places, events, archipelago, worlds, map, lists |
| catalyrst-create | 5144 | builder, camera-reel, registry |
| catalyrst-social | 5145 | communities, comms, notifications, badges, media |
| catalyrst-data | 5146 | market, economy, price, credits, rpc |
| catalyrst-abgen | 5147 | asset-bundle CDN + in-process converter |
| catalyrst-social-rpc | 5148 | dcl-rpc WebSocket (friends/voice) |
| catalyrst-market | 5133 | standalone marketplace (optional; data already serves `/v1`) |

Bundles bind `BUNDLE_HTTP_PORT` on loopback; pick any consistent port range.
Prereqs: PostgreSQL 18 reachable, the marketplace (squid) DB populated, the
content DB synced, a LiveKit SFU reachable.

## 1. Build

```bash
for b in explore create social data abgen social-rpc; do
  cargo build --release --bin catalyrst-$b
done
cargo build --release --bin catalyrst-live
# NixOS: use an FHS/nix shell — see ../build-and-test.md
```

Pin the built binaries at stable paths for the units (symlink from
`target/release/` for a quick bring-up).

## 2. Mint least-privilege DB roles

Read-only roles where a bundle only queries; owner roles where a member runs
its own sqlx migrations at `build_state()` (the writer role needs DDL on its
DB — see the ownership map in [architecture.md §3](../architecture.md)):

```bash
PSQL="psql -h <DB_HOST> -p <DB_PORT> -U <DB_ADMIN>"
$PSQL -c "CREATE ROLE cat_explore_ro LOGIN PASSWORD '…';"   # + cat_data_ro, cat_content_ro
$PSQL -c "CREATE ROLE cat_create_rw  LOGIN PASSWORD '…';"   # + cat_social_rw, cat_data_rw

# explore reads places_events + marketplace + content
for db in places_events marketplace content; do
  $PSQL -d $db -c "GRANT CONNECT ON DATABASE $db TO cat_explore_ro;"
  $PSQL -d $db -c "GRANT USAGE ON SCHEMA public TO cat_explore_ro;"
  $PSQL -d $db -c "GRANT SELECT ON ALL TABLES IN SCHEMA public TO cat_explore_ro;"
  $PSQL -d $db -c "ALTER DEFAULT PRIVILEGES IN SCHEMA public GRANT SELECT TO cat_explore_ro;"
done
# data reads marketplace schemas, owns credits
$PSQL -d marketplace -c "GRANT USAGE ON SCHEMA marketplace, favorites, squid_marketplace TO cat_data_ro;"
$PSQL -d marketplace -c "GRANT SELECT ON ALL TABLES IN SCHEMA marketplace, favorites, squid_marketplace TO cat_data_ro;"
$PSQL -c "CREATE DATABASE credits OWNER cat_data_rw;"
# create owns ab_registry, reads content; social owns its four DBs
$PSQL -c "CREATE DATABASE ab_registry OWNER cat_create_rw;"
for db in communities comms_gatekeeper notifications badges; do
  $PSQL -c "CREATE DATABASE $db OWNER cat_social_rw;"
done
```

## 3. Env files

Each member reads its own `Config::from_env`, so a bundle's env file is the
**concatenation of its members' env files** (see each
`crates/catalyrst-<member>/src/config.rs` for the authoritative variable
list). Duplicate `HTTP_SERVER_PORT`/`HTTP_SERVER_HOST` keys are harmless — the
bundle binds only `BUNDLE_HTTP_PORT` — but check that any *other* overlapping
key (e.g. `DAPPS_PG_COMPONENT_PSQL_*`, shared by market/economy) resolves to
the value you want. `chmod 600` everything.

Gotchas:

- **`catalyrst-live` auto-loads `/etc/catalyrst/content.env`** at boot in
  addition to its unit environment.
- **LiveKit**: the `devkey`/`devsecret` defaults mint JWTs a real SFU rejects
  — set `LIVEKIT_API_KEY`/`_SECRET`/`LIVEKIT_HOST` (+ `LIVEKIT_WEBHOOK_KEY`)
  **consistently across explore (archipelago, worlds) and social (comms)** —
  they share one SFU ([../operations/livekit.md](../operations/livekit.md)).
- Money endpoints: credits/market/economy refuse a non-loopback bind without
  their admin token set — intended; keep them loopback behind the proxy.
- Template units: `nixos/systemd/catalyrst-*.service` (fill the
  `<WORKSPACE>`/`<DATA_DIR>` placeholders, `EnvironmentFile=` per bundle).

## 4. Realm discovery (`/about` public URLs)

Clients discover the realm from `GET /about`; point the content core's public
URLs at the TLS host:

```bash
PUBLIC_URL=https://realm.example.com
CONTENT_URL=https://realm.example.com/content/
LAMBDAS_URL=https://realm.example.com/lambdas/
CONTENT_SERVER_ADDRESS=https://realm.example.com/content
REALM_NAME=my-realm
```

The non-realm-discovered hosts (places/market/communities/…) are reached via
client URL substitution + edge path-routing — no `/about` field carries them
([explorer-pointing.md](./explorer-pointing.md)).

## 5. Start order + health

```bash
systemctl --user start catalyrst.service           # content core first
systemctl --user start catalyrst-explore.service   # depends on content
systemctl --user start catalyrst-{create,social,data,abgen}.service
systemctl --user start catalyrst-social-rpc.service  # after social (gatekeeper)

for p in 5143 5144 5145 5146; do curl -s localhost:$p/health | jq .; done
# {"status":"ok","members":{…:"up"}} per bundle; "degraded" names the down
# member — bundles fail soft, healthy siblings keep serving.
```

Reload the proxy with the bundle server block
([nginx-catalyrst-bundles.conf](./nginx-catalyrst-bundles.conf)).

## 6. Smoke tests

Loopback:

```bash
curl -s 'localhost:5143/api/places?limit=1' | jq '.data|length'
curl -s 'localhost:5143/api/events' | jq '.total'
curl -s 'localhost:5143/pois' | jq 'length'
curl -s 'localhost:5144/profiles/metadata' -o /dev/null -w '%{http_code}\n'
curl -s 'localhost:5145/v1/communities?limit=1' | jq '.data|length'
curl -s 'localhost:5146/v1/catalog?first=1' | jq '.data|length'
curl -s 'localhost:5146/api/v3/simple/price?ids=decentraland&vs_currencies=usd' | jq .
curl -s -o /dev/null -w '%{http_code}\n' 'localhost:5147/manifest/doesnotexist_windows.json'  # 404
curl -s -o /dev/null -w '%{http_code}\n' -H 'Connection: Upgrade' -H 'Upgrade: websocket' \
  -H 'Sec-WebSocket-Key: x' -H 'Sec-WebSocket-Version: 13' localhost:5148/   # 101/426
```

Through the edge:

```bash
H=https://realm.example.com
curl -s "$H/about" | jq '.healthy, .configurations.realmName'
curl -s "$H/content/status" | jq '.version'
# spot parity vs upstream
diff <(curl -s "$H/api/places?limit=1" | jq -S .) \
     <(curl -s 'https://places.decentraland.org/api/places?limit=1' | jq -S .) | head
```

Green = every `/health` ok, edge `/about` says `healthy:true` with the realm
name, smoke curls return data.

## 7. Teardown / notes

```bash
systemctl --user stop catalyrst-{explore,create,social,data,abgen,social-rpc}.service
# repoint the proxy; the content core is independent and can stay up
```

- The standalone `catalyrst-market` is only needed if you want marketplace
  isolated from economy/price/credits.
- Content-serving throughput: enable nginx `X-Accel-Redirect` zero-copy via
  `STORAGE_X_ACCEL_BASE` (DEPLOYMENT.md).
- What still returns 501 and which toggles gate real money movement:
  [../status-and-parity.md](../status-and-parity.md).
