use axum::body::Bytes;
use axum::extract::State;
use axum::http::HeaderMap;
use axum::Json;
use serde_json::{json, Value};

use super::decode_body;
use crate::AppState;

fn base64_decode(input: &str) -> Option<Vec<u8>> {
    fn sextet(c: u8) -> Option<u32> {
        match c {
            b'A'..=b'Z' => Some((c - b'A') as u32),
            b'a'..=b'z' => Some((c - b'a' + 26) as u32),
            b'0'..=b'9' => Some((c - b'0' + 52) as u32),
            b'+' => Some(62),
            b'/' => Some(63),
            _ => None,
        }
    }
    let mut out = Vec::new();
    let mut acc = 0u32;
    let mut bits = 0u32;
    for &c in input.as_bytes() {
        if c == b'=' || c == b'\r' || c == b'\n' || c == b' ' || c == b'\t' {
            continue;
        }
        acc = (acc << 6) | sextet(c)?;
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            out.push((acc >> bits) as u8);
        }
    }
    Some(out)
}

fn write_key(headers: &HeaderMap, body: &Value) -> String {
    if let Some(auth) = headers
        .get(axum::http::header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
    {
        let cred = auth.trim_start_matches("Basic ").trim();
        if let Some(decoded) = base64_decode(cred) {
            if let Ok(text) = String::from_utf8(decoded) {
                let user = text.split(':').next().unwrap_or("");
                if !user.is_empty() {
                    return user.to_string();
                }
            }
        }
        return cred.to_string();
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

    if let Some(batch) = payload.get("batch").and_then(|v| v.as_array()) {
        let admitted = state.ingest.admit_n(&key, batch.len());
        for event in batch.iter().take(admitted) {
            store_event(&state, &key, event).await;
        }
    } else if state.ingest.admit(&key) {
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
