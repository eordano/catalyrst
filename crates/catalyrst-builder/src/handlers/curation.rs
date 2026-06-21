//! Admin item-curation routes, gated by a bearer admin token (not a signed
//! auth-chain) so the admin console can proxy them. `Authorization: Bearer
//! <token>` is compared in constant time against `CATALYRST_BUILDER_ADMIN_TOKEN`
//! and fails closed (403) when that env is unset.

use axum::extract::{Path, State};
use axum::http::HeaderMap;
use axum::Json;
use serde::Deserialize;
use serde_json::{json, Value};
use uuid::Uuid;

use crate::http::errors::ApiError;
use crate::http::response::ApiData;
use crate::AppState;

const CURATION_STATUSES: [&str; 3] = ["pending", "approved", "rejected"];
const MAX_BULK_ITEMS: usize = 1000;

fn bearer_token(headers: &HeaderMap) -> Option<String> {
    headers
        .get("authorization")
        .and_then(|v| v.to_str().ok())
        .and_then(|s| s.strip_prefix("Bearer "))
        .map(|s| s.to_string())
}

fn timing_safe_eq(a: &str, b: &str) -> bool {
    if a.len() != b.len() {
        return false;
    }
    let mut diff = 0u8;
    for (x, y) in a.bytes().zip(b.bytes()) {
        diff |= x ^ y;
    }
    diff == 0
}

fn authorize_admin(state: &AppState, headers: &HeaderMap) -> Result<(), ApiError> {
    let expected = state
        .admin_token
        .as_deref()
        .ok_or_else(|| ApiError::forbidden("Admin token not configured"))?;
    let token = bearer_token(headers)
        .ok_or_else(|| ApiError::forbidden("Missing or invalid bearer token"))?;
    if timing_safe_eq(&token, expected) {
        Ok(())
    } else {
        Err(ApiError::forbidden("Missing or invalid bearer token"))
    }
}

fn validate_status(status: &str) -> Result<(), ApiError> {
    if CURATION_STATUSES.contains(&status) {
        Ok(())
    } else {
        Err(ApiError::bad_request_with(
            "Invalid Status provided",
            json!({ "status": status, "allowed": CURATION_STATUSES }),
        ))
    }
}

fn parse_uuid(raw: &str) -> Result<Uuid, ApiError> {
    Uuid::parse_str(raw.trim())
        .map_err(|_| ApiError::not_found_with("Not found", json!({ "id": raw })))
}

#[derive(Debug, Deserialize)]
pub struct ItemStatusBody {
    pub status: String,
}

pub async fn patch_item_status(
    State(state): State<AppState>,
    Path((id, item)): Path<(String, String)>,
    headers: HeaderMap,
    Json(body): Json<ItemStatusBody>,
) -> Result<Json<ApiData<Value>>, ApiError> {
    authorize_admin(&state, &headers)?;
    validate_status(&body.status)?;

    let collection_id = parse_uuid(&id)?;
    let item_id = parse_uuid(&item)?;

    let updated = state
        .items
        .set_item_curation_status(&collection_id, &item_id, &body.status)
        .await?;

    if updated == 0 {
        return Err(ApiError::not_found_with(
            "Not found",
            json!({ "id": id, "item": item }),
        ));
    }

    Ok(Json(ApiData::ok(json!({
        "id": item,
        "collection_id": id,
        "status": body.status,
        "updated": updated,
    }))))
}

#[derive(Debug, Deserialize)]
pub struct BulkItemStatusBody {
    pub status: String,
    #[serde(rename = "itemIds", alias = "item_ids")]
    pub item_ids: Vec<String>,
}

pub async fn patch_items_status_bulk(
    State(state): State<AppState>,
    Path(id): Path<String>,
    headers: HeaderMap,
    Json(body): Json<BulkItemStatusBody>,
) -> Result<Json<ApiData<Value>>, ApiError> {
    authorize_admin(&state, &headers)?;
    validate_status(&body.status)?;

    if body.item_ids.is_empty() {
        return Err(ApiError::bad_request("itemIds must not be empty"));
    }
    if body.item_ids.len() > MAX_BULK_ITEMS {
        return Err(ApiError::bad_request_with(
            "Too many items in a single request",
            json!({ "max": MAX_BULK_ITEMS, "got": body.item_ids.len() }),
        ));
    }

    let collection_id = parse_uuid(&id)?;
    let mut item_ids = Vec::with_capacity(body.item_ids.len());
    for raw in &body.item_ids {
        item_ids.push(parse_uuid(raw)?);
    }

    let updated = state
        .items
        .set_items_curation_status(&collection_id, &item_ids, &body.status)
        .await?;

    Ok(Json(ApiData::ok(json!({
        "collection_id": id,
        "status": body.status,
        "requested": body.item_ids.len(),
        "updated": updated,
    }))))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn headers_with(auth: Option<&str>) -> HeaderMap {
        let mut h = HeaderMap::new();
        if let Some(a) = auth {
            h.insert("authorization", a.parse().unwrap());
        }
        h
    }

    #[test]
    fn timing_safe_eq_matches_and_mismatches() {
        assert!(timing_safe_eq("secret", "secret"));
        assert!(!timing_safe_eq("secret", "secrxt"));
        assert!(!timing_safe_eq("secret", "secre"));
        assert!(!timing_safe_eq("", "x"));
    }

    #[test]
    fn bearer_token_parsing() {
        assert_eq!(
            bearer_token(&headers_with(Some("Bearer abc"))),
            Some("abc".to_string())
        );
        assert_eq!(bearer_token(&headers_with(Some("Basic abc"))), None);
        assert_eq!(bearer_token(&headers_with(None)), None);
    }

    #[test]
    fn validate_status_accepts_known_and_rejects_unknown() {
        assert!(validate_status("approved").is_ok());
        assert!(validate_status("rejected").is_ok());
        assert!(validate_status("pending").is_ok());
        assert!(validate_status("bogus").is_err());
    }

    #[test]
    fn missing_or_wrong_token_never_matches_expected() {
        let expected = "the-real-token";
        assert!(bearer_token(&headers_with(None)).is_none());
        let got = bearer_token(&headers_with(Some("Bearer nope"))).unwrap();
        assert!(!timing_safe_eq(&got, expected));
    }
}
