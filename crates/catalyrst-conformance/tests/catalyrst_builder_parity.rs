use std::fs;
use std::path::{Path, PathBuf};

use serde_json::Value;

const COLLECTION_META_DATA_KEYS: &[&str] = &[
    "id",
    "name",
    "eth_address",
    "contract_address",
    "urn",
    "third_party_id",
    "is_published",
    "is_approved",
    "created_at",
    "updated_at",
];

const FULL_ITEM_KEYS_VIDEO_PRESENT: &[&str] = &[
    "id",
    "urn",
    "name",
    "description",
    "thumbnail",
    "video",
    "eth_address",
    "collection_id",
    "blockchain_item_id",
    "price",
    "beneficiary",
    "rarity",
    "type",
    "data",
    "metrics",
    "utility",
    "mappings",
    "contents",
    "is_published",
    "is_approved",
    "in_catalyst",
    "total_supply",
    "content_hash",
    "local_content_hash",
    "catalyst_content_hash",
    "created_at",
    "updated_at",
];

const FULL_ITEM_KEYS_VIDEO_ABSENT: &[&str] = &[
    "id",
    "urn",
    "name",
    "description",
    "thumbnail",
    "eth_address",
    "collection_id",
    "blockchain_item_id",
    "price",
    "beneficiary",
    "rarity",
    "type",
    "data",
    "metrics",
    "utility",
    "mappings",
    "contents",
    "is_published",
    "is_approved",
    "in_catalyst",
    "total_supply",
    "content_hash",
    "local_content_hash",
    "catalyst_content_hash",
    "created_at",
    "updated_at",
];

const PAGINATED_DATA_KEYS: &[&str] = &["total", "limit", "pages", "page", "results"];

fn fixtures_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("fixtures/builder-parity")
}

fn routes_md_path() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../catalyrst-builder/ROUTES.md")
}

fn load(rel: &str) -> Value {
    let p = fixtures_dir().join(rel);
    let s = fs::read_to_string(&p).unwrap_or_else(|e| panic!("read {}: {e}", p.display()));
    serde_json::from_str(&s).unwrap_or_else(|e| panic!("parse {}: {e}", p.display()))
}

fn sorted_keys(v: &Value) -> Vec<String> {
    let mut k: Vec<String> = v
        .as_object()
        .unwrap_or_else(|| panic!("expected object, got {v}"))
        .keys()
        .cloned()
        .collect();
    k.sort();
    k
}

fn assert_keys(v: &Value, expected: &[&str]) {
    let mut exp: Vec<String> = expected.iter().map(|s| s.to_string()).collect();
    exp.sort();
    assert_eq!(sorted_keys(v), exp);
}

#[test]
fn ours_fixtures_match_pinned_keysets() {
    let meta = load("ours/collection_meta.json");
    assert_keys(&meta, &["ok", "data"]);
    assert_eq!(meta["ok"], Value::Bool(true));
    assert_keys(&meta["data"], COLLECTION_META_DATA_KEYS);

    let present = load("ours/full_item_video_present.json");
    assert_keys(&present, FULL_ITEM_KEYS_VIDEO_PRESENT);
    assert!(present.get("video").is_some());
    assert_eq!(present["metrics"], serde_json::json!({}));
    assert_eq!(present["mappings"], Value::Null);
    assert_eq!(present["description"], Value::String(String::new()));
    assert_eq!(present["thumbnail"], Value::String(String::new()));

    let absent = load("ours/full_item_video_absent.json");
    assert_keys(&absent, FULL_ITEM_KEYS_VIDEO_ABSENT);
    assert!(
        absent.get("video").is_none(),
        "video key must be omitted, never serialized as null"
    );

    let plain = load("ours/collection_items_plain.json");
    assert_keys(&plain, &["ok", "data"]);
    assert!(plain["data"].is_array());

    let paginated = load("ours/collection_items_paginated.json");
    assert_keys(&paginated, &["ok", "data"]);
    assert_keys(&paginated["data"], PAGINATED_DATA_KEYS);
    assert!(paginated["data"]["results"].is_array());

    let success = load("ours/newsletter_success.json");
    assert_keys(&success, &["ok"]);
    assert_eq!(success["ok"], Value::Bool(true));
    assert!(
        success.get("data").is_none(),
        "newsletter success is a bare {{ok:true}}, never the ApiData envelope"
    );

    let missing = load("ours/newsletter_400_missing.json");
    assert_keys(&missing, &["ok", "error"]);
    assert_eq!(missing["ok"], Value::Bool(false));
    assert_eq!(missing["error"], Value::String("email is required".into()));

    let malformed = load("ours/newsletter_400_malformed.json");
    assert_keys(&malformed, &["ok", "error"]);
    assert_eq!(malformed["ok"], Value::Bool(false));
    assert_eq!(
        malformed["error"],
        Value::String("invalid email address".into())
    );

    for rel in [
        "ours/address_collections_anon_200.json",
        "ours/address_items_anon_200.json",
    ] {
        let anon = load(rel);
        assert_keys(&anon, &["ok", "data"]);
        assert_eq!(anon["ok"], Value::Bool(true));
        assert!(anon["data"].is_array(), "{rel} data must be an array");
    }
}

#[test]
fn divergences_are_documented_and_cite_fixtures() {
    let registry = load("divergences.json");
    let entries = registry.as_array().expect("divergences.json is an array");
    assert_eq!(entries.len(), 2, "exactly the two intentional divergences");

    let routes_md =
        fs::read_to_string(routes_md_path()).unwrap_or_else(|e| panic!("read ROUTES.md: {e}"));

    for entry in entries {
        let ours_rel = entry["ours"].as_str().expect("ours path");
        let upstream_rel = entry["upstream"].as_str().expect("upstream path");
        let callout = entry["routes_md_callout"].as_str().expect("callout");

        let ours = load(ours_rel);
        let upstream = load(upstream_rel);
        assert_ne!(
            ours, upstream,
            "divergence {} must cite differing ours/upstream fixtures",
            entry["route"]
        );
        assert!(
            routes_md.contains(callout),
            "ROUTES.md must carry the callout {callout:?} for {}",
            entry["route"]
        );
        assert!(
            entry["upstream_evidence_date"].is_string(),
            "each divergence records an upstream evidence date"
        );
    }
}

fn base_url() -> Option<String> {
    std::env::var("BUILDER_PARITY_BASE_URL")
        .ok()
        .map(|u| u.trim_end_matches('/').to_string())
}

const ZERO_ADDR: &str = "0x0000000000000000000000000000000000000000";
const ZERO_UUID: &str = "00000000-0000-0000-0000-000000000000";

#[tokio::test]
async fn live_wire_matches_pinned_shapes() {
    let Some(base) = base_url() else {
        eprintln!("BUILDER_PARITY_BASE_URL unset; skipping live parity probe");
        return;
    };
    let client = reqwest::Client::new();

    let missing = client
        .post(format!("{base}/v1/newsletter"))
        .json(&serde_json::json!({}))
        .send()
        .await
        .expect("newsletter empty-body request");
    assert_eq!(missing.status().as_u16(), 400);
    let body: Value = missing.json().await.expect("400 json body");
    assert_keys(&body, &["ok", "error"]);
    assert_eq!(body["error"], Value::String("email is required".into()));

    let malformed = client
        .post(format!("{base}/v1/newsletter"))
        .json(&serde_json::json!({ "email": "notanemail" }))
        .send()
        .await
        .expect("newsletter malformed request");
    assert_eq!(malformed.status().as_u16(), 400);
    let body: Value = malformed.json().await.expect("400 json body");
    assert_keys(&body, &["ok", "error"]);
    assert_eq!(body["error"], Value::String("invalid email address".into()));

    for path in [
        format!("{base}/v1/{ZERO_ADDR}/collections"),
        format!("{base}/v1/{ZERO_ADDR}/items"),
    ] {
        let resp = client.get(&path).send().await.expect("anon address probe");
        assert_eq!(resp.status().as_u16(), 200, "anon 200 expected at {path}");
        let body: Value = resp.json().await.expect("json body");
        assert_keys(&body, &["ok", "data"]);
        assert_eq!(body["ok"], Value::Bool(true));
        assert!(body["data"].is_array());
    }

    let unauth = client
        .get(format!("{base}/v1/collections/{ZERO_UUID}"))
        .send()
        .await
        .expect("anon collection probe");
    assert_eq!(unauth.status().as_u16(), 401);
    let body: Value = unauth.json().await.expect("401 json body");
    assert_eq!(body["ok"], Value::Bool(false));
    assert_eq!(body["error"], Value::String("Unauthorized".into()));
}

#[tokio::test]
async fn upstream_reprobe_matches_captures() {
    if std::env::var("BUILDER_PARITY_UPSTREAM").ok().as_deref() != Some("1") {
        eprintln!("BUILDER_PARITY_UPSTREAM!=1; skipping upstream re-probe");
        return;
    }
    let upstream = "https://builder-api.decentraland.org";
    let client = reqwest::Client::new();

    let resp = client
        .post(format!("{upstream}/v1/newsletter"))
        .json(&serde_json::json!({ "email": "notanemail" }))
        .send()
        .await
        .expect("upstream newsletter probe");
    assert_eq!(resp.status().as_u16(), 200, "upstream accepts anything");
    let body: Value = resp.json().await.expect("json body");
    let pinned = load("upstream/newsletter_notanemail.json");
    assert_eq!(sorted_keys(&body), sorted_keys(&pinned));

    let resp = client
        .get(format!("{upstream}/v1/{ZERO_ADDR}/collections"))
        .send()
        .await
        .expect("upstream anon address probe");
    assert_eq!(
        resp.status().as_u16(),
        401,
        "upstream requires signed-fetch"
    );
    let body: Value = resp.json().await.expect("json body");
    let pinned = load("upstream/address_collections_401.json");
    assert_eq!(sorted_keys(&body), sorted_keys(&pinned));
    assert_eq!(body["error"], pinned["error"]);
}
