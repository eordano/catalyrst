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

/// Only a plain `ws://` signaling URL maps to `http://`; every other scheme,
/// and a bare host (implicitly `wss://`), reports `https://`, as upstream does.
/// What survives is what `URL.host` keeps: no userinfo, path, query or
/// fragment, so a credentialed signaling URL never reaches the public status.
fn livekit_status_url(ws_url: &str) -> String {
    let (scheme, rest) = match ws_url.split_once("://") {
        Some((scheme, rest)) if scheme.eq_ignore_ascii_case("ws") => ("http", rest),
        Some((_, rest)) => ("https", rest),
        None => ("https", ws_url),
    };
    let authority = rest.split(['/', '?', '#']).next().unwrap_or(rest);
    let host = authority.rsplit('@').next().unwrap_or(authority);
    format!("{scheme}://{host}/")
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

#[cfg(test)]
mod tests {
    use super::livekit_status_url;

    #[test]
    fn a_plain_ws_signaling_url_reports_a_plain_http_status_url() {
        assert_eq!(
            livekit_status_url("ws://livekit:7880"),
            "http://livekit:7880/"
        );
    }

    #[test]
    fn a_wss_signaling_url_reports_https() {
        assert_eq!(
            livekit_status_url("wss://lk.example.com"),
            "https://lk.example.com/"
        );
        assert_eq!(
            livekit_status_url("wss://lk.example.com/"),
            "https://lk.example.com/"
        );
    }

    #[test]
    fn a_bare_host_is_implicitly_wss() {
        assert_eq!(
            livekit_status_url("lk.example.com"),
            "https://lk.example.com/"
        );
        assert_eq!(
            livekit_status_url("lk.example.com:7880"),
            "https://lk.example.com:7880/"
        );
    }

    #[test]
    fn only_the_authority_survives_and_only_ws_downgrades() {
        assert_eq!(
            livekit_status_url("wss://lk.example.com/rtc"),
            "https://lk.example.com/"
        );
        assert_eq!(
            livekit_status_url("http://lk.example.com"),
            "https://lk.example.com/"
        );
    }

    #[test]
    fn the_scheme_is_matched_like_a_parsed_url() {
        assert_eq!(
            livekit_status_url("WS://livekit:7880"),
            "http://livekit:7880/"
        );
        assert_eq!(
            livekit_status_url("WSS://lk.example.com"),
            "https://lk.example.com/"
        );
    }

    #[test]
    fn userinfo_query_and_fragment_never_reach_the_status_url() {
        assert_eq!(
            livekit_status_url("wss://user:pw@lk:7880"),
            "https://lk:7880/"
        );
        assert_eq!(
            livekit_status_url("ws://user:p@ss@lk:7880/rtc"),
            "http://lk:7880/"
        );
        assert_eq!(
            livekit_status_url("wss://lk.example.com?x=1"),
            "https://lk.example.com/"
        );
        assert_eq!(
            livekit_status_url("wss://lk.example.com#frag"),
            "https://lk.example.com/"
        );
    }
}
