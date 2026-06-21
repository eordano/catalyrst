# E2E test plan — catalyrst-events (key=events)

Future end-to-end test plan for the catalyrst reimplementation of
`events.decentraland.org`.

- Crate: `catalyrst-events`
- Bind: `127.0.0.1:5135` (`HTTP_SERVER_PORT`, default 5135)
- DB: a shared `places_events` PostgreSQL instance (`5433`). Read path over
  `event`; federation-write path over the pre-existing `events_local` and
  `event_attendance_local` tables. No migrations introduced this phase.
- Workspace under test: `<WORKSPACE>`

This phase filled the previously-501/stub write + federation routes. The plan
below covers those plus a regression pass over the already-working read path,
and a real-client smoke step.

---

## 1. Unity config — how to repoint the client at our host

### The enum and its RawUrl line

The Unity client reaches the events service through a single enum,
`DecentralandUrl.ApiEvents` (value `22`, defined in the unity-explorer source at
`Explorer/Assets/DCL/Infrastructure/Utility/DecentralandUrls/DecentralandUrl.cs`).

The URL template is produced by the `RawUrl(...)` switch in
`Explorer/Assets/DCL/NetworkDefinitions/Browser/DecentralandUrlsSource.cs`,
**line 182**:

```csharp
DecentralandUrl.ApiEvents => $"https://events.decentraland.{ENV}/api/events",
```

### Is this realm-dependent (i.e. /about-discovered)?

**No.** `ApiEvents` is a *static* URL template baked into the `RawUrl` switch.
It is NOT one of the `CacheBehaviour.REALM_DEPENDENT` entries (those are the
ones cleared by `ResetRealmDependentUrls` and resolved from the realm `/about`
response). The events base URL is `{ENV}`-substituted only (`org`/`zone`/`today`)
and otherwise hard-coded — the realm `/about` document has no field that
overrides it. Editing our local Catalyst `/about` response will NOT move this
endpoint.

**To repoint, you must edit Unity** (line 182). Replace the template so it
points at our service. Because `HttpEventsApiService` builds every sub-path by
appending to this base (`{base}/{event_id}/attendees`, `{base}/search`,
`{base}/attending`, etc.), repointing this one line moves the whole surface.

Edit line 182 to one of:

```csharp
// direct to the local catalyrst-events bind (no {ENV} substitution needed):
DecentralandUrl.ApiEvents => "http://127.0.0.1:5135/api/events",
```

or, if fronting the service behind a host that still wants `{ENV}` semantics:

```csharp
DecentralandUrl.ApiEvents => $"http://localhost:5135/api/events",
```

Caveats once repointed:
- The write path (POST/DELETE `…/{id}/attendees`) authenticates with
  **signed-fetch AuthChain headers**, and our handler reconstructs the signed
  payload from the **request URI path** (`OriginalUri::path()`, e.g.
  `/api/events/<id>/attendees`). The Unity side signs over
  `urlsSource.GetOriginalUrl(url)` (the full URL). If the path the client signs
  differs from the path we bind/serve (e.g. a reverse-proxy strips/adds a
  prefix), signature validation fails. For the local smoke test, bind the
  service at the same path the client signs (`/api/events/...`) with no proxy
  rewrite. A configurable public-URL prefix is the documented future fix.
- `HttpEventsApiService` expects the attend/cancel response shape `{ "ok": true }`
  — which our handlers return verbatim.

`unity_config` pointer (for the summary):
`DecentralandUrlsSource.cs line 182, RawUrl switch arm DecentralandUrl.ApiEvents
=> "https://events.decentraland.{ENV}/api/events"; repoint to
http://127.0.0.1:5135/api/events. NOT /about-discovered — ApiEvents is a static
(non-REALM_DEPENDENT) RawUrl arm, so it is changed in Unity, never via /about.`

---

## 2. Bring the service up locally

```bash
# from the workspace, with a Rust toolchain on PATH
cargo run -p catalyrst-events &
# env: HTTP_SERVER_PORT=5135, PLACES_EVENTS_PG_CONNECTION_STRING=<places_events reader>
# (see the service's environment file, <ENV_FILE>)

# liveness
curl -s -o /dev/null -w '%{http_code}\n' http://127.0.0.1:5135/ping   # expect 200
```

All curl checks below assume the service is bound on `127.0.0.1:5135`.

---

## 3. Concrete e2e curl checks

Response envelope conventions (verified in `http/response.rs`):
- Read handlers wrap data in `{"ok":true,"data":...}` (`ApiOk`).
- Attend/cancel writes return bare `{"ok":true}`.
- Errors return `{"ok":false,"error":"..."}` with the matching HTTP status.

### A. Liveness / regression on read path (already-working)

1. **Ping**
   ```bash
   curl -s -w '\n%{http_code}\n' http://127.0.0.1:5135/ping
   ```
   Expect: `200`.

2. **Event list** — `GET /api/events`
   ```bash
   curl -s -w '\n%{http_code}\n' 'http://127.0.0.1:5135/api/events?limit=5'
   ```
   Expect: `200`, body `{"ok":true,"data":[ ... ]}` with up to 5 event objects
   (each carrying `id`, `name`, `total_attendees`, `latest_attendees`,
   `attending`, etc.).

3. **Live list filter** — mirrors Unity `GetEventsAsync(onlyLiveEvents:true)`
   ```bash
   curl -s -w '\n%{http_code}\n' 'http://127.0.0.1:5135/api/events?list=live'
   ```
   Expect: `200`, `{"ok":true,"data":[...]}`.

4. **Categories** — `GET /api/events/categories`
   ```bash
   curl -s -w '\n%{http_code}\n' http://127.0.0.1:5135/api/events/categories
   ```
   Expect: `200`, `{"ok":true,"data":[...]}`.

5. **Per-event attendees (read)** — `GET /api/events/{id}/attendees`
   ```bash
   EID=$(curl -s 'http://127.0.0.1:5135/api/events?limit=1' | jq -r '.data[0].id')
   curl -s -w '\n%{http_code}\n' "http://127.0.0.1:5135/api/events/$EID/attendees"
   ```
   Expect: `200`, `{"ok":true,"data":[...]}`.

### B. Auth-gated write path (this phase — was 501)

6. **Attend without auth → 401** — `POST /api/events/{id}/attendees` (no headers)
   ```bash
   curl -s -X POST -w '\n%{http_code}\n' \
     "http://127.0.0.1:5135/api/events/$EID/attendees"
   ```
   Expect: `401`, body `{"ok":false,"error":"auth chain: ..."}`. Confirms the
   signed-fetch gate (`require_signer`) rejects unsigned requests.

7. **Cancel without auth → 401** — `DELETE /api/events/{id}/attendees`
   ```bash
   curl -s -X DELETE -w '\n%{http_code}\n' \
     "http://127.0.0.1:5135/api/events/$EID/attendees"
   ```
   Expect: `401`.

8. **Attend with a valid AuthChain → 200 {"ok":true}**
   The client signs over method + URI path + timestamp with an AuthChain in the
   signed-fetch headers (`x-identity-auth-chain-*`, `x-identity-timestamp`,
   `x-identity-metadata`). Generating a real chain by hand is involved; the
   recommended way to exercise this is the real-client smoke step (section 4),
   or a helper that emits signed-fetch headers for a throwaway identity. The
   assertion:
   ```bash
   # with valid signed-fetch headers for path /api/events/<id>/attendees, method POST
   curl -s -X POST -w '\n%{http_code}\n' \
     -H 'x-identity-auth-chain-0: ...' -H 'x-identity-auth-chain-1: ...' \
     -H 'x-identity-timestamp: <ms>' -H 'x-identity-metadata: {}' \
     "http://127.0.0.1:5135/api/events/$EID/attendees"
   ```
   Expect: `200`, body exactly `{"ok":true}`. DB side-effect: one row in
   `event_attendance_local` with `(event_id,signer)` PK, `action='attend'`,
   `signed_payload`, `signed_at`.

9. **Attend unknown event → 404** (signed, but bogus id)
   ```bash
   # signed for path /api/events/does-not-exist/attendees
   curl -s -X POST -w '\n%{http_code}\n' <signed headers> \
     http://127.0.0.1:5135/api/events/does-not-exist/attendees
   ```
   Expect: `404`, `{"ok":false,"error":"Not found event \"does-not-exist\""}`.
   (Auth is checked before existence, so headers must still be valid.)

10. **Cancel with valid AuthChain → 200 {"ok":true}**
    DELETE the same `$EID`. Expect `200 {"ok":true}`; the row flips to a
    tombstone with `action='cancel'` (upsert on the same `(event_id,signer)` PK).

11. **Rate limit → 429** — fire >30 signed writes/min for one signer
    ```bash
    for i in $(seq 1 35); do
      curl -s -o /dev/null -w '%{http_code} ' -X POST <signed headers> \
        "http://127.0.0.1:5135/api/events/$EID/attendees"
    done; echo
    ```
    Expect: first 30 → `200`, remainder → `429` with
    `{"ok":false,"error":"rate limit exceeded"}` (per-signer limiter, 30/min).

12. **`attending` reflects the write** — `GET /api/events/attending`
    ```bash
    curl -s -w '\n%{http_code}\n' <signed headers, method GET, path /api/events/attending> \
      http://127.0.0.1:5135/api/events/attending
    ```
    Expect: `200`, `{"ok":true,"data":[...]}`; after check 8 the attended event
    appears with `"attending":true`; after the cancel in check 10 it no longer
    appears. Requires a valid AuthChain to resolve the signer.

### C. Federation read endpoints (this phase — were hardcoded `[]`)

13. **Events feed** — `GET /federation/v1/events/feed`
    ```bash
    curl -s -w '\n%{http_code}\n' \
      'http://127.0.0.1:5135/federation/v1/events/feed?since=0&limit=100'
    ```
    Expect: `200`, body `{"events":[...],"partial":<bool>}`. Each event:
    `{id, signer, signed_at:<unix-secs>, payload}`. `partial` is `true` iff the
    page filled `limit`. Reads `events_local`.

14. **Feed cursor monotonicity** — pull, take the max `signed_at`, re-pull `?since=<that>`
    ```bash
    LAST=$(curl -s 'http://127.0.0.1:5135/federation/v1/events/feed?since=0&limit=1000' \
      | jq '[.events[].signed_at] | max // 0')
    curl -s -w '\n%{http_code}\n' \
      "http://127.0.0.1:5135/federation/v1/events/feed?since=$LAST&limit=1000"
    ```
    Expect: `200`; second pull returns only deltas strictly newer than `$LAST`
    (no overlap), demonstrating the peer-pull cursor is exclusive on `since`.

15. **Per-event attendance feed** — `GET /federation/v1/events/{id}/attendance`
    ```bash
    curl -s -w '\n%{http_code}\n' \
      "http://127.0.0.1:5135/federation/v1/events/$EID/attendance?since=0&limit=500"
    ```
    Expect: `200`, body
    `{"event_id":"<id>","attendances":[...],"partial":<bool>}`. After checks 8/10
    the `attend` then `cancel` deltas for that signer are present (ordered by
    `signed_at ASC`). Reads `event_attendance_local`.

### D. Deferred routes still return 501 (assert they are intentional stubs)

16. **Create event** — `POST /api/events` → `501`
    ```bash
    curl -s -X POST -w '\n%{http_code}\n' -H 'content-type: application/json' \
      -d '{}' http://127.0.0.1:5135/api/events
    ```
    Expect: `501` (federation EventCreate write deferred; not hit by Unity).

17. **Schedules / poster / profile-settings / subscriptions / sitemap** → `501`
    ```bash
    for p in \
      "POST /api/schedules" \
      "POST /api/poster" \
      "POST /api/poster-vertical" \
      "GET /api/profiles/me/settings" \
      "GET /api/profiles/subscriptions" \
      "GET /events/sitemap.xml" ; do
        m=${p% *}; u=${p#* };
        printf '%s %s -> ' "$m" "$u";
        curl -s -o /dev/null -w '%{http_code}\n' -X "$m" "http://127.0.0.1:5135$u";
    done
    ```
    Expect: `501` for each (deferred by design — admin/federation-write, S3, or
    crawler endpoints out of the Unity-driven scope).

---

## 4. Real-client smoke step

The cleanest way to drive checks 8/10/11/12 (which need a real signed-fetch
AuthChain) is through an actual explorer that already mints the identity and
signs the request.

### Preferred: upstream Unity client (this enum belongs to unity-explorer)

1. Repoint Unity per section 1 (edit line 182 of `DecentralandUrlsSource.cs` to
   `http://127.0.0.1:5135/api/events`) in a checkout of `unity-explorer`, then
   build and launch the client.
2. Launch the client and sign in to establish a signed identity.
3. Open the map / a place info panel and toggle "interested" on an event
   (`PlaceInfoPanelController`/`EventInfoPanelController` →
   `MarkAsInterestedAsync`). This fires the signed POST to
   `/api/events/{id}/attendees`.
4. Assert: the panel shows the attend state without an `EventsApiException`
   (the client throws unless it sees `{"ok":true}`), and a fresh
   `GET /federation/v1/events/{id}/attendance?since=0` (check 15) now shows an
   `attend` delta for that wallet. Toggle off → a `cancel` delta appears and
   `GET /api/events/attending` no longer lists the event.

### Note on alternative clients

Other explorer implementations (e.g. bevy/godot-based clients) are useful general
smoke harnesses, but the events-attendance UI and `ApiEvents` URL plumbing live
in **unity-explorer**; `HttpEventsApiService` is a Unity-side service. Prefer the
Unity client for a faithful exercise of the signed write path. Use an alternative
client only as a secondary liveness check that the read endpoints serve real data
when a non-Unity client points at the same `places_events` corpus.

---

## 5. Pass criteria summary

- Read regression (checks 1–5): all `200`, `ApiOk` envelope.
- Auth gate (6,7): unsigned writes → `401`.
- Write happy path (8,10): `200 {"ok":true}` + matching `event_attendance_local`
  rows (`attend` then `cancel` tombstone, same PK).
- Existence (9): signed write to unknown id → `404`.
- Rate limit (11): 31st+ write in a minute → `429`.
- Attending reflection (12): `attending=true` after attend, gone after cancel.
- Federation reads (13–15): real `events`/`attendances` arrays, exclusive
  `since` cursor, correct `partial` flag.
- Deferred (16,17): all `501`.
- Real client (section 4): Unity toggle succeeds end-to-end with no
  `EventsApiException`, deltas visible on the federation feed.
