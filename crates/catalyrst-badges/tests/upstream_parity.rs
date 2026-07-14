use std::sync::Arc;

use axum::body::Body;
use axum::http::{HeaderMap, Request, StatusCode};
use axum::Router;
use catalyrst_badges::config::{Config, DEFAULT_ASSET_BASE_URL};
use catalyrst_badges::ports::badges::BadgesComponent;
use catalyrst_badges::{app_router, AppState, AppStateInner};
use catalyrst_contract_gate::pg::ScratchSchema;
use chrono::DateTime;
use serde_json::{json, Value};
use sqlx::PgPool;
use tower::ServiceExt;

const ADDR: &str = "0xf39fd6e51aad88f6f4ce6ab8827279cfffb92266";
const ADDR_CHECKSUM: &str = "0xf39Fd6e51aad88F6F4ce6aB8827279cffFb92266";
const T_STARTER: &str = "2024-10-20T00:00:00.250Z";
const T_BRONZE: &str = "2024-10-23T00:00:00.500Z";
const T_BUSINESS: &str = "2024-10-25T00:00:00.750Z";

fn ms(rfc3339: &str) -> i64 {
    DateTime::parse_from_rfc3339(rfc3339)
        .unwrap()
        .timestamp_millis()
}

async fn seed(pool: &PgPool) {
    sqlx::migrate!("./migrations").run(pool).await.unwrap();

    sqlx::query(
        "INSERT INTO user_badge_progress \
           (address, badge_id, steps_done, completed_at, last_completed_tier_id, updated_at) \
         VALUES ($1, 'walkabout', 15000, $2::timestamptz, 'walkabout-bronze', now()), \
                ($1, 'open_for_business', 1, $3::timestamptz, NULL, now())",
    )
    .bind(ADDR)
    .bind(T_STARTER)
    .bind(T_BUSINESS)
    .execute(pool)
    .await
    .unwrap();

    sqlx::query(
        "INSERT INTO user_achieved_tiers (address, badge_id, tier_id, completed_at) \
         VALUES ($1, 'walkabout', 'walkabout-starter', $2::timestamptz), \
                ($1, 'walkabout', 'walkabout-bronze', $3::timestamptz)",
    )
    .bind(ADDR)
    .bind(T_STARTER)
    .bind(T_BRONZE)
    .execute(pool)
    .await
    .unwrap();
}

fn app(pool: PgPool) -> Router {
    let cfg = Config {
        http_host: "127.0.0.1".into(),
        http_port: 0,
        badges_database_url: String::new(),
        admin_token: None,
        assets_dir: std::path::PathBuf::from("./assets"),
        public_asset_base_url: DEFAULT_ASSET_BASE_URL.to_string(),
    };
    let state: AppState = Arc::new(AppStateInner::new(
        BadgesComponent::new(pool, cfg.public_asset_base_url.clone()),
        None,
    ));
    app_router(&cfg).with_state(state)
}

async fn send(app: Router, req: Request<Body>) -> (StatusCode, HeaderMap, Value) {
    let resp = app.oneshot(req).await.unwrap();
    let status = resp.status();
    let headers = resp.headers().clone();
    let bytes = axum::body::to_bytes(resp.into_body(), 1 << 20)
        .await
        .unwrap();
    let value = if bytes.is_empty() {
        Value::Null
    } else {
        serde_json::from_slice(&bytes).unwrap()
    };
    (status, headers, value)
}

async fn get(app: Router, uri: &str) -> (StatusCode, Value) {
    let req = Request::builder()
        .method("GET")
        .uri(uri)
        .body(Body::empty())
        .unwrap();
    let (status, _, value) = send(app, req).await;
    (status, value)
}

fn walkabout_tier(
    tier_id: &str,
    tier_name: &str,
    description: &str,
    asset: &str,
    steps: i64,
) -> Value {
    json!({
        "tierId": tier_id,
        "tierName": tier_name,
        "description": description,
        "assets": { "2d": { "normal": asset } },
        "criteria": { "steps": steps },
    })
}

fn business_assets() -> Value {
    json!({
        "2d": {
            "normal": "https://badges.decentraland.org/assets/open_for_business/2d/normal.png",
            "hrm": "https://badges.decentraland.org/assets/open_for_business/2d/hrm.png",
            "baseColor": "https://badges.decentraland.org/assets/open_for_business/2d/baseColor.png"
        },
        "3d": {
            "normal": "https://badges.decentraland.org/assets/open_for_business/3d/normal.png",
            "hrm": "https://badges.decentraland.org/assets/open_for_business/3d/hrm.png",
            "baseColor": "https://badges.decentraland.org/assets/open_for_business/3d/baseColor.png"
        }
    })
}

// Mirrors the migration 0005 backfill: 2D stays normal-only (no real hrm/
// baseColor upstream), 3D is fully populated (what Badge3DImage renders).
fn simple_assets(slug: &str) -> Value {
    json!({
        "2d": { "normal": format!("https://badges.decentraland.org/assets/{slug}/2d/normal.png"), "hrm": "", "baseColor": "" },
        "3d": {
            "normal": format!("https://badges.decentraland.org/assets/{slug}/3d/normal.png"),
            "hrm": format!("https://badges.decentraland.org/assets/{slug}/3d/hrm.png"),
            "baseColor": format!("https://badges.decentraland.org/assets/{slug}/3d/baseColor.png")
        }
    })
}

macro_rules! gate {
    () => {
        match ScratchSchema::create("CATALYRST_BADGES_TEST_PG", "cg_badges_parity").await {
            Some(s) => s,
            None => return,
        }
    };
}

#[tokio::test]
async fn cors_headers_on_get_and_preflight() {
    let scratch = gate!();
    seed(&scratch.pool).await;

    let req = Request::builder()
        .method("GET")
        .uri("/categories")
        .header("origin", "https://example.com")
        .body(Body::empty())
        .unwrap();
    let (status, headers, _) = send(app(scratch.pool.clone()), req).await;
    assert!(status.is_success());
    assert_eq!(headers.get("access-control-allow-origin").unwrap(), "*");

    let req = Request::builder()
        .method("OPTIONS")
        .uri("/categories")
        .header("origin", "https://example.com")
        .header("access-control-request-method", "GET")
        .body(Body::empty())
        .unwrap();
    let (status, headers, _) = send(app(scratch.pool.clone()), req).await;
    assert!(status.is_success());
    assert_eq!(headers.get("access-control-allow-origin").unwrap(), "*");
    assert!(headers.get("access-control-allow-methods").is_some());

    scratch.drop().await;
}

#[tokio::test]
async fn categories_exact_body() {
    let scratch = gate!();
    seed(&scratch.pool).await;
    let (status, body) = get(app(scratch.pool.clone()), "/categories").await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(
        body,
        json!({ "data": { "categories": ["Builder", "Explorer", "Socializer"] } })
    );
    scratch.drop().await;
}

#[tokio::test]
async fn non_tiered_badge_tiers_is_empty_not_404() {
    let scratch = gate!();
    seed(&scratch.pool).await;
    let (status, body) = get(
        app(scratch.pool.clone()),
        "/badges/decentraland_citizen/tiers",
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body, json!({ "data": { "tiers": [] } }));
    scratch.drop().await;
}

#[tokio::test]
async fn unknown_badge_tiers_is_404() {
    let scratch = gate!();
    seed(&scratch.pool).await;
    let (status, body) = get(app(scratch.pool.clone()), "/badges/doesnotexist/tiers").await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    assert_eq!(body["error"], json!("Badge not found"));
    scratch.drop().await;
}

#[tokio::test]
async fn tiered_badge_tiers_full_shape() {
    let scratch = gate!();
    seed(&scratch.pool).await;
    let (status, body) = get(app(scratch.pool.clone()), "/badges/walkabout/tiers").await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(
        body,
        json!({ "data": { "tiers": [
            walkabout_tier("walkabout-starter", "Starter", "Walk 1,000 meters.", "https://badges.decentraland.org/assets/walkabout/starter/2d/normal.png", 1000),
            walkabout_tier("walkabout-bronze", "Bronze", "Walk 10,000 meters.", "https://badges.decentraland.org/assets/walkabout/bronze/2d/normal.png", 10000),
            walkabout_tier("walkabout-silver", "Silver", "Walk 100,000 meters.", "https://badges.decentraland.org/assets/walkabout/silver/2d/normal.png", 100000),
            walkabout_tier("walkabout-gold", "Gold", "Walk 1,000,000 meters.", "https://badges.decentraland.org/assets/walkabout/gold/2d/normal.png", 1000000),
        ] } })
    );
    scratch.drop().await;
}

#[tokio::test]
async fn user_badges_full_body_and_epoch_types() {
    let scratch = gate!();
    seed(&scratch.pool).await;
    let (status, body) = get(
        app(scratch.pool.clone()),
        &format!("/users/{ADDR}/badges?includeNotAchieved=true"),
    )
    .await;
    assert_eq!(status, StatusCode::OK);

    let business = json!({
        "id": "open_for_business",
        "name": "Open for Business",
        "description": "Set up a store and publish a collection.",
        "category": "Builder",
        "isTier": false,
        "completedAt": ms(T_BUSINESS).to_string(),
        "assets": business_assets(),
        "progress": {
            "stepsDone": 1,
            "nextStepsTarget": null,
            "totalStepsTarget": 1,
            "lastCompletedTierAt": null,
            "lastCompletedTierName": null,
            "lastCompletedTierImage": null,
            "achievedTiers": []
        }
    });

    let walkabout = json!({
        "id": "walkabout",
        "name": "Walkabout",
        "description": "Walk a cumulative distance across Decentraland.",
        "category": "Explorer",
        "isTier": true,
        "completedAt": ms(T_STARTER).to_string(),
        "assets": simple_assets("walkabout"),
        "progress": {
            "stepsDone": 15000,
            "nextStepsTarget": 100000,
            "totalStepsTarget": 1000000,
            "lastCompletedTierAt": ms(T_BRONZE),
            "lastCompletedTierName": "Bronze",
            "lastCompletedTierImage": "https://badges.decentraland.org/assets/walkabout/bronze/2d/normal.png",
            "achievedTiers": [
                { "tierId": "walkabout-starter", "completedAt": ms(T_STARTER) },
                { "tierId": "walkabout-bronze", "completedAt": ms(T_BRONZE) }
            ]
        }
    });

    let citizen = json!({
        "id": "decentraland_citizen",
        "name": "Decentraland Citizen",
        "description": "Log in to Decentraland for the first time.",
        "category": "Explorer",
        "isTier": false,
        "completedAt": null,
        "assets": simple_assets("decentraland_citizen"),
        "progress": {
            "stepsDone": 0,
            "nextStepsTarget": 1,
            "totalStepsTarget": 1,
            "lastCompletedTierAt": null,
            "lastCompletedTierName": null,
            "lastCompletedTierImage": null,
            "achievedTiers": []
        }
    });

    let emotionista = json!({
        "id": "emotionista",
        "name": "Emotionista",
        "description": "Play emotes in-world.",
        "category": "Socializer",
        "isTier": true,
        "completedAt": null,
        "assets": simple_assets("emotionista"),
        "progress": {
            "stepsDone": 0,
            "nextStepsTarget": 10,
            "totalStepsTarget": 1000,
            "lastCompletedTierAt": null,
            "lastCompletedTierName": null,
            "lastCompletedTierImage": null,
            "achievedTiers": []
        }
    });

    assert_eq!(
        body,
        json!({ "data": {
            "achieved": [business, walkabout],
            "notAchieved": [citizen, emotionista]
        } })
    );

    let w = body["data"]["achieved"]
        .as_array()
        .unwrap()
        .iter()
        .find(|b| b["id"] == json!("walkabout"))
        .unwrap();
    assert!(w["completedAt"].is_string());
    assert!(w["progress"]["lastCompletedTierAt"].is_i64());
    assert!(w["progress"]["achievedTiers"][0]["completedAt"].is_i64());

    scratch.drop().await;
}

#[tokio::test]
async fn malformed_address_reads_are_200_empty() {
    let scratch = gate!();
    seed(&scratch.pool).await;

    let (status, body) = get(app(scratch.pool.clone()), "/users/not-an-address/badges").await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(
        body,
        json!({ "data": { "achieved": [], "notAchieved": [] } })
    );

    let (status, body) = get(app(scratch.pool.clone()), "/users/not-an-address/preview").await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body, json!({ "data": { "latestAchievedBadges": [] } }));

    scratch.drop().await;
}

#[tokio::test]
async fn checksummed_address_reads_same_rows() {
    let scratch = gate!();
    seed(&scratch.pool).await;

    let (_, lower) = get(
        app(scratch.pool.clone()),
        &format!("/users/{ADDR}/badges?includeNotAchieved=true"),
    )
    .await;
    let (status, checksum) = get(
        app(scratch.pool.clone()),
        &format!("/users/{ADDR_CHECKSUM}/badges?includeNotAchieved=true"),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(lower, checksum);

    scratch.drop().await;
}

#[tokio::test]
async fn preview_latest_first_body() {
    let scratch = gate!();
    seed(&scratch.pool).await;
    let (status, body) = get(app(scratch.pool.clone()), &format!("/users/{ADDR}/preview")).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(
        body,
        json!({ "data": { "latestAchievedBadges": [
            {
                "id": "open_for_business",
                "name": "Open for Business",
                "tierName": null,
                "image": "https://badges.decentraland.org/assets/open_for_business/2d/normal.png"
            },
            {
                "id": "walkabout",
                "name": "Walkabout",
                "tierName": "Bronze",
                "image": "https://badges.decentraland.org/assets/walkabout/bronze/2d/normal.png"
            }
        ] } })
    );
    scratch.drop().await;
}

/// Every asset URL the seed fixture (migrations 0002/0005) advertises must be
/// backed by real art in `assets/` -- the upstream-CDN mirror invariant
/// documented in migrations/0005_fixup_asset_urls.sql. Set equality both
/// ways: a missing file breaks the API's promise, a stray file means the
/// migrations and this pin no longer describe the tree. The two
/// 0002-advertised `open_for_business/2d/{hrm,baseColor}.png` URLs stay
/// unbacked (upstream serves no 2d hrm/basecolor for any badge) and so are
/// absent here.
#[test]
fn advertised_assets_are_backed_by_real_png_files() {
    const BACKED: &[&str] = &[
        "decentraland_citizen/2d/normal.png",
        "decentraland_citizen/3d/baseColor.png",
        "decentraland_citizen/3d/hrm.png",
        "decentraland_citizen/3d/normal.png",
        "emotionista/2d/normal.png",
        "emotionista/3d/baseColor.png",
        "emotionista/3d/hrm.png",
        "emotionista/3d/normal.png",
        "emotionista/bronze/2d/normal.png",
        "emotionista/gold/2d/normal.png",
        "emotionista/silver/2d/normal.png",
        "open_for_business/2d/normal.png",
        "open_for_business/3d/baseColor.png",
        "open_for_business/3d/hrm.png",
        "open_for_business/3d/normal.png",
        "walkabout/2d/normal.png",
        "walkabout/3d/baseColor.png",
        "walkabout/3d/hrm.png",
        "walkabout/3d/normal.png",
        "walkabout/bronze/2d/normal.png",
        "walkabout/gold/2d/normal.png",
        "walkabout/silver/2d/normal.png",
        "walkabout/starter/2d/normal.png",
    ];
    // Real art, not a stub: PNG magic plus a floor well above the
    // few-hundred-byte single-color PNGs the mirror invariant forbids.
    const MIN_BYTES: usize = 5 * 1024;
    const PNG_MAGIC: [u8; 8] = [0x89, b'P', b'N', b'G', b'\r', b'\n', 0x1a, b'\n'];

    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("assets");
    let mut found = Vec::new();
    let mut stack = vec![root.clone()];
    while let Some(dir) = stack.pop() {
        for entry in std::fs::read_dir(&dir).expect("assets dir is readable") {
            let path = entry.expect("dir entry").path();
            if path.is_dir() {
                stack.push(path);
            } else if path.extension().is_some_and(|e| e == "png") {
                // Only PNGs are inventory. assets/README.md documents the CDN
                // provenance of this store and ships alongside the art; it is
                // not an advertised asset, and the MIN_BYTES/PNG_MAGIC check
                // below would reject it.
                found.push(
                    path.strip_prefix(&root)
                        .unwrap()
                        .to_str()
                        .expect("utf-8 asset path")
                        .to_string(),
                );
            }
        }
    }
    found.sort();
    assert_eq!(
        found, BACKED,
        "assets/ tree diverged from the advertised inventory"
    );

    for rel in BACKED {
        let bytes = std::fs::read(root.join(rel)).expect("asset file is readable");
        assert!(
            bytes.starts_with(&PNG_MAGIC),
            "{rel} is not a PNG (bad magic)"
        );
        assert!(
            bytes.len() > MIN_BYTES,
            "{rel} is {} bytes -- placeholder-sized, not real art",
            bytes.len()
        );
    }
}
