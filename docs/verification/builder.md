# Verification: catalyrst-builder (service "builder", create bundle 5144)

Adversarial re-check of the submitted re-check finding against the **committed tree**
(`crates/catalyrst-builder`, branch `feat/service-plane-crates`), upstream
`decentraland/builder-server`, and the Unity consumers in `unity-explorer`.
Net-catalog (`the Unity net-catalog`) used to confirm which
endpoints the client actually calls.

## Endpoints actually called by the Unity client (net-catalog)

```
POST  https://builder-api.decentraland.{ENV}/v1/newsletter   JSON:{email,source:'auth'}  LobbyForNewAccountAuthState.cs:388
OTHER https://builder-api.decentraland.{ENV}/v1/collections/[COL-ID]/items                DynamicWorldContainer.cs:337
OTHER https://builder-api.decentraland.{ENV}/v1/storage/contents/                         DynamicWorldContainer.cs:338
```

`BuilderApiDtos` (collections/items) is consumed by the avatar-loading systems
(`LoadTrimmedElementsByIntentionSystem`) **only behind the dev/QA app arg
`SELF_PREVIEW_BUILDER_COLLECTIONS`** (`ApplicationParametersWearablesProvider.cs:64`,
`ApplicationParamsEmoteProvider.cs:67-70`, both pass `needsBuilderAPISigning:true`). No
normal end-user session hits collections/items. `BuilderApiContent` is the content-bucket
base used by `BuildElementDTO`. `/v1/storage/contents/{hash}` is a 301 redirect surface;
never parsed as JSON. `/v1/storage/contents/{hash}/exists` and `/ping` are not in the
net-catalog at all (builder-web / liveness only).

## Per-endpoint table

| endpoint | shape | client-reaction | severity | failure-modes-ok | notes |
|---|---|---|---|---|---|
| POST /v1/newsletter | divergent: bare `{"ok":true}`, no `data` key (`newsletter.rs:56`) | ok — fire-and-forget | none | yes | `SubscribeToNewsletterAsync` (`LobbyForNewAccountAuthState.cs:381-391`) POSTs hand-built `{"email":..,"source":"auth"}` with `.WithNoOpAsync()` inside try/catch; NO response DTO. **Correction to the finding:** upstream wire body is ALSO `{"ok":true}` — `handleRequest` wraps `subscribe`'s `undefined` as `data:undefined` and `JSON.stringify` drops it. Not even a real wire divergence. |
| GET /v1/collections/{id}/items | match | ok | none | mostly (see gaps) | `ApiData::ok(data)` => `{ok:true,data:...}`. Client path sends no page/limit => our handler returns the bare-array branch `data:[...]` (`collections.rs:79-83`), exactly what C# `BuilderLambdaResponse{ok;data:List}` expects (`CollectionElements => data`, `WearableDTO.cs:61-68`). Matches upstream `getCollectionItems` `page&&limit ? paginated : items` (`Item.router.ts:425`). |
| GET/HEAD /v1/storage/contents/{hash} | match | n/a — 301 redirect | none | yes | Always 301 to `{bucket}/contents/{hash}` (+ optional `?ts=`), immutable cache header (`storage.rs:25-34`). Matches upstream `permanentlyRedirectFile` (`S3Router.ts:89-95`). Never JSON. |
| GET /v1/storage/contents/{hash}/exists | match | n/a — not client-called | none | yes | HEADs bucket; 200 on success else 404 (`storage.rs:37-53`). Matches upstream `handleExists`. |
| GET /ping | n/a | not client-called | none | yes | Mounted in `main.rs:25` only, NOT in `api_router()` — absent from the create bundle. Liveness only. |

## Confirmed issues

1. **Newsletter response omits `data` key — TOLERATED, and effectively a non-issue.**
   Confirmed on the committed tree: `handlers/newsletter.rs:56` returns
   `Json(json!({ "ok": true }))` (bare `serde_json::Value`, not `ApiData`). The submitted
   finding's rationale that "upstream emits a `data` key, we omit it" is **inaccurate**:
   upstream `Newsletter.subscribe` returns `undefined`/`null`; `server.handleRequest` wraps
   it as `{ok:true, data:undefined}`, and `JSON.stringify` drops `undefined`, so upstream's
   actual body is `{"ok":true}` — identical to ours. The Unity client never reads the body
   (`.WithNoOpAsync()` + try/catch). Severity: none. Verdict: ACCEPTED.

Real behavioral divergences worth recording (all harmless, not defects):

2. **Newsletter: missing `email` => 422 here vs 200 upstream.**
   `Json<NewsletterBody>` requires `email: String` (`newsletter.rs:11`); a body without
   `email` is rejected by the axum `Json` extractor with a 422 plain-text body (NOT the
   `{ok:false}` envelope) before the handler runs. Upstream `subscribe(undefined)` forwards
   `email:undefined` to beehiiv inside a try/catch and always resolves to `{"ok":true}`.
   Divergent status and body on this path. Harmless: client discards the response.
   (Empty email after trim: our handler skips the local write `:23` but still forwards to
   SaaS with `""`, both best-effort.)

3. **Newsletter adds a local archive write absent upstream.**
   `NewsletterComponent::subscribe` (`ports/items.rs:291-302`) does
   `INSERT ... ON CONFLICT (email) DO UPDATE` into `newsletter_subscriptions`. Upstream
   `Newsletter.model.ts` has NO local persistence — it only forwards to beehiiv. Additive
   divergence; write failure is logged-and-ignored (`newsletter.rs:24-26`), still 200. The
   beehiiv forward body (`reactivate_existing:true`, `send_welcome_email:false`, `utm_source`,
   `utm_medium:"organic"`) matches upstream `Newsletter.model.ts:22-28` exactly, including the
   `/publications/{id}/subscriptions` path and bearer auth. Neither side checks the SaaS HTTP
   status (we only catch `send()` Err `:51`; upstream returns `response.json()`), so a non-2xx
   beehiiv response is silently OK on both sides.

4. **collections/items access denial: 403 Forbidden here vs 401 Unauthorized upstream.**
   `collections.rs:44-48` returns `ApiError::forbidden` (403) for non-owner/non-admin.
   Upstream throws `HTTPError('Unauthorized', ... unauthorized)` => 401 (`Item.router.ts:411`).
   Also our owner check runs before fetching items; upstream fetches then checks — same
   observable outcome. Not client-crashing (dev-only preview consumer).

## Client-crash risks

None. Verified each consumer:
- **Newsletter**: no response DTO; fire-and-forget; cannot crash.
- **collections/items**: `LoadBuilderItem` guards `CollectionElements is { Count: > 0 }`
  (`LoadTrimmedElementsByIntentionSystem.cs:182`), so empty/`data:[]` is safe. `BuildElementDTO`
  dereferences `contents.Count` (`WearableDTO.cs:81`), but our `to_full_item` ALWAYS emits
  `contents` as a JSON object (`ports/items.rs:98-102`), never null/absent — no NPE. `type`
  is always present (`:87`).
- **storage**: 301/404 only; never deserialized.

## Failure-mode gaps

- **Newsletter missing-`email` => 422 vs upstream 200** (#2): diverges in status and body
  shape (axum default 422 text, not `{ok:false}`); client never reads it. Acceptable.
- **collections/items DB outage => flat 500 `{ok:false,error:"database error"}`**: all sqlx
  errors flow through `From<sqlx::Error>` (`http/errors.rs:23-24,58-61`) to a 500 with the
  message flattened to the static `"database error"` (real error logged via tracing). The
  not-found path IS replicated (`collection_owner -> None -> ApiError::not_found` =>
  404, matching upstream `NonExistentCollectionError` => 404 `Item.router.ts:425`). Other
  service errors that upstream might surface as specific 4xx collapse to 500 here. Acceptable
  for a 500-class fault.
- **collections/items 403-vs-401** (#4): status divergence on the auth-denial path.
- **No panic in any handler.** Startup-only panics: missing `BUILDER_PG_CONNECTION_STRING`
  (`config.rs:26` `required`), unreachable builder DB, or failed migration
  (`lib.rs:32-44` `.context().await?`) — process exits non-zero. No degraded DB mode, which is
  acceptable since the builder DB is this crate's reason to exist (owns
  collections/items/item_contents/newsletter_subscriptions). Optional config degrades
  gracefully: `BUILDER_CONTENT_BUCKET_URL` defaults to `https://builder-items.decentraland.org`
  (`config.rs:27-28`), `admin_addresses` defaults empty (owner-only), `NEWSLETTER_*` are
  `Option`+filtered-empty (`config.rs:30-38`) so absent => SaaS forward silently skipped (local
  archive still attempted).
- **AuthChain error mapping** confirmed coherent (`auth_chain.rs:27-41`, `errors.rs:45-49`):
  `AuthChainError` => `ApiError::Unauthorized` (401) carrying the human string
  ("Invalid Auth Chain" / "Missing timestamp" / "Expired signature" / "Invalid signature" /
  "EIP-1654 not implemented"). All `ApiError` variants serialize to `{"ok":false,"error":..}`
  with the documented status codes.

## Verdict

ACCEPTED with one correction. The single flagged item (newsletter divergence, severity none,
client_reaction ok, all failure-modes ok) is confirmed real on the committed tree and correctly
tolerated. Correction: the "upstream emits a data key, we omit it" rationale is wrong — upstream's
serialized body is also `{"ok":true}`, so the divergence is even more benign than stated. No
client-crash risk anywhere in the crate. The finding under-reported three additional harmless
divergences (#2 missing-email status, #3 local archive write, #4 403-vs-401) now recorded above.
