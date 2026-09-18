use super::*;
use catalyrst_contract_gate::pg::ScratchDb;
use serde_json::json;

#[tokio::test]
async fn profiles_batch_preserves_owner_scope_and_limits_queries() {
    let Some(db) = ScratchDb::create("CATALYRST_TEST_PG", "profile_batch").await else {
        return;
    };
    sqlx::raw_sql("CREATE SCHEMA squid_marketplace; CREATE SCHEMA marketplace;
        CREATE TABLE squid_marketplace.nft (id serial PRIMARY KEY, owner_address text, urn text, category text, name text);
        CREATE INDEX ON squid_marketplace.nft (owner_address, urn);
        CREATE TABLE marketplace.usage_grants (grantee_address text, urn text, status text);")
        .execute(&db.pool).await.unwrap();
    let base = "urn:decentraland:off-chain:base-avatars:green_hoodie";
    let exact = "urn:decentraland:matic:collections-v2:0xabc:1";
    let prefix = "urn:decentraland:matic:collections-v2:0xabc:2";
    let grant = "urn:decentraland:matic:collections-v2:0xabc:3";
    let denied = "urn:decentraland:matic:collections-v2:0xabc:4";
    let entities: Vec<_> = (0..50)
        .map(|i| {
            json!({
                "id": format!("entity-{i}"), "pointers": [format!("0xOWNER{i}")],
                "metadata": {"avatars": [{"name": format!("Name{i}"), "avatar": {
                    "wearables": [base, exact, prefix, grant, denied], "emotes": []
                }}]}
            })
        })
        .collect();
    for i in 0..50 {
        let address = format!("0xowner{i}");
        sqlx::query("INSERT INTO squid_marketplace.nft (owner_address, urn, category, name) VALUES ($1,$2,'wearable',NULL),($1,$3,'wearable',NULL),($1,NULL,'ens',$4)")
            .bind(&address).bind(exact).bind(format!("{prefix}:99")).bind(format!("Name{i}"))
            .execute(&db.pool).await.unwrap();
        sqlx::query(
            "INSERT INTO marketplace.usage_grants VALUES ($1,$2,'active'),($1,$3,'expired')",
        )
        .bind(&address)
        .bind(format!("{grant}:7"))
        .bind(denied)
        .execute(&db.pool)
        .await
        .unwrap();
    }
    sqlx::query("INSERT INTO squid_marketplace.nft (owner_address,urn) VALUES ('other',$1), ('0xowner0',$2)")
        .bind(denied).bind(format!("{denied}0:1")).execute(&db.pool).await.unwrap();
    assert!(super::super::lease_overlay::usage_grants_present(&db.pool).await);
    let capture = catalyrst_testgate::sql_capture::sql_capture();
    let profiles = process_profiles(&entities, Some(&db.pool), "https://cdn").await;
    assert_eq!(capture.count(), 2);
    assert_eq!(profiles.len(), 50);
    for (i, profile) in profiles.iter().enumerate() {
        assert_eq!(
            profile["avatars"][0]["avatar"]["wearables"],
            json!([base, exact, prefix, grant])
        );
        assert_eq!(profile["avatars"][0]["hasClaimedName"], true);
        assert_eq!(profile["avatars"][0]["ethAddress"], format!("0xowner{i}"));
    }
    let stranger = json!({"pointers":["stranger"],"metadata":entities[0]["metadata"]});
    let default = json!({"pointers":["default0"],"metadata":entities[0]["metadata"]});
    let batch = vec![
        stranger,
        entities[0].clone(),
        default,
        json!({}),
        entities[0].clone(),
    ];
    let together = process_profiles(&batch, Some(&db.pool), "https://cdn").await;
    let mut separate = Vec::new();
    for entity in &batch {
        if let Some(profile) = process_profile(entity, Some(&db.pool), "https://cdn").await {
            separate.push(profile);
        }
    }
    assert_eq!(together, separate);
    assert_eq!(
        together[0]["avatars"][0]["avatar"]["wearables"],
        json!([base])
    );
    assert_eq!(together[0]["avatars"][0]["hasClaimedName"], false);
    assert_eq!(
        together[2]["avatars"][0]["avatar"]["wearables"],
        entities[0]["metadata"]["avatars"][0]["avatar"]["wearables"]
    );
    sqlx::query("INSERT INTO squid_marketplace.nft (owner_address,category,name) VALUES ('0xowner0','ens',NULL)")
        .execute(&db.pool).await.unwrap();
    let null_name = process_profile(&entities[0], Some(&db.pool), "")
        .await
        .unwrap();
    assert_eq!(null_name["avatars"][0]["hasClaimedName"], false);
    let many: Vec<_> = (0..257).map(|_| entities[1].clone()).collect();
    capture.reset();
    assert_eq!(process_profiles(&many, Some(&db.pool), "").await.len(), 257);
    assert_eq!(capture.count(), 6);
    db.drop().await;
}
