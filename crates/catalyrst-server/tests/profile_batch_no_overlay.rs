use catalyrst_contract_gate::pg::ScratchDb;
use catalyrst_server::handlers::profile_processing::process_profiles;
use serde_json::json;

#[tokio::test]
async fn profile_batch_without_usage_grants_keeps_owner_scope_and_skips_unneeded_queries() {
    let Some(db) = ScratchDb::create("CATALYRST_TEST_PG", "profiles_no_overlay").await else {
        return;
    };
    sqlx::raw_sql("CREATE SCHEMA squid_marketplace; CREATE SCHEMA marketplace;
        CREATE TABLE squid_marketplace.nft (id serial PRIMARY KEY, owner_address text, urn text, category text, name text);")
        .execute(&db.pool).await.unwrap();
    let exact = "urn:decentraland:matic:collections-v2:0xabc:1";
    let prefix = "urn:decentraland:matic:collections-v2:0xabc:2";
    sqlx::query(
        "INSERT INTO squid_marketplace.nft (owner_address,urn) VALUES ('alice',$1),('bob',$2)",
    )
    .bind(exact)
    .bind(format!("{prefix}:7"))
    .execute(&db.pool)
    .await
    .unwrap();
    let entities: Vec<_> = ["ALICE", "BOB"].into_iter().map(|address| json!({
        "pointers": [address], "metadata": {"avatars": [{"avatar": {"wearables": [exact,prefix]}}]}
    })).collect();
    let profiles = process_profiles(&entities, Some(&db.pool), "").await;
    assert_eq!(
        profiles[0]["avatars"][0]["avatar"]["wearables"],
        json!([exact])
    );
    assert_eq!(
        profiles[1]["avatars"][0]["avatar"]["wearables"],
        json!([prefix])
    );
    let capture = catalyrst_testgate::sql_capture::sql_capture();
    assert_eq!(
        process_profiles(&entities, Some(&db.pool), "").await,
        profiles
    );
    assert_eq!(capture.count_containing("SELECT DISTINCT"), 1);
    assert_eq!(capture.count_containing("SELECT owner_address, name"), 1);
    capture.reset();
    let default = json!({"pointers":["default0"],"metadata":{"avatars":[]}});
    assert_eq!(
        process_profiles(&[default, json!({})], Some(&db.pool), "")
            .await
            .len(),
        1
    );
    assert_eq!(capture.count(), 0);
    let empty = json!({"pointers":["alice"],"metadata":{"avatars":[]}});
    process_profiles(&[empty], Some(&db.pool), "").await;
    assert_eq!(capture.count(), 1);
    db.drop().await;
}
