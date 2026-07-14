use axum::extract::{Query, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::IntoResponse;
use axum::Json;
use catalyrst_types::is_eth_address as is_valid_eth_address;
use serde::{Deserialize, Serialize};
use sqlx::PgPool;

use crate::rest::auth_chain::require_signer;
use crate::rest::handlers::error::{CommError, SignedFetchGateBody};
use crate::rest::http::{get_all, get_first, get_pagination_params, EnvelopeData, Paginated};
use crate::rest::AppState;

#[derive(Serialize, utoipa::ToSchema)]
#[cfg_attr(
    feature = "ts",
    derive(ts_rs::TS),
    ts(export, export_to = "communities/")
)]
pub struct Mute {
    pub address: String,
    pub muted_at: String,
}

#[derive(Deserialize, utoipa::ToSchema)]
#[cfg_attr(
    feature = "ts",
    derive(ts_rs::TS),
    ts(export, export_to = "communities/")
)]
pub struct MuteBody {
    pub muted_address: String,
}

fn mutes_pool(state: &AppState) -> Result<&PgPool, CommError> {
    state.mutes_pool.as_ref().ok_or_else(|| {
        CommError::status(StatusCode::INTERNAL_SERVER_ERROR, "mutes store unavailable")
    })
}

#[utoipa::path(
    get,
    path = "/v1/mutes",
    tag = "mutes",
    responses(
        (status = 200, body = EnvelopeData<Paginated<Mute>>),
        (status = 400, body = SignedFetchGateBody),
        (status = 401, body = catalyrst_types::ApiErrorBody),
        (status = 500, body = catalyrst_types::ApiErrorBody)
    )
)]
pub async fn get_mutes(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(pairs): Query<Vec<(String, String)>>,
) -> Result<impl IntoResponse, CommError> {
    let signer = require_signer(&headers, "get", "/v1/mutes").await?;
    let pool = mutes_pool(&state)?;
    let muter = signer.as_str().to_string();
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

#[utoipa::path(
    post,
    path = "/v1/mutes",
    tag = "mutes",
    request_body(content = MuteBody, description = "{ muted_address }"),
    responses(
        (status = 204),
        (status = 400, body = catalyrst_types::ApiErrorBody),
        (status = 401, body = catalyrst_types::ApiErrorBody),
        (status = 500, body = catalyrst_types::ApiErrorBody)
    )
)]
pub async fn add_mute(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(body): Json<MuteBody>,
) -> Result<impl IntoResponse, CommError> {
    let signer = require_signer(&headers, "post", "/v1/mutes").await?;
    let pool = mutes_pool(&state)?;
    let muter = signer.as_str().to_string();
    let muted = body.muted_address.to_lowercase();
    if !is_valid_eth_address(&muted) {
        return Err(CommError::bad_request("Invalid muted_address"));
    }
    if muted == muter {
        return Err(CommError::bad_request("Cannot mute yourself"));
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

#[utoipa::path(
    delete,
    path = "/v1/mutes",
    tag = "mutes",
    request_body(content = MuteBody, description = "{ muted_address }"),
    responses(
        (status = 204),
        (status = 400, body = catalyrst_types::ApiErrorBody),
        (status = 401, body = catalyrst_types::ApiErrorBody),
        (status = 500, body = catalyrst_types::ApiErrorBody)
    )
)]
pub async fn remove_mute(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(body): Json<MuteBody>,
) -> Result<impl IntoResponse, CommError> {
    let signer = require_signer(&headers, "delete", "/v1/mutes").await?;
    let pool = mutes_pool(&state)?;
    let muter = signer.as_str().to_string();
    let muted = body.muted_address.to_lowercase();
    if !is_valid_eth_address(&muted) {
        return Err(CommError::bad_request("Invalid muted_address"));
    }
    sqlx::query("DELETE FROM user_mutes WHERE muter_address = $1 AND muted_address = $2")
        .bind(&muter)
        .bind(&muted)
        .execute(pool)
        .await?;
    Ok(StatusCode::NO_CONTENT)
}
