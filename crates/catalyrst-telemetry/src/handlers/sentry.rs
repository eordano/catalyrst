use axum::body::Bytes;
use axum::extract::{Path, State};
use axum::http::HeaderMap;
use axum::Json;
use serde_json::{json, Value};

use super::decode_body;
use crate::AppState;

pub async fn envelope(
    State(state): State<AppState>,
    Path(project): Path<String>,
    headers: HeaderMap,
    body: Bytes,
) -> Json<Value> {
    if !state.ingest.admit(&project) {
        return Json(json!({ "id": "" }));
    }
    let raw = decode_body(&headers, body);
    let text = String::from_utf8_lossy(&raw);
    let mut lines = text.split('\n').filter(|l| !l.trim().is_empty());

    let envelope_header: Value = lines
        .next()
        .and_then(|l| serde_json::from_str(l).ok())
        .unwrap_or_else(|| json!({}));
    let event_id = envelope_header
        .get("event_id")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();

    let mut stored = 0usize;
    let mut pending_kind: Option<String> = None;
    for line in lines {
        let value: Value = match serde_json::from_str(line) {
            Ok(v) => v,
            Err(_) => continue,
        };
        match pending_kind.take() {
            None => {
                pending_kind = Some(
                    value
                        .get("type")
                        .and_then(|v| v.as_str())
                        .unwrap_or("unknown")
                        .to_string(),
                );
            }
            Some(kind) => {
                let _ = sqlx::query(
                    "INSERT INTO telemetry_events (source, project, event_kind, body) \
                     VALUES ('sentry', $1, $2, $3)",
                )
                .bind(&project)
                .bind(&kind)
                .bind(&value)
                .execute(&state.pool)
                .await;
                stored += 1;
            }
        }
    }

    if stored == 0 {
        let _ = sqlx::query(
            "INSERT INTO telemetry_events (source, project, event_kind, body) \
             VALUES ('sentry', $1, 'envelope', $2)",
        )
        .bind(&project)
        .bind(json!({ "raw": text }))
        .execute(&state.pool)
        .await;
    }

    Json(json!({ "id": event_id }))
}

pub async fn store(
    State(state): State<AppState>,
    Path(project): Path<String>,
    headers: HeaderMap,
    body: Bytes,
) -> Json<Value> {
    if !state.ingest.admit(&project) {
        return Json(json!({ "id": "" }));
    }
    let raw = decode_body(&headers, body);
    let event: Value = serde_json::from_slice(&raw).unwrap_or_else(|_| json!({}));
    let event_id = event
        .get("event_id")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    let _ = sqlx::query(
        "INSERT INTO telemetry_events (source, project, event_kind, body) \
         VALUES ('sentry', $1, 'event', $2)",
    )
    .bind(&project)
    .bind(&event)
    .execute(&state.pool)
    .await;
    Json(json!({ "id": event_id }))
}
