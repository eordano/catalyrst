use catalyrst_contract_gate::pg::ScratchSchema;
use serde_json::json;
use sqlx::PgPool;

use catalyrst_social_service::rest::handlers::enrich::enrich_with_profiles;
use catalyrst_social_service::rest::ports::profiles::ProfilesComponent;

async fn setup_db() -> Option<ScratchSchema> {
    let scratch = ScratchSchema::create_or_default(
        "CATALYRST_SOCIAL_SERVICE_TEST_PG",
        "postgres://postgres:postgres@127.0.0.1:5432/communities",
        "cg_social_namecolor",
    )
    .await?;

    sqlx::query(sqlx::AssertSqlSafe(
        "CREATE TABLE deployments ( \
            id                  BIGSERIAL PRIMARY KEY, \
            entity_type         TEXT NOT NULL, \
            entity_pointers     TEXT[] NOT NULL, \
            entity_metadata     JSONB NOT NULL, \
            deleter_deployment  BIGINT \
         )",
    ))
    .execute(&scratch.pool)
    .await
    .expect("create deployments");

    Some(scratch)
}

async fn seed_profile(
    pool: &PgPool,
    address: &str,
    face256: &str,
    name_color: Option<(f64, f64, f64)>,
) {
    let mut avatar = json!({
        "name": "Alice",
        "hasClaimedName": true,
        "avatar": { "snapshots": { "face256": face256 } }
    });
    if let Some((r, g, b)) = name_color {
        avatar["nameColor"] = json!({ "r": r, "g": g, "b": b });
    }
    let metadata = json!({ "v": { "avatars": [avatar] } });

    sqlx::query(
        "INSERT INTO deployments (entity_type, entity_pointers, entity_metadata, deleter_deployment) \
         VALUES ('profile', $1, $2::jsonb, NULL)",
    )
    .bind(vec![address.to_string()])
    .bind(metadata.to_string())
    .execute(pool)
    .await
    .expect("seed profile");
}

#[tokio::test]
async fn profiles_resolve_name_color_when_present_and_none_when_absent() {
    let Some(scratch) = setup_db().await else {
        return;
    };
    let pool = scratch.pool.clone();

    let with_color = "0x00000000000000000000000000000000000000a1";
    let no_color = "0x00000000000000000000000000000000000000a2";
    seed_profile(&pool, with_color, "bafyFACE1", Some((0.5, 0.25, 0.125))).await;
    seed_profile(&pool, no_color, "bafyFACE2", None).await;

    let profiles = ProfilesComponent::new(Some(pool.clone()), "https://content".to_string());
    let map = profiles
        .get_profiles(&[with_color.to_string(), no_color.to_string()])
        .await;

    let colored = map.get(with_color).expect("colored profile resolved");
    assert_eq!(colored.name, "Alice");
    assert!(colored.has_claimed_name);
    assert_eq!(
        colored.profile_picture_url,
        "https://content/contents/bafyFACE1"
    );
    let nc = colored
        .name_color
        .as_ref()
        .expect("nameColor present for colored profile");
    assert_eq!(nc.r, 0.5);
    assert_eq!(nc.g, 0.25);
    assert_eq!(nc.b, 0.125);

    let plain = map.get(no_color).expect("plain profile resolved");
    assert!(
        plain.name_color.is_none(),
        "profile without avatar nameColor must resolve to None"
    );

    scratch.drop().await;
}

#[tokio::test]
async fn enrichment_emits_name_color_only_when_present() {
    let Some(scratch) = setup_db().await else {
        return;
    };
    let pool = scratch.pool.clone();

    let with_color = "0x00000000000000000000000000000000000000b1";
    let no_color = "0x00000000000000000000000000000000000000b2";
    seed_profile(&pool, with_color, "bafyFACE1", Some((0.5, 0.25, 0.125))).await;
    seed_profile(&pool, no_color, "bafyFACE2", None).await;

    let profiles = ProfilesComponent::new(Some(pool.clone()), "https://content".to_string());

    let mut rows = vec![
        json!({ "communityId": "c1", "memberAddress": with_color, "role": "member" }),
        json!({ "communityId": "c1", "memberAddress": no_color, "role": "member" }),
    ];
    enrich_with_profiles(&profiles, &mut rows, "memberAddress").await;

    let colored = rows[0].as_object().unwrap();
    assert_eq!(colored["name"], "Alice");
    let nc = colored
        .get("nameColor")
        .expect("colored member carries nameColor")
        .as_object()
        .expect("nameColor is an object");
    assert_eq!(nc["r"].as_f64(), Some(0.5));
    assert_eq!(nc["g"].as_f64(), Some(0.25));
    assert_eq!(nc["b"].as_f64(), Some(0.125));

    let plain = rows[1].as_object().unwrap();
    assert!(
        !plain.contains_key("nameColor"),
        "member without a profile nameColor must not carry the key: {:?}",
        rows[1]
    );

    scratch.drop().await;
}
