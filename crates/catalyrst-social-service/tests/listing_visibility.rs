use std::time::Duration;

use rand::Rng;
use sqlx::postgres::PgPoolOptions;
use sqlx::PgPool;
use uuid::Uuid;

use catalyrst_social_service::rest::http::Pagination;
use catalyrst_social_service::rest::ports::communities::CommunitiesComponent;

fn pg_url() -> Option<String> {
    std::env::var("CATALYRST_SOCIAL_SERVICE_TEST_PG").ok()
}

fn unique_schema() -> String {
    let mut b = [0u8; 8];
    rand::rng().fill_bytes(&mut b);
    format!("test_listing_{}", hex::encode(b))
}

async fn setup_db() -> Option<(PgPool, String, String)> {
    let url = pg_url()?;
    let admin = PgPoolOptions::new()
        .max_connections(2)
        .acquire_timeout(Duration::from_secs(5))
        .connect(&url)
        .await
        .ok()?;
    let schema = unique_schema();
    sqlx::query(sqlx::AssertSqlSafe(format!("CREATE SCHEMA {}", schema)))
        .execute(&admin)
        .await
        .ok()?;
    let suffixed = format!("{}?options=-c%20search_path%3D{}", url, schema);
    let pool = PgPoolOptions::new()
        .max_connections(4)
        .acquire_timeout(Duration::from_secs(5))
        .connect(&suffixed)
        .await
        .ok()?;

    apply_migration(&pool, include_str!("../migrations/0001_initial.sql")).await;
    apply_migration(&pool, include_str!("../migrations/0002_federation.sql")).await;
    apply_migration(
        &pool,
        include_str!("../migrations/0003_voice_moderators.sql"),
    )
    .await;
    apply_migration(&pool, include_str!("../migrations/0004_thumbnail_hash.sql")).await;
    apply_migration(&pool, include_str!("../migrations/0005_suspension.sql")).await;
    apply_migration(
        &pool,
        include_str!("../migrations/0006_role_check_reconcile.sql"),
    )
    .await;

    Some((pool, schema, url))
}

async fn apply_migration(pool: &PgPool, sql: &str) {
    let cleaned = strip_block_comments(sql);
    let mut buf = String::new();
    let mut in_func = false;
    for line in cleaned.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        buf.push_str(line);
        buf.push('\n');
        if trimmed.contains("$$ LANGUAGE plpgsql;") {
            in_func = false;
            sqlx::query(sqlx::AssertSqlSafe(buf.as_str()))
                .execute(pool)
                .await
                .unwrap_or_else(|_| panic!("{}", buf.clone()));
            buf.clear();
            continue;
        }
        if trimmed.contains("CREATE OR REPLACE FUNCTION") || trimmed.contains("CREATE FUNCTION") {
            in_func = true;
        }
        if !in_func && trimmed.ends_with(';') {
            sqlx::query(sqlx::AssertSqlSafe(buf.as_str()))
                .execute(pool)
                .await
                .unwrap_or_else(|_| panic!("{}", buf.clone()));
            buf.clear();
        }
    }
    if !buf.trim().is_empty() {
        sqlx::query(sqlx::AssertSqlSafe(buf.as_str()))
            .execute(pool)
            .await
            .expect("trailing sql");
    }
}

fn strip_block_comments(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for line in s.lines() {
        let trimmed = line.trim_start();
        if trimmed.starts_with("--") {
            continue;
        }
        out.push_str(line);
        out.push('\n');
    }
    out
}

async fn cleanup(admin_url: &str, schema: &str) {
    if let Ok(admin) = PgPoolOptions::new()
        .max_connections(1)
        .connect(admin_url)
        .await
    {
        let _ = sqlx::query(sqlx::AssertSqlSafe(format!(
            "DROP SCHEMA {} CASCADE",
            schema
        )))
        .execute(&admin)
        .await;
    }
}

fn rand_uuid() -> Uuid {
    let mut b = [0u8; 16];
    rand::rng().fill_bytes(&mut b);
    Uuid::from_bytes(b)
}

async fn seed_community(
    pool: &PgPool,
    id: Uuid,
    name: &str,
    owner: &str,
    private: bool,
    unlisted: bool,
) {
    sqlx::query(
        "INSERT INTO communities (id, name, description, owner_address, private, active, unlisted) \
         VALUES ($1, $2, $3, $4, $5, TRUE, $6)",
    )
    .bind(id)
    .bind(name)
    .bind("description")
    .bind(owner)
    .bind(private)
    .bind(unlisted)
    .execute(pool)
    .await
    .expect("seed community");
}

async fn seed_member(pool: &PgPool, id: Uuid, addr: &str, role: &str) {
    sqlx::query(
        "INSERT INTO community_members (community_id, member_address, role) VALUES ($1, $2, $3)",
    )
    .bind(id)
    .bind(addr)
    .bind(role)
    .execute(pool)
    .await
    .expect("seed member");
}

const OWNER: &str = "0xowner00000000000000000000000000000000000";
const MEMBER: &str = "0xmember0000000000000000000000000000000000";
const STRANGER: &str = "0xstranger00000000000000000000000000000000";

struct Seeded {
    listed_public: Uuid,
    listed_private: Uuid,
    unlisted_public: Uuid,
    unlisted_private: Uuid,
}

async fn seed_world(pool: &PgPool) -> Seeded {
    let listed_public = rand_uuid();
    let listed_private = rand_uuid();
    let unlisted_public = rand_uuid();
    let unlisted_private = rand_uuid();

    seed_community(pool, listed_public, "Cool Public", OWNER, false, false).await;
    seed_community(pool, listed_private, "Cool Private", OWNER, true, false).await;
    seed_community(pool, unlisted_public, "Hidden Public", OWNER, false, true).await;
    seed_community(pool, unlisted_private, "Hidden Private", OWNER, true, true).await;

    seed_member(pool, unlisted_public, MEMBER, "member").await;
    seed_member(pool, unlisted_private, MEMBER, "member").await;

    seed_member(pool, listed_public, OWNER, "owner").await;
    seed_member(
        pool,
        listed_public,
        "0xextra000000000000000000000000000000000",
        "member",
    )
    .await;

    Seeded {
        listed_public,
        listed_private,
        unlisted_public,
        unlisted_private,
    }
}

fn page(limit: i64, offset: i64) -> Pagination {
    Pagination { limit, offset }
}

fn result_ids(results: &[serde_json::Value]) -> Vec<String> {
    results
        .iter()
        .filter_map(|o| o.get("id").and_then(|v| v.as_str()).map(str::to_string))
        .collect()
}

#[tokio::test]
async fn default_authenticated_listing_hides_members_unlisted_communities() {
    let Some((pool, schema, admin_url)) = setup_db().await else {
        eprintln!(
            "skipping default_authenticated_listing_hides_members_unlisted_communities: \
             CATALYRST_SOCIAL_SERVICE_TEST_PG unset or Postgres unreachable"
        );
        return;
    };
    let seeded = seed_world(&pool).await;
    let comp = CommunitiesComponent::new(pool.clone());

    let (results, total) = comp
        .list(&page(50, 0), None, Some(MEMBER), false, false, &[])
        .await
        .expect("list");
    let ids = result_ids(&results);

    assert!(
        ids.contains(&seeded.listed_public.to_string()),
        "listed public community should appear in default listing: {ids:?}"
    );
    assert!(
        ids.contains(&seeded.listed_private.to_string()),
        "listed private community should appear in default listing: {ids:?}"
    );
    assert!(
        !ids.contains(&seeded.unlisted_public.to_string()),
        "member's UNLISTED public community must NOT leak into the default listing: {ids:?}"
    );
    assert!(
        !ids.contains(&seeded.unlisted_private.to_string()),
        "member's UNLISTED private community must NOT leak into the default listing: {ids:?}"
    );
    assert_eq!(total, 2, "only the two listed communities are counted");

    cleanup(&admin_url, &schema).await;
}

#[tokio::test]
async fn only_member_of_listing_still_includes_unlisted_member_communities() {
    let Some((pool, schema, admin_url)) = setup_db().await else {
        eprintln!(
            "skipping only_member_of_listing_still_includes_unlisted_member_communities: \
             CATALYRST_SOCIAL_SERVICE_TEST_PG unset or Postgres unreachable"
        );
        return;
    };
    let seeded = seed_world(&pool).await;
    let comp = CommunitiesComponent::new(pool.clone());

    let (results, total) = comp
        .list(&page(50, 0), None, Some(MEMBER), true, false, &[])
        .await
        .expect("list");
    let ids = result_ids(&results);

    assert!(
        ids.contains(&seeded.unlisted_public.to_string()),
        "onlyMemberOf must include the member's unlisted communities: {ids:?}"
    );
    assert!(
        ids.contains(&seeded.unlisted_private.to_string()),
        "onlyMemberOf must include the member's unlisted communities: {ids:?}"
    );
    assert_eq!(
        total, 2,
        "member belongs to exactly the two unlisted communities"
    );

    cleanup(&admin_url, &schema).await;
}

#[tokio::test]
async fn minimal_search_returns_light_shape_and_member_count() {
    let Some((pool, schema, admin_url)) = setup_db().await else {
        eprintln!(
            "skipping minimal_search_returns_light_shape_and_member_count: \
             CATALYRST_SOCIAL_SERVICE_TEST_PG unset or Postgres unreachable"
        );
        return;
    };
    let seeded = seed_world(&pool).await;
    let comp = CommunitiesComponent::new(pool.clone());

    let (results, total) = comp
        .search_communities("Cool", STRANGER, 50, 0)
        .await
        .expect("search");
    assert_eq!(total, 2, "two 'Cool *' communities match");
    let ids: Vec<String> = results.iter().map(|r| r.id.to_string()).collect();
    assert!(ids.contains(&seeded.listed_public.to_string()));
    assert!(ids.contains(&seeded.listed_private.to_string()));

    let public = results
        .iter()
        .find(|r| r.id == seeded.listed_public)
        .expect("listed public in results");
    assert_eq!(public.privacy, "public");
    assert_eq!(public.name, "Cool Public");
    assert_eq!(
        public.members_count, 2,
        "listed_public seeded with 2 members"
    );

    let private = results
        .iter()
        .find(|r| r.id == seeded.listed_private)
        .expect("listed private in results");
    assert_eq!(private.privacy, "private");

    let json = serde_json::to_value(public).expect("serialize search result");
    let obj = json.as_object().expect("object");
    let mut keys: Vec<&str> = obj.keys().map(|s| s.as_str()).collect();
    keys.sort_unstable();
    assert_eq!(
        keys,
        vec!["id", "membersCount", "name", "privacy"],
        "minimal result is exactly id/name/membersCount/privacy"
    );

    cleanup(&admin_url, &schema).await;
}

#[tokio::test]
async fn minimal_search_word_boundary_and_unlisted_visibility() {
    let Some((pool, schema, admin_url)) = setup_db().await else {
        eprintln!(
            "skipping minimal_search_word_boundary_and_unlisted_visibility: \
             CATALYRST_SOCIAL_SERVICE_TEST_PG unset or Postgres unreachable"
        );
        return;
    };
    let seeded = seed_world(&pool).await;
    let comp = CommunitiesComponent::new(pool.clone());

    let (results, total) = comp
        .search_communities("Public", STRANGER, 50, 0)
        .await
        .expect("search");
    let ids: Vec<String> = results.iter().map(|r| r.id.to_string()).collect();
    assert_eq!(
        total, 1,
        "only the listed 'Cool Public' matches for a stranger"
    );
    assert_eq!(ids, vec![seeded.listed_public.to_string()]);

    let (stranger_hidden, stranger_total) = comp
        .search_communities("Hidden", STRANGER, 50, 0)
        .await
        .expect("search");
    assert_eq!(
        stranger_total, 0,
        "stranger cannot search unlisted communities"
    );
    assert!(stranger_hidden.is_empty());

    let (member_hidden, member_total) = comp
        .search_communities("Hidden", MEMBER, 50, 0)
        .await
        .expect("search");
    assert_eq!(
        member_total, 2,
        "member can search their own unlisted communities"
    );
    let member_ids: Vec<String> = member_hidden.iter().map(|r| r.id.to_string()).collect();
    assert!(member_ids.contains(&seeded.unlisted_public.to_string()));
    assert!(member_ids.contains(&seeded.unlisted_private.to_string()));

    cleanup(&admin_url, &schema).await;
}

#[tokio::test]
async fn minimal_search_empty_query_orders_by_name_and_paginates() {
    let Some((pool, schema, admin_url)) = setup_db().await else {
        eprintln!(
            "skipping minimal_search_empty_query_orders_by_name_and_paginates: \
             CATALYRST_SOCIAL_SERVICE_TEST_PG unset or Postgres unreachable"
        );
        return;
    };
    let _seeded = seed_world(&pool).await;
    let comp = CommunitiesComponent::new(pool.clone());

    let (page1, total) = comp
        .search_communities("", MEMBER, 2, 0)
        .await
        .expect("search page 1");
    assert_eq!(total, 4, "member sees all four (incl. their unlisted)");
    let names1: Vec<&str> = page1.iter().map(|r| r.name.as_str()).collect();
    assert_eq!(names1, vec!["Cool Private", "Cool Public"]);

    let (page2, total2) = comp
        .search_communities("", MEMBER, 2, 2)
        .await
        .expect("search page 2");
    assert_eq!(total2, 4);
    let names2: Vec<&str> = page2.iter().map(|r| r.name.as_str()).collect();
    assert_eq!(names2, vec!["Hidden Private", "Hidden Public"]);

    let (stranger, stranger_total) = comp
        .search_communities("", STRANGER, 50, 0)
        .await
        .expect("search stranger");
    assert_eq!(stranger_total, 2);
    let stranger_names: Vec<&str> = stranger.iter().map(|r| r.name.as_str()).collect();
    assert_eq!(stranger_names, vec!["Cool Private", "Cool Public"]);

    cleanup(&admin_url, &schema).await;
}
