use axum::extract::{Query, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::IntoResponse;
use axum::Json;
use serde::{Deserialize, Serialize};
use sqlx::PgPool;

use crate::rest::auth_chain::require_signer;
use crate::rest::http::{
    get_all, get_first, get_pagination_params, ApiError, EnvelopeData, Paginated,
};
use crate::rest::AppState;

#[derive(Serialize)]
pub struct Mute {
    pub address: String,
    pub muted_at: String,
}

#[derive(Deserialize)]
pub struct MuteBody {
    pub muted_address: String,
}

fn mutes_pool(state: &AppState) -> Result<&PgPool, ApiError> {
    state
        .mutes_pool
        .as_ref()
        .ok_or_else(|| ApiError::internal("mutes store unavailable"))
}

fn is_valid_eth_address(addr: &str) -> bool {
    addr.len() == 42 && addr.starts_with("0x") && addr[2..].chars().all(|c| c.is_ascii_hexdigit())
}

pub async fn get_mutes(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(pairs): Query<Vec<(String, String)>>,
) -> Result<impl IntoResponse, ApiError> {
    let signer = require_signer(&headers, "get", "/v1/mutes")
        .map_err(|e| ApiError::bad_request(format!("{e}")))?;
    let pool = mutes_pool(&state)?;
    let muter = signer.to_lowercase();
    let pagination = get_pagination_params(&pairs);

    let mut filter: Vec<String> = get_all(&pairs, "addresses")
        .into_iter()
        .filter(|a| !a.is_empty())
        .map(|a| a.to_lowercase())
        .collect();
    if let Some(a) = get_first(&pairs, "address").filter(|a| !a.is_empty()) {
        filter.push(a.to_lowercase());
    }
    let filter: Option<Vec<String>> = if filter.is_empty() {
        None
    } else {
        Some(filter)
    };

    let total: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM user_mutes \
         WHERE muter_address = $1 AND ($2::text[] IS NULL OR muted_address = ANY($2))",
    )
    .bind(&muter)
    .bind(&filter)
    .fetch_one(pool)
    .await?;

    let rows: Vec<(String, chrono::NaiveDateTime)> = sqlx::query_as(
        "SELECT muted_address, muted_at FROM user_mutes \
         WHERE muter_address = $1 AND ($2::text[] IS NULL OR muted_address = ANY($2)) \
         ORDER BY muted_at DESC LIMIT $3 OFFSET $4",
    )
    .bind(&muter)
    .bind(&filter)
    .bind(pagination.limit)
    .bind(pagination.offset)
    .fetch_all(pool)
    .await?;

    let results: Vec<Mute> = rows
        .into_iter()
        .map(|(address, muted_at)| Mute {
            address,
            muted_at: format!("{}Z", muted_at.format("%Y-%m-%dT%H:%M:%S%.3f")),
        })
        .collect();

    Ok(Json(EnvelopeData {
        data: Paginated::new(results, total, &pagination),
    }))
}

pub async fn add_mute(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(body): Json<MuteBody>,
) -> Result<impl IntoResponse, ApiError> {
    let signer = require_signer(&headers, "post", "/v1/mutes")
        .map_err(|e| ApiError::bad_request(format!("{e}")))?;
    let pool = mutes_pool(&state)?;
    let muter = signer.to_lowercase();
    let muted = body.muted_address.to_lowercase();
    if !is_valid_eth_address(&muted) {
        return Err(ApiError::bad_request("Invalid muted_address"));
    }
    if muted == muter {
        return Err(ApiError::bad_request("Cannot mute yourself"));
    }
    sqlx::query(
        "INSERT INTO user_mutes (muter_address, muted_address) VALUES ($1, $2) \
         ON CONFLICT (muter_address, muted_address) DO NOTHING",
    )
    .bind(&muter)
    .bind(&muted)
    .execute(pool)
    .await?;
    Ok(StatusCode::NO_CONTENT)
}

pub async fn remove_mute(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(body): Json<MuteBody>,
) -> Result<impl IntoResponse, ApiError> {
    let signer = require_signer(&headers, "delete", "/v1/mutes")
        .map_err(|e| ApiError::bad_request(format!("{e}")))?;
    let pool = mutes_pool(&state)?;
    let muter = signer.to_lowercase();
    let muted = body.muted_address.to_lowercase();
    if !is_valid_eth_address(&muted) {
        return Err(ApiError::bad_request("Invalid muted_address"));
    }
    sqlx::query("DELETE FROM user_mutes WHERE muter_address = $1 AND muted_address = $2")
        .bind(&muter)
        .bind(&muted)
        .execute(pool)
        .await?;
    Ok(StatusCode::NO_CONTENT)
}
