use axum::extract::{Path, Query, State};
use axum::Json;
use serde::Deserialize;
use serde_json::{json, Value};

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

fn normalize_address(address: &str) -> Result<String, ApiError> {
    let trimmed = address.trim();
    if trimmed.is_empty() {
        return Err(ApiError::bad_request("address is required"));
    }
    Ok(trimmed.to_ascii_lowercase())
}
