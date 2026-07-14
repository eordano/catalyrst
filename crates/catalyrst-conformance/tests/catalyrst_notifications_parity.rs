use std::path::PathBuf;

use axum::response::IntoResponse;
use catalyrst_notifications::http::ApiError;
use catalyrst_notifications::ports::{
    BroadcastResponse, CommunityOptOutStatus, MarkReadResponse, NotificationItem,
    NotificationsListResponse, OptOutResponse, Subscription,
};
use serde_json::{json, Value};
use uuid::Uuid;

fn fixture(name: &str) -> Value {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("fixtures")
        .join("notifications")
        .join(name);
    let text = std::fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("reading {}: {e}", path.display()));
    serde_json::from_str(&text).unwrap_or_else(|e| panic!("parsing {}: {e}", path.display()))
}

fn binding(name: &str) -> String {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../catalyrst-notifications/bindings/notifications")
        .join(name);
    std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("reading {}: {e}", path.display()))
}

fn sample_item() -> NotificationItem {
    NotificationItem {
        id: Uuid::parse_str("00000000-0000-0000-0000-000000000001").unwrap(),
        kind: "badge_granted".to_string(),
        address: "0x0000000000000000000000000000000000dead".to_string(),
        timestamp: 1699999999999,
        read: false,
        created_at: "2023-11-14T22:13:19.999Z".to_string(),
        updated_at: "2023-11-14T22:13:19.999Z".to_string(),
        metadata: json!({ "badgeId": "welcome" }),
    }
}

#[test]
fn notifications_list_response_matches_fixture() {
    let resp = NotificationsListResponse {
        notifications: vec![sample_item()],
    };
    assert_eq!(
        serde_json::to_value(&resp).unwrap(),
        fixture("list-response.json")
    );
}

#[test]
fn notification_item_timestamp_serializes_as_string() {
    let value = serde_json::to_value(sample_item()).unwrap();
    assert!(
        value["timestamp"].is_string(),
        "timestamp must serialize as a decimal string, got {:?}",
        value["timestamp"]
    );
    assert_eq!(
        value["timestamp"],
        Value::String("1699999999999".to_string())
    );
}

#[test]
fn mark_read_response_matches_fixture() {
    let resp = MarkReadResponse { updated: 3 };
    assert_eq!(
        serde_json::to_value(&resp).unwrap(),
        fixture("mark-read.json")
    );
}

#[test]
fn community_opt_out_status_matches_fixture() {
    let resp = CommunityOptOutStatus {
        scope: "community".to_string(),
        scope_id: "00000000-0000-0000-0000-000000000002".to_string(),
        opted_out: true,
    };
    assert_eq!(
        serde_json::to_value(&resp).unwrap(),
        fixture("community-opt-out.json")
    );
}

#[tokio::test]
async fn opt_out_response_matches_created_fixture() {
    let resp = (
        axum::http::StatusCode::CREATED,
        axum::Json(OptOutResponse { ok: true }),
    )
        .into_response();
    assert_eq!(resp.status(), axum::http::StatusCode::CREATED);
    let bytes = axum::body::to_bytes(resp.into_body(), 1024).await.unwrap();
    let body: Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(body, fixture("opt-out-created.json"));
}

#[tokio::test]
async fn broadcast_response_matches_fixture() {
    let resp = (
        axum::http::StatusCode::OK,
        axum::Json(BroadcastResponse {
            ok: true,
            broadcast_id: "00000000-0000-0000-0000-000000000003".to_string(),
            kind: "badge_granted".to_string(),
            recipients: 42,
        }),
    )
        .into_response();
    assert_eq!(resp.status(), axum::http::StatusCode::OK);
    let bytes = axum::body::to_bytes(resp.into_body(), 1024).await.unwrap();
    let body: Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(body, fixture("broadcast.json"));
}

#[test]
fn subscription_matches_fixture() {
    let resp = Subscription {
        address: "0x0000000000000000000000000000000000dead".to_string(),
        email: Some("wallet@example.com".to_string()),
        unconfirmed_email: Some("pending@example.com".to_string()),
        details: json!({ "ignore_all_email": false, "ignore_all_in_app": false }),
    };
    assert_eq!(
        serde_json::to_value(&resp).unwrap(),
        fixture("subscription.json")
    );
}

#[tokio::test]
async fn unauthorized_error_envelope_matches_interconnected_baseline() {
    let resp = ApiError::unauthorized("Invalid Auth Chain").into_response();
    assert_eq!(resp.status(), axum::http::StatusCode::UNAUTHORIZED);
    let bytes = axum::body::to_bytes(resp.into_body(), 1024).await.unwrap();
    let body: Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(body, fixture("auth-error.json"));
}

#[test]
fn notification_item_binding_pins_string_timestamp() {
    let ts = binding("NotificationItem.ts");
    assert!(
        ts.contains("timestamp: string"),
        "NotificationItem.ts must declare timestamp: string (regen bindings after any Rust type change): {ts}"
    );
}

#[test]
fn every_new_response_struct_has_an_exported_binding() {
    for name in [
        "NotificationItem.ts",
        "NotificationsListResponse.ts",
        "MarkReadResponse.ts",
        "OptOutResponse.ts",
        "CommunityOptOutStatus.ts",
        "BroadcastResponse.ts",
    ] {
        let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../catalyrst-notifications/bindings/notifications")
            .join(name);
        assert!(
            path.is_file(),
            "missing ts-rs binding {name}; run the notifications binding regen"
        );
    }
}

fn live_probe_enabled() -> bool {
    std::env::var("NOTIFICATIONS_PARITY_LIVE").ok().as_deref() == Some("1")
}

fn sorted_keys(v: &Value) -> Vec<String> {
    let mut keys: Vec<String> = v
        .as_object()
        .map(|m| m.keys().cloned().collect())
        .unwrap_or_default();
    keys.sort();
    keys
}

#[tokio::test]
async fn live_upstream_auth_errors_match_pinned_fixtures() {
    if !live_probe_enabled() {
        eprintln!("NOTIFICATIONS_PARITY_LIVE!=1; skipping live upstream probe");
        return;
    }
    let client = reqwest::Client::new();

    let resp = client
        .get("https://notifications.interconnected.online/notifications")
        .timeout(std::time::Duration::from_secs(8))
        .send()
        .await
        .expect("interconnected notifications probe");
    assert_eq!(resp.status().as_u16(), 401);
    let body: Value = resp.json().await.expect("interconnected json body");
    let pinned = fixture("auth-error.json");
    assert_eq!(sorted_keys(&body), sorted_keys(&pinned));
    assert_eq!(body["error"], pinned["error"]);

    let resp = client
        .get("https://notifications.decentraland.org/notifications")
        .timeout(std::time::Duration::from_secs(8))
        .send()
        .await
        .expect("production notifications probe");
    assert_eq!(resp.status().as_u16(), 400);
    let body: Value = resp.json().await.expect("production json body");
    let pinned = fixture("auth-error-production.json");
    assert_eq!(sorted_keys(&body), sorted_keys(&pinned));
    assert_eq!(body["error"], pinned["error"]);
}
