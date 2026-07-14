use std::sync::Arc;

use axum::body::Body;
use axum::http::{Request, StatusCode};
use catalyrst_map::handlers::tiles::to_legacy;
use catalyrst_map::map::{MapComponent, Tile, TileType};
use catalyrst_map::nft::{NftAttribute, NftMetadata};
use catalyrst_map::satellite::SatelliteState;
use catalyrst_map::AppStateInner;
use serde_json::Value;
use tower::ServiceExt;

const SNAPSHOT: &str = include_str!("fixtures/tiles_snapshot.json");
const NFT_PARCEL: &str = include_str!("fixtures/nft_parcel.json");

fn numbers_equal(a: &Value, b: &Value) -> bool {
    match (a, b) {
        (Value::Number(x), Value::Number(y)) => match (x.as_f64(), y.as_f64()) {
            (Some(xf), Some(yf)) => xf == yf,
            _ => x == y,
        },
        (Value::Array(xs), Value::Array(ys)) => {
            xs.len() == ys.len() && xs.iter().zip(ys).all(|(p, q)| numbers_equal(p, q))
        }
        (Value::Object(xo), Value::Object(yo)) => {
            let xk: Vec<&String> = xo.keys().collect();
            let yk: Vec<&String> = yo.keys().collect();
            xk == yk
                && xo
                    .iter()
                    .all(|(k, v)| yo.get(k).map(|w| numbers_equal(v, w)).unwrap_or(false))
        }
        _ => a == b,
    }
}

fn keys(v: &Value) -> Vec<String> {
    v.as_object().unwrap().keys().cloned().collect()
}

#[test]
fn legacy_mapping_parity_against_upstream_snapshot() {
    let snap: Value = serde_json::from_str(SNAPSHOT).unwrap();
    let v1 = snap["v1"].as_object().unwrap();
    let v2 = snap["v2"].as_object().unwrap();
    assert!(!v2.is_empty(), "snapshot must carry v2 tiles");

    for (id, v2obj) in v2 {
        let tile: Tile = serde_json::from_value(v2obj.clone())
            .unwrap_or_else(|e| panic!("v2 tile {id} must deserialize into Tile: {e}"));
        let got = serde_json::to_value(to_legacy(&tile)).unwrap();
        let want = v1
            .get(id)
            .unwrap_or_else(|| panic!("v1 snapshot missing coord {id}"));
        assert_eq!(
            keys(&got),
            keys(want),
            "legacy key order/set drifted for {id}"
        );
        assert!(
            numbers_equal(&got, want),
            "legacy tile {id} drifted: ours={got} upstream={want}"
        );
    }
}

fn tile_of(tile_type: TileType, price: Option<f64>) -> Tile {
    Tile {
        id: "0,0".into(),
        x: 0,
        y: 0,
        nft_id: None,
        tile_type,
        top: false,
        left: false,
        top_left: false,
        updated_at: 0,
        name: None,
        owner: None,
        estate_id: None,
        token_id: None,
        price,
        expires_at: None,
        rental_listing: None,
    }
}

#[test]
fn legacy_type_code_table() {
    assert_eq!(to_legacy(&tile_of(TileType::District, None)).tile_type, 5);
    assert_eq!(to_legacy(&tile_of(TileType::Road, None)).tile_type, 7);
    assert_eq!(to_legacy(&tile_of(TileType::Plaza, None)).tile_type, 8);
    assert_eq!(to_legacy(&tile_of(TileType::Owned, None)).tile_type, 9);
    assert_eq!(
        to_legacy(&tile_of(TileType::Owned, Some(1.0))).tile_type,
        10
    );
    assert_eq!(to_legacy(&tile_of(TileType::Unowned, None)).tile_type, 11);
}

#[test]
fn nft_metadata_envelope_parity() {
    let want: Value = serde_json::from_str(NFT_PARCEL).unwrap();

    let nft = NftMetadata {
        id: "0".into(),
        name: "Parcel 0,0".into(),
        description: String::new(),
        image: "https://example.test/v2/parcels/0/0/map.png?size=24&width=1024&height=1024".into(),
        external_url: None,
        background_color: "000000".into(),
        attributes: vec![
            NftAttribute {
                trait_type: "X".into(),
                value: 0,
                display_type: "number".into(),
            },
            NftAttribute {
                trait_type: "Y".into(),
                value: 0,
                display_type: "number".into(),
            },
        ],
    };
    let got = serde_json::to_value(&nft).unwrap();

    assert_eq!(keys(&got), keys(&want), "NFT top-level key order drifted");
    assert_eq!(got["background_color"], "000000");
    assert!(
        got["external_url"].is_null(),
        "external_url must serialize null"
    );
    assert!(
        want["external_url"].is_null(),
        "fixture external_url must be null"
    );

    for attr in got["attributes"].as_array().unwrap() {
        assert_eq!(keys(attr), vec!["display_type", "trait_type", "value"]);
    }
    for attr in want["attributes"].as_array().unwrap() {
        assert_eq!(keys(attr), vec!["display_type", "trait_type", "value"]);
    }

    let suffix = "/parcels/0/0/map.png?size=24&width=1024&height=1024";
    assert!(want["image"].as_str().unwrap().ends_with(suffix));
    assert!(got["image"].as_str().unwrap().ends_with(suffix));
}

fn scratch_state() -> Arc<AppStateInner> {
    let pool = sqlx::PgPool::connect_lazy("postgres://map@127.0.0.1:1/none")
        .expect("lazy pool must construct without connecting");
    let map = MapComponent::new(
        pool.clone(),
        "squid_marketplace".into(),
        "0xf87e31492faf9a91b02ee0deaad50d51d56d5d4d".into(),
        "0x959e104e1a4db6317fa58f8295f586e1a978c297".into(),
    );
    let dir = std::env::temp_dir().join("catalyrst-map-parity-satellite");
    let satellite = SatelliteState::new(dir, 1, 1);
    Arc::new(AppStateInner {
        map,
        pool,
        map_schema: "squid_marketplace".into(),
        satellite,
    })
}

async fn header(
    router: &axum::Router,
    req: Request<Body>,
    name: &str,
) -> (StatusCode, Option<String>) {
    let resp = router.clone().oneshot(req).await.unwrap();
    let status = resp.status();
    let value = resp
        .headers()
        .get(name)
        .and_then(|v| v.to_str().ok())
        .map(|s| s.to_string());
    (status, value)
}

#[tokio::test]
async fn cors_permissive_on_every_response() {
    let router = catalyrst_map::service_router(scratch_state());

    let get = |path: &str| {
        Request::builder()
            .method("GET")
            .uri(path)
            .body(Body::empty())
            .unwrap()
    };

    for (path, want_status) in [
        ("/v2/districts", StatusCode::OK),
        ("/v2/tiles", StatusCode::SERVICE_UNAVAILABLE),
        ("/v1/map.png", StatusCode::SERVICE_UNAVAILABLE),
        ("/no-such-route", StatusCode::NOT_FOUND),
    ] {
        let (origin_status, origin) =
            header(&router, get(path), "access-control-allow-origin").await;
        assert_eq!(origin_status, want_status, "status for {path}");
        assert_eq!(origin.as_deref(), Some("*"), "allow-origin for {path}");

        let (_, expose) = header(&router, get(path), "access-control-expose-headers").await;
        assert_eq!(expose.as_deref(), Some("*"), "expose-headers for {path}");
    }

    let preflight = Request::builder()
        .method("OPTIONS")
        .uri("/v2/tiles")
        .header("origin", "https://example.test")
        .header("access-control-request-method", "GET")
        .body(Body::empty())
        .unwrap();
    let (pf_status, pf_origin) = header(&router, preflight, "access-control-allow-origin").await;
    assert_eq!(pf_status, StatusCode::OK, "preflight status");
    assert_eq!(pf_origin.as_deref(), Some("*"), "preflight allow-origin");

    let preflight_methods = Request::builder()
        .method("OPTIONS")
        .uri("/v2/tiles")
        .header("origin", "https://example.test")
        .header("access-control-request-method", "GET")
        .body(Body::empty())
        .unwrap();
    let (_, methods) = header(&router, preflight_methods, "access-control-allow-methods").await;
    assert_eq!(methods.as_deref(), Some("*"), "preflight allow-methods");
}
