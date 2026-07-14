//! A soft-deleted community grants no standing on the **client** path.
//!
//! Community deletion never removes `community_members` rows -- both
//! `rest::handlers::client::communities::delete` and `rest::fed::apply::apply_delete`
//! only `UPDATE communities SET active = FALSE`. These tests pin that
//! `load_standing_from_community_members` refuses to read a role out of those orphaned
//! rows, and that it still refuses *by failing closed* rather than by swallowing an error
//! (the BC-1 policy the loader's doc comment describes).
//!
//! The RPC-side twin of this file is `tests/community_voice_soft_delete.rs`.

use std::time::Duration;

use catalyrst_authenticated_principal::AuthorityNotEstablished;
use catalyrst_contract_gate::pg::ScratchDb;
use catalyrst_social_service::rest::community_membership_authority::{
    load_standing_from_community_members, CommunityMembershipTier,
};
use catalyrst_social_service::rest::handlers::permissions::Permission;
use sqlx::postgres::PgPoolOptions;
use sqlx::PgPool;
use uuid::Uuid;

const OWNER: &str = "0x00000000000000000000000000000000d31e7ed0";
const MEMBER: &str = "0x00000000000000000000000000000000d31e7ed1";

// A scratch database, not a scratch schema: `sqlx::migrate!` takes a per-database advisory
// lock, so concurrent tests sharing one database deadlock against each other.
async fn setup() -> Option<ScratchDb> {
    let scratch =
        ScratchDb::create("CATALYRST_SOCIAL_SERVICE_TEST_PG", "cg_social_softdel").await?;
    sqlx::migrate!("./migrations")
        .run(&scratch.pool)
        .await
        .expect("migration run");
    Some(scratch)
}

async fn seed_community(pool: &PgPool, active: bool) -> Uuid {
    let id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO communities (id, name, description, owner_address, private, active, unlisted) \
         VALUES ($1, $2, $3, $4, FALSE, $5, FALSE)",
    )
    .bind(id)
    .bind("Soft Delete Community")
    .bind("description")
    .bind(OWNER)
    .bind(active)
    .execute(pool)
    .await
    .expect("seed community");
    id
}

async fn seed_member(pool: &PgPool, community: Uuid, address: &str, role: &str) {
    sqlx::query(
        "INSERT INTO community_members (community_id, member_address, role) VALUES ($1, $2, $3)",
    )
    .bind(community)
    .bind(address)
    .bind(role)
    .execute(pool)
    .await
    .expect("seed member");
}

/// The control: nothing about the guard changes a live community's answer.
#[tokio::test]
async fn an_active_community_still_reports_the_stored_role() {
    let Some(scratch) = setup().await else {
        return;
    };
    let community = seed_community(&scratch.pool, true).await;
    seed_member(&scratch.pool, community, OWNER, "owner").await;
    seed_member(&scratch.pool, community, MEMBER, "member").await;

    let owner = load_standing_from_community_members(&scratch.pool, community, OWNER)
        .await
        .expect("owner standing");
    assert_eq!(owner.tier(), CommunityMembershipTier::OwnerOfThisCommunity);
    assert!(owner.a_membership_row_exists_for_this_wallet());
    assert_eq!(
        owner.stored_role_text_defaulting_to_none_when_no_row_exists(),
        "owner"
    );

    let member = load_standing_from_community_members(&scratch.pool, community, MEMBER)
        .await
        .expect("member standing");
    assert_eq!(
        member.tier(),
        CommunityMembershipTier::OrdinaryMemberOfThisCommunity
    );

    scratch.drop().await;
}

/// The fix: the member rows survive the soft delete, the standing does not.
#[tokio::test]
async fn a_soft_deleted_community_grants_its_former_owner_no_standing() {
    let Some(scratch) = setup().await else {
        return;
    };
    let community = seed_community(&scratch.pool, true).await;
    seed_member(&scratch.pool, community, OWNER, "owner").await;

    sqlx::query("UPDATE communities SET active = FALSE WHERE id = $1")
        .bind(community)
        .execute(&scratch.pool)
        .await
        .expect("soft delete");

    let rows_still_present: i64 =
        sqlx::query_scalar("SELECT count(*) FROM community_members WHERE community_id = $1")
            .bind(community)
            .fetch_one(&scratch.pool)
            .await
            .expect("count member rows");
    assert_eq!(
        rows_still_present, 1,
        "the soft delete must leave the membership row in place -- otherwise this test \
         would pass for the wrong reason"
    );

    let standing = load_standing_from_community_members(&scratch.pool, community, OWNER)
        .await
        .expect("standing lookup must succeed, and answer 'not a member'");
    assert_eq!(
        standing.tier(),
        CommunityMembershipTier::NotAMemberOfThisCommunity,
        "a former owner of a deleted community must hold no tier"
    );
    assert!(
        !standing.a_membership_row_exists_for_this_wallet(),
        "the loader must report no row, so presence checks fail closed too"
    );
    assert!(!standing.counts_as_a_member_of_this_community());
    assert!(!standing.holds_capability_within_this_community(Permission::BanPlayers));
    assert_eq!(
        standing.stored_role_text_defaulting_to_none_when_no_row_exists(),
        "none"
    );

    scratch.drop().await;
}

/// The join must filter on *this* community's `active`, not on any community's.
#[tokio::test]
async fn a_deleted_community_does_not_suppress_a_live_one_for_the_same_wallet() {
    let Some(scratch) = setup().await else {
        return;
    };
    let deleted = seed_community(&scratch.pool, false).await;
    let live = seed_community(&scratch.pool, true).await;
    seed_member(&scratch.pool, deleted, OWNER, "owner").await;
    seed_member(&scratch.pool, live, OWNER, "moderator").await;

    let dead = load_standing_from_community_members(&scratch.pool, deleted, OWNER)
        .await
        .expect("deleted standing");
    assert_eq!(
        dead.tier(),
        CommunityMembershipTier::NotAMemberOfThisCommunity
    );

    let alive = load_standing_from_community_members(&scratch.pool, live, OWNER)
        .await
        .expect("live standing");
    assert_eq!(
        alive.tier(),
        CommunityMembershipTier::ModeratorOfThisCommunity,
        "the guard must not leak across communities"
    );

    scratch.drop().await;
}

/// BC-1 regression: the added join must not have turned a query fault into "not a member".
///
/// Needs no database -- an unreachable pool is the fault.
#[tokio::test]
async fn a_query_fault_is_still_undetermined_and_never_a_silent_not_a_member() {
    let pool = PgPoolOptions::new()
        .acquire_timeout(Duration::from_millis(250))
        .connect_lazy("postgres://postgres@127.0.0.1:1/unreachable")
        .expect("lazy pool");

    let refusal = load_standing_from_community_members(&pool, Uuid::new_v4(), OWNER)
        .await
        .expect_err("an unreachable store must not resolve to a standing");
    match refusal {
        AuthorityNotEstablished::UndeterminedStoreUnavailable { store, .. } => {
            assert_eq!(store, "community_members");
        }
        other => panic!("a store fault must render as Undetermined, got {other:?}"),
    }
}
