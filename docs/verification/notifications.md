# Verification: catalyrst-notifications (service "notifications", port 5148 / social bundle 5145)

Adversarial re-check of the two flagged findings (`PUT /subscription` input
permissiveness, `GET /health` shape) plus the crate-level startup/error-model
claims, against the committed tree on `feat/service-plane-crates`.

Sources read:
- Rust: `crates/catalyrst-notifications/src/{config,lib,main,http,auth_chain}.rs`,
  `src/ports/mod.rs`, `src/handlers/{notifications,subscription,ping}.rs`,
  `migrations/0001_initial.sql`.
- Bundle: `crates/catalyrst-social/src/main.rs`.
- Upstream shape: `github.com-decentraland/schemas/src/platform/notifications/{subscription-details,notifications}.ts`.
- Unity consumers: `unity-explorer/.../Notifications/NotificationsRequestController.cs`,
  `.../Notifications/Serialization/NotificationJsonDtoConverter.cs`,
  `.../MarketplaceCredits/.../MarketplaceCreditsAPIClient.cs`,
  `EmailSubscription{Response,Error}.cs`.
- Call catalog: `the Unity net-catalog`.

## Net-catalog: which endpoints Unity actually calls

```
GET    /subscription                                          (no body)
PUT    /set-email                JSON {email,isCreditsWorkflow}
PUT    /notifications/read       JSON {notificationIds:[]}
GET    /notifications?limit=50
GET    /notifications?onlyUnread=true&from={timestamp}
POST   /subscription/opt-outs    JSON CreateCommunityNotificationOptOutPostBody
GET    /subscription/opt-outs/community/{communityId}
DELETE /subscription/opt-outs/community/{communityId}
```

`PUT /subscription` is **NOT** in the catalog — confirms it is web-only
(decentraland-dapps), not a Unity client surface. `/confirm-email` is also
not Unity-called (web confirmation flow).

## Per-endpoint table

| Endpoint | Shape | Client reaction | Severity | Failure-modes OK | Notes |
|---|---|---|---|---|---|
| `GET /subscription` | match | ok | none | yes | Unity `GetEmailSubscriptionInfoAsync` deserializes only `{email, unconfirmedEmail}` via JsonUtility; ignores `details`. Our response provides both (email nullable; unconfirmedEmail omitted when None). No-subscription path returns synthetic record with normalized `details` — tolerated. |
| `PUT /subscription` | divergent (input only) | ok | minor | yes | **CONFIRMED divergence, accepted.** Body `SubscriptionDetails` has all 3 fields `serde(default)` (ports/mod.rs:23-28) so partial/empty body is accepted (200), whereas upstream `subscription-details.ts` is strict (`required` all 3, `additionalProperties:false`, ALL 71 NotificationType keys required) and would 400. We are strictly MORE permissive on input. Stored raw; on read re-normalized via `normalize_details`. Not Unity-called (web-only, sends full valid object). Output always schema-shaped. |
| `PUT /set-email` | match | ok | none | yes | Body matches `{email,isCreditsWorkflow}`. Error body `{ok:false,message}` parsed by `EmailSubscriptionErrorResponse{error,message}` reading `.message` only, and only on HTTP 400 (MarketplaceCreditsAPIClient.cs:142-152). Non-400 -> EmptyError. Envelope tolerated. |
| `PUT /notifications/read` | match | ok | none | yes | Body `{notificationIds:[]}` matches. Response `{updated:N}` is `.WithNoOpAsync()` — discarded, never parsed. |
| `GET /notifications` | match | ok | none | yes | Returns `{notifications:[...]}`; converter reads the `notifications` array, skips items missing `type` or of unknown type. Each item has `id,type,address,timestamp,read,created_at,updated_at,metadata`. `from`/`limit`/`onlyUnread` query params handled; `from` and stored `timestamp` are both i64 ms — consistent. |
| `PUT /confirm-email` | match | n/a (web-only) | none | yes | Not Unity-called. Returns `{ok:true}` or 400. |
| `POST /subscription/opt-outs` | match | ok | none | yes | Body `{scope,scopeId}` matches; non-community scope -> 400. Returns 201 `{ok:true}`. |
| `GET /subscription/opt-outs/community/{id}` | match | ok | none | yes | Returns `{scope,scopeId,optedOut}` (camelCase matches Unity `CheckCommunityNotificationOptOutResponse`). |
| `DELETE /subscription/opt-outs/community/{id}` | match | ok | none | yes | Returns 204; Unity `.WithNoOpAsync()`. |
| `GET /health` (standalone) | n/a | ok | none | yes | main.rs:25 -> `{ok:true}` (ping handler). Infra-only, not client-consumed, no spec. |
| `GET /health` (bundle) | divergent | ok | none | yes | **CONFIRMED.** Shadowed by social bundle's own `/health` (catalyrst-social/main.rs:80-91) -> `{status:"ok"|"degraded", members:{...}}`. Different shape but infra-only. |
| `GET /ping` | n/a | n/a | none | yes | Standalone only; absent from bundle. Not client-called. |

## NotificationType parity (load-bearing for GET /subscription `details`)

The `NOTIFICATION_TYPES` const (ports/mod.rs:31-103, 71 entries) was diffed
against the `NotificationType` enum (notifications.ts:5-77, 71 members):
**IDENTICAL set** (verified with `comm`/`diff` — zero rust-only, zero
upstream-only). `normalize_details` therefore emits exactly the canonical
71-key `message_type` on every read, matching the strict upstream output
schema. (An initial "73 keys" worry was a miscount — the array is 71 entries;
a stray `community` from a separate `NotificationOptOutScope` enum had leaked
into the first diff, then ruled out.)

## Confirmed issues

1. **`PUT /subscription` input permissiveness (minor, accepted).** Real on the
   committed tree (ports/mod.rs:23-28): the three body fields are
   `serde(default)`, so we accept a partial/empty/extra-field body and respond
   200 where upstream would 400. Input-only divergence; output is always
   re-normalized to the canonical shape. Not Unity-reachable (absent from
   net-catalog) and the only real client (decentraland-dapps) always sends a
   full valid object. No client impact.

2. **`GET /health` bundle shape divergence (none).** Real but infra-only.
   Standalone `{ok:true}` vs bundle `{status,members}`. Not client-consumed,
   no spec.

Neither flagged finding is cosmetic-on-a-called-endpoint nor already-fixed;
both are accurately characterized and benign. Both `ok`/`minor`/`none`
verdicts and all per-trigger failure-mode claims are **upheld**.

## Client-crash risks

None. Verified non-null handling on each Unity path:
- `NotificationJsonDtoConverter.ReadJson` is fully null-safe: returns null on
  null token, defaults a missing `notifications` array to `EMPTY_J_ARRAY`
  (line 78), skips non-object items (83-84), skips items with null `type`
  (86-88), skips unknown types (`_ => null` + continue, 129-133). Our wrapped
  `{notifications:[]}` deserializes to an empty (non-null) list; the polling
  loop's `notifications.Count == 0` guard is therefore safe. No element field
  is dereferenced unconditionally.
- `GET /subscription` -> `EmailSubscriptionResponse` is a struct of two
  nullable strings via JsonUtility; missing/null `unconfirmedEmail` and null
  `email` are tolerated.
- `PUT /notifications/read` and `set-email` responses are discarded / parsed
  only on 400.

## Failure-mode gaps

None. Verified error paths:
- **Auth:** every endpoint calls `require_signer` first; missing/short/malformed
  chain, missing timestamp, expired (>5min), bad signature, EIP-1654 all map via
  `From<AuthChainError>` to 401 before touching the DB. Matches upstream 401.
- **DB down mid-flight:** every handler propagates `sqlx::Error` ->
  `ApiError::Database` -> 500 `{ok:false,message:"database error"}`; the real
  sqlx error is logged, not leaked (http.rs:48-51). Matches upstream 500.
- **Malformed JSON / wrong content-type / oversize:** axum `Json` extractor ->
  400/415/422 with a plain-text body (NOT our `{ok,message}` envelope), and
  `DefaultBodyLimit::max(64KiB)` (lib.rs:72) -> 413. Acceptable: clients ignore
  those bodies; only `set-email` parses an error body and only on 400.
- **Startup:** `Config::from_env()?` (single required
  `NOTIFICATIONS_PG_CONNECTION_STRING`; HTTP host/port optional, default
  127.0.0.1:5148) and `build_state().await?` (pool connect, 10s acquire timeout,
  + `sqlx::migrate!`) both use `?`. Standalone: Err -> clean anyhow exit (no
  panic; verified no unwrap/expect/panic in the crate). Bundle:
  `build_notifications()` Err caught by `mount()` (social/main.rs:67-77) ->
  warn + serve WITHOUT notifications routes; bundle `/health` reports
  `status:"degraded", members.notifications:"down"`. Graceful degradation
  confirmed.
- Migration schema (`0001_initial.sql`) provides every column the queries read
  (`read_at`, `is_credits_workflow`, `subscription_opt_outs`, etc.) — no
  startup mismatch.
