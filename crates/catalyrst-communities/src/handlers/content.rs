use axum::body::Bytes;
use axum::extract::{Path, State};
use axum::http::{header, HeaderMap, HeaderValue, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::Json;
use serde_json::json;
use std::collections::HashSet;

use crate::content_store::{ContentError, MAX_BODY_BYTES};
use crate::AppState;

fn err(code: StatusCode, message: impl Into<String>) -> (StatusCode, Json<serde_json::Value>) {
    (
        code,
        Json(json!({ "ok": false, "message": message.into() })),
    )
}

fn map_put_err(e: ContentError) -> (StatusCode, Json<serde_json::Value>) {
    match e {
        ContentError::TooLarge { max } => err(
            StatusCode::PAYLOAD_TOO_LARGE,
            format!("body exceeds {} bytes", max),
        ),
        ContentError::HashMismatch { expected, actual } => err(
            StatusCode::BAD_REQUEST,
            format!("hash mismatch: expected {}, got {}", expected, actual),
        ),
        ContentError::InvalidHash(h) => err(
            StatusCode::BAD_REQUEST,
            format!("invalid content hash: {}", h),
        ),
        ContentError::Io(io_err) => {
            tracing::error!(error = %io_err, "content store io error");
            err(StatusCode::INTERNAL_SERVER_ERROR, "io error")
        }
    }
}

pub async fn put_content(
    State(state): State<AppState>,
    body: Bytes,
) -> (StatusCode, Json<serde_json::Value>) {
    if body.len() > MAX_BODY_BYTES {
        return err(
            StatusCode::PAYLOAD_TOO_LARGE,
            format!("body exceeds {} bytes", MAX_BODY_BYTES),
        );
    }
    match state.content_store.put(&body).await {
        Ok(hash) => (
            StatusCode::OK,
            Json(json!({
                "ok": true,
                "content_hash": hash,
                "size": body.len(),
            })),
        ),
        Err(e) => map_put_err(e),
    }
}

pub async fn get_content(State(state): State<AppState>, Path(hash): Path<String>) -> Response {
    let hash_lc = hash.to_ascii_lowercase();
    match state.content_store.get(&hash_lc).await {
        Ok(Some(bytes)) => {
            let mut headers = HeaderMap::new();
            headers.insert(
                header::CONTENT_TYPE,
                HeaderValue::from_static("application/octet-stream"),
            );
            if let Ok(v) = HeaderValue::from_str(&bytes.len().to_string()) {
                headers.insert(header::CONTENT_LENGTH, v);
            }
            if let Ok(v) = HeaderValue::from_str(&hash_lc) {
                headers.insert("x-content-hash", v);
            }
            (StatusCode::OK, headers, bytes).into_response()
        }
        Ok(None) => err(StatusCode::NOT_FOUND, "content not found").into_response(),
        Err(e) => map_put_err(e).into_response(),
    }
}

pub async fn gc_content(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> (StatusCode, Json<serde_json::Value>) {
    let bearer = headers
        .get(header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .and_then(|s| s.strip_prefix("Bearer "));
    match (&state.admin_token, bearer) {
        (Some(expected), Some(got)) if expected == got => (),
        _ => return err(StatusCode::UNAUTHORIZED, "admin bearer token required"),
    }

    let rows: Vec<(String,)> = match sqlx::query_as(
        "SELECT content_hash FROM community_posts_log WHERE content_hash IS NOT NULL",
    )
    .fetch_all(&state.pool)
    .await
    {
        Ok(r) => r,
        Err(e) => {
            tracing::error!(error = %e, "gc: failed to enumerate referenced hashes");
            return err(StatusCode::INTERNAL_SERVER_ERROR, "db error");
        }
    };
    let mut referenced: HashSet<String> = HashSet::with_capacity(rows.len());
    for (h,) in rows {
        let h = h.to_ascii_lowercase();
        if crate::content_store::is_valid_hash(&h) {
            referenced.insert(h);
        }
    }

    match state.content_store.gc(&referenced).await {
        Ok(stats) => (
            StatusCode::OK,
            Json(json!({
                "ok": true,
                "scanned": stats.scanned,
                "kept": stats.kept,
                "removed": stats.removed,
                "referenced": referenced.len(),
            })),
        ),
        Err(e) => map_put_err(e),
    }
}
