# Parity report — catalyrst-notifications (service "notifications")

Crate: `crates/catalyrst-notifications` (handlers in `src/handlers/`, ports/DB in `src/ports/mod.rs`).
Upstream notifications-server is **not** mirrored on this machine (the archived `notificator`
repo is a *different*, status-only service — only `status-handler.ts` + `error-handler.ts`).
Parity therefore checked against the **clients** that talk to `notifications.decentraland.org`:

- `@dcl/schemas` Subscription / SubscriptionDetails (`github.com-decentraland/schemas/src/platform/notifications/`)
- `decentraland-dapps` `NotificationsAPI` (`src/modules/notifications.ts`)
- `account` dapp subscription saga + reducer + NotificationGroupCard (`github.com-decentraland/account`)
- `hooks` `createNotificationsClient.ts` + `useNotifications.type.ts`
- `decentraland-ui` Notifications feed/types
- Unity net-catalog (`the Unity net-catalog`)

## Per-endpoint table

| Endpoint | Shape | Efficiency | Severity | Notes |
|---|---|---|---|---|
| `GET /ping` | match | same | none | Local liveness `{ok:true}`; no upstream counterpart; not in `api_router`. |
| `GET /notifications` | divergent (benign) | same | minor | Wrapper `{notifications:[...]}` matches. Items omit `created_at`/`updated_at` that upstream type declares — but no client reads them (see below). `onlyUnread`/`from`/`limit` query params match Unity catalog + dapps. |
| `PUT /notifications/read` | match | same | none | `{notificationIds:[...]}` in; both clients ignore the response body, so our extra `{updated:n}` is invisible. |
| `GET /subscription` | divergent (benign) | same | minor | `email` omitted-when-None vs upstream `null` — **proven** identical to the client. Default-synthesized `message_type:{}` fails strict `@dcl/schemas` validate(), but no client validates on read. |
| `PUT /subscription` | divergent (benign) | same | minor | Input MORE permissive (all 3 fields `serde(default)`); happy path matches. Response email divergence not observed (saga ignores response body). |
| `PUT /set-email` | divergent | same | **major** | Body `{email,isCreditsWorkflow}` matches Unity + dapps. **Email-confirmation flow cannot complete**: mailer stubbed AND `PUT /confirm-email` unimplemented. Impacts the web `account` dapp, not Unity. |
| `POST /subscription/opt-outs` | unknown | same | minor | Our body `{scope,scopeId}`; Unity DTO `CreateCommunityNotificationOptOutPostBody` field names unresolvable from catalog; no server source mirrored. Response `{ok:true}` unverified. |
| `GET /subscription/opt-outs/community/{communityId}` | unknown | same | minor | Response key `{opted_out:bool}` is our convention; real upstream key unverified (could be `optedOut` / bare bool). |
| `DELETE /subscription/opt-outs/community/{communityId}` | match | same | none | 204 no-body; Unity sends empty-body DELETE; nothing to diverge on. |

## Confirmed shape issues

### 1. `PUT /set-email` — email-confirmation flow is non-functional (major)
- Our `set_email` (`ports/mod.rs:179-212`) upserts `unconfirmed_email` + a fresh `email_confirmation_token`
  (`Uuid::new_v4()`) but dispatches **no** email (no SMTP/SQS) — handler comment at `subscription.rs:75`
  says "Stubbed confirmation mailer".
- `PUT /confirm-email` is **not routed** (`lib.rs:48-71` has no such route). The `decentraland-dapps`
  client exposes it (`notifications.ts:88-94 postEmailConfirmationCode -> PUT /confirm-email`,
  body `{address,code,turnstileToken?,source?}`) and the `account` saga drives it
  (`account/src/modules/subscription/sagas.ts:75,87,99`).
- Net effect: the user never receives the code (no mailer) and could not submit it anyway (no route).
  Verified the account flow `putEmail` -> `postEmailConfirmationCode`; the second leg 404s.
- **Scope:** web `account` dapp only. The Unity net-catalog has **no** `/confirm-email` entry — Unity
  sends `/set-email` but does not drive confirmation, so Unity is unaffected by the missing route.

### 2. `GET /notifications` — items omit `created_at`/`updated_at` (minor, benign)
- Upstream `RawDecentralandNotification` (`ui/src/components/Notifications/types.ts:10-19`) declares
  both as required `string`. Our `NotificationItem` (`ports/mod.rs:10-19`) and the SELECT
  (`ports/mod.rs:74-83`) omit them.
- **Why benign (verified):** the `hooks` client type only requires `{id,type,read}` + an index
  signature (`useNotifications.type.ts:8-13`), so it tolerates absence. The dapps/`decentraland-ui`
  feed renders only `notification.timestamp` (`NotificationsFeed.tsx:163`); the only references to
  `created_at`/`updated_at` outside `types.ts` are in `Notifications.stories.tsx` fixtures, never in
  a rendering path. No client reads these fields.

### 3. `GET /subscription` (and PUT responses) — `email` omitted vs `null`, and empty `message_type` (minor, benign)
- `email` uses `skip_serializing_if=Option::is_none` (`ports/mod.rs:49`), so absent rather than `null`.
  Upstream schema marks `email` nullable + not-required (`schemas/.../subscription.ts:23,26`).
- **Why benign (verified):** account reducer guards with `if (email)` (`reducer.ts:103`) — `undefined`
  and `null` are both falsy, identical branch. `unconfirmedEmail` is assigned straight through and is
  optional in the dapps type (`notifications.ts:53-54`), so omission == upstream `undefined`.
- Default `message_type:{}` (`SubscriptionDetails::default`, `ports/mod.rs:35-43`; emitted by the
  no-row path at `subscription.rs:29`) **fails** a strict `@dcl/schemas` `Subscription.validate()`
  (message_type requires every `NotificationType` key, `additionalProperties:false`,
  `subscription-details.ts:42,52-54`). No client runs the validator on read, so the UI tolerates it.
  A theoretically harder consequence — `NotificationGroupCard.tsx:109`
  `subscriptionDetails.messageType[toCamel(type)].email` would throw on a missing key — is **gated by
  `hasEmail`** (`NotificationGroupCard.tsx:106`); the empty-message_type path only occurs when no
  subscription row exists, which implies no email, so `hasEmail` is false and the indexing is skipped.
  Not reachable through the normal default path.

## Confirmed efficiency wins

None. Every endpoint is a single SQL statement on both sides (single SELECT / single UPDATE /
single upsert `ON CONFLICT ... RETURNING` / single `EXISTS` / single DELETE). No N+1 upstream, no
caching on our side, no redis on either read path. All `efficiency_verdict: same` stand. The
`PUT /set-email` case does strictly less work (skips the mailer) but that is a **missing feature**,
not a structural win — correctly not claimed as "better".

## Rejected / downgraded during verification

- **"Missing created_at/updated_at affects the explorer."** Rejected as client-affecting. The fields
  are declared in the upstream TS type but never read at runtime by any client (hooks tolerates via
  index signature; UI feed sorts on `timestamp`; only storybook fixtures reference them). Real literal
  divergence, zero observable impact — kept at minor/benign, not elevated.
- **"email omitted-vs-null is an observable shape break."** Rejected as observable. `reducer.ts:103`
  `if (email)` collapses `null` and `undefined` to the same path; `unconfirmedEmail` optional. No
  difference reaches state. Literal divergence only.
- **"Empty message_type will crash the settings UI."** Rejected as reachable. The crashing index
  (`NotificationGroupCard.tsx:109`) is behind `hasEmail`, which is false on exactly the no-row path
  that yields empty message_type. Cannot trigger via normal flow.
- **"PUT /set-email response email divergence breaks the client."** Rejected as observed. The dapps
  `putEmail` and the account saga `handlePutSubscriptionEmailRequest` (`sagas.ts:63-70`) ignore the
  response body entirely and use `action.payload.email` for the success action.
- **"`notificator` is the upstream to diff against."** Rejected. `notificator` is a status-only
  service; the real notifications-server is not mirrored. The mailer/`/confirm-email` gap is therefore
  inferred from the **clients** (dapps `postEmailConfirmationCode`, account saga) that require it — not
  from mirrored server source.
- **Opt-out endpoints stay "unknown," not "match."** The Unity DTO
  `CreateCommunityNotificationOptOutPostBody` field names and the GET opt-out response key are not
  resolvable from the catalog and no server source is mirrored; cannot confirm `scope/scopeId` or
  `{opted_out}` against the real wire shape.
