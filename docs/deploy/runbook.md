# catalyrst bundle stack — bring-up runbook

Stand up the full catalyrst service plane as **one TLS host** backed by six
bundle binaries plus the content core. All processes bind loopback; a reverse
proxy terminates TLS and path-routes (`docs/deploy/nginx-catalyrst-bundles.conf`).

| Process | Port | Members |
|---|---|---|
| catalyrst-live (content) | 5141 | content, lambdas, about |
| catalyrst-explore | 5143 | places, events, archipelago, worlds, map, lists |
| catalyrst-create | 5144 | builder, camera-reel, ab-registry |
| catalyrst-social | 5145 | communities, comms, notifications, badges, media |
| catalyrst-data | 5146 | market, economy, price, credits, rpc |
| catalyrst-ab-cdn | 5147 | asset-bundle CDN (LOD/manifest/binaries) |
| catalyrst-social-rpc | 5148 | dcl-rpc WebSocket (friends/voice) |
| catalyrst-market | 5133 | standalone marketplace (optional, parallel to data /v1) |

Each process binds a distinct loopback port; assign ports from a range of your
choosing and keep them consistent across the units and the reverse-proxy config.

Prereqs: a PostgreSQL instance reachable on `<DB_HOST>:5433`, the
marketplace DB populated, the content DB synced, a LiveKit SFU reachable.

---

## 1. Build the release binaries

```bash
cd /path/to/catalyrst
for b in explore create social data ab-cdn social-rpc; do
  cargo build --release --bin catalyrst-$b
done
# also the content core + standalone market if not already pinned:
cargo build --release --bin catalyrst-live
cargo build --release --bin catalyrst-market
# binaries land in target/release/catalyrst-<name>
```

On NixOS, run the `cargo build` invocations inside an FHS shell so the toolchain
finds its expected dynamic libraries.

The systemd units expect each binary at a stable path. Either pin the built
binaries into your deployment's gcroots/store path, or symlink the freshly built
`target/release/*` into the path your units reference for a quick bring-up:

```bash
for b in explore create social data ab-cdn social-rpc; do
  mkdir -p <DATA_DIR>/bin/catalyrst-$b
  ln -sf /path/to/catalyrst/target/release/catalyrst-$b \
         <DATA_DIR>/bin/catalyrst-$b/catalyrst-$b
done
```

---

## 2. Mint DB reader/writer roles

Bundles read the shared PostgreSQL cluster. Create **least-privilege** roles —
read-only where a bundle only queries, read/write where it owns a DB.

```bash
PSQL="psql -h <DB_HOST> -p 5433 -U <DB_ADMIN>"

# read-only roles
$PSQL -c "CREATE ROLE cat_explore_ro LOGIN PASSWORD 'CHANGE_ME';"
$PSQL -c "CREATE ROLE cat_data_ro    LOGIN PASSWORD 'CHANGE_ME';"
$PSQL -c "CREATE ROLE cat_content_ro LOGIN PASSWORD 'CHANGE_ME';"
# read/write roles (own their service DBs)
$PSQL -c "CREATE ROLE cat_create_rw  LOGIN PASSWORD 'CHANGE_ME';"
$PSQL -c "CREATE ROLE cat_social_rw  LOGIN PASSWORD 'CHANGE_ME';"
$PSQL -c "CREATE ROLE cat_data_rw    LOGIN PASSWORD 'CHANGE_ME';"

# explore: read places_events + marketplace + content
for db in places_events marketplace content; do
  $PSQL -d $db -c "GRANT CONNECT ON DATABASE $db TO cat_explore_ro;"
  $PSQL -d $db -c "GRANT USAGE ON SCHEMA public TO cat_explore_ro;"
  $PSQL -d $db -c "GRANT SELECT ON ALL TABLES IN SCHEMA public TO cat_explore_ro;"
  $PSQL -d $db -c "ALTER DEFAULT PRIVILEGES IN SCHEMA public GRANT SELECT TO cat_explore_ro;"
done

# data: read marketplace (market/economy/price), write credits
$PSQL -d marketplace -c "GRANT CONNECT ON DATABASE marketplace TO cat_data_ro;"
$PSQL -d marketplace -c "GRANT USAGE ON SCHEMA marketplace, favorites, squid_marketplace TO cat_data_ro;"
$PSQL -d marketplace -c "GRANT SELECT ON ALL TABLES IN SCHEMA marketplace, favorites, squid_marketplace TO cat_data_ro;"
$PSQL -c "CREATE DATABASE credits OWNER cat_data_rw;" 2>/dev/null || true

# create: own ab_registry, read content
$PSQL -c "CREATE DATABASE ab_registry OWNER cat_create_rw;" 2>/dev/null || true
$PSQL -d content -c "GRANT CONNECT ON DATABASE content TO cat_content_ro;"
$PSQL -d content -c "GRANT USAGE ON SCHEMA public TO cat_content_ro;"
$PSQL -d content -c "GRANT SELECT ON ALL TABLES IN SCHEMA public TO cat_content_ro;"

# social: own communities, comms_gatekeeper, notifications, badges
for db in communities comms_gatekeeper notifications badges; do
  $PSQL -c "CREATE DATABASE $db OWNER cat_social_rw;" 2>/dev/null || true
done
```

Each member crate runs its own sqlx migrations at `build_state()` against the
DB it owns, so the writer roles just need ownership/DDL on their database.

---

## 3. Install env files + units

Copy the templates, fill the `CHANGE_ME` / `example.com` placeholders:

```bash
for b in explore create social data ab-cdn social-rpc; do
  cp docs/deploy/env/catalyrst-$b.env.example <ENV_DIR>/catalyrst-$b.env
  chmod 600 <ENV_DIR>/catalyrst-$b.env
done
# then edit each: real DB passwords, LIVEKIT_* (key/secret/host/webhook),
# the public realm host, moderator/admin addresses, content dirs.

cp nixos/systemd/catalyrst-*.service ~/.config/systemd/user/
systemctl --user daemon-reload
```

LiveKit: the `devkey`/`devsecret` defaults mint JWTs a real SFU rejects. Set
`LIVEKIT_API_KEY` / `LIVEKIT_API_SECRET` / `LIVEKIT_HOST` (or `LIVEKIT_WS_URL`
for archipelago) / `LIVEKIT_WEBHOOK_KEY` consistently across the **explore**
(archipelago, worlds) and **social** (comms) env files — they share one SFU.

---

## 4. Set /about public URLs (realm discovery)

The Unity client discovers the realm from `GET /about`. Point the content core's
public URLs at the TLS host so clients resolve content/lambdas correctly:

```bash
# in the content service's environment file (or the catalyrst-live unit Environment=)
PUBLIC_URL=https://realm.example.com
CONTENT_URL=https://realm.example.com/content/
LAMBDAS_URL=https://realm.example.com/lambdas/
CONTENT_SERVER_ADDRESS=https://realm.example.com/content
REALM_NAME=my-realm
```

The non-realm-discovered hosts (places/market/communities/etc.) are reached by
the client through `DecentralandUrlsSource` host substitution; the reverse proxy
maps each external host's paths to the right bundle (see the conf header table).
No `/about` field carries those — they ride the same TLS host by path.

---

## 5. Start the stack (dependency order)

```bash
# content core first, then explore (depends on content),
# then the rest; social-rpc after social (comms gatekeeper).
systemctl --user start catalyrst.service           # catalyrst-live
systemctl --user start catalyrst-explore.service
systemctl --user start catalyrst-create.service
systemctl --user start catalyrst-social.service
systemctl --user start catalyrst-data.service
systemctl --user start catalyrst-ab-cdn.service
systemctl --user start catalyrst-social-rpc.service

systemctl --user enable catalyrst-{explore,create,social,data,ab-cdn,social-rpc}.service
systemctl --user list-units 'catalyrst-*'
```

Reload the reverse proxy after dropping in the bundle server block:

```bash
sudo cp docs/deploy/nginx-catalyrst-bundles.conf /etc/nginx/sites-available/catalyrst-bundles.conf
sudo nginx -t && sudo systemctl reload nginx
```

Each bundle reports member health on its own port:

```bash
for p in 5143 5144 5145 5146; do
  echo "== :$p =="; curl -s localhost:$p/health | jq .
done
# expect {"status":"ok","members":{...:"up"}} per bundle.
# a "degraded" status names the down member; check that member's env/DB.
```

---

## 6. Smoke tests (loopback, then through the edge)

Loopback (per bundle — substitute each bundle's loopback port):

```bash
# explore
curl -s 'localhost:5143/api/places?limit=1' | jq '.data|length'
curl -s 'localhost:5143/api/events' | jq '.total'
curl -s 'localhost:5143/pois' | jq 'length'
curl -s 'localhost:5143/v2/ping'                      # -> "/v2/ping"
curl -s 'localhost:5143/hot-scenes' | jq 'length'
# create
curl -s 'localhost:5144/profiles/metadata' -o /dev/null -w '%{http_code}\n'
# social
curl -s 'localhost:5145/v1/communities?limit=1' | jq '.data|length'
curl -s -o /dev/null -w '%{http_code}\n' 'localhost:5145/notifications'   # 400/401 ok (needs auth)
# data
curl -s 'localhost:5146/v1/catalog?first=1' | jq '.data|length'
curl -s 'localhost:5146/api/v3/simple/price?ids=decentraland&vs_currencies=usd' | jq .
# ab-cdn
curl -s -o /dev/null -w '%{http_code}\n' 'localhost:5147/manifest/doesnotexist_windows.json'  # 404 json
# social-rpc — WS upgrade
curl -s -o /dev/null -w '%{http_code}\n' -H 'Connection: Upgrade' -H 'Upgrade: websocket' \
  -H 'Sec-WebSocket-Key: x' -H 'Sec-WebSocket-Version: 13' localhost:5148/   # 101/426
```

Through the edge (TLS host):

```bash
H=https://realm.example.com
curl -s "$H/about" | jq '.healthy, .configurations.realmName'
curl -s "$H/api/places?limit=1" | jq '.data|length'
curl -s "$H/api/events" | jq '.total'
curl -s "$H/v1/communities?limit=1" | jq '.data|length'
curl -s "$H/v1/catalog?first=1" | jq '.data|length'
curl -s "$H/pois" | jq 'length'
curl -s -o /dev/null -w '%{http_code}\n' "$H/manifest/x_windows.json"   # 404
curl -s "$H/content/status" | jq '.version'
# parity vs upstream (sanity)
diff <(curl -s "$H/api/places?limit=1" | jq -S .) \
     <(curl -s 'https://places.decentraland.org/api/places?limit=1' | jq -S .) | head
```

A green run: every `/health` is `ok`, the edge `/about` reports `healthy:true`
with the realm name, and the per-host smoke curls return data (not 404/502).

---

## 7. Teardown / rollback

```bash
systemctl --user stop catalyrst-{explore,create,social,data,ab-cdn,social-rpc}.service
# repoint the reverse proxy back to the prior block and reload; the content core
# is independent and can stay up.
```

Notes:
- Bundles fail-soft: a member whose `build_state()` errors is dropped and the
  bundle still serves its healthy siblings (`/health` shows it `down`). Fix the
  member's env/DB and `systemctl --user restart` the bundle.
- Standalone `catalyrst-market` can run in parallel; the data bundle already
  serves the canonical `/v1` marketplace tree, so the standalone market is only
  needed if you want marketplace isolated from economy/price/credits.
- Write paths (communities, builder content ingest, economy relay) require the
  federation signed-write contract and currently return 501 — see `docs/write-path.md`.
```
