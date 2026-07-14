use axum::extract::{OriginalUri, State};
use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::Json;
use chrono::Utc;
use serde::Serialize;
use serde_json::json;

use crate::http::ApiError;
use crate::AppState;

pub async fn ping(OriginalUri(uri): OriginalUri) -> impl IntoResponse {
    uri.path().to_string()
}

#[derive(Debug, Serialize, utoipa::ToSchema)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export, export_to = "worlds/"))]
#[serde(rename_all = "camelCase")]
pub struct WorldsCount {
    #[cfg_attr(feature = "ts", ts(type = "number"))]
    pub ens: i64,
    #[cfg_attr(feature = "ts", ts(type = "number"))]
    pub dcl: i64,
}

#[derive(Debug, Serialize, utoipa::ToSchema)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export, export_to = "worlds/"))]
#[serde(rename_all = "camelCase")]
pub struct ContentStatus {
    pub commit_hash: String,
    pub worlds_count: WorldsCount,
}

#[derive(Debug, Serialize, utoipa::ToSchema)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export, export_to = "worlds/"))]
#[serde(rename_all = "camelCase")]
pub struct CommsStatus {
    pub adapter_type: String,
    pub status_url: String,
    #[cfg_attr(feature = "ts", ts(type = "number"))]
    pub rooms: i64,
    #[cfg_attr(feature = "ts", ts(type = "number"))]
    pub users: i64,
    #[cfg_attr(feature = "ts", ts(type = "number"))]
    pub timestamp: i64,
}

#[derive(Debug, Serialize, utoipa::ToSchema)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export, export_to = "worlds/"))]
#[serde(rename_all = "camelCase")]
pub struct StatusResponse {
    pub content: ContentStatus,
    pub comms: CommsStatus,
}

fn livekit_status_url(ws_url: &str) -> String {
    let host = ws_url
        .strip_prefix("wss://")
        .or_else(|| ws_url.strip_prefix("ws://"))
        .unwrap_or(ws_url);
    format!("https://{}/", host.trim_end_matches('/'))
}

#[utoipa::path(
    get,
    path = "/status",
    tag = "status",
    responses((status = 200, body = StatusResponse))
)]
pub async fn status(State(state): State<AppState>) -> Result<Json<StatusResponse>, ApiError> {
    let worlds_count = state.worlds.get_deployed_world_count().await?;
    let comms = state.presence.comms_stats();

    Ok(Json(StatusResponse {
        content: ContentStatus {
            commit_hash: option_env!("GIT_REV").unwrap_or("unknown").to_string(),
            worlds_count: WorldsCount {
                ens: worlds_count.ens,
                dcl: worlds_count.dcl,
            },
        },
        comms: CommsStatus {
            adapter_type: "livekit".to_string(),
            status_url: livekit_status_url(&state.cfg.livekit_ws_url),
            rooms: comms.rooms,
            users: comms.users,
            timestamp: Utc::now().timestamp_millis(),
        },
    }))
}

pub async fn health(State(state): State<AppState>) -> impl IntoResponse {
    let db_ok = sqlx::query_scalar::<_, i32>("SELECT 1")
        .fetch_one(state.worlds.pool())
        .await
        .is_ok();

    let body = json!({
        "ok": db_ok,
        "version": env!("CARGO_PKG_VERSION"),
        "components": {
            "database": if db_ok { "healthy" } else { "unavailable" },
            "livekit": if state.cfg.livekit_configured { "configured" } else { "unconfigured" },
        },
    });

    let code = if db_ok {
        StatusCode::OK
    } else {
        StatusCode::SERVICE_UNAVAILABLE
    };
    (code, Json(body))
}
