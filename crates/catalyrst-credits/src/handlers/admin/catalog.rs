use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::Json;
use serde::{Deserialize, Serialize};
use serde_json::json;

use crate::http::ApiError;
use crate::AppState;

use super::common::{
    validate_currency, validate_positive_amount, validate_price_cents, validate_sku, RequireAdmin,
};

#[derive(Debug, Serialize)]
pub(super) struct PackOut {
    sku: String,
    title: String,
    credits: String,
    #[serde(rename = "priceCents")]
    price_cents: i64,
    currency: String,
    active: bool,
    #[serde(rename = "sortOrder")]
    sort_order: i32,
}

impl From<crate::ports::admin::PackAdminRow> for PackOut {
    fn from(p: crate::ports::admin::PackAdminRow) -> Self {
        PackOut {
            sku: p.sku,
            title: p.title,
            credits: p.credits,
            price_cents: p.price_cents,
            currency: p.currency,
            active: p.active,
            sort_order: p.sort_order,
        }
    }
}

#[derive(Debug, Deserialize)]
pub(super) struct PackCreateBody {
    sku: String,
    title: String,

    credits: String,
    #[serde(rename = "priceCents")]
    price_cents: i64,
    currency: String,
    #[serde(default = "default_true")]
    active: bool,
    #[serde(rename = "sortOrder", default)]
    sort_order: i32,
}

#[derive(Debug, Deserialize)]
pub(super) struct PackUpdateBody {
    title: String,
    credits: String,
    #[serde(rename = "priceCents")]
    price_cents: i64,
    currency: String,
    #[serde(default = "default_true")]
    active: bool,
    #[serde(rename = "sortOrder", default)]
    sort_order: i32,
}

fn default_true() -> bool {
    true
}

fn validate_pack_title(raw: &str) -> Result<(), ApiError> {
    if raw.trim().is_empty() || raw.len() > 200 {
        return Err(ApiError::bad_request("title must be 1..200 chars"));
    }
    Ok(())
}

pub(super) async fn list_packs(
    _admin: RequireAdmin,
    State(state): State<AppState>,
) -> Result<Json<Vec<PackOut>>, ApiError> {
    let rows = state.credits.admin_list_packs().await?;
    Ok(Json(rows.into_iter().map(PackOut::from).collect()))
}

pub(super) async fn create_pack(
    _admin: RequireAdmin,
    State(state): State<AppState>,
    body: Option<Json<PackCreateBody>>,
) -> Result<(StatusCode, Json<PackOut>), ApiError> {
    let Json(b) = body.ok_or_else(|| ApiError::bad_request("missing JSON body"))?;
    let sku = validate_sku(&b.sku)?;
    validate_pack_title(&b.title)?;
    let credits = validate_positive_amount(&b.credits)?;
    let price_cents = validate_price_cents(b.price_cents)?;
    let currency = validate_currency(&b.currency)?;
    let detail = json!({
        "sku": sku, "credits": credits, "priceCents": price_cents,
        "currency": currency, "active": b.active, "sortOrder": b.sort_order,
    });
    tracing::info!(action = "pack.create", sku = %sku, "admin pack create");
    let pack = state
        .credits
        .admin_create_pack(
            &sku,
            &b.title,
            &credits,
            price_cents,
            &currency,
            b.active,
            b.sort_order,
            &detail,
        )
        .await?;
    Ok((StatusCode::CREATED, Json(pack.into())))
}

pub(super) async fn update_pack(
    _admin: RequireAdmin,
    State(state): State<AppState>,
    Path(sku): Path<String>,
    body: Option<Json<PackUpdateBody>>,
) -> Result<Json<PackOut>, ApiError> {
    let sku = validate_sku(&sku)?;
    let Json(b) = body.ok_or_else(|| ApiError::bad_request("missing JSON body"))?;
    validate_pack_title(&b.title)?;
    let credits = validate_positive_amount(&b.credits)?;
    let price_cents = validate_price_cents(b.price_cents)?;
    let currency = validate_currency(&b.currency)?;
    let detail = json!({
        "sku": sku, "credits": credits, "priceCents": price_cents,
        "currency": currency, "active": b.active, "sortOrder": b.sort_order,
    });
    tracing::info!(action = "pack.update", sku = %sku, "admin pack update");
    let pack = state
        .credits
        .admin_update_pack(
            &sku,
            &b.title,
            &credits,
            price_cents,
            &currency,
            b.active,
            b.sort_order,
            &detail,
        )
        .await?;
    Ok(Json(pack.into()))
}

pub(super) async fn delete_pack(
    _admin: RequireAdmin,
    State(state): State<AppState>,
    Path(sku): Path<String>,
) -> Result<StatusCode, ApiError> {
    let sku = validate_sku(&sku)?;
    let detail = json!({ "sku": sku });
    tracing::info!(action = "pack.delete", sku = %sku, "admin pack delete");
    state.credits.admin_delete_pack(&sku, &detail).await?;
    Ok(StatusCode::NO_CONTENT)
}
