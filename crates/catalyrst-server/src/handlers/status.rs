use std::sync::Arc;

use axum::extract::State;
use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::Json;
use serde_json::Value;

use crate::state::AppState;
use crate::wire_types::{ContentStatusResponse, SynchronizationStatus};

pub async fn get_status(State(state): State<Arc<AppState>>) -> impl IntoResponse {
    let sync_state = state.synchronization_state.get_state();

    let mut cluster_extra = match state.content_cluster.get_status() {
        Value::Object(map) => map,
        _ => serde_json::Map::new(),
    };

    let last_sync_with_dao = cluster_extra
        .remove("lastSyncWithDAO")
        .and_then(|v| v.as_i64())
        .unwrap_or_else(|| chrono::Utc::now().timestamp_millis());
    cluster_extra.remove("synchronizationState");
    cluster_extra.remove("syncFrontier");
    cluster_extra.remove("lastHeartbeat");
    cluster_extra.remove("up");

    let (last_heartbeat, up) = match state.synchronization_state.sync_heartbeat_ms() {
        Some(heartbeat) => {
            let age_ms = chrono::Utc::now().timestamp_millis() - heartbeat;
            (Some(heartbeat), Some(age_ms < 300_000))
        }
        None => (None, None),
    };

    let synchronization_status = SynchronizationStatus {
        last_sync_with_dao,
        synchronization_state: sync_state.clone(),
        sync_frontier: state.synchronization_state.sync_frontier_ms(),
        last_heartbeat,
        up,
        cluster_extra,
    };

    let body = ContentStatusResponse {
        version: state.content_version.clone(),
        commit_hash: state.commit_hash.clone(),
        eth_network: state.eth_network.clone(),
        synchronization_status,
    };

    let status = if sync_state == "Failed" {
        StatusCode::SERVICE_UNAVAILABLE
    } else {
        StatusCode::OK
    };

    (status, Json(body))
}
