use axum::extract::{Path, Query, State};
use axum::http::{HeaderMap, StatusCode};
use axum::Json;
use serde::{Deserialize, Serialize};
use serde_json::json;

use crate::http::ApiError;
use crate::AppState;

use super::common::{
    authorize_admin, validate_currency, validate_max_mana, validate_positive_amount,
    validate_price_cents, validate_season_state, validate_sku,
};

#[derive(Debug, Serialize)]
pub(super) struct SeasonOut {
    id: i32,
    name: String,
    #[serde(rename = "startDate")]
    start_date: String,
    #[serde(rename = "endDate")]
    end_date: String,
    #[serde(rename = "maxMana")]
    max_mana: String,
    #[serde(rename = "amountOfWeeks")]
    amount_of_weeks: i32,
    state: String,
}

impl From<crate::ports::admin::SeasonAdminRow> for SeasonOut {
    fn from(s: crate::ports::admin::SeasonAdminRow) -> Self {
        SeasonOut {
            id: s.id,
            name: s.name,
            start_date: s.start_date.to_rfc3339(),
            end_date: s.end_date.to_rfc3339(),
            max_mana: s.max_mana,
            amount_of_weeks: s.amount_of_weeks,
            state: s.state,
        }
    }
}

#[derive(Debug, Deserialize)]
pub(super) struct SeasonBody {
    name: String,
    #[serde(rename = "startDate")]
    start_date: chrono::DateTime<chrono::Utc>,
    #[serde(rename = "endDate")]
    end_date: chrono::DateTime<chrono::Utc>,
    #[serde(rename = "maxMana")]
    max_mana: String,
    #[serde(rename = "amountOfWeeks")]
    amount_of_weeks: i32,
    state: String,
}

fn validate_season_body(b: &SeasonBody) -> Result<(String, String), ApiError> {
    if b.name.trim().is_empty() || b.name.len() > 200 {
        return Err(ApiError::bad_request("name must be 1..200 chars"));
    }
    if b.end_date <= b.start_date {
        return Err(ApiError::bad_request("endDate must be after startDate"));
    }
    if b.amount_of_weeks < 0 || b.amount_of_weeks > 520 {
        return Err(ApiError::bad_request("amountOfWeeks out of range"));
    }
    let max_mana = validate_max_mana(&b.max_mana)?;
    let state = validate_season_state(&b.state)?;
    Ok((max_mana, state))
}

pub(super) async fn list_seasons(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<Vec<SeasonOut>>, ApiError> {
    authorize_admin(&state, &headers)?;
    let rows = state.credits.admin_list_seasons().await?;
    Ok(Json(rows.into_iter().map(SeasonOut::from).collect()))
}

pub(super) async fn create_season(
    State(state): State<AppState>,
    headers: HeaderMap,
    body: Option<Json<SeasonBody>>,
) -> Result<(StatusCode, Json<SeasonOut>), ApiError> {
    authorize_admin(&state, &headers)?;
    let Json(b) = body.ok_or_else(|| ApiError::bad_request("missing JSON body"))?;
    let (max_mana, vstate) = validate_season_body(&b)?;
    let detail = json!({ "name": b.name, "maxMana": max_mana, "state": vstate });
    let season = state
        .credits
        .admin_create_season(
            b.name.trim(),
            b.start_date,
            b.end_date,
            &max_mana,
            b.amount_of_weeks,
            &vstate,
            &detail,
        )
        .await?;
    Ok((StatusCode::CREATED, Json(season.into())))
}

pub(super) async fn update_season(
    State(state): State<AppState>,
    Path(id): Path<i32>,
    headers: HeaderMap,
    body: Option<Json<SeasonBody>>,
) -> Result<Json<SeasonOut>, ApiError> {
    authorize_admin(&state, &headers)?;
    let Json(b) = body.ok_or_else(|| ApiError::bad_request("missing JSON body"))?;
    let (max_mana, vstate) = validate_season_body(&b)?;
    let detail = json!({ "id": id, "name": b.name, "maxMana": max_mana, "state": vstate });
    let season = state
        .credits
        .admin_update_season(
            id,
            b.name.trim(),
            b.start_date,
            b.end_date,
            &max_mana,
            b.amount_of_weeks,
            &vstate,
            &detail,
        )
        .await?;
    Ok(Json(season.into()))
}

pub(super) async fn delete_season(
    State(state): State<AppState>,
    Path(id): Path<i32>,
    headers: HeaderMap,
) -> Result<StatusCode, ApiError> {
    authorize_admin(&state, &headers)?;
    let detail = json!({ "id": id });
    state.credits.admin_delete_season(id, &detail).await?;
    Ok(StatusCode::NO_CONTENT)
}

#[derive(Debug, Serialize)]
pub(super) struct GoalOut {
    id: i32,
    #[serde(rename = "weekId")]
    week_id: i32,
    title: String,
    description: String,
    thumbnail: String,
    reward: String,
    #[serde(rename = "totalSteps")]
    total_steps: i32,
    #[serde(rename = "sortOrder")]
    sort_order: i32,
    kind: String,
}

impl From<crate::ports::admin::GoalAdminRow> for GoalOut {
    fn from(g: crate::ports::admin::GoalAdminRow) -> Self {
        GoalOut {
            id: g.id,
            week_id: g.week_id,
            title: g.title,
            description: g.description,
            thumbnail: g.thumbnail,
            reward: g.reward,
            total_steps: g.total_steps,
            sort_order: g.sort_order,
            kind: g.kind,
        }
    }
}

fn default_goal_kind() -> String {
    "manual".to_string()
}

#[derive(Debug, Deserialize)]
pub(super) struct GoalBody {
    #[serde(rename = "weekId")]
    week_id: i32,
    title: String,
    #[serde(default)]
    description: String,
    #[serde(default)]
    thumbnail: String,
    reward: String,
    #[serde(rename = "totalSteps")]
    total_steps: i32,
    #[serde(rename = "sortOrder", default)]
    sort_order: i32,
    #[serde(default = "default_goal_kind")]
    kind: String,
}

#[derive(Debug, Deserialize)]
pub(super) struct GoalUpdateBody {
    title: String,
    #[serde(default)]
    description: String,
    #[serde(default)]
    thumbnail: String,
    reward: String,
    #[serde(rename = "totalSteps")]
    total_steps: i32,
    #[serde(rename = "sortOrder", default)]
    sort_order: i32,
    #[serde(default = "default_goal_kind")]
    kind: String,
}

const VALID_GOAL_KINDS: [&str; 4] = ["manual", "login", "scene_visit", "purchase"];

fn validate_goal_kind(raw: &str) -> Result<String, ApiError> {
    let k = raw.trim().to_lowercase();
    if VALID_GOAL_KINDS.contains(&k.as_str()) {
        Ok(k)
    } else {
        Err(ApiError::bad_request(
            "kind must be manual, login, scene_visit, or purchase",
        ))
    }
}

fn validate_goal_common(title: &str, reward: &str, total_steps: i32) -> Result<String, ApiError> {
    if title.trim().is_empty() || title.len() > 300 {
        return Err(ApiError::bad_request("title must be 1..300 chars"));
    }
    if !(1..=100_000).contains(&total_steps) {
        return Err(ApiError::bad_request("totalSteps out of range"));
    }
    validate_max_mana(reward).map_err(|_| ApiError::bad_request("invalid reward"))
}

#[derive(Debug, Deserialize)]
pub(super) struct GoalListQuery {
    #[serde(rename = "weekId")]
    week_id: Option<i32>,
}

pub(super) async fn list_goals(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(q): Query<GoalListQuery>,
) -> Result<Json<Vec<GoalOut>>, ApiError> {
    authorize_admin(&state, &headers)?;
    let rows = state.credits.admin_list_goals(q.week_id).await?;
    Ok(Json(rows.into_iter().map(GoalOut::from).collect()))
}

pub(super) async fn create_goal(
    State(state): State<AppState>,
    headers: HeaderMap,
    body: Option<Json<GoalBody>>,
) -> Result<(StatusCode, Json<GoalOut>), ApiError> {
    authorize_admin(&state, &headers)?;
    let Json(b) = body.ok_or_else(|| ApiError::bad_request("missing JSON body"))?;
    let reward = validate_goal_common(&b.title, &b.reward, b.total_steps)?;
    let kind = validate_goal_kind(&b.kind)?;
    let detail = json!({ "weekId": b.week_id, "title": b.title, "reward": reward, "kind": kind });
    let goal = state
        .credits
        .admin_create_goal(
            b.week_id,
            b.title.trim(),
            &b.description,
            &b.thumbnail,
            &reward,
            b.total_steps,
            b.sort_order,
            &kind,
            &detail,
        )
        .await?;
    Ok((StatusCode::CREATED, Json(goal.into())))
}

pub(super) async fn update_goal(
    State(state): State<AppState>,
    Path(id): Path<i32>,
    headers: HeaderMap,
    body: Option<Json<GoalUpdateBody>>,
) -> Result<Json<GoalOut>, ApiError> {
    authorize_admin(&state, &headers)?;
    let Json(b) = body.ok_or_else(|| ApiError::bad_request("missing JSON body"))?;
    let reward = validate_goal_common(&b.title, &b.reward, b.total_steps)?;
    let kind = validate_goal_kind(&b.kind)?;
    let detail = json!({ "id": id, "title": b.title, "reward": reward, "kind": kind });
    let goal = state
        .credits
        .admin_update_goal(
            id,
            b.title.trim(),
            &b.description,
            &b.thumbnail,
            &reward,
            b.total_steps,
            b.sort_order,
            &kind,
            &detail,
        )
        .await?;
    Ok(Json(goal.into()))
}

pub(super) async fn delete_goal(
    State(state): State<AppState>,
    Path(id): Path<i32>,
    headers: HeaderMap,
) -> Result<StatusCode, ApiError> {
    authorize_admin(&state, &headers)?;
    let detail = json!({ "id": id });
    state.credits.admin_delete_goal(id, &detail).await?;
    Ok(StatusCode::NO_CONTENT)
}

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
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<Vec<PackOut>>, ApiError> {
    authorize_admin(&state, &headers)?;
    let rows = state.credits.admin_list_packs().await?;
    Ok(Json(rows.into_iter().map(PackOut::from).collect()))
}

pub(super) async fn create_pack(
    State(state): State<AppState>,
    headers: HeaderMap,
    body: Option<Json<PackCreateBody>>,
) -> Result<(StatusCode, Json<PackOut>), ApiError> {
    authorize_admin(&state, &headers)?;
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
    State(state): State<AppState>,
    Path(sku): Path<String>,
    headers: HeaderMap,
    body: Option<Json<PackUpdateBody>>,
) -> Result<Json<PackOut>, ApiError> {
    authorize_admin(&state, &headers)?;
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
    State(state): State<AppState>,
    Path(sku): Path<String>,
    headers: HeaderMap,
) -> Result<StatusCode, ApiError> {
    authorize_admin(&state, &headers)?;
    let sku = validate_sku(&sku)?;
    let detail = json!({ "sku": sku });
    tracing::info!(action = "pack.delete", sku = %sku, "admin pack delete");
    state.credits.admin_delete_pack(&sku, &detail).await?;
    Ok(StatusCode::NO_CONTENT)
}
