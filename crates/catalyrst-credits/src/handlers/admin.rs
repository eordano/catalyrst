//! High-risk financial admin controls: seasons/goals CRUD, grant/revoke credits,
//! block a user. Every route is gated by a constant-time bearer compare against
//! `CATALYRST_CREDITS_ADMIN_TOKEN`; when that env is unset the gate fails closed.

use axum::extract::{Path, Query, State};
use axum::http::{HeaderMap, StatusCode};
use axum::routing::{get, post};
use axum::{Json, Router};
use serde::{Deserialize, Serialize};
use serde_json::json;

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
}

fn bearer_token(headers: &HeaderMap) -> Option<&str> {
    headers
        .get("authorization")
        .and_then(|v| v.to_str().ok())
        .and_then(|s| s.strip_prefix("Bearer "))
}

/// Constant-time byte compare.
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

/// Fails closed with 403 when the token env is unset, and 403 on any mismatch.
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

/// Operator identity from the `X-Catalyrst-Admin` header (set by the admin
/// console). Recorded in the audit row only; authz is the bearer token above.
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

/// Validate and lowercase a wallet address (0x-prefixed 40-hex).
fn normalize_address(raw: &str) -> Result<String, ApiError> {
    let a = raw.trim().to_lowercase();
    let ok = a.len() == 42 && a.starts_with("0x") && a[2..].bytes().all(|b| b.is_ascii_hexdigit());
    if ok {
        Ok(a)
    } else {
        Err(ApiError::bad_request("invalid wallet address"))
    }
}

/// Validate a positive decimal amount. Kept as a string (never a JSON number)
/// so MANA-wei values never pass through f64. Accepts one optional decimal point.
fn validate_positive_amount(raw: &str) -> Result<String, ApiError> {
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
        }
    }
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
    let detail = json!({ "weekId": b.week_id, "title": b.title, "reward": reward });
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
    let detail = json!({ "id": id, "title": b.title, "reward": reward });
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
    /// Decimal string, never a JSON number (preserves MANA-wei precision).
    amount: String,
    #[serde(default)]
    reason: Option<String>,
    /// Honored on grant only (revoke ignores it).
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
        // No token configured => every request is rejected, even with a bearer.
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
        // Must use the `Bearer ` scheme; a bare token is rejected.
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
}
