# Preventing authorization confusion in scoped writes

An authorization check and its effect must use the same verified actor and the
same resource scope. Two failure modes matter here: accepting a caller-supplied
identity when signature verification fails, and checking a role in one community
before updating a child row belonging to another.

This document describes the current controls and the remaining work. The earlier
investigation and staged rollout are retained in Git history.

## Verified identity

[`catalyrst-crypto::Signer`](../crates/catalyrst-crypto/src/signer.rs) is implemented.
Its address field is private, its production constructor is crate-private, and
there is no general conversion from an arbitrary string. The unchecked constructor
is available only under `cfg(test)` or the `test-signer` feature.

The [signed-fetch verifier](../crates/catalyrst-crypto/src/signed_fetch/mod.rs)
returns `Signer` from `validate_signature`, `verify_signed_fetch` and related
entry points; `try_extract_signer` returns `Option<Signer>`. This prevents directly
substituting a body `String` for a verified signer. Keep the type through the
identity boundary and use a required verifier for authenticated writes. A
`Signer` proves the identity was verified, not that it has permission for a
particular resource.

The [server scene adapter](../crates/catalyrst-comms/src/handlers/scene_adapter.rs)
requires the signed-fetch identity and compares it with the configured
`authoritative_server_address`. Its request DTO has no caller-supplied `identity`
field. The [regression suite](../crates/catalyrst-comms/tests/server_scene_adapter_auth.rs)
exercises that boundary.

## Community scope

Community role and permission types stay inside `catalyrst-social-service`.
[`community_membership_authority`](../crates/catalyrst-social-service/src/rest/community_membership_authority/mod.rs)
contains membership standings and ban authorities. Client standings use
`community_members`; [federated write authority](../crates/catalyrst-social-service/src/rest/fed/authority.rs)
uses `community_role_current`. Those sources can disagree and must not be treated
as interchangeable. Store failures are represented as undetermined authority and
returned as errors, rather than converted to a permission decision.

For `community_requests`, the current paths bind scope as follows:

| Path | Current behavior |
|---|---|
| [Client request update](../crates/catalyrst-social-service/src/rest/handlers/client/requests.rs) | Authenticates the signed request path, selects the request by both request ID and community ID, checks the actor's permission, and updates with `WHERE id = $1 AND community_id = $3`. An absent scoped row is rejected by the initial read. |
| [Federated HTTP update](../crates/catalyrst-social-service/src/rest/handlers/writes/requests.rs) | Verifies the envelope, rejects mismatches between its community/request IDs and the URL, resolves moderator-or-higher authority in that community, and calls the shared apply function. |
| [Federation consumer](../crates/catalyrst-social-service/src/rest/fed/consumer.rs) | Verifies the envelope and resolves authority against its signed community ID before calling the same apply function. This path has no HTTP URL. |
| [Shared `apply_request_status`](../crates/catalyrst-social-service/src/rest/fed/apply.rs) | Restricts the status vocabulary and updates with both request ID and community ID. If no row changes and the request belongs to another community, returns 404 before appending the status log. |

The shared apply function still permits a status log entry when a request has no
local row (and currently also when its ID is not a UUID). Federation peers may
receive status updates without a locally created request. The function returns
a signature hash, without distinguishing a local row update from a log-only
application. The HTTP caller also accepts that outcome and emits gossip; it does
not currently return 404 for every fresh, nonexistent request ID.

The client update does not check `rows_affected` after its initial read. A scoped
SQL predicate prevents updating another community's row, but does not make the
read, permission check and write atomic. Existing membership authority types do
not yet force `apply_request_status` to accept a proven scope: its parameters
remain the pool, signed message and signer string.

The [cross-community regression suite](../crates/catalyrst-social-service/tests/cross_community_writes.rs)
checks request and post mutations through federation HTTP and gossip, reads back
the affected rows, and exercises valid operations in the owner's community.
This is concrete route coverage, not an automatic probe for every scoped write.

## Other implemented boundaries

- [Report uploads](../crates/catalyrst-places/src/handlers/report.rs) verify the
  signer and pass it to [report persistence](../crates/catalyrst-places/src/ports/places/component.rs).
  The update binds both filename and reporter, with an explicit missing-owned-row
  outcome. [Tests](../crates/catalyrst-places/tests/report_upload_authz.rs) cover
  unauthenticated and cross-reporter attempts.
- [`MetaTxSender`](../crates/catalyrst-economy/src/ports/transaction.rs) binds the
  claimed sender to the `userAddress` decoded from meta-transaction calldata.
  [Transaction submission](../crates/catalyrst-economy/src/handlers/transactions.rs)
  uses that value for quota reservation.
  [Tests](../crates/catalyrst-economy/tests/meta_tx_sender_binding.rs) cover the
  sender mismatch. This type does not itself verify the transaction signature.
- The comms [service-token gate](../crates/catalyrst-comms/src/moderator.rs)
  returns 503 when `COMMS_GATEKEEPER_AUTH_TOKEN` is unconfigured, and 401 when a
  presented token is missing or wrong. The
  [voice gate tests](../crates/catalyrst-comms/tests/voice_auth_fail_closed.rs)
  exercise this fail-closed behavior.

The [CI authorization-scoping job](../.github/workflows/ci.yml) runs
these regressions alongside heartbeat-presence, MLS community-binding and world
owner-escalation tests. They are integration targets and do not run under
`cargo test --workspace --lib`.

For a focused local run, enter the dev shell from `catalyrst/` and use a scratch
Postgres server as described in [Building and testing](./build-and-test.md):

```bash
export CATALYRST_TEST_PG=postgres://postgres@127.0.0.1:5432/postgres
cargo test -p catalyrst-social-service --test cross_community_writes
cargo test -p catalyrst-comms --test server_scene_adapter_auth --test voice_auth_fail_closed
cargo test -p catalyrst-places --test report_upload_authz
cargo test -p catalyrst-economy --test meta_tx_sender_binding
```

## Remaining defenses

These are proposed work, not guarantees supplied by the current harness:

1. Add reusable cross-scope probes that seed two scopes, use both a foreign child
   ID and a fresh child ID, assert the refusal, and inspect resulting rows and
   other effects. Exercise both HTTP and federation consumers, plus an authorized
   success case. Token issuance, uploads and gossip need effect-specific checks.
2. Require explicit authentication/authorization status coverage in
   [`Gate::assert_covered`](../crates/catalyrst-contract-gate/src/lib.rs).
   It currently checks route hits and broad success/error coverage with waivers;
   it does not require each documented 401 or 403 to be observed.
3. Carry proven community scope into shared mutations and give request-status
   application an explicit outcome for an updated local row versus an absent
   local row. Keep scope types crate-local and preserve the different HTTP and
   federation absence contracts deliberately. Existing membership standings are
   a starting point, not enforcement at every SQL call.
4. Add a maintained source-analysis check for identity fallbacks and unscoped
   mutations, with planted positive fixtures. No `cargo xtask authz` gate is
   currently implemented. A future detector must account for the ports layer and
   `INSERT` effects rather than relying only on handler-local `UPDATE` text.

Role correctness and concurrent revocation still need explicit review: verifying
that a role was checked does not prove it was the right role, and separate role
reads and writes can race. New tests should assert actual effects as well as
HTTP status; the presence of a middleware layer or an OpenAPI response declaration
alone does not establish authorization.
