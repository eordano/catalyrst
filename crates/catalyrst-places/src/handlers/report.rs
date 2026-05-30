use axum::extract::{OriginalUri, Path, State};
use axum::http::{HeaderMap, Method};
use axum::Json;
use serde_json::{json, Value};

use crate::auth::auth_address_verified;
use crate::http::errors::ApiError;
use crate::AppState;

fn request_base_url(headers: &HeaderMap) -> String {
    let host = headers
        .get("x-forwarded-host")
        .or_else(|| headers.get("host"))
        .and_then(|v| v.to_str().ok())
        .map(|s| s.to_string())
        .unwrap_or_else(|| "127.0.0.1:5134".to_string());
    let scheme = headers
        .get("x-forwarded-proto")
        .and_then(|v| v.to_str().ok())
        .map(|s| s.to_string())
        .unwrap_or_else(|| {
            if host.starts_with("127.0.0.1") || host.starts_with("localhost") {
                "http".to_string()
            } else {
                "https".to_string()
            }
        });
    format!("{}://{}", scheme, host)
}

fn is_federation_envelope(body: &Option<Json<Value>>) -> bool {
    body.as_ref()
        .and_then(|Json(v)| v.as_object())
        .map(|o| o.contains_key("domain") && o.contains_key("message") && o.contains_key("signature"))
        .unwrap_or(false)
}

pub async fn post_report(
    State(state): State<AppState>,
    method: Method,
    OriginalUri(uri): OriginalUri,
    headers: HeaderMap,
    body: Option<Json<Value>>,
) -> Result<Json<Value>, ApiError> {
    // Signed federation report (places.md §3): Signed<PlaceReport> envelope ->
    // verify -> replay -> log -> gossip. Advisory-only per ADR.
    if is_federation_envelope(&body) {
        return crate::handlers::federation::fed_post_report(&state, &headers, &body).await;
    }
    let user = auth_address_verified(&headers, method.as_str(), uri.path())?;
    let payload = body.map(|Json(v)| v).unwrap_or_else(|| json!({}));
    let entity_id = payload
        .get("entity_id")
        .and_then(|v| v.as_str())
        .or_else(|| payload.get("place_id").and_then(|v| v.as_str()))
        .map(|s| s.to_string());

    let now = chrono::Utc::now().timestamp();
    let time_hash = format!("{:x}", now);
    let user_hash = user.chars().rev().take(8).collect::<String>();
    let user_hash: String = user_hash.chars().rev().collect();
    let filename = format!("{}{}.json", user_hash, time_hash);
    let signed_url = format!("{}/api/report/upload/{}", request_base_url(&headers), filename);

    state
        .places
        .record_report(entity_id.as_deref(), &user, &signed_url, &filename, &payload)
        .await?;

    Ok(Json(json!({ "ok": true, "data": { "signed_url": signed_url } })))
}

pub async fn put_report_upload(
    State(state): State<AppState>,
    Path(filename): Path<String>,
    body: Option<Json<Value>>,
) -> Result<Json<Value>, ApiError> {
    let payload = body.map(|Json(v)| v).unwrap_or_else(|| json!({}));
    state
        .places
        .record_report_upload(&filename, &payload)
        .await?;
    Ok(Json(json!({ "ok": true })))
}
