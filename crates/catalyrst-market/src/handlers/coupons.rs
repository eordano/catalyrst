use axum::extract::{Path, Query, State};
use axum::http::HeaderMap;
use axum::Json;
use chrono::Utc;
use serde::Serialize;

use catalyrst_crypto::signed_fetch::signed_fetch_path;

use crate::auth_chain::{
    self, AuthChainError, AuthChainErrorExt, AUTH_METADATA_HEADER, AUTH_TIMESTAMP_HEADER,
    FIVE_MINUTES,
};
use crate::http::pagination::get_pagination_params;
use crate::http::params::is_address;
use crate::http::response::ApiError;
use crate::ports::coupons::types::CouponPagination;
use crate::ports::coupons::{validate_creation_schema, Coupon, CouponCreation, CouponError};
use crate::ports::lists::is_uuid;
use crate::AppState;

const COULD_NOT_CREATE: &str = "Coupon could not be created";
const COULD_NOT_FETCH_ONE: &str = "Could not fetch the coupon";
const COULD_NOT_FETCH_MANY: &str = "Could not fetch the coupons";
const SIGNER_REQUIRED: &str = "A valid signer address is required";

fn auth_error_to_api(e: AuthChainError) -> ApiError {
    match e {
        AuthChainError::EipNotImplemented => {
            ApiError::Http(catalyrst_types::HttpError::new(501, e.message()))
        }
        _ => ApiError::Http(catalyrst_types::HttpError::new(401, e.message())),
    }
}

/// Upstream answers 400 for every refusal a creator can act on, including the signer mismatch
/// that the analogous trade route answers 401 for. That inconsistency is upstream's own and is
/// ported as it stands.
fn coupon_error_to_api(e: CouponError) -> ApiError {
    let message = e.to_string();
    match e {
        CouponError::Duplicate => ApiError::Http(catalyrst_types::HttpError::new(409, message)),
        CouponError::NotFound(_) => ApiError::Http(catalyrst_types::HttpError::new(404, message)),
        CouponError::Db(_) | CouponError::Internal(_) => {
            tracing::error!(error = %message, "coupon request failed");
            ApiError::Http(catalyrst_types::HttpError::new(
                500,
                COULD_NOT_CREATE.to_string(),
            ))
        }
        _ => ApiError::bad_request(message),
    }
}

#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct CouponEnvelope {
    pub ok: bool,
    pub data: Coupon,
}

#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct CouponsEnvelope {
    pub ok: bool,
    pub data: Vec<Coupon>,
}

#[utoipa::path(
    post,
    path = "/v1/coupons",
    tag = "market",
    request_body = serde_json::Value,
    responses(
        (status = 201, body = CouponEnvelope),
        (status = 400, body = crate::http::response::MarketErrorBody),
        (status = 401, body = crate::http::response::MarketErrorBody),
        (status = 409, body = crate::http::response::MarketErrorBody),
        (status = 500, body = crate::http::response::MarketErrorBody)
    )
)]
pub async fn add_coupon(
    State(state): State<AppState>,
    headers: HeaderMap,
    body: String,
) -> Result<(axum::http::StatusCode, Json<CouponEnvelope>), ApiError> {
    let chain = auth_chain::extract_auth_chain(&headers).map_err(auth_error_to_api)?;

    let timestamp = headers
        .get(AUTH_TIMESTAMP_HEADER)
        .and_then(|v| v.to_str().ok())
        .ok_or_else(|| auth_error_to_api(AuthChainError::MissingTimestamp))?;
    let metadata = headers
        .get(AUTH_METADATA_HEADER)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("{}");

    auth_chain::require_auth_metadata(
        &headers,
        auth_chain::MARKETPLACE_AUTH_SIGNERS,
        Some(auth_chain::CREATE_COUPON_INTENT),
    )?;

    let path = signed_fetch_path(&headers, "/v1/coupons");

    let now = Utc::now();
    let recovered = auth_chain::validate_signature_either_payload(
        &chain,
        "post",
        &path,
        timestamp,
        metadata,
        FIVE_MINUTES,
        now.timestamp(),
    )
    .await
    .map_err(auth_error_to_api)?;

    let raw: serde_json::Value = serde_json::from_str(&body)
        .map_err(|e| ApiError::bad_request(format!("invalid coupon body: {e}")))?;
    validate_creation_schema(&raw).map_err(coupon_error_to_api)?;
    let coupon: CouponCreation = serde_json::from_value(raw)
        .map_err(|e| ApiError::bad_request(format!("invalid coupon body: {e}")))?;

    let data = state
        .coupons
        .add_coupon(coupon, recovered.as_str(), now.timestamp_millis())
        .await
        .map_err(coupon_error_to_api)?;

    Ok((
        axum::http::StatusCode::CREATED,
        Json(CouponEnvelope { ok: true, data }),
    ))
}

#[utoipa::path(
    get,
    path = "/v1/coupons",
    tag = "market",
    params(
        ("signer" = String, Query),
        ("limit" = Option<i64>, Query),
        ("offset" = Option<i64>, Query)
    ),
    responses(
        (status = 200, body = CouponsEnvelope),
        (status = 400, body = crate::http::response::MarketErrorBody),
        (status = 500, body = crate::http::response::MarketErrorBody)
    )
)]
pub async fn get_coupons(
    State(state): State<AppState>,
    Query(pairs): Query<Vec<(String, String)>>,
) -> Result<Json<CouponsEnvelope>, ApiError> {
    let signer = pairs
        .iter()
        .find(|(k, _)| k == "signer")
        .map(|(_, v)| v.as_str())
        .filter(|v| is_address(v))
        .ok_or_else(|| ApiError::bad_request(SIGNER_REQUIRED))?;

    let page = get_pagination_params(&pairs);
    let data = state
        .coupons
        .get_coupons_by_signer(
            signer,
            CouponPagination {
                limit: Some(page.limit),
                offset: Some(page.offset),
            },
        )
        .await
        .map_err(|e| {
            tracing::error!(error = %e, "could not fetch the coupons");
            ApiError::Http(catalyrst_types::HttpError::new(
                500,
                COULD_NOT_FETCH_MANY.to_string(),
            ))
        })?;

    Ok(Json(CouponsEnvelope { ok: true, data }))
}

#[utoipa::path(
    get,
    path = "/v1/coupons/{id}",
    tag = "market",
    params(("id" = String, Path)),
    responses(
        (status = 200, body = CouponEnvelope),
        (status = 404, body = crate::http::response::MarketErrorBody),
        (status = 500, body = crate::http::response::MarketErrorBody)
    )
)]
pub async fn get_coupon(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<CouponEnvelope>, ApiError> {
    if !is_uuid(&id) {
        return Err(ApiError::Http(catalyrst_types::HttpError::new(
            404,
            CouponError::NotFound(id).to_string(),
        )));
    }
    let data = state.coupons.get_coupon(&id).await.map_err(|e| match e {
        CouponError::NotFound(_) => {
            ApiError::Http(catalyrst_types::HttpError::new(404, e.to_string()))
        }
        other => {
            tracing::error!(error = %other, "could not fetch the coupon");
            ApiError::Http(catalyrst_types::HttpError::new(
                500,
                COULD_NOT_FETCH_ONE.to_string(),
            ))
        }
    })?;
    Ok(Json(CouponEnvelope { ok: true, data }))
}
