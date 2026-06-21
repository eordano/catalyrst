use std::sync::Arc;

use axum::extract::State;
use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::Json;
use serde_json::{json, Value};

use crate::state::AppState;

pub async fn get_status(State(state): State<Arc<AppState>>) -> impl IntoResponse {
    let sync_state = state.synchronization_state.get_state();
    let cluster_status = state.content_cluster.get_status();

    let mut sync_status = match cluster_status {
        Value::Object(map) => map,
        _ => serde_json::Map::new(),
    };
    sync_status
        .entry("lastSyncWithDAO".to_string())
        .or_insert_with(|| json!(chrono::Utc::now().timestamp_millis()));
    sync_status.insert(
        "synchronizationState".to_string(),
        Value::String(sync_state.clone()),
    );

    let body = json!({
        "version": state.content_version,
        "commitHash": state.commit_hash,
        "ethNetwork": state.eth_network,
        "synchronizationStatus": Value::Object(sync_status),
    });

    let status = if sync_state == "Failed" {
        StatusCode::SERVICE_UNAVAILABLE
    } else {
        StatusCode::OK
    };

    (status, Json(body))
}
