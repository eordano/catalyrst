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
pub struct PersonalWorldsStatus {
    #[cfg_attr(feature = "ts", ts(type = "number"))]
    pub max_worlds: u64,
    #[cfg_attr(feature = "ts", ts(type = "number"))]
    pub max_size_bytes: i64,
}

#[derive(Debug, Serialize, utoipa::ToSchema)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export, export_to = "worlds/"))]
#[serde(rename_all = "camelCase")]
pub struct StatusResponse {
    pub content: ContentStatus,
    pub comms: CommsStatus,
    pub personal_worlds: Option<PersonalWorldsStatus>,
}

/// Only a plain `ws://` signaling URL maps to `http://`; every other scheme,
/// and a bare host (implicitly `wss://`), reports `https://`, as upstream does.
/// What survives is what `URL.host` keeps: no userinfo, path, query or
/// fragment, so a credentialed signaling URL never reaches the public status.
fn livekit_status_url(ws_url: &str) -> String {
    let (scheme, rest) = match ws_url.split_once("://") {
        Some((scheme, rest)) => (scheme.to_ascii_lowercase(), rest),
        None => ("wss".to_string(), ws_url),
    };
    let authority = rest.split(['/', '?', '#']).next().unwrap_or(rest);
    let host = authority.rsplit('@').next().unwrap_or(authority);
    let status_scheme = if scheme == "ws" { "http" } else { "https" };
    format!("{status_scheme}://{}/", url_host(&scheme, host))
}

/// What `URL.host` serializes for a special scheme: the hostname lowercased and the
/// scheme's default port dropped. A scheme with no default port carries an opaque
/// host, which the parser leaves exactly as delivered.
fn url_host(scheme: &str, authority: &str) -> String {
    let default_port: u16 = match scheme {
        "ws" | "http" => 80,
        "wss" | "https" => 443,
        _ => return authority.to_string(),
    };
    let after_ipv6 = authority.rfind(']').map(|end| end + 1).unwrap_or(0);
    let Some(colon) = authority[after_ipv6..].rfind(':').map(|i| after_ipv6 + i) else {
        return authority.to_ascii_lowercase();
    };
    let (host, port) = (&authority[..colon], &authority[colon + 1..]);
    if port.is_empty() {
        return host.to_ascii_lowercase();
    }
    match port.parse::<u16>() {
        Ok(port) if port == default_port => host.to_ascii_lowercase(),
        Ok(port) => format!("{}:{port}", host.to_ascii_lowercase()),
        Err(_) => authority.to_ascii_lowercase(),
    }
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
    let policy = &state.cfg.personal_worlds;
    let personal_worlds = policy.enabled().then_some(PersonalWorldsStatus {
        max_worlds: policy.max_worlds,
        max_size_bytes: policy.max_size_bytes,
    });

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
        personal_worlds,
    }))
}

const DB_PROBE_TTL: std::time::Duration = std::time::Duration::from_secs(1);

/// `SELECT 1` liveness shared by `/health` and `/about`, remembered for one second.
pub(crate) async fn db_ok(pool: &sqlx::PgPool) -> bool {
    static LAST: std::sync::Mutex<Option<(std::time::Instant, bool)>> = std::sync::Mutex::new(None);
    if let Some((at, ok)) = *LAST.lock().unwrap_or_else(|e| e.into_inner()) {
        if at.elapsed() < DB_PROBE_TTL {
            return ok;
        }
    }
    let ok = sqlx::query_scalar::<_, i32>("SELECT 1")
        .fetch_one(pool)
        .await
        .is_ok();
    *LAST.lock().unwrap_or_else(|e| e.into_inner()) = Some((std::time::Instant::now(), ok));
    ok
}

pub async fn health(State(state): State<AppState>) -> impl IntoResponse {
    let db_ok = db_ok(state.worlds.pool()).await;

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

    /// `URL.host` lowercases the hostname and drops the scheme's default port, so two
    /// spellings of the same signaling endpoint publish one status URL.
    #[test]
    fn the_host_is_serialized_the_way_url_host_serializes_it() {
        assert_eq!(
            livekit_status_url("wss://EXAMPLE.com:443"),
            "https://example.com/"
        );
        assert_eq!(livekit_status_url("ws://Host:80"), "http://host/");
        assert_eq!(livekit_status_url("wss://host:7880"), "https://host:7880/");
        assert_eq!(
            livekit_status_url("https://LK.Example.com"),
            "https://lk.example.com/"
        );
        assert_eq!(
            livekit_status_url("LK.Example.com:443"),
            "https://lk.example.com/"
        );
    }

    #[test]
    fn an_ipv6_literal_keeps_its_brackets_and_its_non_default_port() {
        assert_eq!(
            livekit_status_url("wss://[::1]:7880"),
            "https://[::1]:7880/"
        );
        assert_eq!(livekit_status_url("wss://[::1]:443"), "https://[::1]/");
        assert_eq!(livekit_status_url("ws://[::1]"), "http://[::1]/");
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
