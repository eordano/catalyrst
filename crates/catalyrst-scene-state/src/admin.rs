use axum::extract::{Path, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::IntoResponse;
use axum::routing::{get, post};
use axum::{Json, Router};
use serde::Serialize;

use crate::crdt::{decode_batch, CrdtMessage};
use crate::loader::load_or_reload;
use crate::state::AppState;

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/admin/scene/{scene}/crdt", get(inspect_crdt))
        .route("/admin/scene/{scene}/kick-all", post(kick_all))
        .route("/admin/scene/{scene}/reset", post(reset_state))
}

fn bearer_token(headers: &HeaderMap) -> Option<&str> {
    headers
        .get("authorization")
        .and_then(|v| v.to_str().ok())
        .and_then(|s| s.strip_prefix("Bearer "))
}

fn authorize(state: &AppState, headers: &HeaderMap) -> Result<(), axum::response::Response> {
    let forbidden = || (StatusCode::FORBIDDEN, "Not authorized").into_response();
    let Some(expected) = state.cfg.admin_token.as_deref() else {
        return Err(forbidden());
    };
    match bearer_token(headers) {
        Some(token) if timing_safe_eq(token.as_bytes(), expected.as_bytes()) => Ok(()),
        _ => Err(forbidden()),
    }
}

fn timing_safe_eq(a: &[u8], b: &[u8]) -> bool {
    let mut diff: u8 = (a.len() ^ b.len()) as u8 | ((a.len() ^ b.len()) >> 8) as u8;
    let n = a.len().max(b.len());
    for i in 0..n {
        let x = a.get(i).copied().unwrap_or(0);
        let y = b.get(i).copied().unwrap_or(0);
        diff |= x ^ y;
    }
    diff == 0 && a.len() == b.len()
}

#[derive(Serialize)]
struct KickAllResp {
    scene: String,
    kicked: usize,
}

async fn kick_all(
    State(s): State<AppState>,
    headers: HeaderMap,
    Path(scene_name): Path<String>,
) -> axum::response::Response {
    if let Err(resp) = authorize(&s, &headers) {
        return resp;
    }
    let Some(scene) = s.scenes.get(&scene_name) else {
        return (StatusCode::NOT_FOUND, format!("{scene_name} is not loaded")).into_response();
    };
    let kicked = scene.kick_all();
    tracing::info!(scene = %scene_name, kicked, "admin kick-all");
    Json(KickAllResp {
        scene: scene_name,
        kicked,
    })
    .into_response()
}

#[derive(Serialize)]
struct CrdtMsgView {
    #[serde(rename = "type")]
    kind: &'static str,
    entity: u32,
    #[serde(rename = "componentId", skip_serializing_if = "Option::is_none")]
    component_id: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    timestamp: Option<u32>,
    #[serde(rename = "dataLen", skip_serializing_if = "Option::is_none")]
    data_len: Option<usize>,
}

#[derive(Serialize)]
struct InspectResp {
    scene: String,
    hash: String,
    connections: usize,
    #[serde(rename = "snapshotBytes")]
    snapshot_bytes: usize,
    #[serde(rename = "messageCount")]
    message_count: usize,

    #[serde(rename = "snapshotHex")]
    snapshot_hex: String,

    messages: Vec<CrdtMsgView>,
}

fn view(msg: &CrdtMessage) -> CrdtMsgView {
    match msg {
        CrdtMessage::Put {
            entity,
            component_id,
            timestamp,
            data,
        } => CrdtMsgView {
            kind: "PUT_COMPONENT",
            entity: *entity,
            component_id: Some(*component_id),
            timestamp: Some(*timestamp),
            data_len: Some(data.len()),
        },
        CrdtMessage::Append {
            entity,
            component_id,
            timestamp,
            data,
        } => CrdtMsgView {
            kind: "APPEND_VALUE",
            entity: *entity,
            component_id: Some(*component_id),
            timestamp: Some(*timestamp),
            data_len: Some(data.len()),
        },
        CrdtMessage::DeleteComponent {
            entity,
            component_id,
            timestamp,
        } => CrdtMsgView {
            kind: "DELETE_COMPONENT",
            entity: *entity,
            component_id: Some(*component_id),
            timestamp: Some(*timestamp),
            data_len: None,
        },
        CrdtMessage::DeleteEntity { entity } => CrdtMsgView {
            kind: "DELETE_ENTITY",
            entity: *entity,
            component_id: None,
            timestamp: None,
            data_len: None,
        },
    }
}

fn to_hex(bytes: &[u8]) -> String {
    let mut s = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        s.push(char::from_digit((b >> 4) as u32, 16).unwrap());
        s.push(char::from_digit((b & 0xf) as u32, 16).unwrap());
    }
    s
}

async fn inspect_crdt(
    State(s): State<AppState>,
    headers: HeaderMap,
    Path(scene_name): Path<String>,
) -> axum::response::Response {
    if let Err(resp) = authorize(&s, &headers) {
        return resp;
    }
    let Some(scene) = s.scenes.get(&scene_name) else {
        return (StatusCode::NOT_FOUND, format!("{scene_name} is not loaded")).into_response();
    };
    let snapshot = scene.snapshot();
    let messages: Vec<CrdtMsgView> = decode_batch(&snapshot).iter().map(view).collect();
    Json(InspectResp {
        scene: scene_name,
        hash: scene.scene_hash(),
        connections: scene.client_count(),
        snapshot_bytes: snapshot.len(),
        message_count: messages.len(),
        snapshot_hex: to_hex(&snapshot),
        messages,
    })
    .into_response()
}

#[derive(Serialize)]
struct ResetResp {
    scene: String,
    kicked: usize,
    reloaded: bool,
}

async fn reset_state(
    State(s): State<AppState>,
    headers: HeaderMap,
    Path(scene_name): Path<String>,
) -> axum::response::Response {
    if let Err(resp) = authorize(&s, &headers) {
        return resp;
    }
    let Some(scene) = s.scenes.get(&scene_name) else {
        return (StatusCode::NOT_FOUND, format!("{scene_name} is not loaded")).into_response();
    };
    let kicked = scene.kick_all();
    drop(scene);
    match load_or_reload(&s, &scene_name).await {
        Ok(()) => {
            tracing::info!(scene = %scene_name, kicked, "admin reset-state (kick + reload)");
            Json(ResetResp {
                scene: scene_name,
                kicked,
                reloaded: true,
            })
            .into_response()
        }
        Err(e) => (
            StatusCode::SERVICE_UNAVAILABLE,
            format!("kicked {kicked} but reload failed: {e}"),
        )
            .into_response(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn timing_safe_eq_matches_semantics() {
        assert!(timing_safe_eq(b"tok", b"tok"));
        assert!(!timing_safe_eq(b"tok", b"toK"));
        assert!(!timing_safe_eq(b"tok", b"token"));
        assert!(timing_safe_eq(b"", b""));
    }

    fn state_with(token: Option<&str>) -> AppState {
        let mut cfg = test_cfg();
        cfg.admin_token = token.map(|t| t.to_string());
        std::sync::Arc::new(crate::state::AppStateInner {
            cfg,
            scenes: crate::scene::SceneManager::new(),
            http: reqwest::Client::new(),
        })
    }

    fn test_cfg() -> crate::config::Config {
        crate::config::Config {
            http_host: "127.0.0.1".into(),
            http_port: 5153,
            local_scene_path: None,
            world_server_url: None,
            debugging_secret: None,
            admin_token: None,
            http_base_url: None,
            auth_timeout_secs: 5,
            disable_js_runtime: true,
            realm_name: None,
            commit_hash: String::new(),
            js_heap_limit_mb: 384,
            js_tick_budget_ms: 250,
            js_shutdown_join_ms: 2000,
            js_update_failure_cap: 30,
            client_outbound_max: 1024,
            client_inbound_max: 1024,
            crdt_max_components: 100_000,
            ws_max_frame_bytes: 2 * 1024 * 1024,
            fetch_max_body_bytes: crate::config::DEFAULT_FETCH_MAX_BODY_BYTES,
            storage_url: None,
            storage_allow_http: false,
            delegation_minter_url: None,
            delegation_minter_token: None,
            storage_delegation: None,
            signed_fetch_max_response_bytes: 2 * 1024 * 1024,
            signed_fetch_max_body_bytes: 1024 * 1024,
            signed_fetch_max_in_flight: 8,
            signed_fetch_timeout_ms: 10_000,
        }
    }

    fn bearer(tok: &str) -> HeaderMap {
        let mut h = HeaderMap::new();
        h.insert("authorization", format!("Bearer {tok}").parse().unwrap());
        h
    }

    #[test]
    fn unconfigured_token_fails_closed() {
        let s = state_with(None);

        assert!(authorize(&s, &bearer("anything")).is_err());
    }

    #[test]
    fn wrong_and_missing_bearer_rejected() {
        let s = state_with(Some("secret"));
        assert!(authorize(&s, &HeaderMap::new()).is_err());
        assert!(authorize(&s, &bearer("nope")).is_err());
    }

    #[test]
    fn correct_bearer_accepted() {
        let s = state_with(Some("secret"));
        assert!(authorize(&s, &bearer("secret")).is_ok());
    }
}
