use axum::extract::{Path, Query, State};
use axum::http::{HeaderMap, StatusCode};
use axum::routing::{get, post};
use axum::{Json, Router};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value as JsonValue};

use crate::http::ApiError;
use crate::AppState;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/admin/seasons", get(list_seasons).post(create_season))
        .route(
            "/admin/seasons/{id}",
            axum::routing::put(update_season).delete(delete_season),
        )
        .route("/admin/goals", get(list_goals).post(create_goal))
        .route(
            "/admin/goals/{id}",
            axum::routing::put(update_goal).delete(delete_goal),
        )
        .route("/admin/credits/grant", post(grant_credits))
        .route("/admin/credits/revoke", post(revoke_credits))
        .route("/admin/users/{address}/block", post(block_user))
        .route("/admin/packs", get(list_packs).post(create_pack))
        .route(
            "/admin/packs/{sku}",
            axum::routing::put(update_pack).delete(delete_pack),
        )
        .route("/admin/purchases", get(list_purchases))
        .route("/admin/checkouts", get(list_checkouts))
        .route("/admin/ledger", get(list_ledger))
        .route("/admin/checkouts/{id}/refund", post(refund_checkout))
        .route(
            "/admin/checkouts/{id}/force-fulfill",
            post(force_fulfill_checkout),
        )
        .route("/admin/grants/{escrow_ref}/reclaim", post(reclaim_grant))
        .route("/admin/grants/{escrow_ref}/release", post(release_grant))
        .route("/admin/reconcile", get(reconcile))
}

fn bearer_token(headers: &HeaderMap) -> Option<&str> {
    headers
        .get("authorization")
        .and_then(|v| v.to_str().ok())
        .and_then(|s| s.strip_prefix("Bearer "))
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
    authorize_with_token(state.admin_token.as_deref(), headers)
}

fn authorize_with_token(expected: Option<&str>, headers: &HeaderMap) -> Result<(), ApiError> {
    let Some(expected) = expected else {
        return Err(ApiError::forbidden(
            "admin controls are disabled (CATALYRST_CREDITS_ADMIN_TOKEN unset)",
        ));
    };
    match bearer_token(headers) {
        Some(token) if timing_safe_eq(token, expected) => Ok(()),
        _ => Err(ApiError::forbidden("invalid admin token")),
    }
}

fn clean_actor(raw: &str) -> Option<String> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed.chars().take(100).collect())
    }
}

fn admin_actor(headers: &HeaderMap) -> Option<String> {
    headers
        .get("x-catalyrst-admin")
        .and_then(|v| v.to_str().ok())
        .and_then(clean_actor)
}

fn validate_idempotency_key(raw: &Option<String>) -> Result<Option<String>, ApiError> {
    match raw {
        None => Ok(None),
        Some(k) => {
            let t = k.trim();
            if t.is_empty() {
                Ok(None)
            } else if t.len() > 200 {
                Err(ApiError::bad_request("idempotencyKey too long (max 200)"))
            } else if !t.chars().all(|c| c.is_ascii_graphic()) {
                Err(ApiError::bad_request(
                    "idempotencyKey must be printable ASCII",
                ))
            } else {
                Ok(Some(t.to_string()))
            }
        }
    }
}

fn normalize_address(raw: &str) -> Result<String, ApiError> {
    let a = raw.trim().to_lowercase();
    let ok = a.len() == 42 && a.starts_with("0x") && a[2..].bytes().all(|b| b.is_ascii_hexdigit());
    if ok {
        Ok(a)
    } else {
        Err(ApiError::bad_request("invalid wallet address"))
    }
}

pub(crate) fn validate_positive_amount(raw: &str) -> Result<String, ApiError> {
    let s = raw.trim();
    if s.is_empty() || s.len() > 78 {
        return Err(ApiError::bad_request("invalid amount"));
    }
    let mut seen_dot = false;
    let mut any_digit = false;
    let mut any_nonzero = false;
    for c in s.chars() {
        match c {
            '0'..='9' => {
                any_digit = true;
                if c != '0' {
                    any_nonzero = true;
                }
            }
            '.' if !seen_dot => seen_dot = true,
            _ => return Err(ApiError::bad_request("invalid amount")),
        }
    }
    if !any_digit || !any_nonzero {
        return Err(ApiError::bad_request("amount must be a positive number"));
    }
    Ok(s.to_string())
}

fn validate_max_mana(raw: &str) -> Result<String, ApiError> {
    let s = raw.trim();
    if s.is_empty() || s.len() > 78 {
        return Err(ApiError::bad_request("invalid maxMana"));
    }
    let mut seen_dot = false;
    let mut any_digit = false;
    for c in s.chars() {
        match c {
            '0'..='9' => any_digit = true,
            '.' if !seen_dot => seen_dot = true,
            _ => return Err(ApiError::bad_request("invalid maxMana")),
        }
    }
    if !any_digit {
        return Err(ApiError::bad_request("invalid maxMana"));
    }
    Ok(s.to_string())
}

const VALID_SEASON_STATES: [&str; 3] = ["NOT_STARTED", "IN_PROGRESS", "FINISHED"];

fn validate_season_state(raw: &str) -> Result<String, ApiError> {
    let s = raw.trim().to_uppercase();
    if VALID_SEASON_STATES.contains(&s.as_str()) {
        Ok(s)
    } else {
        Err(ApiError::bad_request(
            "state must be NOT_STARTED, IN_PROGRESS, or FINISHED",
        ))
    }
}

#[derive(Debug, Serialize)]
struct SeasonOut {
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
struct SeasonBody {
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

async fn list_seasons(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<Vec<SeasonOut>>, ApiError> {
    authorize_admin(&state, &headers)?;
    let rows = state.credits.admin_list_seasons().await?;
    Ok(Json(rows.into_iter().map(SeasonOut::from).collect()))
}

async fn create_season(
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

async fn update_season(
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

async fn delete_season(
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
struct GoalOut {
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
struct GoalBody {
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
struct GoalUpdateBody {
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
struct GoalListQuery {
    #[serde(rename = "weekId")]
    week_id: Option<i32>,
}

async fn list_goals(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(q): Query<GoalListQuery>,
) -> Result<Json<Vec<GoalOut>>, ApiError> {
    authorize_admin(&state, &headers)?;
    let rows = state.credits.admin_list_goals(q.week_id).await?;
    Ok(Json(rows.into_iter().map(GoalOut::from).collect()))
}

async fn create_goal(
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

async fn update_goal(
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

async fn delete_goal(
    State(state): State<AppState>,
    Path(id): Path<i32>,
    headers: HeaderMap,
) -> Result<StatusCode, ApiError> {
    authorize_admin(&state, &headers)?;
    let detail = json!({ "id": id });
    state.credits.admin_delete_goal(id, &detail).await?;
    Ok(StatusCode::NO_CONTENT)
}

#[derive(Debug, Deserialize)]
struct GrantBody {
    address: String,

    amount: String,
    #[serde(default)]
    reason: Option<String>,

    #[serde(rename = "idempotencyKey", default)]
    idempotency_key: Option<String>,
}

#[derive(Debug, Serialize)]
struct GrantOut {
    address: String,
    applied: String,
    available: String,
    replayed: bool,
}

fn validated_reason(reason: &Option<String>) -> Result<Option<String>, ApiError> {
    match reason {
        None => Ok(None),
        Some(r) => {
            let t = r.trim();
            if t.is_empty() {
                Ok(None)
            } else if t.len() > 500 {
                Err(ApiError::bad_request("reason too long (max 500)"))
            } else {
                Ok(Some(t.to_string()))
            }
        }
    }
}

async fn grant_credits(
    State(state): State<AppState>,
    headers: HeaderMap,
    body: Option<Json<GrantBody>>,
) -> Result<Json<GrantOut>, ApiError> {
    authorize_admin(&state, &headers)?;
    let Json(b) = body.ok_or_else(|| ApiError::bad_request("missing JSON body"))?;
    let address = normalize_address(&b.address)?;
    let amount = validate_positive_amount(&b.amount)?;
    let reason = validated_reason(&b.reason)?;
    let idempotency_key = validate_idempotency_key(&b.idempotency_key)?;
    let actor = admin_actor(&headers);
    let detail = json!({
        "address": address, "amount": amount, "reason": reason,
        "idempotencyKey": idempotency_key,
    });
    let outcome = state
        .credits
        .admin_grant_credits(
            &address,
            &amount,
            "grant",
            reason.as_deref(),
            actor.as_deref(),
            idempotency_key.as_deref(),
            &detail,
        )
        .await?;
    Ok(Json(GrantOut {
        address,
        applied: outcome.applied,
        available: outcome.available,
        replayed: outcome.replayed,
    }))
}

async fn revoke_credits(
    State(state): State<AppState>,
    headers: HeaderMap,
    body: Option<Json<GrantBody>>,
) -> Result<Json<GrantOut>, ApiError> {
    authorize_admin(&state, &headers)?;
    let Json(b) = body.ok_or_else(|| ApiError::bad_request("missing JSON body"))?;
    let address = normalize_address(&b.address)?;
    let amount = validate_positive_amount(&b.amount)?;
    let reason = validated_reason(&b.reason)?;
    let actor = admin_actor(&headers);
    let detail = json!({ "address": address, "amount": amount, "reason": reason });
    let outcome = state
        .credits
        .admin_revoke_credits(
            &address,
            &amount,
            reason.as_deref(),
            actor.as_deref(),
            &detail,
        )
        .await?;
    Ok(Json(GrantOut {
        address,
        applied: outcome.applied,
        available: outcome.available,
        replayed: outcome.replayed,
    }))
}

#[derive(Debug, Deserialize)]
struct BlockBody {
    blocked: bool,
    #[serde(default)]
    reason: Option<String>,
}

#[derive(Debug, Serialize)]
struct BlockOut {
    address: String,
    blocked: bool,
}

async fn block_user(
    State(state): State<AppState>,
    Path(address): Path<String>,
    headers: HeaderMap,
    body: Option<Json<BlockBody>>,
) -> Result<Json<BlockOut>, ApiError> {
    authorize_admin(&state, &headers)?;
    let address = normalize_address(&address)?;
    let Json(b) = body.ok_or_else(|| ApiError::bad_request("missing JSON body { blocked }"))?;
    let reason = validated_reason(&b.reason)?;
    let actor = admin_actor(&headers);
    let detail = json!({ "address": address, "blocked": b.blocked, "reason": reason });
    let blocked = state
        .credits
        .admin_set_blocked(
            &address,
            b.blocked,
            reason.as_deref(),
            actor.as_deref(),
            &detail,
        )
        .await?;
    Ok(Json(BlockOut { address, blocked }))
}

fn validate_sku(raw: &str) -> Result<String, ApiError> {
    let s = raw.trim();
    if s.is_empty() || s.len() > 100 {
        return Err(ApiError::bad_request("invalid sku"));
    }
    if !s
        .chars()
        .all(|c| c.is_ascii_graphic() && c != '/' && c != '\\')
    {
        return Err(ApiError::bad_request("invalid sku"));
    }
    Ok(s.to_string())
}

fn validate_escrow_ref(raw: &str) -> Result<String, ApiError> {
    let s = raw.trim();
    if s.is_empty() || s.len() > 200 {
        return Err(ApiError::bad_request("invalid escrowRef"));
    }
    if !s
        .chars()
        .all(|c| c.is_ascii_graphic() && c != '/' && c != '\\')
    {
        return Err(ApiError::bad_request("invalid escrowRef"));
    }
    Ok(s.to_string())
}

fn validate_price_cents(v: i64) -> Result<i64, ApiError> {
    if v < 0 {
        return Err(ApiError::bad_request("priceCents must be >= 0"));
    }
    Ok(v)
}

fn validate_currency(raw: &str) -> Result<String, ApiError> {
    let s = raw.trim().to_lowercase();
    if s.is_empty() || s.len() > 10 || !s.chars().all(|c| c.is_ascii_alphabetic()) {
        return Err(ApiError::bad_request("invalid currency"));
    }
    Ok(s)
}

fn paginate(limit: Option<i64>, offset: Option<i64>) -> (i64, i64) {
    let limit = limit.unwrap_or(50).clamp(1, 200);
    let offset = offset.unwrap_or(0).max(0);
    (limit, offset)
}

#[derive(Debug, Serialize)]
struct PackOut {
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
struct PackCreateBody {
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
struct PackUpdateBody {
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

async fn list_packs(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<Vec<PackOut>>, ApiError> {
    authorize_admin(&state, &headers)?;
    let rows = state.credits.admin_list_packs().await?;
    Ok(Json(rows.into_iter().map(PackOut::from).collect()))
}

async fn create_pack(
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

async fn update_pack(
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

async fn delete_pack(
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

#[derive(Debug, Deserialize)]
struct PurchaseListQuery {
    status: Option<String>,
    address: Option<String>,
    limit: Option<i64>,
    offset: Option<i64>,
}

#[derive(Debug, Serialize)]
struct PurchaseOut {
    id: i64,
    address: String,
    sku: String,
    credits: String,
    #[serde(rename = "amountCents")]
    amount_cents: i64,
    currency: String,
    #[serde(rename = "stripePaymentIntent")]
    stripe_payment_intent: Option<String>,
    method: String,
    status: String,
    #[serde(rename = "createdAt")]
    created_at: String,
}

async fn list_purchases(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(q): Query<PurchaseListQuery>,
) -> Result<Json<Vec<PurchaseOut>>, ApiError> {
    authorize_admin(&state, &headers)?;
    let address = match q.address.as_deref() {
        Some(a) => Some(normalize_address(a)?),
        None => None,
    };
    let (limit, offset) = paginate(q.limit, q.offset);
    let rows = state
        .credits
        .admin_list_purchases(q.status.as_deref(), address.as_deref(), limit, offset)
        .await?;
    let out = rows
        .into_iter()
        .map(|p| PurchaseOut {
            id: p.id,
            address: p.address,
            sku: p.sku,
            credits: p.credits,
            amount_cents: p.amount_cents,
            currency: p.currency,
            stripe_payment_intent: p.stripe_payment_intent,
            method: p.method,
            status: p.status,
            created_at: p.created_at.to_rfc3339(),
        })
        .collect();
    Ok(Json(out))
}

#[derive(Debug, Deserialize)]
struct CheckoutListQuery {
    address: Option<String>,
    status: Option<String>,
    limit: Option<i64>,
    offset: Option<i64>,
}

#[derive(Debug, Serialize)]
struct OutboxLineOut {
    id: i64,
    #[serde(rename = "itemId")]
    item_id: String,
    urn: String,
    #[serde(rename = "tokenId")]
    token_id: Option<String>,
    #[serde(rename = "unitPriceCredits")]
    unit_price_credits: String,
    mode: String,
    status: String,
    attempts: i32,
    #[serde(rename = "lastError")]
    last_error: Option<String>,
    #[serde(rename = "externalRef")]
    external_ref: Option<String>,
}

#[derive(Debug, Serialize)]
struct CheckoutOut {
    id: i64,
    address: String,
    #[serde(rename = "totalCredits")]
    total_credits: String,
    status: String,
    #[serde(rename = "createdAt")]
    created_at: String,
    lines: Vec<OutboxLineOut>,
}

async fn list_checkouts(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(q): Query<CheckoutListQuery>,
) -> Result<Json<Vec<CheckoutOut>>, ApiError> {
    authorize_admin(&state, &headers)?;
    let address = match q.address.as_deref() {
        Some(a) => Some(normalize_address(a)?),
        None => None,
    };
    let (limit, offset) = paginate(q.limit, q.offset);
    let rows = state
        .credits
        .admin_list_checkouts(address.as_deref(), q.status.as_deref(), limit, offset)
        .await?;
    let out = rows
        .into_iter()
        .map(|c| CheckoutOut {
            id: c.id,
            address: c.address,
            total_credits: c.total_credits,
            status: c.status,
            created_at: c.created_at.to_rfc3339(),
            lines: c
                .lines
                .into_iter()
                .map(|l| OutboxLineOut {
                    id: l.id,
                    item_id: l.item_id,
                    urn: l.urn,
                    token_id: l.token_id,
                    unit_price_credits: l.unit_price_credits,
                    mode: l.mode,
                    status: l.status,
                    attempts: l.attempts,
                    last_error: l.last_error,
                    external_ref: l.external_ref,
                })
                .collect(),
        })
        .collect();
    Ok(Json(out))
}

#[derive(Debug, Deserialize)]
struct LedgerListQuery {
    address: String,
    limit: Option<i64>,
    offset: Option<i64>,
}

#[derive(Debug, Serialize)]
struct LedgerOut {
    id: i64,
    address: String,
    kind: String,
    amount: String,
    #[serde(rename = "txRef")]
    tx_ref: Option<String>,
    #[serde(rename = "createdAt")]
    created_at: String,
}

async fn list_ledger(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(q): Query<LedgerListQuery>,
) -> Result<Json<Vec<LedgerOut>>, ApiError> {
    authorize_admin(&state, &headers)?;
    let address = normalize_address(&q.address)?;
    let (limit, offset) = paginate(q.limit, q.offset);
    let rows = state
        .credits
        .admin_list_ledger(&address, limit, offset)
        .await?;
    let out = rows
        .into_iter()
        .map(|e| LedgerOut {
            id: e.id,
            address: e.address,
            kind: e.kind,
            amount: e.amount,
            tx_ref: e.tx_ref,
            created_at: e.created_at.to_rfc3339(),
        })
        .collect();
    Ok(Json(out))
}

#[derive(Debug, Deserialize, Default)]
struct ManualOpBody {
    #[serde(default)]
    reason: Option<String>,
}

async fn refund_checkout(
    State(state): State<AppState>,
    Path(id): Path<i64>,
    headers: HeaderMap,
    body: Option<Json<ManualOpBody>>,
) -> Result<Json<JsonValue>, ApiError> {
    authorize_admin(&state, &headers)?;
    let reason = validated_reason(&body.map(|b| b.0.reason).unwrap_or(None))?;
    let actor = admin_actor(&headers);

    let checkout = state
        .credits
        .get_checkout(id)
        .await?
        .ok_or_else(|| ApiError::not_found("checkout not found"))?;

    if checkout.status != "fulfilling" && checkout.status != "fulfilled" {
        return Err(ApiError::conflict(
            "checkout is not refundable (only a debited, not-yet-reversed checkout \
             in 'fulfilling'/'fulfilled' can be manually refunded)",
        ));
    }

    let idem = format!("admin:refund:{}", id);
    let tx_ref = format!("checkout:{}", id);
    tracing::info!(
        action = "checkout.refund",
        checkout_id = id,
        "admin manual refund"
    );
    let outcome = state
        .credits
        .refund(
            &checkout.address,
            &checkout.total_credits,
            &tx_ref,
            Some(&idem),
        )
        .await?;

    let detail = json!({
        "checkoutId": id, "address": checkout.address,
        "amount": checkout.total_credits, "replayed": outcome.replayed, "reason": reason,
    });
    state
        .credits
        .admin_audit_op(
            "checkout.refund",
            Some(&checkout.address),
            Some(id),
            Some(&checkout.total_credits),
            actor.as_deref(),
            &detail,
        )
        .await?;

    Ok(Json(json!({
        "checkoutId": id,
        "address": checkout.address,
        "refunded": checkout.total_credits,
        "available": outcome.available,
        "replayed": outcome.replayed,
    })))
}

async fn force_fulfill_checkout(
    State(state): State<AppState>,
    Path(id): Path<i64>,
    headers: HeaderMap,
    body: Option<Json<ManualOpBody>>,
) -> Result<Json<JsonValue>, ApiError> {
    authorize_admin(&state, &headers)?;
    let reason = validated_reason(&body.map(|b| b.0.reason).unwrap_or(None))?;
    let actor = admin_actor(&headers);

    tracing::info!(
        action = "checkout.force_fulfill",
        checkout_id = id,
        "admin force-fulfill"
    );
    let rearmed = state.credits.admin_force_fulfill(id).await?;

    let detail = json!({ "checkoutId": id, "rearmedLines": rearmed, "reason": reason });
    state
        .credits
        .admin_audit_op(
            "checkout.force_fulfill",
            None,
            Some(id),
            None,
            actor.as_deref(),
            &detail,
        )
        .await?;

    Ok(Json(json!({ "checkoutId": id, "rearmedLines": rearmed })))
}

fn escrow_deps(state: &AppState) -> Result<(&sqlx::PgPool, &str), ApiError> {
    let Some(pool) = state.usage_grants_pool.as_ref() else {
        return Err(ApiError::not_implemented(
            "escrow ops disabled (USAGE_GRANTS_PG_CONNECTION_STRING unset)",
        ));
    };
    let Some(token) = state.economy_admin_token.as_deref() else {
        return Err(ApiError::not_implemented(
            "escrow ops disabled (CATALYRST_ECONOMY_ADMIN_TOKEN unset)",
        ));
    };
    Ok((pool, token))
}

async fn reclaim_grant(
    State(state): State<AppState>,
    Path(escrow_ref): Path<String>,
    headers: HeaderMap,
    body: Option<Json<ManualOpBody>>,
) -> Result<Json<JsonValue>, ApiError> {
    authorize_admin(&state, &headers)?;
    let escrow_ref = validate_escrow_ref(&escrow_ref)?;
    let reason = validated_reason(&body.map(|b| b.0.reason).unwrap_or(None))?;
    let actor = admin_actor(&headers);
    let (pool, token) = escrow_deps(&state)?;

    let grant = crate::ports::admin::fetch_usage_grant(pool, &escrow_ref)
        .await?
        .ok_or_else(|| ApiError::not_found("usage_grant not found"))?;

    if grant.status != "active" && grant.status != "revoked" {
        return Err(ApiError::conflict(
            "usage_grant is not reclaimable (must be active or a resumable revoked)",
        ));
    }
    let (Some(collection), Some(token_id)) =
        (grant.collection.as_deref(), grant.token_id.as_deref())
    else {
        return Err(ApiError::bad_request(
            "grant has no on-chain token_id/collection; cannot reclaim (primary mint pending)",
        ));
    };

    let idem = format!("admin:reclaim:{}", escrow_ref);
    tracing::info!(action = "grant.reclaim", escrow_ref = %escrow_ref, "admin reclaim");
    let tx_hash = crate::ports::escrow::reclaim_escrowed(
        &state.economy_http,
        &state.economy_base_url,
        token,
        collection,
        token_id,
        &idem,
    )
    .await?;

    state.credits.revoke_usage_grant(pool, &escrow_ref).await?;

    let refunded = match state
        .credits
        .find_confirmed_line_by_ref(&escrow_ref)
        .await?
    {
        Some((address, amount)) => {
            let tx_ref = format!("reclaim:{}", escrow_ref);
            state
                .credits
                .refund(&address, &amount, &tx_ref, Some(&idem))
                .await?;
            Some((address, amount))
        }
        None => None,
    };

    let detail = json!({
        "escrowRef": escrow_ref, "grantee": grant.grantee_address, "urn": grant.urn,
        "txHash": tx_hash, "refunded": refunded.as_ref().map(|(_, a)| a.clone()),
        "reason": reason,
    });
    state
        .credits
        .admin_audit_op(
            "grant.reclaim",
            refunded.as_ref().map(|(a, _)| a.as_str()),
            None,
            refunded.as_ref().map(|(_, a)| a.as_str()),
            actor.as_deref(),
            &detail,
        )
        .await?;

    Ok(Json(json!({
        "escrowRef": escrow_ref,
        "txHash": tx_hash,
        "refunded": refunded.map(|(addr, amt)| json!({ "address": addr, "amount": amt })),
    })))
}

async fn release_grant(
    State(state): State<AppState>,
    Path(escrow_ref): Path<String>,
    headers: HeaderMap,
    body: Option<Json<ManualOpBody>>,
) -> Result<Json<JsonValue>, ApiError> {
    authorize_admin(&state, &headers)?;
    let escrow_ref = validate_escrow_ref(&escrow_ref)?;
    let reason = validated_reason(&body.map(|b| b.0.reason).unwrap_or(None))?;
    let actor = admin_actor(&headers);
    let (pool, token) = escrow_deps(&state)?;

    let grant = crate::ports::admin::fetch_usage_grant(pool, &escrow_ref)
        .await?
        .ok_or_else(|| ApiError::not_found("usage_grant not found"))?;

    if grant.status != "active" {
        return Err(ApiError::conflict(
            "usage_grant is not releasable (must be active)",
        ));
    }
    if grant.unlock_at > chrono::Utc::now() {
        return Err(ApiError::conflict(format!(
            "usage_grant is still in the return window; release is not allowed until unlock_at ({}). Use reclaim to return during the window.",
            grant.unlock_at.to_rfc3339()
        )));
    }
    let (Some(collection), Some(token_id)) =
        (grant.collection.as_deref(), grant.token_id.as_deref())
    else {
        return Err(ApiError::bad_request(
            "grant has no on-chain token_id/collection; cannot release (primary mint pending)",
        ));
    };

    let idem = format!("admin:release:{}", escrow_ref);
    tracing::info!(action = "grant.release", escrow_ref = %escrow_ref, "admin release");
    let tx_hash = crate::ports::escrow::release_escrowed(
        &state.economy_http,
        &state.economy_base_url,
        token,
        collection,
        token_id,
        &grant.grantee_address,
        &idem,
    )
    .await?;
    crate::ports::admin::mark_usage_grant_released(pool, &escrow_ref).await?;

    let detail = json!({
        "escrowRef": escrow_ref, "grantee": grant.grantee_address, "urn": grant.urn,
        "txHash": tx_hash, "reason": reason,
    });
    state
        .credits
        .admin_audit_op(
            "grant.release",
            Some(&grant.grantee_address),
            None,
            None,
            actor.as_deref(),
            &detail,
        )
        .await?;

    Ok(Json(json!({ "escrowRef": escrow_ref, "txHash": tx_hash })))
}

async fn reconcile(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<crate::ports::reconcile::ReconcileReport>, ApiError> {
    authorize_admin(&state, &headers)?;
    tracing::info!(action = "reconcile", "admin reconciliation run");
    let report = state
        .credits
        .reconcile(state.usage_grants_pool.as_ref())
        .await?;
    Ok(Json(report))
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::http::HeaderValue;

    fn headers_with(auth: Option<&str>) -> HeaderMap {
        let mut h = HeaderMap::new();
        if let Some(a) = auth {
            h.insert("authorization", HeaderValue::from_str(a).unwrap());
        }
        h
    }

    #[test]
    fn unset_token_fails_closed() {
        let err = authorize_with_token(None, &headers_with(Some("Bearer anything"))).unwrap_err();
        assert!(matches!(err, ApiError::Forbidden(_)));
    }

    #[test]
    fn missing_bearer_is_forbidden() {
        let err = authorize_with_token(Some("secret"), &headers_with(None)).unwrap_err();
        assert!(matches!(err, ApiError::Forbidden(_)));
    }

    #[test]
    fn wrong_token_is_forbidden() {
        let err =
            authorize_with_token(Some("secret"), &headers_with(Some("Bearer nope"))).unwrap_err();
        assert!(matches!(err, ApiError::Forbidden(_)));
    }

    #[test]
    fn correct_token_authorizes() {
        assert!(authorize_with_token(Some("secret"), &headers_with(Some("Bearer secret"))).is_ok());
    }

    #[test]
    fn raw_token_without_bearer_prefix_is_forbidden() {
        let err = authorize_with_token(Some("secret"), &headers_with(Some("secret"))).unwrap_err();
        assert!(matches!(err, ApiError::Forbidden(_)));
    }

    #[test]
    fn validates_address() {
        assert!(normalize_address("0x1234567890abcdef1234567890abcdef12345678").is_ok());
        assert_eq!(
            normalize_address("0xABCDEF1234567890ABCDEF1234567890ABCDEF12").unwrap(),
            "0xabcdef1234567890abcdef1234567890abcdef12"
        );
        assert!(normalize_address("notanaddress").is_err());
        assert!(normalize_address("0x123").is_err());
    }

    #[test]
    fn validates_positive_amount() {
        assert_eq!(validate_positive_amount("100").unwrap(), "100");
        assert_eq!(validate_positive_amount(" 12.5 ").unwrap(), "12.5");
        assert!(validate_positive_amount("0").is_err());
        assert!(validate_positive_amount("0.0").is_err());
        assert!(validate_positive_amount("-5").is_err());
        assert!(validate_positive_amount("1e9").is_err());
        assert!(validate_positive_amount("").is_err());
    }

    #[test]
    fn validates_idempotency_key() {
        assert_eq!(validate_idempotency_key(&None).unwrap(), None);
        assert_eq!(validate_idempotency_key(&Some("  ".into())).unwrap(), None);
        assert_eq!(
            validate_idempotency_key(&Some(" grant-2026-001 ".into())).unwrap(),
            Some("grant-2026-001".to_string())
        );
        assert!(validate_idempotency_key(&Some("x".repeat(201))).is_err());
        assert!(validate_idempotency_key(&Some("bad key".into())).is_err());
        assert!(validate_idempotency_key(&Some("bad\nkey".into())).is_err());
    }

    #[test]
    fn header_actor_resolves() {
        let mut h = HeaderMap::new();
        assert_eq!(admin_actor(&h), None);
        h.insert("x-catalyrst-admin", HeaderValue::from_static("  alice  "));
        assert_eq!(admin_actor(&h).as_deref(), Some("alice"));
    }

    #[test]
    fn validates_season_state() {
        assert_eq!(validate_season_state("in_progress").unwrap(), "IN_PROGRESS");
        assert!(validate_season_state("BOGUS").is_err());
    }

    #[test]
    fn validates_sku_phase8() {
        assert_eq!(validate_sku(" pack_100 ").unwrap(), "pack_100");
        assert!(validate_sku("").is_err());
        assert!(validate_sku("a/b").is_err());
        assert!(validate_sku("a\\b").is_err());
        assert!(validate_sku(&"x".repeat(101)).is_err());
    }

    #[test]
    fn validates_escrow_ref() {
        assert_eq!(validate_escrow_ref(" 0xdeadBEEF ").unwrap(), "0xdeadBEEF");
        assert!(validate_escrow_ref("").is_err());
        assert!(validate_escrow_ref("a/b").is_err());
        assert!(validate_escrow_ref(&"x".repeat(201)).is_err());
    }

    #[test]
    fn validates_price_cents() {
        assert_eq!(validate_price_cents(0).unwrap(), 0);
        assert_eq!(validate_price_cents(999).unwrap(), 999);
        assert!(validate_price_cents(-1).is_err());
    }

    #[test]
    fn validates_currency() {
        assert_eq!(validate_currency(" USD ").unwrap(), "usd");
        assert_eq!(validate_currency("eur").unwrap(), "eur");
        assert!(validate_currency("").is_err());
        assert!(validate_currency("us1").is_err());
        assert!(validate_currency(&"a".repeat(11)).is_err());
    }

    #[test]
    fn paginates_with_bounds() {
        assert_eq!(paginate(None, None), (50, 0));
        assert_eq!(paginate(Some(10), Some(5)), (10, 5));
        assert_eq!(paginate(Some(0), Some(-3)), (1, 0));
        assert_eq!(paginate(Some(9999), None), (200, 0));
    }
}
