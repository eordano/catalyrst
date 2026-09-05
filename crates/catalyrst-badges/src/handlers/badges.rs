use axum::extract::{Path, Query, State};
use axum::Json;
use serde::Deserialize;

use crate::admin::RequireAdmin;
use crate::http::errors::ApiError;
use crate::http::response::Data;
use crate::ports::types::{
    CategoriesBody, GrantResult, PreviewBody, RevokeResult, TiersBody, UserBadgesBody,
};
use crate::AppState;

pub async fn get_categories(
    State(state): State<AppState>,
) -> Result<Json<Data<CategoriesBody>>, ApiError> {
    if let Some(cached) = state.categories_cache.get(&()).await {
        return Ok(Json(Data::new(CategoriesBody { categories: cached })));
    }
    let categories = state.badges.list_categories().await?;
    state.categories_cache.insert((), categories.clone()).await;
    Ok(Json(Data::new(CategoriesBody { categories })))
}

pub async fn get_user_preview(
    State(state): State<AppState>,
    Path(address): Path<String>,
) -> Result<Json<Data<PreviewBody>>, ApiError> {
    let address = lenient_address(&address);
    let latest = state.badges.latest_achieved(&address, 5).await?;
    Ok(Json(Data::new(PreviewBody {
        latest_achieved_badges: latest,
    })))
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
) -> Result<Json<Data<UserBadgesBody>>, ApiError> {
    let address = lenient_address(&address);
    let include_not_achieved = q
        .include_not_achieved
        .as_deref()
        .map(|s| s.eq_ignore_ascii_case("true"))
        .unwrap_or(false);

    let (achieved, not_achieved) = state
        .badges
        .user_badges(&address, include_not_achieved)
        .await?;

    Ok(Json(Data::new(UserBadgesBody {
        achieved,
        not_achieved,
    })))
}

pub async fn get_badge_tiers(
    State(state): State<AppState>,
    Path(badge_id): Path<String>,
) -> Result<Json<Data<TiersBody>>, ApiError> {
    if let Some(cached) = state.tiers_cache.get(&badge_id).await {
        return Ok(Json(Data::new(TiersBody { tiers: cached })));
    }
    let tiers = state.badges.list_tiers(&badge_id).await?;
    state.tiers_cache.insert(badge_id, tiers.clone()).await;
    Ok(Json(Data::new(TiersBody { tiers })))
}

#[derive(Debug, Default, Deserialize)]
pub struct GrantBody {
    #[serde(default, rename = "tierId")]
    pub tier_id: Option<String>,
}

pub async fn grant_user_badge(
    admin: RequireAdmin,
    State(state): State<AppState>,
    Path((address, badge_id)): Path<(String, String)>,
    body: Option<Json<GrantBody>>,
) -> Result<Json<Data<GrantResult>>, ApiError> {
    let actor = admin.audit_actor_description();
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
    Ok(Json(Data::new(GrantResult {
        granted: true,
        address,
        badge_id,
        tier_id,
    })))
}

pub async fn revoke_user_badge(
    admin: RequireAdmin,
    State(state): State<AppState>,
    Path((address, badge_id)): Path<(String, String)>,
) -> Result<Json<Data<RevokeResult>>, ApiError> {
    let actor = admin.audit_actor_description();
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
    Ok(Json(Data::new(RevokeResult {
        revoked: true,
        address,
        badge_id,
    })))
}

fn normalize_badge_id(badge_id: &str) -> Result<String, ApiError> {
    let trimmed = badge_id.trim();
    if trimmed.is_empty() {
        return Err(ApiError::bad_request("badge_id is required"));
    }
    Ok(trimmed.to_string())
}

fn normalize_address(address: &str) -> Result<String, ApiError> {
    if address.trim().is_empty() {
        return Err(ApiError::bad_request("address is required"));
    }
    catalyrst_types::normalize_eth_address(address)
        .ok_or_else(|| ApiError::bad_request("invalid address"))
}

fn lenient_address(address: &str) -> String {
    catalyrst_types::normalize_eth_address(address).unwrap_or_else(|| address.trim().to_string())
}

#[cfg(test)]
mod normalize_address_tests {
    use super::normalize_address;

    #[test]
    fn accepts_and_lowercases_valid_addresses() {
        assert_eq!(
            normalize_address(" 0xf39Fd6e51aad88F6F4ce6aB8827279cffFb92266 ").unwrap(),
            "0xf39fd6e51aad88f6f4ce6ab8827279cfffb92266"
        );
    }

    #[test]
    fn rejects_malformed_addresses() {
        assert!(normalize_address("not-an-address").is_err());
        assert!(normalize_address("0x1234").is_err());
        assert!(normalize_address("0xZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZ").is_err());
    }
}
