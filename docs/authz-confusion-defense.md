# Preventing authorization confusion -- staged defense for scoped writes

The bug class: a handler authorizes against one identifier and then writes against
another. Two shapes appear in this workspace. Shape (a), *fail-open identity* -- the
verified signer and a caller-supplied field have the same type, so `.or(body.identity)`
type-checks. Shape (b), *parent-authorized / child-unbound* -- the role check names a
parent scope, the `UPDATE` names only the child row, and the parent never reaches the
`WHERE` clause.

Four independent investigations proposed defenses. This document picks between them.
The short version: the proof is worth more than the framework. Land a cross-scope
probe in the existing contract gate first, because it fails on real code today and no
type work is required to write it. Then land one newtype in `catalyrst-crypto` and one
scope parameter in `catalyrst-social-service`. Reject the generic capability-token and
row-level-security frameworks -- both are sized for a codebase we do not have, and the
worst instance of the bug is on a code path neither of them can reach.

## The decision

| Mechanism | Verdict | Why |
|---|---|---|
| Cross-scope probe in `catalyrst-contract-gate` (fresh child id, row read back) | Adopt first | Fails on real code today; needs no type work; cannot be mocked green |
| Unwaivable 401/403 coverage in `Gate::assert_covered` | Adopt first | ~20 lines; a documented auth status that never fires is a contract lie |
| `Signer` newtype on the `catalyrst-crypto` verifier entry points | Adopt next | Would have turned `scene_adapter.rs:211` into E0308 -- that instance was since fixed by hand (`6a5c92069`), so the mechanism now buys recurrence-prevention, not a live fix; pattern already proven in-tree |
| Scope as an explicit parameter of the shared `apply_*` / port function | Adopt next | The only mechanism that reaches the federation-consumer path |
| Differentiated write outcome (`Settled` / `NoSuchRowInScope`) at scoped writes | Adopt next | The in-flight fix is silent on a fresh id; see the worked example |
| Static detector: unbound mutating SQL + auth-option fallback (`cargo xtask authz`) | Adopt as ratchet | Found two defects nobody named; ~1 day; no new toolchain |
| Generic `Authorized<Scope, Cap>` + `ScopedUpdate` builder framework | Reject as designed | Path-scoped by construction; the confluence write has no path |
| `x-write-subject` OpenAPI extension + spec lint | Defer | Its flagship rule measures zero hits until 38 free-form bodies are typed |
| Postgres `FORCE ROW LEVEL SECURITY` | Reject as primary; defer as backstop | Needs 569 implicit-transaction call sites converted; see below |
| `ParentSlot<S>` unreadable DTO field | Reject | Zero path/body shadowing hits workspace-wide; axum's typed extractors make the shape inexpressible |
| Compile-time `sqlx::query!` | Reject | `WHERE id = $1` type-checks perfectly; would have caught none of the five defects |
| `dylint` | Reject for now | `flake.nix` pins stable `1.97.1`; `rustc-private` means a second, recurring toolchain |

## The class as it stands in this tree

Five instances were identified. Three have point fixes in the working tree already
(uncommitted at the time of writing); two do not. The point fixes are not the defense --
they are the reason the defense is affordable, because they establish the shape.

| Site | State at `HEAD` | State in the working tree |
|---|---|---|
| `catalyrst-social-service/src/rest/fed/apply.rs:589` | `UPDATE community_requests ... WHERE id = $1`, no community predicate | Predicate added; a fresh id still returns 200 (below) |
| `catalyrst-social-service/src/rest/handlers/client/requests.rs:162` | Same unscoped `UPDATE`, made safe only by the scoped `SELECT` at `:60` | Unchanged -- still unscoped |
| `catalyrst-comms/src/handlers/scene_adapter.rs:210` | `try_extract_signer(..).or(body.identity.clone())` | Unchanged; a red test now exists at `tests/server_scene_adapter_auth.rs` |
| `catalyrst-places/src/handlers/report.rs:185` | No `HeaderMap`; `UPDATE place_reports_local WHERE filename = $1` | `auth_address_verified` + reporter predicate added by hand |
| `catalyrst-economy/src/handlers/transactions.rs:13` | Quota keyed on body `tx.from` | `check_data` now returns `MetaTxSender` and keys the quota |

`MetaTxSender` (`catalyrst-economy/src/ports/transaction.rs:205`) is the important one.
It is a tuple struct with a private field whose only constructor recovers the address
from the meta-transaction calldata and rejects a mismatch against `from`. That is
exactly the `Signer` newtype proposal, already written, already merged into a working
tree, by hand, for one crate. The recommendation below is to generalize a pattern the
codebase has already reached for on its own -- not to import a new one.

## Worked example: `community_requests`

### Before (`HEAD`)

`crates/catalyrst-social-service/src/rest/fed/apply.rs:567-596`:

```rust
pub async fn apply_request_status(
    pool: &PgPool,
    signed: &Signed<CommunityRequestStatusUpdate>,
    signer: &str,
) -> Result<String, ApiError> {
    let sig_hash = signature_hash_hex(&signed.hash());
    let now = now_secs();
    sqlx::query(
        "INSERT INTO community_requests_log (signature_hash, community_id, request_id, status, signer, signed_at, received_at) \
         VALUES ($1,$2,$3,$4,$5,$6,$7) ON CONFLICT (signature_hash) DO NOTHING",
    )
    .bind(&sig_hash)
    .bind(&signed.message.community_id)
    .bind(&signed.message.request_id)
    .bind(&signed.message.status)
    .bind(signer.to_ascii_lowercase())
    .bind(signed.signed_at)
    .bind(now)
    .execute(pool)
    .await?;

    if let Ok(uuid) = Uuid::parse_str(&signed.message.request_id) {
        sqlx::query("UPDATE community_requests SET status = $2, updated_at = now() WHERE id = $1")
            .bind(uuid)
            .bind(&signed.message.status)
            .execute(pool)
            .await?;
    }
    Ok(sig_hash)
}
```

The caller at `rest/handlers/writes/requests.rs:358` runs
`require_min_role(&state.pool, &signed.message.community_id, &signer, Role::Mod)` and
discards the result with `Ok(_) => {}`. A moderator of community A therefore settles any
request row in the database by id. The authorization decision and the write share no
value: `require_min_role` returns `Result<Role, ApiError>`, and `Role` carries no scope.

The same function is reached from `rest/fed/consumer.rs:399-405` off the federation
gossip queue, with the same role check against the same body field. There is no axum
route, no path parameter, and no OpenAPI operation on that path.

### The in-flight fix, and why it is not yet sufficient

The working tree narrows the `UPDATE` to `WHERE id = $1 AND community_id = $3`, binding
`community_uuid_from_hex(&signed.message.community_id)`. That is the right shape: a
forcing predicate inside the mutating statement, not an existence guard in front of it.
It holds for a freshly minted id -- the row is not touched because it does not match, not
because a prior `SELECT` found it elsewhere.

But it is silent. When `rows_affected() == 0` the code consults
`SELECT community_id FROM community_requests WHERE id = $1` and returns 404 only if the
row exists under another community. For an id that exists nowhere, it falls through,
appends to `community_requests_log` with the body-supplied `community_id`, returns
`Ok(sig_hash)` -- and the HTTP handler answers 200 and emits gossip. A caller learns
nothing, and a regression that reverts the predicate is indistinguishable from success
in every log line. This is the failure mode that made the upstream fix untestable.

There is a second, subtler problem the loudness fix must respect. `community_requests`
rows are inserted only on the node-local HTTP path
(`rest/handlers/writes/requests.rs:230`); creation is never gossiped. A remote node
applying a `CommunityRequestStatusUpdate` will therefore *legitimately* see
`rows_affected() == 0`. A blanket "zero rows is a 403" rule inside `apply_request_status`
would break federation replication. The outcome has to be returned to the caller, which
knows which path it is on.

### After

Add a scope newtype next to the authorization function it belongs to, in
`crates/catalyrst-social-service/src/rest/fed/authority.rs`:

```rust
pub struct CommunityScope(Uuid);

impl CommunityScope {
    pub fn as_uuid(&self) -> Uuid {
        self.0
    }
}

pub async fn require_min_role(
    pool: &PgPool,
    community_id: &str,
    signer: &str,
    min: Role,
) -> Result<(Role, CommunityScope), ApiError> {
    let actual = load_role(pool, community_id, signer).await?;
    if actual == Role::Banned {
        return Err(ApiError::Http(HttpError::new(
            403,
            "Forbidden: banned from this community",
        )));
    }
    if actual < min {
        return Err(ApiError::Http(HttpError::new(
            403,
            format!(
                "Forbidden: signer role {} below required {}",
                actual.as_str(),
                min.as_str()
            ),
        )));
    }
    Ok((actual, CommunityScope(community_uuid_from_hex(community_id))))
}
```

The field is private and the constructor is not `pub`, so `CommunityScope` cannot be
built from a path parameter, a body field, or a `Uuid` literal anywhere outside
`authority.rs`. The write then takes it and the outcome becomes a value:

```rust
pub enum RequestSettlement {
    Settled,
    NoSuchRequestInScope,
}

pub async fn apply_request_status(
    pool: &PgPool,
    signed: &Signed<CommunityRequestStatusUpdate>,
    signer: &str,
    scope: &CommunityScope,
) -> Result<(String, RequestSettlement), ApiError> {
    let sig_hash = signature_hash_hex(&signed.hash());
    let status = settled_request_status(&signed.message.status)?;
    let request_uuid = Uuid::parse_str(&signed.message.request_id)
        .map_err(|_| ApiError::Http(HttpError::new(400, "request_id is not a uuid")))?;

    let settled = sqlx::query(
        "UPDATE community_requests SET status = $2, updated_at = now() \
          WHERE id = $1 AND community_id = $3",
    )
    .bind(request_uuid)
    .bind(status)
    .bind(scope.as_uuid())
    .execute(pool)
    .await?;

    append_request_log(pool, &sig_hash, signed, signer, status, scope).await?;

    Ok(match settled.rows_affected() {
        1 => (sig_hash, RequestSettlement::Settled),
        _ => (sig_hash, RequestSettlement::NoSuchRequestInScope),
    })
}
```

The HTTP handler maps `NoSuchRequestInScope` to 404 and does not gossip. The federation
consumer maps it to a counted, logged no-op and returns `Ok` -- replication stays correct
and the case stops being invisible.

### Why the "before" form becomes unwriteable or loud

Four properties, in decreasing order of strength.

1. `community_uuid_from_hex(&signed.message.community_id)` no longer appears anywhere in
   `apply_request_status`. The only `Uuid` in scope is the one inside `CommunityScope`,
   and the only way to get one is a `require_min_role` that already passed. Restoring the
   original statement means either deleting the `$3` bind -- which leaves an unused
   parameter the compiler warns on and the detector in Stage 3 flags -- or reaching for a
   `Uuid` the function does not have.
2. `Ok(_) => {}` at `writes/requests.rs:358` stops compiling. The caller must bind the
   scope to pass it on, so the authorization decision cannot be discarded.
3. Both callers of the confluence function are forced through the same door. This is the
   property no path-extractor design has: `rest/fed/consumer.rs` has no path parameter to
   extract, and would have kept the bug under a `PathScope`-shaped defense.
4. `rows_affected()` is consumed by the return type, not dropped. A reverted predicate no
   longer produces a 200.

What this does **not** do: it binds the write to the *authorized* community, not the
authorized community to the *URL*. The provenance link is still the hand-written equality
check at `writes/requests.rs:352`. Make that mechanical in the same PR with a named
helper -- `scope_from_bound_envelope(path_uuid, &signed.message.community_id)` for HTTP,
`scope_from_unbound_envelope(..)` for the consumer -- so the unbound case is one grep away
instead of an absence. The probe in Stage 1 drives the URL, so deleting the check turns
it red.

## Stage 1 -- one PR, no migration, no crate touched beyond the harness

Land the proof before the mechanism. Both items go in `crates/catalyrst-contract-gate`
plus the callers' `tests/`, and neither requires a line of production code to change.

1a. The cross-scope probe. The harness supplies every assertion; the test author
supplies a request builder and a 4xx code the harness validates is a 4xx. Two cases per
scoped route, against the real router, with a real signed-fetch chain, on the scratch
Postgres the gates already stand up:

- Existing row, foreign tenant. Seed a child under tenant B, drive the route at tenant
  A's path with B's child id, assert a refusal **and** re-`SELECT` the tenancy column and
  assert it still reads B.
- Freshly minted id. Mint an id that exists nowhere, drive the route at tenant A's
  path, assert a refusal **and** `SELECT count(*)` on that id is still zero.

The second case is the one that matters. An existence guard -- "does a row with this id
already exist under a different parent?" -- matches nothing for a fresh id, stays silent,
and lets the write land. Such a defense passes case one and fails case two. That is the
upstream fix, reproduced and killed, and it is the reason this probe is Stage 1 rather
than a footnote on the type work.

Neither case can be mocked green: the refusal check and the row read-back are the
harness's, not the author's, and the read goes to the same pool the handler wrote to.
`crates/catalyrst-social-service/tests/cross_community_writes.rs` (untracked, 764 lines)
is a working template -- real EIP-712 envelopes, real migrations, real handler functions,
`ScratchSchema` teardown.

1b. Unwaivable 401/403 coverage. In `Gate::assert_covered`, before the waiver
consultation and not consulting `error_waivers`: if an operation documents 401 or 403 and
no request in the run was ever refused with 401 or 403, that is a gap. Waivers do not
apply.

This is a waiver-policy fix, not a contract mechanism, and it is the cheapest real
control in this document. The evidence is in the tree: `HEAD`'s
`crates/catalyrst-places/tests/contract_gate.rs:507` waives
`PUT /api/report/upload/{filename}` with the reason string *"handler is tolerant by
design: no auth gate and unknown filenames are accepted"* -- on an endpoint that documents
401 and updates a row by a filename built from the last eight characters of an address
plus a hex timestamp. The harness looked straight at the vulnerability and a free-text
string blessed it. Expect roughly 25-35 operations across the five gated crates to need a
one-line unauthenticated probe; each route where you cannot write one is a finding, not a
waiver candidate.

Run both against `HEAD` and treat the red output as a findings list, not test debt.

## Stage 2 -- one crate at a time, still no migration

2a. `Signer` in `catalyrst-crypto`. Change the return types of `try_extract_signer`,
`verify_signed_fetch`, and `require_signer` from `Option<String>` / `Result<EthAddress>`
to a newtype with a private field, no `Deserialize`, no `From<String>`, no `FromStr`, and
a `#[cfg(test)]` `unchecked` escape hatch enforced by a CI grep.

`crates/catalyrst-comms/src/handlers/scene_adapter.rs:210` becomes E0308 at the exact
defect line, and the fallback cannot be repaired in-crate because there is no way to build
a `Signer` from a body string. About 93 call sites across the workspace, every one of them
compiler-driven and mechanical. Do **not** touch `catalyrst_types::EthAddress` -- it is
`pub type EthAddress = String` at `entity.rs:10`, flows through `DeploymentRow` with serde
and ts-rs, and changing the alias is a workspace-wide break for zero extra safety.

Two pleasant interactions: utoipa never sees extractor types, so specs are unaffected; and
`#[derive(TS)]` on a struct containing a `Signer` fails to compile for want of a `TS` impl,
which turns "do not leak the authorized type into a wire DTO" from a convention into a
build error.

`catalyrst-economy` is already done -- treat `MetaTxSender` as the reference implementation.

2b. Scope parameters in `catalyrst-social-service`. The worked example above, plus
the twin at `rest/handlers/client/requests.rs:162`, which is still unscoped and safe today
only because the `SELECT` at `:60` happens to bind both keys. Fourteen `require_min_role`
call sites; roughly thirty scoped writes. Extend the Stage-1 probe to each route as it is
converted.

Keep the scope type crate-local. Promote it to `catalyrst-db` (which already carries the
repository idiom in `deployments_repository.rs` and `pointers_repository.rs`) only when a
second crate needs the same shape, and even then as a plain newtype -- not as a generic
`Scope<T: Tenancy>` with a table registry.

## Stage 3 -- the ratchet

A `cargo xtask authz` binary in-workspace, using `syn` for Rust and `sqlparser` for SQL.
No new toolchain, runs in the existing `build-test` job in a couple of seconds.

- Unbound mutating SQL. Build a schema model from the 89 migration files; extract every
  `UPDATE`/`DELETE` string literal from `crates/*/src`; flag any whose `WHERE` names none
  of the table's declared tenancy columns. It reads only the mutating statement's own
  `WHERE`, so an existence guard in front does not suppress it and a freshly minted id is
  irrelevant -- it never queries rows. Measured 16 flags out of 119 literals with call-graph
  reachability filtering, including both social-service defects. It runs on source text, so
  no test double can make it pass.
- Auth-option fallback. Flag `.or` / `.or_else` / `.unwrap_or*` applied to a value
  originating from one of the ten enumerable auth-source functions, resolving through one
  `let` binding. Measured exactly one hit workspace-wide with zero false positives -- the
  comms defect. A same-line grep finds zero, because rustfmt puts the `.or(` on the
  following line.
- Dynamic mutating SQL is a hard error in its own right. There are 222 `AssertSqlSafe`
  sites; if a `format!`-built `UPDATE` is allowed to pass quietly it becomes the escape
  hatch that eats the whole gate.

Declare tenancy rather than inferring it. Put the declaration in the migration so it cannot
drift, and read it back through the `ScratchDb` the gates already create:

```sql
COMMENT ON COLUMN community_requests.community_id IS 'dcl:tenancy';
```

```sql
SELECT c.relname, a.attname
  FROM pg_description d
  JOIN pg_class c ON c.oid = d.objoid
  JOIN pg_attribute a ON a.attrelid = c.oid AND a.attnum = d.objsubid
 WHERE d.description = 'dcl:tenancy';
```

Sixty-six tables carry `UPDATE`/`DELETE` and need an annotation. Seed from inference and
hand-correct; the inference was right on both real defects and wrong in an obvious,
reviewable way elsewhere.

Ship every rule with a planted-positive fixture reproducing the real code shape, plus a
variant with an existence guard in front asserting the guard does *not* suppress the flag,
and CI fails if a fixture stops being flagged. Without that, the detector can silently
degrade to matching nothing and stay green -- which is the failure this whole document
exists to avoid, relocated into the tooling.

Waivers follow the contract-gate discipline: a checked-in file with a reason and an owner,
never an inline `#[allow]`.

## Stage 4 -- conditional, not scheduled

Row-level security on a short list of tables. Justified only where a single-column
predicate suffices: `place_reports_local.reporter`, `camera_reel`,
`transactions.user_address`. Not on `community_requests`, where the policy needs an
`EXISTS` subquery against `community_members` on every write.

The measured constraints, none of which are optional:

- Services run `sqlx::migrate!` at boot, so the runtime role *owns* its tables and plain
  `ENABLE ROW LEVEL SECURITY` is a complete no-op for it. `FORCE` is mandatory, and then
  migrations need a distinct owner role.
- RLS is default-deny *per command*. A partial policy set makes the service's own `SELECT`
  return zero rows and its own `INSERT` raise `new row violates row-level security policy`.
  Every protected table needs the full read/append/write/delete set.
- `SET LOCAL app.actor = $1` is a syntax error; `SET` takes no bind parameters. The correct
  form is `SELECT set_config('app.actor', $1, true)`. Interpolating instead would put SQL
  injection inside the security control.
- `PgPoolOptions::after_connect` is the wrong hook -- it installs a *session*-level value
  that persists on the pooled connection and leaks the previous request's actor into the
  next checkout. Transaction-scoped values are correct, and they evaporate outside an
  explicit transaction, which every one of the 569 `.execute(&state.pool)` sites is.

Converting the write path to explicit transactions, not writing the policies, is the
dominant cost. That is why this is a backstop for raw SQL that escapes Stages 2-3, not a
primary control. Gate any adoption on a `pg_catalog` drift test asserting
`relrowsecurity AND relforcerowsecurity` plus a complete `pg_policy.polcmd` set for every
registered table.

Typed request bodies and the spec lint. Thirty-eight of 72 mutating operations export
`"schema": {}`. Typing them with `#[serde(deny_unknown_fields)]` is independently valuable
-- the specs currently promise nothing about request shape and sites gets no types for any
write -- but it is weeks of work, and only *after* it does a path-parameter/body-field
shadow rule have anything to detect. Do not sequence anything behind it.

## Rejected, with reasons

The generic `Authorized<Scope, Capability>` + `ScopedUpdate` framework. Its
`PathScope<S>` extractor, which is the part that closes "proved the wrong scope," is
constructible only from an axum route parameter. The single confirmed instance of the bug
is reachable from `rest/fed/consumer.rs:399`, off a gossip queue, where the scope
legitimately lives in the body and there is no path at all. The escape hatch for that case
degenerates back to a function argument -- which is Stage 2b, at a fraction of the cost.
Beyond that: all 1266 queries are runtime `sqlx::query()` with zero `query!` macros, and
`Query::bind<T: Encode>` erases the newtype at the call, so the builder must own the SQL
text to bite; 222 `AssertSqlSafe` sites already route around any such builder; and its own
authors note that a half-adopted capability scheme, with `Authorized` values minted in one
crate and carried as decoration in another, is worse than none.

`ParentSlot<S>` -- a DTO field that deserializes and discards. Zero path-parameter /
body-field shadowing hits across 136 operations, and zero is the structurally correct
answer: axum's `Path<T>` and `Json<T>` are separate typed extractors, so the merged
request object that carried the upstream bug has no Rust analogue. It defends against a
TypeScript-shaped defect in a codebase that cannot express it.

Compile-time `sqlx::query!` -- verifies that SQL parses, that bind types match, and
that result columns match the schema. It has no notion of authorization; `WHERE id = $1`
type-checks perfectly. Its one genuine benefit is forcing query text to be a literal, and
that guarantee is already defeated in 222 places.

`dylint` -- the only route to typed resolution of route registration and to the
interprocedural taint that the `INSERT` half of the class needs. `flake.nix` pins
`rust-bin.stable."1.97.1"`, which does not carry `rustc-dev`; adopting it means a second
nightly pin bumped in lockstep and a lint crate that breaks on rustc internal API churn.
Revisit only if Stage 3 has proven itself and is being maintained.

Rule "path parameter never reaches the write" -- 5 flags, 5 false positives, 100%. This
codebase uses a ports layer, so the handler passes the path binding as a named argument and
the `.bind()` lives in another file. Not shippable without the interprocedural analysis
`dylint` gates on.

## Residual risk

What still gets through after all of the above, and what covers it.

- The `INSERT` half of the class -- the exact upstream shape. Sixty-one
  handler-reachable `INSERT`s write a tenancy column and the taint rule found zero of
  interest. Zero validated positives means the rule does not work, not that the code is
  clean. `UPDATE`-shaped `WHERE`-clause analysis structurally cannot see `INSERT`s, and in
  a ports layer the value arrives at the SQL site as a named `&str` one or more frames up.
  Compensating control: Stage 1's fresh-id probe, which is `INSERT`-agnostic because it
  asserts on the row afterwards. This is the strongest argument for extending the probe to
  every create route, not just update routes.
- Authorizing on the right scope but the wrong value. `WHERE id = $1 AND
  community_id = $2` satisfies every mechanism here even when `$2` came from the body. The
  scope newtype binds the write to the authorized value; it does not bind the authorized
  value to the URL. Compensating control: the named `scope_from_bound_envelope` helper
  plus the Stage-1 probe, which drives the URL.
- Semantic authorization errors. A scope proved at `Mod` where `Owner` was intended, or
  a wrong permission matrix -- `role_has_invite_users` at `writes/requests.rs:280` admitting
  `mod` where product says owner-only -- compiles and passes every probe. Types encode
  "a role was proven", never "the correct role". No mechanical control. Review only.
- Middleware that is present but does not authenticate. *(Closed for this instance; the
  general gap stands. Verified at `fafde9633`, 2026-07-29.)* `voice_auth_layer`
  (`catalyrst-comms/src/lib.rs`) delegates to `moderator.rs::require_service_token`, which
  fails closed: an unset `COMMS_GATEKEEPER_AUTH_TOKEN` answers 503 to every bearer-gated
  route rather than serving them unauthenticated, and startup logs an error saying so.
  `tests/voice_auth_fail_closed.rs` pins it (4 tests, green). Earlier revisions of this
  document reported the opposite; that claim was wrong and was relayed onward as fact. The
  *general* gap it illustrates is real and unmechanised: a static rule cannot distinguish a
  layer that authenticates from one that is merely present, so only Stage 1b's runtime probe
  can, and only once comms is spec'd.
- Non-DB effects. The comms defect's payoff is a LiveKit JWT with `can_publish`, not a
  row. Row read-back has nothing to assert on. Same for S3 report uploads, Redis, and the
  federation gossip log. Compensating control: Stage 2a only.
- Crates with no spec. `catalyrst-comms` and `catalyrst-economy` -- two of the five bug
  sites -- carry no `utoipa` dependency and no contract gate. Everything gate-shaped is
  blind to them by construction. Compensating control: Stage 3, which is source-shaped
  and reaches all 49 crates, plus the hand-written integration tests already in the tree.
- TOCTOU. The role check and the write are separate statements throughout. A role
  revoked in between is not caught. Inlining the role predicate into the write statement
  fixes it and is not proposed here.
- Silent coverage loss in the detector. Widening one route-matching regex moved the
  write-handler count from 78 to 152 -- half the surface had been invisible with no error
  emitted. Any regex or tree-sitter rule under-covers by an amount you cannot measure from
  inside the tool. Compensating control: the planted-positive fixtures, which at least
  catch total degradation.

## Where the lens reports overstated their reach

- The type-level lens sells `Authorized`/`ScopedUpdate` as making the unscoped write
  inexpressible. It is inexpressible only for statements routed through the builder, and
  222 `AssertSqlSafe` sites plus the builder's own use of it mean the fence is a CI grep,
  not a type. Its `Signer` layer is excellent and unrelated to the rest; unbundle them.
- The database lens's L1/L2 claim is the same overstatement. Its RLS measurements, by
  contrast, are the most useful negative result in the whole set -- take them at face value.
  Its claim that a scope argument would make `put_report_upload` fail to compile is now
  moot: the handler grew a `HeaderMap` by hand.
- The contract lens's flagship shadow rule measured zero hits and will keep measuring zero
  until 38 bodies are typed. Its genuine contributions are the unwaivable 401/403 rule and
  the harness-owned fresh-id probe, neither of which is really about OpenAPI. Its own
  biggest miss is decisive here: `fed/consumer.rs:399` has no operation to annotate.
- The static-analysis lens is honest about its two dead rules. Its live findings are the
  best evidence in the set -- it found the `client/requests.rs:162` twin and the comms voice
  routes in code nobody was reading. Its telemetry finding (three write routes registered
  outside `route_layer(require_telemetry_admin)` at `catalyrst-telemetry/src/lib.rs:198`,
  `:206`, `:208`) is reported here as unverified and worth a look.

## Verified at `fafde9633` (2026-07-29)

Every claim below carries the commit it was checked at. A date alone does not say
whether the code moved underneath it -- a bullet in the previous revision, dated
2026-07-26, said `scene_adapter.rs:210` was unfixed with a red test against it; the fix
had landed the next day in `6a5c92069`, and the stale bullet was relayed onward as
verified fact more than once. Re-check before citing, stamp the sha you re-checked at,
and never cite this section without reading the file it names.

- (`fafde9633`) `HEAD` `apply.rs:589` reproduces the upstream shape exactly; the
  `AND community_id = $3` predicate remains silent on a freshly minted id, returning 200
  plus a log append plus gossip.
- (`fafde9633`) `community_requests` rows are created only at `writes/requests.rs:230`
  and creation is never gossiped, so a remote node applying a status update legitimately
  sees zero rows -- the loudness contract must differ by caller.
- (`fafde9633`) `catalyrst-comms/src/handlers/scene_adapter.rs:210` is fixed:
  `get_server_scene_adapter` derives the identity from `require_signer(&headers, ...)` and
  compares it to `authoritative_server_address`; `SceneAdapterRequest` no longer carries an
  `identity` field, so there is nothing to `.or()` a body value into.
  `tests/server_scene_adapter_auth.rs` is green (3 tests). Fixed in `6a5c92069`.
- (`fafde9633`) `rest/handlers/client/requests.rs:162` is still unfixed on `main`: the
  `UPDATE community_requests ... WHERE id = $1` carries no `community_id` predicate. A fix
  is in flight on the `authz/signer-newtype` lane (`0398709cc`), unmerged as of this stamp.
- (`fafde9633`) `MetaTxSender` (`catalyrst-economy/src/ports/transaction.rs:205`) is a
  private-field newtype minted only by calldata recovery -- the `Signer` pattern, already
  in-tree.
- (`fafde9633`) Workspace measurements: 49 crates, sqlx 0.9, 0 `query!` macros, 222
  `AssertSqlSafe` sites, 594 `.execute(` sites against 59 `rows_affected` mentions,
  39 `try_extract_signer` references, `clippy.toml` holding a single line. (Was 569 and
  41 at the 2026-07-26 stamp -- these two drift with every merge; re-count, do not cite.)
- (`fafde9633`) Five crates carry `tests/contract_gate.rs`; three are in `OPENAPI_CRATES`.
  `catalyrst-comms` and `catalyrst-economy` have no `utoipa` dependency.
- (`fafde9633`) `COMMS_GATEKEEPER_AUTH_TOKEN` is absent from the deployment's
  `catalyrst-comms` env file and commented out in its template, and `voice_auth_layer`
  fails closed there: 503 on every bearer-gated route, pinned by
  `tests/voice_auth_fail_closed.rs` (green). Voice is off in that configuration, not open.
