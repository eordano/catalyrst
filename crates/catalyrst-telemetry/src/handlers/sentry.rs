use axum::body::Bytes;
use axum::extract::{Path, State};
use axum::http::HeaderMap;
use axum::Json;
use serde_json::{json, Value};

use super::{decode_body, ingest::Row};
use crate::AppState;

fn row(project: &str, kind: String, body: Value) -> Row {
    Row {
        source: "sentry",
        project: project.to_string(),
        kind,
        body,
        invalid_reason: None,
    }
}

pub async fn envelope(
    State(state): State<AppState>,
    Path(project): Path<String>,
    headers: HeaderMap,
    body: Bytes,
) -> Json<Value> {
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

    let mut items: Vec<(String, Value)> = Vec::new();
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
                items.push((kind, value));
            }
        }
    }

    if items.is_empty() {
        if state.ingest.admit(&project) {
            state
                .writer
                .push(row(&project, "envelope".into(), json!({ "raw": text })))
                .await;
        }
        return Json(json!({ "id": event_id }));
    }

    let admitted = state.ingest.admit_n(&project, items.len());
    let rows: Vec<Row> = items
        .into_iter()
        .take(admitted)
        .map(|(kind, value)| row(&project, kind, value))
        .collect();
    state.writer.push_all(rows).await;

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
    state
        .writer
        .push(row(&project, "event".into(), event))
        .await;
    Json(json!({ "id": event_id }))
}
