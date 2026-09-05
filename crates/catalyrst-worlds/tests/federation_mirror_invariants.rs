//! Storage-level invariants of the read mirror.
//!
//! These are deliberately written against the *schema* rather than against
//! `fed::store`'s Rust API. The properties below are the ones that must hold no
//! matter how the poller is refactored: a method can be rewritten, a type can be
//! renamed, but a column that does not exist cannot be written and a FOREIGN KEY
//! that does not exist cannot be satisfied. Asserting them through
//! `information_schema` and through real constraint violations means the test
//! keeps its meaning after the code above it changes shape.
//!
//! The companion lanes:
//!   - `federation_peer_admission.rs` -- the admission gate, no DB, cannot skip.
//!   - `rig/scripts/worlds-fed-check.sh` -- the two-node black-box lane, which
//!     proves the same properties end to end over HTTP against a real peer.
//!
//! # Skipping
//!
//! Every test here needs Postgres and goes through `ScratchSchema::create`, which
//! routes the refusal through `catalyrst_testgate`: with no `CATALYRST_WORLDS_TEST_PG`
//! (or the workspace-wide `CATALYRST_TEST_PG`) these **fail** unless
//! `ALLOW_SKIPPED_INTEGRATION=1` is set, and when they do skip they say so on stderr
//! and append to `$CATALYRST_TESTGATE_SKIPLOG`.
//!
//! Read the skiplog, never the pass tally. A fully skipped run of this file prints
//! the same "ok. N passed" as a real one -- that is a property of libtest, not of the
//! harness, and it is why `skipped()` below logs a line naming the invariant that did
//! NOT get checked.

use catalyrst_contract_gate::pg::ScratchSchema;
use catalyrst_worlds::ports::worlds::{
    WorldsComponent, WorldsListFilters, WorldsListOptions, WorldsOrderBy,
};

const PEER: &str = "worlds.peer-operator.org";

async fn setup_db() -> Option<ScratchSchema> {
    let scratch = ScratchSchema::create("CATALYRST_WORLDS_TEST_PG", "cg_worlds_fed_mirror").await?;
    for sql in [
        include_str!("../migrations/0001_init.sql"),
        include_str!("../migrations/0002_access_log.sql"),
        include_str!("../migrations/0003_permission_parcels.sql"),
        include_str!("../migrations/0004_lower_name_indexes.sql"),
        include_str!("../migrations/0005_federation_remote_worlds.sql"),
        include_str!("../migrations/0006_federation_deadmission.sql"),
        include_str!("../migrations/0010_world_settings_version.sql"),
        include_str!("../migrations/0012_world_realm_name_override.sql"),
        include_str!("../migrations/0013_world_preview_wearables.sql"),
    ] {
        scratch.apply_sql(sql).await;
    }
    Some(scratch)
}

/// Names, on stderr, the invariant that went unchecked. `ScratchSchema::create` has
/// already recorded the missing dependency; this says what was lost by it.
fn skipped(invariant: &str) {
    eprintln!(
        "SKIPPED-INVARIANT {}: not verified on this run \u{2014} {}",
        catalyrst_testgate::current_test(),
        invariant
    );
}

macro_rules! db_or_skip {
    ($invariant:expr) => {
        match setup_db().await {
            Some(s) => s,
            None => {
                skipped($invariant);
                return;
            }
        }
    };
}

async fn seed_local_world(pool: &sqlx::PgPool, name: &str, owner: &str) {
    sqlx::query(
        "INSERT INTO worlds (name, owner, title, created_at, updated_at)
         VALUES ($1, $2, 'local', now(), now()) ON CONFLICT (name) DO NOTHING",
    )
    .bind(name)
    .bind(owner)
    .execute(pool)
    .await
    .expect("seed local world");
}

async fn insert_remote(pool: &sqlx::PgPool, peer: &str, name: &str) -> Result<(), sqlx::Error> {
    sqlx::query(
        "INSERT INTO remote_worlds (peer_id, world_name, title, deployed_scenes, observed_at)
         VALUES ($1, $2, 'from the peer', 0, now())",
    )
    .bind(peer)
    .bind(name)
    .execute(pool)
    .await
    .map(|_| ())
}

async fn count(pool: &sqlx::PgPool, table: &str) -> i64 {
    sqlx::query_scalar::<_, i64>(sqlx::AssertSqlSafe(format!("SELECT count(*) FROM {table}")))
        .fetch_one(pool)
        .await
        .unwrap_or_else(|e| panic!("count({table}): {e}"))
}

async fn columns_of(pool: &sqlx::PgPool, table: &str) -> Vec<String> {
    sqlx::query_scalar::<_, String>(
        "SELECT column_name FROM information_schema.columns
         WHERE table_name = $1 AND table_schema = current_schema()",
    )
    .bind(table)
    .fetch_all(pool)
    .await
    .expect("introspect columns")
}

// The structural claim: a peer's ownership assertion has nowhere to land

/// A peer's `/worlds` carries `owner` on every entry -- this crate emits it itself
/// at `handlers/worlds_list.rs`. The mirror does not drop that field by remembering
/// to; it drops it because there is no column, no struct field, and no bind
/// parameter that could accept it.
#[tokio::test]
async fn remote_worlds_has_no_column_that_could_hold_a_peer_ownership_claim() {
    let scratch = db_or_skip!("remote_worlds may have gained an owner/access/permissions column");
    let cols = columns_of(&scratch.pool, "remote_worlds").await;
    assert!(
        !cols.is_empty(),
        "remote_worlds does not exist; migration 0005 did not apply"
    );
    for forbidden in [
        "owner",
        "access",
        "permissions",
        "blocked_since",
        "deployer",
        "deployment_auth_chain",
        "single_player",
    ] {
        assert!(
            !cols.iter().any(|c| c == forbidden),
            "remote_worlds grew a `{forbidden}` column. A peer asserts this in-band on \
             every poll; the table exists in this shape so the assertion has nowhere \
             to land. Columns: {cols:?}"
        );
    }
    scratch.drop().await;
}

/// The hostile payload, driven at the storage layer: every key a lying peer would
/// add is rejected by Postgres itself, not by a filter someone has to maintain.
#[tokio::test]
async fn every_forbidden_key_from_a_hostile_peer_payload_is_rejected_by_the_database() {
    let scratch = db_or_skip!("a forbidden peer-supplied column may now be insertable");
    for (col, val) in [
        ("owner", "'0xdeadbeef'"),
        ("access", "'{}'"),
        ("blocked_since", "now()"),
        ("deployer", "'0xdeadbeef'"),
    ] {
        let sql = format!(
            "INSERT INTO remote_worlds (peer_id, world_name, {col}) VALUES ('{PEER}', 'x.dcl.eth', {val})"
        );
        let err = sqlx::query(sqlx::AssertSqlSafe(sql))
            .execute(&scratch.pool)
            .await
            .expect_err(&format!(
                "INSERT naming `{col}` succeeded against remote_worlds \u{2014} a peer-supplied \
                 {col} now has somewhere to land"
            ));
        let msg = err.to_string();
        assert!(
            msg.contains(col) || msg.contains("column"),
            "expected an undefined-column error for `{col}`, got: {msg}"
        );
    }
    scratch.drop().await;
}

/// `world_permissions` and `world_permission_parcels` attach by FK to `worlds(name)`.
/// A remote-only name has no `worlds` row, so no ACL row can be created against it --
/// enforced by the schema, not by a check in a handler.
#[tokio::test]
async fn an_acl_row_cannot_attach_to_a_remote_only_world() {
    let scratch = db_or_skip!("world_permissions may now be attachable to a mirrored world");
    insert_remote(&scratch.pool, PEER, "remote-only.dcl.eth")
        .await
        .expect("insert remote world");

    let err = sqlx::query(
        "INSERT INTO world_permissions (world_name, permission_type, address)
         VALUES ('remote-only.dcl.eth', 'deployment', '0x1111111111111111111111111111111111111111')",
    )
    .execute(&scratch.pool)
    .await
    .expect_err(
        "an ACL row was created against a world that exists only as a peer's claim \u{2014} \
         world_permissions lost its FK to worlds(name)",
    );
    assert!(
        err.to_string().contains("foreign key") || err.to_string().contains("violates"),
        "expected a foreign-key violation, got: {err}"
    );
    scratch.drop().await;
}

#[tokio::test]
async fn remote_worlds_has_no_foreign_key_into_the_authoritative_tables() {
    let scratch = db_or_skip!("remote_worlds may have gained an FK to worlds(name)");
    let fks = sqlx::query_scalar::<_, String>(
        "SELECT tc.constraint_name FROM information_schema.table_constraints tc
         WHERE tc.table_name = 'remote_worlds'
           AND tc.table_schema = current_schema()
           AND tc.constraint_type = 'FOREIGN KEY'",
    )
    .fetch_all(&scratch.pool)
    .await
    .expect("introspect fks");
    assert!(
        fks.is_empty(),
        "remote_worlds gained a FOREIGN KEY ({fks:?}). Coupling the mirror to worlds(name) \
         makes a peer's listing able to constrain, cascade into, or block writes on the \
         authoritative table."
    );
    scratch.drop().await;
}

// The load-bearing invariant: the mirror never touches the authoritative tables

/// `resolve_world_owner` returns `stored_owner` **first** and only consults squid ENS
/// when it is NULL, so any write that populated `worlds.owner` would become the
/// permanent authority over the chain. The rule is therefore not "don't copy the
/// owner field" but "never touch that table" -- asserted here as a count.
#[tokio::test]
async fn mirroring_a_peer_writes_zero_rows_to_worlds_and_world_scenes() {
    let scratch = db_or_skip!("the mirror path may now write the authoritative worlds tables");
    seed_local_world(
        &scratch.pool,
        "local.dcl.eth",
        "0x1111111111111111111111111111111111111111",
    )
    .await;
    let worlds_before = count(&scratch.pool, "worlds").await;
    let scenes_before = count(&scratch.pool, "world_scenes").await;

    for n in ["a.dcl.eth", "b.dcl.eth", "local.dcl.eth"] {
        insert_remote(&scratch.pool, PEER, n)
            .await
            .expect("mirror insert");
    }

    assert_eq!(
        count(&scratch.pool, "worlds").await,
        worlds_before,
        "mirroring changed the row count of `worlds`"
    );
    assert_eq!(
        count(&scratch.pool, "world_scenes").await,
        scenes_before,
        "mirroring changed the row count of `world_scenes`"
    );
    assert_eq!(count(&scratch.pool, "remote_worlds").await, 3);
    scratch.drop().await;
}

/// The collision case at the storage layer: the same ENS name held locally and
/// claimed by a peer are two independent rows in two independent tables. Neither
/// upserts over the other, and the local row's owner is untouched.
#[tokio::test]
async fn a_colliding_name_is_two_independent_rows_and_the_local_owner_survives() {
    let scratch = db_or_skip!("a peer's claim on a local name may now overwrite the local row");
    let local_owner = "0xcccccccccccccccccccccccccccccccccccccccc";
    seed_local_world(&scratch.pool, "collide.dcl.eth", local_owner).await;
    insert_remote(&scratch.pool, PEER, "collide.dcl.eth")
        .await
        .expect("peer claims the same name");

    let owner: Option<String> =
        sqlx::query_scalar("SELECT owner FROM worlds WHERE lower(name) = 'collide.dcl.eth'")
            .fetch_one(&scratch.pool)
            .await
            .expect("read local owner");
    assert_eq!(
        owner.as_deref(),
        Some(local_owner),
        "a peer's claim on a colliding name changed the local worlds.owner"
    );
    assert_eq!(count(&scratch.pool, "worlds").await, 1);
    assert_eq!(count(&scratch.pool, "remote_worlds").await, 1);
    scratch.drop().await;
}

/// The mirror is invisible to the endpoint the ecosystem actually reads. If a
/// mirrored row ever reached `list_worlds_public`, it would list under our origin
/// with `owner = NULL`, which is the `worlds-mirror` hazard exactly.
#[tokio::test]
async fn mirrored_rows_are_invisible_to_list_worlds_public() {
    let scratch = db_or_skip!("mirrored rows may now be listed by /worlds");
    for n in ["one.dcl.eth", "two.dcl.eth"] {
        insert_remote(&scratch.pool, PEER, n)
            .await
            .expect("mirror insert");
    }
    let wc = WorldsComponent::new(scratch.pool.clone());
    let (worlds, total) = wc
        .list_worlds_public(
            &WorldsListFilters::default(),
            &WorldsListOptions {
                limit: 50,
                offset: 0,
                order_by: WorldsOrderBy::Name,
                order_direction: catalyrst_worlds::ports::worlds::OrderDirection::Asc,
            },
        )
        .await
        .expect("list_worlds_public");
    assert_eq!(total, 0, "a mirrored world was counted by /worlds");
    assert!(
        worlds.is_empty(),
        "/worlds returned mirrored entries: {:?}",
        worlds.iter().map(|w| &w.name).collect::<Vec<_>>()
    );
    scratch.drop().await;
}

// Namespacing and the local operator veto

/// `remote_worlds` is keyed `(peer_id, world_name)`. Two peers claiming the same name
/// are two rows; one peer cannot displace another's claim.
#[tokio::test]
async fn two_peers_claiming_one_name_do_not_displace_each_other() {
    let scratch = db_or_skip!("one peer's listing may now overwrite another peer's rows");
    insert_remote(&scratch.pool, "worlds.peer-a.org", "shared.dcl.eth")
        .await
        .expect("peer a");
    insert_remote(&scratch.pool, "worlds.peer-b.org", "shared.dcl.eth")
        .await
        .expect("peer b");
    assert_eq!(count(&scratch.pool, "remote_worlds").await, 2);

    let dup = insert_remote(&scratch.pool, "worlds.peer-a.org", "shared.dcl.eth").await;
    assert!(
        dup.is_err(),
        "the (peer_id, world_name) primary key no longer rejects a duplicate claim"
    );
    scratch.drop().await;
}

/// The mixed-case landmine that `worlds` still carries is closed at the schema level
/// for the new table: a peer cannot mint a case-duplicate row that `lower()` reads
/// resolve non-deterministically.
#[tokio::test]
async fn a_case_variant_peer_name_violates_the_lowercase_check_constraint() {
    let scratch = db_or_skip!("remote_worlds may now accept mixed-case names from a peer");
    let err = insert_remote(&scratch.pool, PEER, "Collide.DCL.eth")
        .await
        .expect_err("a mixed-case world_name was accepted into remote_worlds");
    assert!(
        err.to_string().contains("remote_worlds_name_lowercase")
            || err.to_string().contains("check"),
        "expected the lowercase CHECK to fire, got: {err}"
    );

    let err = insert_remote(&scratch.pool, "Worlds.Peer-Operator.ORG", "x.dcl.eth")
        .await
        .expect_err("a mixed-case peer_id was accepted into remote_worlds");
    assert!(
        err.to_string().contains("remote_worlds_peer_id_lowercase")
            || err.to_string().contains("check"),
        "expected the peer_id lowercase CHECK to fire, got: {err}"
    );
    scratch.drop().await;
}

/// `hidden_since` is ours. The poller never writes that column, so re-listing a world
/// cannot un-hide it -- the operator's veto is not reversible by the peer it vetoes.
#[tokio::test]
async fn a_peer_relisting_a_world_cannot_clear_the_local_operator_veto() {
    let scratch = db_or_skip!("a peer may now clear hidden_since by re-listing");
    insert_remote(&scratch.pool, PEER, "vetoed.dcl.eth")
        .await
        .expect("initial mirror");
    sqlx::query(
        "UPDATE remote_worlds SET hidden_since = now() WHERE world_name = 'vetoed.dcl.eth'",
    )
    .execute(&scratch.pool)
    .await
    .expect("operator veto");

    // The poll's replace step, as specified: it must not delete or overwrite a
    // vetoed row's hidden_since.
    let mut tx = scratch.pool.begin().await.expect("begin");
    sqlx::query("DELETE FROM remote_worlds WHERE peer_id = $1 AND hidden_since IS NULL")
        .bind(PEER)
        .execute(&mut *tx)
        .await
        .expect("delete un-vetoed");
    sqlx::query(
        "INSERT INTO remote_worlds (peer_id, world_name, title, observed_at)
         VALUES ($1, 'vetoed.dcl.eth', 're-listed by the peer', now())
         ON CONFLICT (peer_id, world_name)
         DO UPDATE SET title = EXCLUDED.title, observed_at = EXCLUDED.observed_at",
    )
    .bind(PEER)
    .execute(&mut *tx)
    .await
    .expect("upsert");
    tx.commit().await.expect("commit");

    let hidden: Option<chrono::DateTime<chrono::Utc>> = sqlx::query_scalar(
        "SELECT hidden_since FROM remote_worlds WHERE world_name = 'vetoed.dcl.eth'",
    )
    .fetch_one(&scratch.pool)
    .await
    .expect("read hidden_since");
    assert!(
        hidden.is_some(),
        "a peer re-listing a world cleared the local operator's veto"
    );
    scratch.drop().await;
}

/// A poll that fails partway must leave the previous view intact. An unreachable or
/// erroring peer omits; it never empties. A half-replaced peer view is the one
/// outcome that is worse than a stale one.
#[tokio::test]
async fn a_poll_that_fails_partway_leaves_the_previous_rows_intact() {
    let scratch = db_or_skip!("a failed poll may now leave a partially-replaced peer view");
    for n in ["keep-one.dcl.eth", "keep-two.dcl.eth"] {
        insert_remote(&scratch.pool, PEER, n).await.expect("seed");
    }
    let before = count(&scratch.pool, "remote_worlds").await;

    let mut tx = scratch.pool.begin().await.expect("begin");
    sqlx::query("DELETE FROM remote_worlds WHERE peer_id = $1 AND hidden_since IS NULL")
        .bind(PEER)
        .execute(&mut *tx)
        .await
        .expect("delete");
    // The peer's next entry is malformed and the transaction must not commit.
    let boom = sqlx::query(
        "INSERT INTO remote_worlds (peer_id, world_name, deployed_scenes)
         VALUES ($1, 'Bad-Case.dcl.eth', 0)",
    )
    .bind(PEER)
    .execute(&mut *tx)
    .await;
    assert!(boom.is_err(), "the malformed entry was accepted");
    tx.rollback().await.expect("rollback");

    assert_eq!(
        count(&scratch.pool, "remote_worlds").await,
        before,
        "a failed poll destroyed the previous view instead of leaving it stale"
    );
    scratch.drop().await;
}

/// An unreachable peer is reported as stale, not as empty. `last_success_at` lagging
/// `last_attempt_at` is the wire-visible difference between "this peer holds no
/// worlds" and "we could not reach this peer".
#[tokio::test]
async fn an_unreachable_peer_is_recorded_as_stale_rather_than_as_holding_nothing() {
    let scratch = db_or_skip!("an unreachable peer may now be indistinguishable from an empty one");
    insert_remote(&scratch.pool, PEER, "still-here.dcl.eth")
        .await
        .expect("seed");
    sqlx::query(
        "INSERT INTO remote_peer_status (peer_id, last_attempt_at, last_success_at, worlds_observed)
         VALUES ($1, now() - interval '1 hour', now() - interval '1 hour', 1)",
    )
    .bind(PEER)
    .execute(&scratch.pool)
    .await
    .expect("seed status");

    sqlx::query(
        "UPDATE remote_peer_status SET last_attempt_at = now(), last_error = 'connection refused'
         WHERE peer_id = $1",
    )
    .bind(PEER)
    .execute(&scratch.pool)
    .await
    .expect("failed attempt");

    let (attempt, success, err): (
        Option<chrono::DateTime<chrono::Utc>>,
        Option<chrono::DateTime<chrono::Utc>>,
        Option<String>,
    ) = sqlx::query_as(
        "SELECT last_attempt_at, last_success_at, last_error FROM remote_peer_status WHERE peer_id = $1",
    )
    .bind(PEER)
    .fetch_one(&scratch.pool)
    .await
    .expect("read status");

    assert!(
        attempt > success,
        "a failed poll did not advance last_attempt_at past last_success_at, so staleness \
         is invisible on the wire"
    );
    assert_eq!(err.as_deref(), Some("connection refused"));
    assert_eq!(
        count(&scratch.pool, "remote_worlds").await,
        1,
        "an unreachable peer's last-good rows were dropped; the mirror emptied instead of \
         going stale"
    );
    scratch.drop().await;
}
