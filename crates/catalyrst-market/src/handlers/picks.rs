use axum::extract::{Path, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::Json;
use chrono::Utc;
use serde::{Deserialize, Serialize};

use catalyrst_crypto::signed_fetch::signed_fetch_path;

use crate::auth_chain::{
    self, build_payload, AuthChainError, AuthChainErrorExt, AUTH_METADATA_HEADER,
    AUTH_TIMESTAMP_HEADER, FIVE_MINUTES,
};
use crate::http::response::ApiError;
use crate::ports::lists::is_uuid;
use crate::AppState;

fn auth_chain_error_to_api(e: AuthChainError) -> ApiError {
    match e {
        AuthChainError::EipNotImplemented => {
            ApiError::Http(catalyrst_types::HttpError::new(501, e.message()))
        }
        _ => ApiError::Http(catalyrst_types::HttpError::new(401, e.message())),
    }
}

async fn authenticate(
    headers: &HeaderMap,
    method: &str,
    fallback_path: &str,
) -> Result<String, ApiError> {
    // @dcl/crypto-middleware >=5.1.0: reject non-canonical signer/intent metadata
    // (mixed case or whitespace) with 400 before the signature is validated.
    auth_chain::require_canonical_metadata(headers)?;

    let chain = auth_chain::extract_auth_chain(headers).map_err(auth_chain_error_to_api)?;

    let timestamp = headers
        .get(AUTH_TIMESTAMP_HEADER)
        .and_then(|v| v.to_str().ok())
        .ok_or_else(|| auth_chain_error_to_api(AuthChainError::MissingTimestamp))?;
    let metadata = headers
        .get(AUTH_METADATA_HEADER)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("{}");

    let path = signed_fetch_path(headers, fallback_path);
    let payload = build_payload(method, path.as_ref(), timestamp, metadata);

    let now = Utc::now().timestamp();
    let recovered = auth_chain::validate_signature(&chain, &payload, timestamp, FIVE_MINUTES, now)
        .await
        .map_err(auth_chain_error_to_api)?;
    Ok(recovered.as_str().to_string())
}

#[derive(Debug, Default, Deserialize, utoipa::ToSchema)]
pub struct PickUnpickInBulkBody {
    #[serde(default, rename = "pickedFor")]
    pub picked_for: Option<Vec<String>>,
    #[serde(default, rename = "unpickedFrom")]
    pub unpicked_from: Option<Vec<String>>,
}

#[derive(Debug, Serialize, utoipa::ToSchema)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export, export_to = "market/"))]
pub struct PickUnpickResult {
    #[serde(rename = "pickedByUser")]
    #[cfg_attr(feature = "ts", ts(rename = "pickedByUser"))]
    pub picked_by_user: bool,
}

#[derive(Debug, Serialize, utoipa::ToSchema)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export, export_to = "market/"))]
pub struct PickUnpickEnvelope {
    pub ok: bool,
    pub data: PickUnpickResult,
}

fn validate_list_ids(ids: &[String]) -> Result<(), ApiError> {
    if ids.iter().any(|id| !is_uuid(id)) {
        return Err(ApiError::bad_request("list ids must be UUIDs"));
    }
    Ok(())
}

/// Errors for the pick/unpick route. Upstream's `ListsNotFoundError` /
/// `ItemNotFoundError` 404s carry the offending ids in a `data` payload
/// (picks-handlers.ts:211-235: `{ok:false, message, data:{listIds}}` and
/// `{ok:false, message, data:{itemId}}`), which the plain `ApiError` envelope
/// (`{ok, message}`) cannot express.
#[derive(Debug)]
pub enum PicksError {
    Api(ApiError),
    /// 404 `{ok:false, message:"Some lists were not found.", data:{listIds}}`.
    ListsNotFound(Vec<String>),
    /// 404 `{ok:false, message:"The item trying to get saved doesn't
    /// exist.", data:{itemId}}`.
    ItemNotFound(String),
}

impl From<ApiError> for PicksError {
    fn from(e: ApiError) -> Self {
        PicksError::Api(e)
    }
}

pub(crate) const LISTS_NOT_FOUND_MESSAGE: &str = "Some lists were not found.";
pub(crate) const ITEM_NOT_FOUND_MESSAGE: &str = "The item trying to get saved doesn't exist.";

/// `data` payload of the lists-not-found 404: the offending list ids.
#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct ListIdsData {
    #[serde(rename = "listIds")]
    pub list_ids: Vec<String>,
}

/// 404 body for upstream's `ListsNotFoundError`
/// (`{ok:false, message:"Some lists were not found.", data:{listIds}}`).
#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct ListsNotFoundBody {
    pub ok: bool,
    pub message: String,
    pub data: ListIdsData,
}

/// `data` payload of the item-not-found 404: the missing item id.
#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct ItemIdData {
    #[serde(rename = "itemId")]
    pub item_id: String,
}

/// 404 body for upstream's `ItemNotFoundError`
/// (`{ok:false, message:"The item trying to get saved doesn't exist.",
/// data:{itemId}}`).
#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct ItemNotFoundBody {
    pub ok: bool,
    pub message: String,
    pub data: ItemIdData,
}

/// The two documented 404 shapes of `POST /v1/picks/{item_id}`. Untagged, so
/// it serializes exactly as the inner body while utoipa renders a `oneOf` --
/// the runtime response and the published contract share one definition and
/// cannot drift.
#[derive(Debug, Serialize, utoipa::ToSchema)]
#[serde(untagged)]
pub enum PickUnpick404 {
    Lists(ListsNotFoundBody),
    Item(ItemNotFoundBody),
}

impl IntoResponse for PicksError {
    fn into_response(self) -> Response {
        match self {
            PicksError::Api(e) => e.into_response(),
            PicksError::ListsNotFound(list_ids) => (
                StatusCode::NOT_FOUND,
                Json(PickUnpick404::Lists(ListsNotFoundBody {
                    ok: false,
                    message: LISTS_NOT_FOUND_MESSAGE.to_string(),
                    data: ListIdsData { list_ids },
                })),
            )
                .into_response(),
            PicksError::ItemNotFound(item_id) => (
                StatusCode::NOT_FOUND,
                Json(PickUnpick404::Item(ItemNotFoundBody {
                    ok: false,
                    message: ITEM_NOT_FOUND_MESSAGE.to_string(),
                    data: ItemIdData { item_id },
                })),
            )
                .into_response(),
        }
    }
}

#[utoipa::path(
    post,
    path = "/v1/picks/{item_id}",
    tag = "market",
    params(("item_id" = String, Path)),
    request_body = PickUnpickInBulkBody,
    responses(
        (status = 200, body = PickUnpickEnvelope),
        (status = 400, body = crate::http::response::MarketErrorBody),
        (status = 401, body = crate::http::response::MarketErrorBody),
        (status = 404, body = PickUnpick404,
         description = "Lists not found (data.listIds) or item not found (data.itemId)"),
        (status = 500, body = crate::http::response::MarketErrorBody)
    )
)]
pub async fn pick_unpick_in_bulk(
    State(state): State<AppState>,
    Path(item_id): Path<String>,
    headers: HeaderMap,
    body: Option<Json<PickUnpickInBulkBody>>,
) -> Result<Json<PickUnpickEnvelope>, PicksError> {
    let user = authenticate(&headers, "post", &format!("/v1/picks/{item_id}")).await?;

    let body = body.map(|Json(b)| b).unwrap_or_default();
    let picked_for = body.picked_for.unwrap_or_default();
    let unpicked_from = body.unpicked_from.unwrap_or_default();

    if picked_for.iter().any(|id| unpicked_from.contains(id)) {
        return Err(ApiError::bad_request(
            "The item cannot be be picked and unpicked from a list at the same time.",
        )
        .into());
    }
    validate_list_ids(&picked_for)?;
    validate_list_ids(&unpicked_from)?;

    if !state.lists.item_exists(&item_id).await? {
        return Err(PicksError::ItemNotFound(item_id));
    }

    let (pick_ids, unpick_ids) = if picked_for.is_empty() && unpicked_from.is_empty() {
        (
            vec![state.lists.get_or_create_default_list(&user).await?],
            Vec::new(),
        )
    } else {
        let mut all = picked_for.clone();
        all.extend(unpicked_from.iter().cloned());
        all.sort();
        all.dedup();
        // Upstream `checkNonEditableLists`: flags only lists the caller
        // demonstrably cannot edit (owned by someone else AND carrying an ACL
        // that excludes them). Lists without ACL rows -- including the shared
        // default Wishlist -- pass, and nonexistent ids no-op downstream.
        let non_editable = state.lists.check_non_editable_lists(&all, &user).await?;
        if !non_editable.is_empty() {
            // Upstream's ListsNotFoundError carries the offending list ids in
            // the 404 body's `data.listIds`.
            return Err(PicksError::ListsNotFound(non_editable));
        }
        (picked_for, unpicked_from)
    };

    state
        .lists
        .pick_in_lists(&item_id, &user, &pick_ids)
        .await?;
    state
        .lists
        .unpick_from_lists(&item_id, &user, &unpick_ids)
        .await?;

    let picked_by_user = if !pick_ids.is_empty() {
        true
    } else {
        state.lists.is_picked_by_user(&item_id, &user).await?
    };

    Ok(Json(PickUnpickEnvelope {
        ok: true,
        data: PickUnpickResult { picked_by_user },
    }))
}

#[utoipa::path(
    delete,
    path = "/v1/picks/{item_id}",
    tag = "market",
    params(("item_id" = String, Path)),
    responses(
        (status = 200, body = PickUnpickEnvelope),
        (status = 401, body = crate::http::response::MarketErrorBody),
        (status = 500, body = crate::http::response::MarketErrorBody)
    )
)]
pub async fn unpick_everywhere(
    State(state): State<AppState>,
    Path(item_id): Path<String>,
    headers: HeaderMap,
) -> Result<Json<PickUnpickEnvelope>, ApiError> {
    let user = authenticate(&headers, "delete", &format!("/v1/picks/{item_id}")).await?;
    state.lists.unpick_everywhere(&item_id, &user).await?;
    Ok(Json(PickUnpickEnvelope {
        ok: true,
        data: PickUnpickResult {
            picked_by_user: false,
        },
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn envelope_serializes_camel_case() {
        let env = PickUnpickEnvelope {
            ok: true,
            data: PickUnpickResult {
                picked_by_user: true,
            },
        };
        assert_eq!(
            serde_json::to_value(&env).unwrap(),
            json!({ "ok": true, "data": { "pickedByUser": true } })
        );
    }

    #[test]
    fn body_parses_upstream_shape_and_defaults() {
        let b: PickUnpickInBulkBody =
            serde_json::from_value(json!({ "pickedFor": ["a"], "unpickedFrom": null })).unwrap();
        assert_eq!(b.picked_for.as_deref(), Some(&["a".to_string()][..]));
        assert!(b.unpicked_from.is_none());

        let empty: PickUnpickInBulkBody = serde_json::from_value(json!({})).unwrap();
        assert!(empty.picked_for.is_none() && empty.unpicked_from.is_none());
    }

    /// Upstream ListsNotFoundError 404 (picks-handlers.ts:213-224): the body
    /// carries the offending ids as `data.listIds`, not just the message.
    #[tokio::test]
    async fn lists_not_found_404_carries_list_ids_payload() {
        let ids = vec![
            "6a0e4b1e-0f6e-4c7a-9d2b-2f1c9a1a0001".to_string(),
            "6a0e4b1e-0f6e-4c7a-9d2b-2f1c9a1a0002".to_string(),
        ];
        let resp = PicksError::ListsNotFound(ids.clone()).into_response();
        assert_eq!(resp.status(), StatusCode::NOT_FOUND);
        let bytes = axum::body::to_bytes(resp.into_body(), usize::MAX)
            .await
            .unwrap();
        let v: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(
            v,
            json!({
                "ok": false,
                "message": "Some lists were not found.",
                "data": { "listIds": ids },
            })
        );
    }

    /// Upstream ItemNotFoundError 404 (picks-handlers.ts:225-235): fixed
    /// message plus the item id in `data.itemId` (the id is NOT embedded in
    /// the message).
    #[tokio::test]
    async fn item_not_found_404_carries_item_id_payload() {
        let item = "0xf1483f042614105cb943d3dd67157256cd003028-15";
        let resp = PicksError::ItemNotFound(item.to_string()).into_response();
        assert_eq!(resp.status(), StatusCode::NOT_FOUND);
        let bytes = axum::body::to_bytes(resp.into_body(), usize::MAX)
            .await
            .unwrap();
        let v: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(
            v,
            json!({
                "ok": false,
                "message": "The item trying to get saved doesn't exist.",
                "data": { "itemId": item },
            })
        );
    }

    /// Api-wrapped errors keep the plain `{ok, message}` envelope untouched.
    #[tokio::test]
    async fn api_error_passthrough_keeps_plain_envelope() {
        let resp = PicksError::from(ApiError::bad_request("nope")).into_response();
        assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
        let bytes = axum::body::to_bytes(resp.into_body(), usize::MAX)
            .await
            .unwrap();
        let v: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(v, json!({ "ok": false, "message": "nope" }));
    }

    #[test]
    fn uuid_validation() {
        assert!(is_uuid("6a0e4b1e-0f6e-4c7a-9d2b-2f1c9a1a0001"));
        assert!(!is_uuid("not-a-uuid"));
        assert!(!is_uuid(""));
        assert!(!is_uuid("6a0e4b1e-0f6e-4c7a-9d2b-2f1c9a1a000g"));
        assert!(validate_list_ids(&["nope".to_string()]).is_err());
        assert!(validate_list_ids(&[]).is_ok());
    }
}
