//! HTTP control + status surface. Port of `src/controllers/routes.ts` and the
//! `ping` / `status` / `debugging/reload` handlers.

use axum::extract::State;
use axum::http::{header, StatusCode};
use axum::response::IntoResponse;
use axum::routing::{get, post};
use axum::{Json, Router};
use chrono::Utc;
use serde::{Deserialize, Serialize};

use crate::loader::load_or_reload;
use crate::state::AppState;

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/ping", get(ping))
        .route("/status", get(status))
        .route("/debugging/reload", post(reload))
}

async fn ping() -> &'static str {
    "/ping"
}

#[derive(Serialize)]
struct StatusResp {
    #[serde(rename = "commitHash")]
    commit_hash: String,
    version: String,
    #[serde(rename = "currentTime")]
    current_time: i64,
    connections: u32,
    #[serde(rename = "loadedScenes")]
    loaded_scenes: Vec<String>,
}

async fn status(State(s): State<AppState>) -> impl IntoResponse {
    let body = Json(StatusResp {
        commit_hash: s.cfg.commit_hash.clone(),
        version: env!("CARGO_PKG_VERSION").to_string(),
        current_time: Utc::now().timestamp_millis(),
        connections: s.scenes.connections(),
        loaded_scenes: s.scenes.loaded(),
    });
    ([(header::ACCESS_CONTROL_ALLOW_ORIGIN, "*")], body)
}

#[derive(Deserialize)]
struct ReloadReq {
    secret: String,
    name: String,
}

/// `POST /debugging/reload` — load or restart a world's scene state.
/// Port of `reloadHandler`: secret-gated, returns 204 on success.
async fn reload(State(s): State<AppState>, body: axum::body::Bytes) -> impl IntoResponse {
    let Some(expected) = s.cfg.debugging_secret.clone() else {
        return (StatusCode::SERVICE_UNAVAILABLE, "DEBUGGING_SECRET not configured").into_response();
    };
    let req: ReloadReq = match serde_json::from_slice(&body) {
        Ok(r) => r,
        Err(e) => return (StatusCode::BAD_REQUEST, format!("bad json: {e}")).into_response(),
    };
    if !constant_time_eq(req.secret.as_bytes(), expected.as_bytes()) {
        return (StatusCode::UNAUTHORIZED, "Not authorized").into_response();
    }
    if req.name.is_empty() {
        return (StatusCode::BAD_REQUEST, "Missing scene name").into_response();
    }
    match load_or_reload(&s, &req.name).await {
        Ok(()) => StatusCode::NO_CONTENT.into_response(),
        Err(e) => (StatusCode::BAD_REQUEST, e.to_string()).into_response(),
    }
}

/// Compare two byte slices in time independent of where (or whether) they
/// differ, so the debugging secret can't be recovered via timing. Folds the
/// length difference into the accumulator so unequal lengths still take the
/// full pass and never early-return. (No `subtle` dependency to keep the
/// offline build self-contained.)
fn constant_time_eq(a: &[u8], b: &[u8]) -> bool {
    // Mix in the length difference so a length mismatch can't be a no-op.
    let mut diff: u8 = (a.len() ^ b.len()) as u8 | ((a.len() ^ b.len()) >> 8) as u8;
    let n = a.len().max(b.len());
    for i in 0..n {
        let x = a.get(i).copied().unwrap_or(0);
        let y = b.get(i).copied().unwrap_or(0);
        diff |= x ^ y;
    }
    diff == 0 && a.len() == b.len()
}

#[cfg(test)]
mod tests {
    use super::constant_time_eq;

    #[test]
    fn ct_eq_matches_semantics() {
        assert!(constant_time_eq(b"hunter2", b"hunter2"));
        assert!(!constant_time_eq(b"hunter2", b"hunter3"));
        assert!(!constant_time_eq(b"hunter2", b"hunter22"));
        assert!(!constant_time_eq(b"", b"x"));
        assert!(constant_time_eq(b"", b""));
    }
}
