use axum::extract::{OriginalUri, Path, State};
use axum::http::HeaderMap;
use axum::Json;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::auth_chain::require_signer;
use crate::http::errors::ApiError;
use crate::http::response::ApiData;
use crate::ports::marketplace::{CommitteeMemberOut, ReviewRowOut};
use crate::AppState;

/// `data` payload of `GET /v1/collections/curation`. This struct replaces a
/// `json!()` payload; the `curation_wire_bytes_match_the_old_json_macro` test
/// asserts it carries the same wire shape, compared as parsed JSON so object
/// key order (which flips with serde_json's preserve_order feature) is not
/// part of the contract.
#[derive(Debug, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export, export_to = "builder/"))]
pub struct CurationCollectionsOut {
    pub collections: Vec<ReviewRowOut>,
    pub committee: Vec<CommitteeMemberOut>,
}

/// `data` payload of `PATCH /v1/collections/{id}/items/{item}/status`.
#[derive(Debug, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export, export_to = "builder/"))]
pub struct ItemStatusPatchOut {
    pub collection_id: String,
    pub id: String,
    pub status: String,
    #[cfg_attr(feature = "ts", ts(type = "number"))]
    pub updated: u64,
}

/// `data` payload of `PATCH /v1/collections/{id}/items/status`.
#[derive(Debug, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export, export_to = "builder/"))]
pub struct BulkItemStatusPatchOut {
    pub collection_id: String,
    #[cfg_attr(feature = "ts", ts(type = "number"))]
    pub requested: u64,
    pub status: String,
    #[cfg_attr(feature = "ts", ts(type = "number"))]
    pub updated: u64,
}

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

pub async fn authorize_admin(
    admin_token: Option<&str>,
    admin_addresses: &[String],
    headers: &HeaderMap,
    method: &str,
    path: &str,
) -> Result<(), ApiError> {
    if let (Some(expected), Some(token)) = (admin_token, bearer_token(headers)) {
        if !expected.is_empty() && timing_safe_eq(&token, expected) {
            return Ok(());
        }
    }
    if let Ok(signer) = require_signer(headers, method, path).await {
        if admin_addresses.iter().any(|a| a == &signer) {
            return Ok(());
        }
    }
    Err(ApiError::forbidden(
        "Not authorized: curation requires the admin token or a committee-address signed request",
    ))
}

fn validate_status(status: &str) -> Result<(), ApiError> {
    if CURATION_STATUSES.contains(&status) {
        Ok(())
    } else {
        Err(ApiError::bad_request("Invalid Status provided"))
    }
}

fn parse_uuid(raw: &str) -> Result<Uuid, ApiError> {
    Uuid::parse_str(raw.trim()).map_err(|_| ApiError::not_found("Not found"))
}

pub async fn get_curation_collections(
    State(state): State<AppState>,
    OriginalUri(uri): OriginalUri,
    headers: HeaderMap,
) -> Result<Json<ApiData<CurationCollectionsOut>>, ApiError> {
    authorize_admin(
        state.admin_token.as_deref(),
        &state.admin_addresses,
        &headers,
        "get",
        uri.path(),
    )
    .await?;

    let (committee, collections) = match &state.marketplace {
        Some(mp) => {
            let committee = mp.committee_members().await?;
            let collections = mp.collections_under_review().await?;
            (committee, collections)
        }
        None => (Vec::new(), Vec::new()),
    };

    Ok(Json(ApiData::ok(CurationCollectionsOut {
        collections,
        committee,
    })))
}

#[derive(Debug, Deserialize)]
pub struct ItemStatusBody {
    pub status: String,
}

pub async fn patch_item_status(
    State(state): State<AppState>,
    Path((id, item)): Path<(String, String)>,
    OriginalUri(uri): OriginalUri,
    headers: HeaderMap,
    Json(body): Json<ItemStatusBody>,
) -> Result<Json<ApiData<ItemStatusPatchOut>>, ApiError> {
    authorize_admin(
        state.admin_token.as_deref(),
        &state.admin_addresses,
        &headers,
        "patch",
        uri.path(),
    )
    .await?;
    validate_status(&body.status)?;

    let collection_id = parse_uuid(&id)?;
    let item_id = parse_uuid(&item)?;

    let updated = state
        .items
        .set_item_curation_status(&collection_id, &item_id, &body.status)
        .await?;

    if updated == 0 {
        return Err(ApiError::not_found("Not found"));
    }

    Ok(Json(ApiData::ok(ItemStatusPatchOut {
        collection_id: id,
        id: item,
        status: body.status,
        updated,
    })))
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
    OriginalUri(uri): OriginalUri,
    headers: HeaderMap,
    Json(body): Json<BulkItemStatusBody>,
) -> Result<Json<ApiData<BulkItemStatusPatchOut>>, ApiError> {
    authorize_admin(
        state.admin_token.as_deref(),
        &state.admin_addresses,
        &headers,
        "patch",
        uri.path(),
    )
    .await?;
    validate_status(&body.status)?;

    if body.item_ids.is_empty() {
        return Err(ApiError::bad_request("itemIds must not be empty"));
    }
    if body.item_ids.len() > MAX_BULK_ITEMS {
        return Err(ApiError::bad_request("Too many items in a single request"));
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

    Ok(Json(ApiData::ok(BulkItemStatusPatchOut {
        collection_id: id,
        requested: body.item_ids.len() as u64,
        status: body.status,
        updated,
    })))
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

    const NO_ADMINS: &[String] = &[];

    #[tokio::test]
    async fn authorize_admin_accepts_the_configured_bearer() {
        let h = headers_with(Some("Bearer the-real-token"));
        assert!(
            authorize_admin(Some("the-real-token"), NO_ADMINS, &h, "get", "/x")
                .await
                .is_ok()
        );
    }

    #[tokio::test]
    async fn authorize_admin_rejects_a_wrong_bearer() {
        let h = headers_with(Some("Bearer wrong"));
        assert!(
            authorize_admin(Some("the-real-token"), NO_ADMINS, &h, "get", "/x")
                .await
                .is_err()
        );
    }

    #[tokio::test]
    async fn authorize_admin_rejects_when_no_credentials_present() {
        let h = headers_with(None);
        assert!(
            authorize_admin(Some("the-real-token"), NO_ADMINS, &h, "get", "/x")
                .await
                .is_err()
        );
        assert!(authorize_admin(None, NO_ADMINS, &h, "get", "/x")
            .await
            .is_err());
    }

    #[tokio::test]
    async fn authorize_admin_rejects_a_bearer_scheme_that_is_not_bearer() {
        let h = headers_with(Some("Basic the-real-token"));
        assert!(
            authorize_admin(Some("the-real-token"), NO_ADMINS, &h, "get", "/x")
                .await
                .is_err()
        );
    }

    #[tokio::test]
    async fn authorize_admin_never_authorizes_on_an_empty_token() {
        let empty_bearer = headers_with(Some("Bearer "));
        assert_eq!(bearer_token(&empty_bearer), Some(String::new()));
        assert!(
            authorize_admin(Some(""), NO_ADMINS, &empty_bearer, "get", "/x")
                .await
                .is_err()
        );
        assert!(
            authorize_admin(Some(""), NO_ADMINS, &headers_with(None), "get", "/x")
                .await
                .is_err()
        );
    }

    /// The typed payloads must carry the same wire shape the retired
    /// `json!({...})` payloads produced. Compared as parsed JSON, since object
    /// key order is not part of the contract and flips with serde_json's
    /// preserve_order feature under workspace-wide unification.
    #[test]
    fn curation_wire_bytes_match_the_old_json_macro() {
        use serde_json::json;

        let envelope = ApiData::ok(CurationCollectionsOut {
            collections: Vec::new(),
            committee: Vec::new(),
        });
        let old = json!({
            "committee": [],
            "collections": [],
        });
        assert_eq!(
            serde_json::to_value(&envelope).unwrap(),
            json!({ "ok": true, "data": old }),
        );

        let item = ItemStatusPatchOut {
            collection_id: "col-1".into(),
            id: "item-1".into(),
            status: "approved".into(),
            updated: 1,
        };
        let old_item = json!({
            "id": "item-1",
            "collection_id": "col-1",
            "status": "approved",
            "updated": 1u64,
        });
        assert_eq!(serde_json::to_value(&item).unwrap(), old_item);

        let bulk = BulkItemStatusPatchOut {
            collection_id: "col-1".into(),
            requested: 2,
            status: "rejected".into(),
            updated: 2,
        };
        let old_bulk = json!({
            "collection_id": "col-1",
            "status": "rejected",
            "requested": 2u64,
            "updated": 2u64,
        });
        assert_eq!(serde_json::to_value(&bulk).unwrap(), old_bulk);
    }

    #[tokio::test]
    async fn authorize_admin_forbidden_error_is_403() {
        use axum::response::IntoResponse;
        let err = authorize_admin(Some("real"), NO_ADMINS, &headers_with(None), "get", "/x")
            .await
            .unwrap_err();
        assert_eq!(
            err.into_response().status(),
            axum::http::StatusCode::FORBIDDEN
        );
    }
}
