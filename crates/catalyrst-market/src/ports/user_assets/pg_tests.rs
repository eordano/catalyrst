use catalyrst_contract_gate::pg::ScratchDb;
use sqlx::PgPool;

use super::component::UserAssetsComponent;
use super::sql::{
    emotes_count_sql, emotes_unique_sql, grouped_emotes_count_sql, grouped_wearables_count_sql,
    wearables_count_sql, wearables_unique_sql,
};
use super::types::UserAssetsFilters;

const OWNER: &str = "0x00000000000000000000000000000000000000aa";

async fn scalar(pool: &PgPool, sql: String) -> i64 {
    sqlx::query_scalar(sqlx::AssertSqlSafe(sql))
        .bind(OWNER)
        .fetch_one(pool)
        .await
        .unwrap()
}

async fn seed(pool: &PgPool) {
    for ddl in [
        "CREATE TABLE squid_marketplace.nft (id text, contract_address text, token_id numeric, \
            network text, created_at int8, updated_at int8, urn text, owner_address text, \
            image text, item_id text, item_type text, metadata_id text, transferred_at int8, \
            category text, ens_id text, active_order_id text)",
        "CREATE TABLE squid_marketplace.metadata (id text, wearable_id text, item_type text)",
        "CREATE TABLE squid_marketplace.wearable (id text, category text, rarity text, \
            name text, description text)",
        "CREATE TABLE squid_marketplace.emote (id text, category text, rarity text, \
            name text, description text)",
        "CREATE TABLE squid_marketplace.item (id text, price numeric)",
        "CREATE TABLE squid_marketplace.ens (id text, subdomain text)",
        "CREATE TABLE squid_marketplace.order (id text, price numeric)",
        "INSERT INTO squid_marketplace.metadata VALUES ('m1', 'w1', 'wearable_v2'), \
            ('m2', 'w2', 'wearable_v2')",
        "INSERT INTO squid_marketplace.wearable VALUES \
            ('w1', 'hat', 'rare', 'Red Hat', 'd'), ('w2', 'eyes', 'epic', 'Blue Eyes', 'd')",
        "INSERT INTO squid_marketplace.emote VALUES \
            ('item-e1', 'dance', 'rare', 'Wave', 'd'), ('item-e2', 'fun', 'common', 'Clap', 'd')",
        "INSERT INTO squid_marketplace.ens VALUES ('ens-1', 'alice'), ('ens-2', 'bob')",
        "INSERT INTO squid_marketplace.nft (id, contract_address, token_id, network, created_at, \
            updated_at, urn, owner_address, item_id, item_type, metadata_id, transferred_at, \
            category, ens_id) VALUES \
            ('n1', '0xc', 1, 'MATIC', 300, 300, 'urn:w:1', $1, 'item-1', 'wearable_v2', 'm1', 300, 'wearable', NULL), \
            ('n2', '0xc', 2, 'MATIC', 200, 200, 'urn:w:1', $1, 'item-1', 'wearable_v2', 'm1', 200, 'wearable', NULL), \
            ('n3', '0xc', 3, 'MATIC', 100, 100, 'urn:w:2', $1, 'item-2', 'wearable_v2', 'm2', 100, 'wearable', NULL), \
            ('n4', '0xc', 4, 'MATIC', 90, 90, 'urn:e:1', $1, 'item-e1', 'emote_v1', NULL, 90, 'emote', NULL), \
            ('n5', '0xc', 5, 'MATIC', 80, 80, 'urn:e:2', $1, 'item-e2', 'emote_v1', NULL, 80, 'emote', NULL), \
            ('n6', '0xe', 6, 'ETHEREUM', 70, 70, 'urn:ens:1', $1, NULL, 'dcl', NULL, 70, 'ens', 'ens-1'), \
            ('n7', '0xe', 7, 'ETHEREUM', 60, 60, 'urn:ens:2', $1, NULL, 'dcl', NULL, 60, 'ens', 'ens-2'), \
            ('n8', '0xc', 8, 'MATIC', 50, 50, 'urn:w:9', '0xsomeoneelse', 'item-9', 'wearable_v2', NULL, 50, 'wearable', NULL)",
        "INSERT INTO marketplace.usage_grants (grantee_address, urn, token_id, category, unlock_at, status) VALUES \
            ($1, 'urn:g:1', '11', 'wearable', now() + interval '10 days', 'active'), \
            ($1, 'urn:g:1', '12', 'wearable', now() + interval '10 days', 'active'), \
            ($1, 'urn:g:2', '13', 'wearable', now() + interval '10 days', 'revoked'), \
            ($1, 'urn:ge:1', '21', 'emote', now() + interval '10 days', 'active')",
    ] {
        let q = sqlx::query(ddl);
        let q = if ddl.contains("$1") {
            q.bind(OWNER)
        } else {
            q
        };
        q.execute(pool).await.unwrap();
    }
}

#[tokio::test]
async fn window_totals_match_the_standalone_counts() {
    let Some(scratch) = ScratchDb::builder("CATALYRST_MARKET_TEST_PG", "user_assets")
        .schemas(["marketplace", "squid_marketplace"])
        .build()
        .await
    else {
        return;
    };
    scratch
        .apply_sql(include_str!("../../../migrations/0007_usage_grants.sql"))
        .await;
    scratch
        .apply_sql(include_str!(
            "../../../migrations/0008_usage_grants_collection.sql"
        ))
        .await;
    seed(&scratch.pool).await;
    let pool = &scratch.pool;

    for grants in [false, true] {
        let c = UserAssetsComponent::new(pool.clone(), grants);
        let expect_total = scalar(pool, wearables_count_sql(grants)).await;
        let expect_items = scalar(pool, wearables_unique_sql(grants)).await;
        assert_eq!(
            (expect_total, expect_items),
            if grants { (5, 3) } else { (3, 2) }
        );

        let (rows, total, items) = c.get_wearables_by_owner(OWNER, 2, 0).await.unwrap();
        assert_eq!((rows.len(), total, items), (2, expect_total, expect_items));
        assert_eq!(
            rows[0].1, grants,
            "newest first: a fresh grant outranks the 2001 mints"
        );
        let (rows, total, items) = c.get_wearables_by_owner(OWNER, 2, 50).await.unwrap();
        assert_eq!((rows.len(), total, items), (0, expect_total, expect_items));
        let (rows, total, items) = c.get_wearables_by_owner("0xnobody", 2, 0).await.unwrap();
        assert_eq!((rows.len(), total, items), (0, 0, 0));

        let (rows, total) = c
            .get_owned_wearables_urn_and_token_id(OWNER, 1, 0)
            .await
            .unwrap();
        assert_eq!((rows.len(), total), (1, expect_total));
        let (rows, total) = c
            .get_owned_wearables_urn_and_token_id(OWNER, 1, 50)
            .await
            .unwrap();
        assert_eq!((rows.len(), total), (0, expect_total));

        let expect_total = scalar(pool, emotes_count_sql(grants)).await;
        let expect_items = scalar(pool, emotes_unique_sql(grants)).await;
        assert_eq!(
            (expect_total, expect_items),
            if grants { (3, 3) } else { (2, 2) }
        );
        let (rows, total, items) = c.get_emotes_by_owner(OWNER, 10, 0).await.unwrap();
        assert_eq!(
            (rows.len() as i64, total, items),
            (expect_total, expect_total, expect_items)
        );
        let (rows, total, items) = c.get_emotes_by_owner(OWNER, 10, 50).await.unwrap();
        assert_eq!((rows.len(), total, items), (0, expect_total, expect_items));
        let (rows, total) = c
            .get_owned_emotes_urn_and_token_id(OWNER, 10, 50)
            .await
            .unwrap();
        assert_eq!((rows.len(), total), (0, expect_total));

        let filters = UserAssetsFilters {
            first: 10,
            ..Default::default()
        };
        let expect_grouped = scalar(
            pool,
            grouped_wearables_count_sql(
                grants,
                "",
                " AND nft.item_type IN ('wearable_v1', 'wearable_v2', 'smart_wearable_v1')",
            ),
        )
        .await;
        assert_eq!(expect_grouped, if grants { 3 } else { 2 });
        let (rows, total) = c
            .get_grouped_wearables_by_owner(OWNER, &filters)
            .await
            .unwrap();
        assert_eq!((rows.len() as i64, total), (expect_grouped, expect_grouped));
        assert_eq!(
            rows.iter().filter(|(_, leased)| *leased).count(),
            grants as usize
        );
        let past_end = UserAssetsFilters {
            first: 10,
            skip: 50,
            ..Default::default()
        };
        let (rows, total) = c
            .get_grouped_wearables_by_owner(OWNER, &past_end)
            .await
            .unwrap();
        assert_eq!((rows.len() as i64, total), (0, expect_grouped));
        let by_name = UserAssetsFilters {
            first: 10,
            name: Some("hat".to_string()),
            ..Default::default()
        };
        let (rows, total) = c
            .get_grouped_wearables_by_owner(OWNER, &by_name)
            .await
            .unwrap();
        assert_eq!(
            (rows.len(), total),
            (1, 1 + grants as i64),
            "the total never applied the name filter to the grants leg"
        );

        let expect_grouped = scalar(pool, grouped_emotes_count_sql(grants, "")).await;
        assert_eq!(expect_grouped, if grants { 3 } else { 2 });
        let (rows, total) = c
            .get_grouped_emotes_by_owner(OWNER, &filters)
            .await
            .unwrap();
        assert_eq!((rows.len() as i64, total), (expect_grouped, expect_grouped));
        let (rows, total) = c
            .get_grouped_emotes_by_owner(OWNER, &past_end)
            .await
            .unwrap();
        assert_eq!((rows.len() as i64, total), (0, expect_grouped));

        let (rows, total) = c.get_names_by_owner(OWNER, &filters).await.unwrap();
        assert_eq!((rows.len(), total), (2, 2));
        assert_eq!(rows[0].name, "alice");
        let (rows, total) = c.get_names_by_owner(OWNER, &past_end).await.unwrap();
        assert_eq!((rows.len(), total), (0, 2));
        let (rows, total) = c.get_owned_names_only(OWNER, 1, 1).await.unwrap();
        assert_eq!((rows.len(), total), (1, 2));
        assert_eq!(rows[0].name, "bob");
        let (rows, total) = c.get_owned_names_only(OWNER, 1, 50).await.unwrap();
        assert_eq!((rows.len(), total), (0, 2));
        let (rows, total) = c.get_owned_names_only("0xnobody", 1, 0).await.unwrap();
        assert_eq!((rows.len(), total), (0, 0));
    }

    scratch.drop().await;
}
