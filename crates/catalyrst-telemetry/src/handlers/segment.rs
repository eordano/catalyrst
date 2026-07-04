use axum::body::Bytes;
use axum::extract::State;
use axum::http::HeaderMap;
use axum::Json;
use serde_json::{json, Value};

use super::decode_body;
use crate::AppState;

fn write_key(headers: &HeaderMap, body: &Value) -> String {
    if let Some(auth) = headers
        .get(axum::http::header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
    {
        return auth.trim_start_matches("Basic ").chars().take(32).collect();
    }
    body.get("writeKey")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string()
}

async fn store_event(state: &AppState, key: &str, event: &Value) {
    let kind = event
        .get("type")
        .and_then(|v| v.as_str())
        .unwrap_or("track")
        .to_string();
    let _ = sqlx::query(
        "INSERT INTO telemetry_events (source, project, event_kind, body) \
         VALUES ('segment', $1, $2, $3)",
    )
    .bind(key)
    .bind(&kind)
    .bind(event)
    .execute(&state.pool)
    .await;
}

pub async fn batch(State(state): State<AppState>, headers: HeaderMap, body: Bytes) -> Json<Value> {
    let raw = decode_body(&headers, body);
    let payload: Value = serde_json::from_slice(&raw).unwrap_or_else(|_| json!({}));
    let key = write_key(&headers, &payload);

    if !state.ingest.admit(&key) {
        return Json(json!({ "success": true }));
    }
    if let Some(batch) = payload.get("batch").and_then(|v| v.as_array()) {
        for event in batch {
            store_event(&state, &key, event).await;
        }
    } else {
        store_event(&state, &key, &payload).await;
    }
    Json(json!({ "success": true }))
}

pub async fn single(State(state): State<AppState>, headers: HeaderMap, body: Bytes) -> Json<Value> {
    let raw = decode_body(&headers, body);
    let payload: Value = serde_json::from_slice(&raw).unwrap_or_else(|_| json!({}));
    let key = write_key(&headers, &payload);
    if !state.ingest.admit(&key) {
        return Json(json!({ "success": true }));
    }
    store_event(&state, &key, &payload).await;
    Json(json!({ "success": true }))
}
