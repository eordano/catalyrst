use axum::extract::{Path, Query, State};
use axum::http::HeaderMap;
use axum::Json;
use serde::Deserialize;
use serde_json::{json, Value};

use crate::admin::{admin_actor, authorize_admin};
use crate::http::errors::ApiError;
use crate::http::response::Data;
use crate::AppState;

pub async fn get_categories(State(state): State<AppState>) -> Result<Json<Value>, ApiError> {
    if let Some(cached) = state.categories_cache.get(&()).await {
        return Ok(Json(json!({ "data": { "categories": cached } })));
    }
    let categories = state.badges.list_categories().await?;
    state.categories_cache.insert((), categories.clone()).await;
    Ok(Json(json!({ "data": { "categories": categories } })))
}

pub async fn get_user_preview(
    State(state): State<AppState>,
    Path(address): Path<String>,
) -> Result<Json<Data<Value>>, ApiError> {
    let address = normalize_address(&address)?;
    let latest = state.badges.latest_achieved(&address, 5).await?;
    Ok(Json(Data::new(json!({ "latestAchievedBadges": latest }))))
}

#[derive(Debug, Deserialize)]
pub struct BadgesQuery {
    #[serde(default, rename = "includeNotAchieved")]
    pub include_not_achieved: Option<String>,
}

pub async fn get_user_badges(
    State(state): State<AppState>,
    Path(address): Path<String>,
    Query(q): Query<BadgesQuery>,
) -> Result<Json<Data<Value>>, ApiError> {
    let address = normalize_address(&address)?;
    let include_not_achieved = q
        .include_not_achieved
        .as_deref()
        .map(|s| s.eq_ignore_ascii_case("true"))
        .unwrap_or(false);

    let (achieved, not_achieved) = state
        .badges
        .user_badges(&address, include_not_achieved)
        .await?;

    Ok(Json(Data::new(json!({
        "achieved": achieved,
        "notAchieved": not_achieved,
    }))))
}

pub async fn get_badge_tiers(
    State(state): State<AppState>,
    Path(badge_id): Path<String>,
) -> Result<Json<Data<Value>>, ApiError> {
    if let Some(cached) = state.tiers_cache.get(&badge_id).await {
        return Ok(Json(Data::new(json!({ "tiers": cached }))));
    }
    let tiers = state.badges.list_tiers(&badge_id).await?;
    let value = serde_json::to_value(&tiers).map_err(|e| ApiError::Internal(e.to_string()))?;
    state.tiers_cache.insert(badge_id, value.clone()).await;
    Ok(Json(Data::new(json!({ "tiers": value }))))
}

#[derive(Debug, Default, Deserialize)]
pub struct GrantBody {
    #[serde(default, rename = "tierId")]
    pub tier_id: Option<String>,
}

pub async fn grant_user_badge(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path((address, badge_id)): Path<(String, String)>,
    body: Option<Json<GrantBody>>,
) -> Result<Json<Value>, ApiError> {
    authorize_admin(&state, &headers)?;
    let actor = admin_actor(&headers);
    let address = normalize_address(&address)?;
    let badge_id = normalize_badge_id(&badge_id)?;
    let tier_id = body
        .and_then(|Json(b)| b.tier_id)
        .map(|t| t.trim().to_string())
        .filter(|t| !t.is_empty());

    let granted = state
        .badges
        .grant_badge(&address, &badge_id, tier_id.as_deref(), &actor)
        .await?;
    if !granted {
        return Err(ApiError::not_found(format!(
            "no badge found with id: {badge_id}"
        )));
    }
    Ok(Json(json!({ "data": {
        "granted": true,
        "address": address,
        "badgeId": badge_id,
        "tierId": tier_id,
    } })))
}

pub async fn revoke_user_badge(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path((address, badge_id)): Path<(String, String)>,
) -> Result<Json<Value>, ApiError> {
    authorize_admin(&state, &headers)?;
    let actor = admin_actor(&headers);
    let address = normalize_address(&address)?;
    let badge_id = normalize_badge_id(&badge_id)?;

    let exists = state
        .badges
        .revoke_badge(&address, &badge_id, &actor)
        .await?;
    if !exists {
        return Err(ApiError::not_found(format!(
            "no badge found with id: {badge_id}"
        )));
    }
    Ok(Json(json!({ "data": {
        "revoked": true,
        "address": address,
        "badgeId": badge_id,
    } })))
}

fn normalize_badge_id(badge_id: &str) -> Result<String, ApiError> {
    let trimmed = badge_id.trim();
    if trimmed.is_empty() {
        return Err(ApiError::bad_request("badge_id is required"));
    }
    Ok(trimmed.to_string())
}

fn normalize_address(address: &str) -> Result<String, ApiError> {
    let trimmed = address.trim();
    if trimmed.is_empty() {
        return Err(ApiError::bad_request("address is required"));
    }
    Ok(trimmed.to_ascii_lowercase())
}
