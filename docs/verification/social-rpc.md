# Verification: catalyrst-social-rpc (service "social-rpc", bundle port 5148, WS)

Upstream: `social-service-ea` (dcl-rpc friends, proto `social_service_v2`).
Our tree: `crates/catalyrst-social-rpc/` (committed).
Client: Unity `RPCFriendsService.cs` over `wss://rpc.decentraland.{ENV}` + dcl-rpc C# (`com.decentraland.rpc-csharp`).

Adversarial method: for each flagged endpoint I opened our Rust handler (`src/service.rs`),
the proto (`proto/.../social_service_v2.proto`), the upstream TS handler
(`social-service-ea/src/controllers/handlers/rpc/*.ts`), and the C# consumer + the
dcl-rpc transport (`ClientRequestDispatcher.cs`, `RpcClientModule.cs`) to confirm what a
RemoteError frame does to the client.

## Transport fact that drives every "request-throws" verdict (CONFIRMED)

`RpcClientModule.CallUnaryProcedure<T>` (lines 30-42) awaits
`SendAndWaitForResponse<Response>` and unconditionally parses the payload as the
expected `Response` message. In `ClientRequestDispatcher.SendAndWaitForResponse`
(lines 145-151), if the frame the server sent is NOT a `Response` (i.e. it is a
`RemoteError`, message type 9), the code hits `parsedMessage.message is not TResponse`
and **throws** `Exception("Invalid response type RemoteError for request ...")`.

So: any handler that returns `Err(SocialError)` (our out-of-band path) makes the
C# `CallUnaryProcedure` task **throw at the await**, before any switch on
`ResponseCase` runs. This is materially different from upstream, which `try/catch`es
every handler body and returns a valid in-band message.

Our error model (confirmed in `src/service.rs`): every `db.X().await?` in a read
handler converts `DbError -> SocialError::Internal` (`From` impl, lines 37-41) and is
returned as `Err`, serialized by dcl-rpc as a RemoteError frame. Upstream never does
this for these endpoints (verified per-handler below).

## Per-endpoint table

| Endpoint | Shape | Client reaction | Severity | Failure-modes OK? | Notes |
|---|---|---|---|---|---|
| GET /info | divergent | n/a | none | yes | catalyrst-only `{service,version,ws}`. NOT in Unity net-catalog (0 hits). Client never calls it. `main.rs:72,87`. |
| GET /health, /health/live | divergent | n/a | none | yes | catalyrst-only; `/health/live` returns "alive". NOT in net-catalog. `main.rs:73-74,98`. |
| RPC GetFriends | match | ok (happy); **request-throws on DB error** | minor | NO (DB error) | Shape matches. C# `GetFriendsAsync` reads `response.PaginationData.Total` UNGUARDED (`RPCFriendsService.cs:287`) — we always set it (service.rs:208), so no crash on happy path. DB-error path diverges: see gaps. |
| RPC GetMutualFriends | match | ok; request-throws on DB error | minor | NO (DB error) | Same as GetFriends (`cs:315`, unguarded `.Total`, we set it at service.rs:233). Missing `request.user` -> normalized to empty addr -> empty query (no error); upstream returns InvalidRequest variant. Client uses only `.Total`, no visible diff. |
| RPC GetPendingFriendshipRequests | match | request-throws on error | major | NO (DB error) | Oneof `{requests|internalServerError}` + `pagination_data`. C# `GetReceivedFriendRequestsAsync` switch THROWS on `InternalServerError`/default (`cs:404-406`); reads `PaginationData?.Total ?? 0` (null-safe, `cs:409`). We always return `Requests` (service.rs:1418). DB error -> RemoteError -> throw; upstream returns in-band `internalServerError` (client ALSO throws). Same net reaction, different wire path. |
| RPC GetSentFriendshipRequests | match | request-throws on error | major | NO (DB error) | Same as Pending (`cs:452-454`, `cs:457` null-safe). |
| RPC UpsertFriendship | match | ok | minor | NO (pre-apply DB error) | Oneof `{accepted|invalidFriendshipAction|internalServerError|invalidRequest}`. C# `UpdateFriendshipAsync` returns `Accepted` else THROWS (`cs:575-579`). We populate `friend` (service.rs:419). apply-time error -> InternalServerError in-band (MATCHES upstream, service.rs:394-400). BUT pre-apply `is_friendship_blocked?` (line 353) and `last_friendship_action?` (line 368) use `?` -> RemoteError -> throw. See gaps. |
| RPC GetFriendshipStatus | match | degraded / throws on DB error | minor | NO (DB error) | Oneof `{accepted|internalServerError|invalidRequest}`. C# switches on `Accepted.Status`, THROWS on `InternalServerError` (`cs:340-342`); `InvalidRequest` unhandled -> falls to `return NONE` (`cs:361`). Missing user -> we return InvalidRequest (matches upstream) -> client silently NONE. DB error -> RemoteError -> throw vs upstream in-band InternalServerError (client also throws): same net reaction. |
| RPC BlockUser | match | ok | minor | NO (DB error) | Oneof `{ok|internalServerError|invalidRequest|profileNotFound}`. C# `BlockUserAsync` reads `response.Ok.Profile` only inside Ok branch (`cs:217-219`), else throws. We always set `Ok.profile` w/ blocked_at (service.rs:483-486). We never emit ProfileNotFound (upstream can) — harmless. `db.block_user?` (line 450) -> RemoteError on DB error vs upstream in-band InternalServerError (client throws either way). |
| RPC UnblockUser | match | ok | minor | NO (DB error) | Oneof `{ok|...}`. C# `UnblockUserAsync` reads `Ok.Profile`, parses `BlockedAt` via `FromUnixTimeMilliseconds(profile.BlockedAt)` (`cs:614`). We pass `blocked_at=None` (service.rs:536) -> optional int64 default 0 -> epoch DateTime. NO crash; cosmetically meaningless timestamp. `db.unblock_user?` (line 507) -> RemoteError on DB error. |
| RPC GetBlockedUsers | match | ok; request-throws on DB error | minor | NO (DB error) | `{profiles:[BlockedUserProfile], pagination_data}`. C# `GetBlockedUsersAsync` reads `response.PaginationData.Total` UNGUARDED (`cs:197`) and per-profile `BlockedAt` (`cs:614`) — we set both (service.rs:559,582). Upstream get-blocked-users IGNORES pagination (returns all); we apply limit/offset. Client uses only `.Total`, no visible diff. |
| RPC GetBlockingStatus | **match (finding misdescribed shape)** | ok; request-throws on DB error | minor | NO (DB error) | Finding text claimed `{profiles, pagination_data}` with unguarded `PaginationData.Total` (major/crash). **FALSE on committed tree.** Proto `GetBlockingStatusResponse{blocked_users:[string], blocked_by_users:[string]}` (proto:283-286); our handler returns exactly that (service.rs:589-600); C# `GetUserBlockingStatusAsync` reads `response.BlockedUsers`/`BlockedByUsers` (RepeatedField, never null) (`cs:261`). No `PaginationData`, no crash. REJECT the crash claim. Real issue is the DB-error path only. |

## Confirmed issues

1. **Read handlers emit out-of-band RemoteError on DB failure where upstream returns a
   valid in-band message — the central, real divergence.** Affected: `get_friends`,
   `get_mutual_friends`, `get_pending/sent_friendship_requests`, `get_friendship_status`,
   `get_blocked_users`, `get_blocking_status` (and `get_social_settings`,
   `get_private_messages_settings`). Each uses `?` on a `Db` call (e.g. service.rs:203-204,
   228-229, 274-278, 552-553, 595, 386-387). Verified upstream behavior:
   - `get-friends.ts`, `get-mutual-friends.ts`, `get-blocked-users.ts`,
     `get-blocking-status.ts`: catch -> return **empty valid response**
     (`{friends:[],paginationData:{total:0,page:1}}` / `{profiles:[],...}` /
     `{blockedUsers:[],blockedByUsers:[]}`). Client renders an empty list, no throw.
   - `get-pending/sent-friendship-requests.ts`, `get-friendship-status.ts`: catch ->
     return **in-band `internalServerError` variant**. Client throws on that variant
     (same net reaction as our RemoteError, but via a different wire path).

2. **UpsertFriendship pre-apply reads throw out-of-band; upstream wraps the whole body.**
   `db.is_friendship_blocked(&me,&other).await?` (service.rs:353) and
   `db.last_friendship_action(&me,&other).await?` (service.rs:368) propagate `Err` ->
   RemoteError -> C# `UpdateFriendshipAsync` throws at the await. Upstream
   `upsert-friendship.ts` try/catches the entire body and returns `internalServerError`
   in-band. The apply-time error IS handled in-band by us (service.rs:394-400, matches
   upstream), so the gap is specifically the two pre-apply DB reads. (The finding named only
   `last_friendship_action`; `is_friendship_blocked` is the same class and also affected.)

3. **block_user / unblock_user DB writes throw out-of-band.** `db.block_user?`
   (service.rs:450) and `db.unblock_user?` (service.rs:507) -> RemoteError vs upstream
   in-band InternalServerError. Net client reaction is identical (C# throws on any non-Ok
   anyway), so client-visible impact is nil, but it diverges from upstream's wire contract.

## Client-crash risks

- **No null-deref crash is reachable on any endpoint on the committed tree.** The unguarded
  `response.PaginationData.Total` reads in `GetFriendsAsync` (cs:287), `GetMutualFriendsAsync`
  (cs:315), and `GetBlockedUsersAsync` (cs:197) WOULD NullReferenceException if
  `pagination_data` were omitted — but our handlers ALWAYS set `pagination_data`
  (service.rs:208, 233, 582). Latent risk noted; not currently triggerable.
- The finding's GetBlockingStatus "major / unguarded PaginationData.Total" crash risk is
  **REJECTED**: that response has no `pagination_data` field at all; the client reads only
  RepeatedFields. No crash path exists.
- `UnblockUser` `BlockedAt` parse on a defaulted `0` yields an epoch DateTime, not a crash.

## Failure-mode gaps (error paths that diverge from upstream)

- **DB-error -> transport RemoteError instead of in-band response, on every read handler
  using `?`.** For `get_friends` / `get_mutual_friends` / `get_blocked_users` /
  `get_blocking_status` this is a genuine BEHAVIORAL REGRESSION vs upstream: upstream's
  client renders an empty list (graceful degrade); ours makes the client THROW an exception
  at the await. For `get_pending/sent` and `get_friendship_status` the net reaction matches
  (client throws either way) but the wire encoding still diverges (out-of-band vs in-band
  `internalServerError`).
- **UpsertFriendship pre-apply DB reads (is_friendship_blocked, last_friendship_action)**
  throw out-of-band instead of folding into the in-band `internalServerError` variant
  upstream produces.
- **block_user / unblock_user DB-write errors** throw out-of-band vs upstream in-band
  `internalServerError` (no client-visible difference, contract difference only).
- **Cosmetic, non-crash divergences (ignore):** `page` value = `offset/limit` (0-based floor,
  service.rs:210 etc.) vs upstream `getPage` = `Math.ceil(offset/limit)+1` (1-based,
  pagination.ts) — client never reads `.Page`. get-blocked-users honors pagination for us but
  upstream returns all — client uses only `.Total`. UnblockUser `blocked_at` omitted -> epoch
  timestamp client-side.

## Rejected / corrected findings

- **GetBlockingStatus shape "divergent {profiles,pagination_data}" with major crash risk:
  REJECTED / corrected.** The committed proto + handler return
  `{blocked_users, blocked_by_users}`, exactly what the C# consumer reads. Verdict corrected
  to shape=match, severity=minor (DB-error failure-mode only). The finding's text was
  truncated and misdescribed this response as paginated.
- `/info`, `/health/live` flagged severity none — CONFIRMED correct; not in net-catalog,
  not client-called.

## Net assessment

Startup robustness as described holds (DATABASE_URL required for clean start; content DB +
gatekeeper optional). The single substantive parity problem is the **out-of-band error
encoding on read handlers**: for the four "graceful empty-list" upstream endpoints
(get_friends, get_mutual_friends, get_blocked_users, get_blocking_status) our service turns a
recoverable DB hiccup into a client-side thrown exception instead of an empty list. Most
findings' verdicts are accurate; the GetBlockingStatus shape/crash claim is the one that must
be downgraded.

(Note: this file previously held the social-rpc endpoint map; it is the unique output path
for this verification lane and has been replaced with the verification report.)
