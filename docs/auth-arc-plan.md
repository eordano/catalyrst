# The typed-principal admin arc -- a shared `AuthenticatedAdminIdentity` extractor

Status: PLAN. No code written yet. Scope of this pass: `catalyrst-badges`,
`catalyrst-economy`, `catalyrst-credits`, `catalyrst-telemetry`. `catalyrst-market`,
`catalyrst-social-service`, and any `catalyrst-worlds` admin surface are **deferred** --
see S5.

## The defect this arc closes

Admin-ness in these four crates is a **forgettable function call** made inside the handler
body, not a fact the type system forces. Each crate hand-rolls the same three helpers --
`timing_safe_eq`, `bearer_token`, and an `authorize`/`require_admin`/`authorize_admin`
that compares the presented `Bearer` token against a per-crate `admin_token` and fails
closed when it is unset -- and then trusts a `?`-propagating call at the top of every admin
handler:

| Crate | Gate fn | Call shape | Sites |
|---|---|---|---|
| `catalyrst-badges` | `admin::authorize_admin` | `authorize_admin(&state, &headers)?` inside the handler | `handlers/badges.rs:84,115` (2) |
| `catalyrst-economy` | `handlers::admin::require_admin` | `require_admin(&state, &headers)?` | `admin.rs:85,99,126`; `payments.rs:169`; `escrow.rs:59,90`; `broker.rs:64,365`; `names.rs:83,345,518` (11, six files) |
| `catalyrst-credits` | `handlers::admin::common::authorize_admin` | `authorize_admin(&state, &headers)?` | `admin/ops.rs` (11) + `admin/catalog.rs:85,95,129,162` (4) = 15 |
| `catalyrst-telemetry` | `handlers::admin::authorize` | inline `authorize(&st, &headers)?` on `/dash/admin/*` **plus** a `route_layer` middleware on `/dash` reads/writes | `admin.rs` inline (8) + `lib.rs:171,199` middleware |

Nothing makes any of these calls mandatory. Delete the line and the handler compiles, ships,
and serves a production mutation to an unauthenticated stranger. This is exactly the shape
`catalyrst-server` already closed for its SIWE console: `AdminSession`
(`crates/catalyrst-server/src/admin/auth.rs`) is an unforgeable `FromRequestParts`
extractor with a private field, a single construction site, and a source-discipline test
(`crates/catalyrst-server/tests/source_discipline.rs`). This arc ports that model to the
bearer-token gates.

## What kind of identity these four gates actually prove

All four use a **static shared bearer secret** (`CATALYRST_{BADGES,ECONOMY,CREDITS,TELEMETRY}_ADMIN_TOKEN`),
compared in constant time, fail-closed when unconfigured. That is not a person and not a
wallet -- it is a *service credential*. In the `catalyrst-authenticated-principal`
vocabulary it is precisely
`AuthenticatedPrincipal::PlatformServiceProvenBySharedBearerToken(AuthenticatedPlatformServiceIdentity)`,
and the principal crate already ships the exact chokepoint that mints it:

```
establish_platform_service_identity_by_comparing_presented_shared_secret(
    environment_variable_that_named_this_credential: &'static str,
    configured_secret: Option<&str>,
    presented_secret: Option<&str>,
) -> Result<AuthenticatedPlatformServiceIdentity, AuthorityNotEstablished>
```
(`crates/catalyrst-authenticated-principal/src/platform_service_identity.rs:95`). Its
constant-time compare is behaviour-identical to every crate's `timing_safe_eq`, and its
refusals are already differentiated: unconfigured => 503, missing => 401, mismatch => 401.

So the shared extractor **delegates to this existing chokepoint** rather than re-deriving
the compare. It is not a new verifier; it is an axum front door onto a verifier that
already exists and is already tested.

> The SIWE + HMAC + live-allowlist mechanism (the `catalyrst-server` console, and the
> `ConfiguredWalletAllowlist` type) is **not** used by any of these four crates. It is the
> reference model, not a migration target here. Reframing the server console to emit
> `AuthenticatedPrincipal::VerifiedAdminConsoleWallet` is a separate, server-local step and
> is out of scope (server sync files are concurrent-hot; see S5).

## 1. Shape and home of the shared extractor

### Where it lives -- and why not in `catalyrst-authenticated-principal`

It cannot live in `catalyrst-authenticated-principal`. That crate's
`tests/source_discipline.rs::the_crate_has_no_io_dependencies` asserts its `Cargo.toml`
contains none of `tokio`, `reqwest`, **`axum`**, `sqlx`, `std::fs`, `std::net` -- the crate
is deliberately I/O-free vocabulary. A `FromRequestParts` impl requires `axum`. Adding the
extractor there would fail that crate's own gate on the first `cargo test`.

Recommendation: a new small crate `catalyrst-authenticated-admin`. It is the
axum-facing companion to the principal crate:

```
[dependencies]
axum = { workspace = true }
catalyrst-authenticated-principal = { workspace = true }
```

Nothing else. No `sqlx`, no per-domain `AppState`, no `ApiError`. The four admin crates
(and later market/social/worlds) depend on this one crate; it depends on neither of them.
This respects `docs/crate-boundaries.md` -- one domain per crate, no source-tree fusion.

The name uses the noun `admin` at the crate level, which the principal crate's own naming
rule (`lib.rs`) bans only for *types inside that crate*. A sibling crate may be named for
what it does. The public type keeps the task's name, `AuthenticatedAdminIdentity`; its
honest long-form meaning, stated in the doc comment, is *"this request presented the
operator-configured admin bearer secret for this service"* -- a service credential, never a
verified human.

### The type

```rust
/// Proof that this request carried the operator-configured admin bearer secret for the
/// service reached through `S`. The inner principal is deliberately private: a public
/// field would let any handler mint one from a bare value and hand it to a gate, which is
/// the forgeable `require_admin()` this type exists to replace. The only constructor is the
/// `FromRequestParts` impl below; `tests/source_discipline.rs` pins that as a fact.
pub struct AuthenticatedAdminIdentity {
    principal: AuthenticatedPrincipal, // always the PlatformServiceProvenBySharedBearerToken variant
}

impl AuthenticatedAdminIdentity {
    pub fn principal(&self) -> &AuthenticatedPrincipal { &self.principal }
    /// `service-token:CATALYRST_X_ADMIN_TOKEN` -- server-chosen, never client-supplied.
    pub fn audit_actor_description(&self) -> String { self.principal.audit_actor_description() }
}
```

No `#[derive(...)]` at all -- no `Deserialize` (a request body must not become an admin
identity), no `Clone`/`Default` (they widen how a value comes to exist). Same discipline as
`AdminSession`.

### How the extractor learns the expected secret -- `FromRef`

The token lives in each crate's `AppState` under a different path (`state.admin_token`,
`state.config.admin_token`). The extractor pulls it through axum's `FromRef`, over a small
carrier the shared crate owns:

```rust
/// The operator-configured admin secret and the env var that named it. Constructed by each
/// crate's FromRef impl; carries no verification of its own.
#[derive(Clone)]
pub struct ConfiguredAdminBearerSecret {
    pub environment_variable: &'static str,
    pub configured: Option<String>,
}

impl<S> FromRequestParts<S> for AuthenticatedAdminIdentity
where
    S: Send + Sync,
    ConfiguredAdminBearerSecret: FromRef<S>,
{
    type Rejection = AdminAuthRejection; // impl IntoResponse; see below

    async fn from_request_parts(parts: &mut Parts, state: &S) -> Result<Self, Self::Rejection> {
        let secret = ConfiguredAdminBearerSecret::from_ref(state);
        let presented = bearer_token(&parts.headers); // the shared crate parses "Bearer " itself
        let identity = establish_platform_service_identity_by_comparing_presented_shared_secret(
            secret.environment_variable,
            secret.configured.as_deref(),
            presented.as_deref(),
        )?; // AuthorityNotEstablished -> AdminAuthRejection
        Ok(AuthenticatedAdminIdentity {
            principal: AuthenticatedPrincipal::PlatformServiceProvenBySharedBearerToken(identity),
        })
    }
}
```

`FromRef` is not yet used anywhere in these crates (verified: no `FromRef` in any
`crates/*/src`), so this introduces one standard axum pattern. All four crates use
`pub type AppState = Arc<AppStateInner>`, and `FromRef` over an `Arc` state is textbook.

The principal crate deliberately refuses to parse the `Authorization` header
(`platform_service_identity.rs` doc: *"Pass the already-extracted token here"*), so the
lone piece of header parsing -- the exact `"Bearer "` prefix that 20 of 21 gates already
require -- lives in the shared crate's `bearer_token`.

### The rejection -- uniform, no per-crate `ApiError` plumbing

`AuthorityNotEstablished` already carries `http_status()` (401 / 503). The shared crate
wraps it in `AdminAuthRejection(AuthorityNotEstablished)` with an `impl IntoResponse` that
uses that status. Every adopting crate gets correct, identical status semantics for free;
none has to teach its own `ApiError` about admin auth.

> Behaviour change to flag loudly. Today all four crates return **403** for both an
> unset token and a bad/missing token. The principal-crate chokepoint returns **503** for
> unconfigured (a deployment fault, not a denial) and **401** for missing/mismatch. This is
> the principal crate's deliberate, documented semantics, and adopting it is the point -- but
> it *will* change status codes on these endpoints, and each crate's unit tests assert the
> old 403 today (e.g. `badges/src/admin.rs::unset_token_fails_closed`,
> `economy/src/handlers/admin.rs::unset_token_fails_closed`,
> `credits/.../common.rs::missing_bearer_is_forbidden`,
> `telemetry/src/handlers/admin.rs::fails_closed_when_token_unset`). Those tests move to the
> shared crate and are rewritten to the 401/503 shape. If the team wants zero status change,
> the extractor can instead map every `AuthorityNotEstablished` to 403 -- but that throws
> away the 401-vs-503 distinction the principal crate exists to draw, so the recommendation
> is to adopt 401/503 and note the change in each crate's PR.

### The audit actor -- from client-chosen string to server-verified fact

Today the audit actor is read from the **client-supplied** `x-catalyrst-admin` header
(telemetry also blends a `?actor=` query param), with per-crate fallbacks
(`"admin-token"` / `"console"` / `"loopback"`). None of it is verified. The honest
replacement is `admin.audit_actor_description()` -> `service-token:CATALYRST_X_ADMIN_TOKEN`,
built by the principal crate from a `&'static str` the server configured -- the principal
crate's `no_audit_string_can_contain_client_supplied_text` test guarantees this.

If operators still want the human label the header carried, the principal crate already
ships the correct type for it: `UnverifiedAdminDisplayName` (a claim, cannot be compared to
an allowlist). Keep it in a separate, clearly-unverified audit column; do not let it be the
actor.

### Source-discipline test in the shared crate

Clone the three `catalyrst-server` tests verbatim (renamed):

- `the_admin_identity_field_is_not_public` -- the `principal` field stays private.
- `the_admin_identity_derives_nothing` -- no `Deserialize`/`Clone`-style derive appears in
  the contiguous attribute prelude above the struct.
- `admin_identity_is_constructed_only_in_from_request_parts` -- exactly one
  `AuthenticatedAdminIdentity { ... }` literal in the crate, and it sits inside
  `from_request_parts`.

Plus `the_scan_is_reading_real_sources` (non-vacuity). These are the same asserts, with the
same wording caveat the server file already carries: *a convention with a script attached,
not a type guarantee.*

## 2. Adoption order

All four crates are outside the concurrent session's file set (market / fed /
server-sync / worlds / env-contract), so none *collides*. Within them, ascend by call-site
count and wiring irregularity so the pattern is proven before the diffuse migration:

1. `catalyrst-badges` -- first (pilot). Two admin handlers
   (`grant_user_badge`, `revoke_user_badge`) on a single route line, both inline calls in
   one file, a dedicated `src/admin.rs`. Smallest possible surface that still exercises the
   whole pattern end to end: new crate dependency, `FromRef` impl, extractor argument, the
   shared source-discipline test, and the per-crate route scan. Prove it here.

2. `catalyrst-credits` -- second. All 13 admin routes are already isolated in one
   sub-router (`handlers/admin/mod.rs`), backed by `handlers/admin/ops.rs` (11 sites) and
   `handlers/admin/catalog.rs` (4). Because the admin router is a single module, the
   per-crate "every admin route names the extractor" scan (S4) is trivially anchored.
   `admin_token` is the only inbound gate -- `economy_admin_token` is an *outbound* client
   credential (`ports/economy.rs`, `ports/escrow.rs`) and is untouched.

3. `catalyrst-telemetry` -- third. Two mechanisms coexist: 8 inline `authorize()` calls
   on `/dash/admin/*`, and a `route_layer` middleware (`require_telemetry_admin`) on the
   `/dash` gated reads/writes. Migrate the **8 inline handlers** to the extractor. Leave the
   `route_layer` middleware as a documented follow-on: those handlers don't take `HeaderMap`
   today, and a middleware layer is a router-construction gate rather than a per-handler
   compile gate, so converting them is a larger, separate change with its own review.

4. `catalyrst-economy` -- last. Most diffuse: 11 `require_admin` sites scattered across
   six handler files (`admin.rs`, `payments.rs`, `escrow.rs`, `broker.rs`, `names.rs`),
   mixed into non-admin routers. Highest chance of a missed site -- do it once the pattern is
   settled, and lean on the route scan (S4) to prove none were skipped.

Each crate ships as its own PR with its own per-crate gates (`cargo check/test -p`,
`clippy --no-deps`, `fmt`).

## 3. How each current call maps to the extractor

The mechanical transform is the same everywhere: move the check out of the body and into
the signature.

Before (badges `grant_user_badge`):
```rust
pub async fn grant_user_badge(
    State(state): State<AppState>, headers: HeaderMap, Path(...): ..., Json(body): ...,
) -> Result<Json<Value>, ApiError> {
    authorize_admin(&state, &headers)?;
    let actor = admin_actor(&headers);
    ...
}
```
After:
```rust
pub async fn grant_user_badge(
    admin: AuthenticatedAdminIdentity, State(state): State<AppState>, Path(...): ..., Json(body): ...,
) -> Result<Json<Value>, ApiError> {
    let actor = admin.audit_actor_description();
    ...
}
```

| Crate | Delete | Replace call sites with | Keep |
|---|---|---|---|
| badges | `src/admin.rs`'s `timing_safe_eq`, `bearer_token`, `check_admin`, `authorize_admin` | `admin: AuthenticatedAdminIdentity` param on `grant_user_badge`, `revoke_user_badge`; drop the two `authorize_admin`+`admin_actor` calls | `admin_actor` only if a human label is wanted -- relabel it as `UnverifiedAdminDisplayName` |
| economy | `handlers/admin.rs`'s `timing_safe_eq`, `bearer_token`, `check_bearer`, `require_admin`, `audit_actor` | param on all 11 handlers; `audit_actor(&headers)` -> `admin.audit_actor_description()` | the relayer handlers `relayer_status/toggle/signer` themselves (only their guard changes) |
| credits | `handlers/admin/common.rs`'s `timing_safe_eq`, `bearer_token`, `authorize_with_token`, `authorize_admin`, `admin_actor` | param on all 15 sites in `ops.rs` + `catalog.rs`; `admin_actor` -> verified description | every validator in `common.rs` (`normalize_address`, `validate_*`, `paginate`, ...) -- they are unrelated |
| telemetry | `handlers/admin.rs`'s `timing_safe_eq`, `bearer_token`, `token_ok`, `authorize` | param on the 8 `/dash/admin/*` handlers; the verified part of `actor_of` -> `admin.audit_actor_description()` | `actor_of` / `ActorQuery` only if a human label is wanted, relabelled as unverified; the `route_layer` middleware stays (follow-on) |

Handlers that gain the extractor argument no longer need `HeaderMap` *for auth* (they may
still take it for other reasons). The extractor is placed **first** in the argument list by
convention, so the auth obligation is the first thing a reader sees.

## 4. The compile-forcing property, and the test that pins it

### What the compiler forces

axum implements its `Handler` trait for `async fn(A1, ..., An) -> R` only when every `Ai`
implements `FromRequestParts<S>` (the last may be `FromRequest<S>`). So
`Router::route("/admin/x", post(handler))` **fails to compile** unless every argument of
`handler` is a valid extractor. `AuthenticatedAdminIdentity`'s only `FromRequestParts` impl
runs the bearer + chokepoint verification, and its field is private with a single
construction site (pinned below). Therefore:

> Any handler that **names** `AuthenticatedAdminIdentity` in its signature has the
> verification wired in by construction -- there is no `::new`, no `From`, no public field,
> no `Deserialize`, so the value cannot be conjured another way -- and `Router::route` will
> not accept the function unless that argument type resolves.

This is strictly stronger than `require_admin()`: the check is no longer a statement that
can be deleted from a body; it is a term in the type the router demands.

### The residual gap, and how it is closed

The compiler forces the check *if the argument is present*. It cannot force a **new** admin
handler to declare the argument at all -- a developer can still write an admin route whose
handler simply omits `AuthenticatedAdminIdentity`. Two mechanisms close this, mirroring how
`catalyrst-server` pins its own P2:

1. Per-crate source-discipline test (primary; zero new deps).
   `tests/admin_routes_are_gated.rs` in each adopting crate scans the admin router
   registration and asserts that **every** admin route's handler function takes
   `AuthenticatedAdminIdentity`. Anchoring differs by crate:
   - *credits* -- one module (`handlers/admin/`): assert every `pub async fn` reachable from
     `admin/mod.rs`'s `.route(...)` list names the extractor in its signature.
   - *telemetry* -- scan the eight `/dash/admin/*` registrations in `lib.rs` and assert the
     referenced handler fns in `handlers/admin.rs` name it.
   - *badges / economy* -- admin routes are interleaved with public ones, so the scan keys
     off the guard: assert that no `src/` file still defines a local
     `fn require_admin`/`authorize_admin`/`authorize`/`check_admin` (the old forgeable
     gate is gone) **and** that the handlers previously listed in S1 name the extractor.
   This is the exact species of guard the workspace already trusts:
   `catalyrst-server/tests/source_discipline.rs::admin_session_is_constructed_only_in_from_request_parts`.
   State plainly, as those tests do, that it is a convention with a script attached.

2. Trybuild reinforcement (optional; adds a dev-dep). `trybuild` is not currently in
   the workspace (verified). If adopted, add two cases in the shared crate:
   - `compile_fail`: `AuthenticatedAdminIdentity { principal: ... }` from outside the crate ->
     E0451 (private field); `AuthenticatedAdminIdentity::from(...)`/`::new(...)` -> E0599.
     This pins the *forgery* half at the type level, not by scan.
   - `pass`: a handler taking `AuthenticatedAdminIdentity` registers via `Router::route`
     against a state that provides `ConfiguredAdminBearerSecret: FromRef<S>`.

   Recommendation: ship (1) with every crate (it matches house convention and needs no new
   toolchain); add (2) once, in the shared crate, if the team wants the forgery half proven
   by the compiler rather than by a source scan.

## 5. Explicitly deferred -- do not touch until the concurrent merge clears

A concurrent session holds `~/.dcl-one-gates-fix-lock` and is mid-merge of `world-storage`
into `catalyrst-worlds`, with uncommitted deletions across `catalyrst-market/src/ports/lists.rs`,
`catalyrst-fed/src/peer.rs`, `catalyrst-server/src/sync/{snapshots,sync_orchestrator}.rs`,
and `catalyrst/deploy/env-contract.nix`. Until that clears, these admin surfaces are **out
of scope** and must not be edited:

| Deferred surface | Why held | Slots in later? |
|---|---|---|
| `catalyrst-market/src/handlers/admin.rs` | market is concurrent-hot (active deletions in `ports/lists.rs`) | Yes -- same static-bearer pattern; the principal crate docs already name market's literal `"admin-token"` actor as a target for `service-token:MARKET_ADMIN_TOKEN` |
| `catalyrst-social-service/src/rest/handlers/admin.rs` | concurrent-hot (federation/world-storage merge) | Yes -- identical shape; also named in the principal crate docs |
| any `catalyrst-worlds` admin surface | the active merge target | Assess after the merge; not yet characterised |
| `catalyrst-server` SIWE console (`AdminSession`) | the **reference model**, and `server/src/sync` is concurrent-hot | Optional, separate arc: reframe to emit `AuthenticatedPrincipal::VerifiedAdminConsoleWallet`. Not part of this pass. |

The arc is designed to absorb market and social-service with **no new design** once
unlocked -- both already use the static-bearer gate this extractor was built around, so
migrating them is the same four-step transform (S3) plus the same per-crate route scan (S4).

## Sequenced checklist

1. Land `catalyrst-authenticated-admin` (crate + `AuthenticatedAdminIdentity` +
   `ConfiguredAdminBearerSecret` + `AdminAuthRejection` + source-discipline test). No
   consumer yet. Gate: `cargo test -p catalyrst-authenticated-admin`, clippy, fmt.
2. `catalyrst-badges` -- `FromRef` impl, migrate 2 handlers, delete local gate, add route
   scan, rewrite the 403->401/503 tests.
3. `catalyrst-credits` -- migrate the `handlers/admin/` sub-router (15 sites), route scan.
4. `catalyrst-telemetry` -- migrate the 8 `/dash/admin/*` handlers; note the `route_layer`
   follow-on.
5. `catalyrst-economy` -- migrate the 11 scattered sites; route scan proves none missed.
6. (When the lock clears) market, social-service, worlds -- same transform, no redesign.
