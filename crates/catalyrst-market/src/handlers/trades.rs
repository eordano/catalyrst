use axum::extract::{Path, Query, State};
use axum::http::HeaderMap;
use axum::Json;
use chrono::Utc;
use serde::Serialize;

use catalyrst_crypto::signed_fetch::signed_fetch_path;

use crate::auth_chain::{
    self, build_payload, AuthChainError, AuthChainErrorExt, AUTH_METADATA_HEADER,
    AUTH_TIMESTAMP_HEADER, FIVE_MINUTES,
};
use crate::http::pagination::get_number_parameter;
use crate::http::params::Params;
use crate::http::response::ApiError;
use crate::ports::trades::{
    create_trade, DbTradeListRow, Trade, TradeChainAccess, TradeCreation, TradeCreationError,
    TRADE_TYPE_PUBLIC_ITEM_ORDER, TRADE_TYPE_PUBLIC_NFT_ORDER,
};
use crate::AppState;

fn auth_error_to_api(e: AuthChainError) -> ApiError {
    match e {
        AuthChainError::Expired { .. } | AuthChainError::InvalidSignature(_) => {
            ApiError::Http(catalyrst_types::HttpError::new(401, e.message()))
        }
        AuthChainError::EipNotImplemented => {
            ApiError::Http(catalyrst_types::HttpError::new(501, e.message()))
        }
        _ => ApiError::bad_request(e.message()),
    }
}

fn creation_error_to_api(e: TradeCreationError) -> ApiError {
    let message = e.to_string();
    match e {
        TradeCreationError::SignerMismatch | TradeCreationError::InvalidSignature(_) => {
            ApiError::Http(catalyrst_types::HttpError::new(401, message))
        }
        TradeCreationError::NotTheOwner { .. } => {
            ApiError::Http(catalyrst_types::HttpError::new(403, message))
        }
        TradeCreationError::OwnershipUnverifiable(_)
        | TradeCreationError::OwnershipLookupFailed(_) => {
            ApiError::Http(catalyrst_types::HttpError::new(503, message))
        }
        TradeCreationError::Duplicate => {
            ApiError::Http(catalyrst_types::HttpError::new(409, message))
        }
        TradeCreationError::Db(_) => ApiError::Http(catalyrst_types::HttpError::new(500, message)),
        _ => ApiError::bad_request(message),
    }
}

#[derive(Debug, Serialize)]
pub struct CreatedTradeEnvelope {
    pub ok: bool,
    pub data: CreatedTrade,
}

#[derive(Debug, Serialize)]
pub struct CreatedTrade {
    pub id: String,
}

#[utoipa::path(
    post,
    path = "/v1/trades",
    tag = "market",
    responses(
        (status = 201, body = serde_json::Value),
        (status = 400, body = crate::http::response::MarketErrorBody),
        (status = 401, body = crate::http::response::MarketErrorBody),
        (status = 409, body = crate::http::response::MarketErrorBody),
        (status = 500, body = crate::http::response::MarketErrorBody)
    )
)]
pub async fn post_trade(
    State(state): State<AppState>,
    headers: HeaderMap,
    body: String,
) -> Result<(axum::http::StatusCode, Json<CreatedTradeEnvelope>), ApiError> {
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
        Some(auth_chain::CREATE_TRADE_INTENT),
    )?;

    let path = signed_fetch_path(&headers, "/v1/trades");
    let payload = build_payload("post", path.as_ref(), timestamp, metadata);

    let now = Utc::now();
    let recovered =
        auth_chain::validate_signature(&chain, &payload, timestamp, FIVE_MINUTES, now.timestamp())
            .await
            .map_err(auth_error_to_api)?;

    let trade: TradeCreation = serde_json::from_str(&body)
        .map_err(|e| ApiError::bad_request(format!("invalid trade body: {e}")))?;

    let chain = TradeChainAccess {
        http: &state.http,
        endpoints: &state.trade_rpc,
    };
    let id = create_trade(
        &state.pool,
        &trade,
        recovered.as_str(),
        now.timestamp_millis(),
        Some(&chain),
    )
    .await
    .map_err(creation_error_to_api)?;

    // A listing has to be visible to the signer's very next read, and every price
    // read goes through mv_trades -- so force a refresh now instead of waiting out
    // the 30s periodic cadence. Listings only: a bid never appears in the view, so
    // refreshing for one would be seconds of I/O for a row that does not exist.
    if trade.trade_type == TRADE_TYPE_PUBLIC_ITEM_ORDER
        || trade.trade_type == TRADE_TYPE_PUBLIC_NFT_ORDER
    {
        crate::spawn_forced_mv_trades_refresh(
            state.pool.clone(),
            state.mv_trades_refresh_lock.clone(),
        );
    }

    Ok((
        axum::http::StatusCode::CREATED,
        Json(CreatedTradeEnvelope {
            ok: true,
            data: CreatedTrade { id },
        }),
    ))
}

#[derive(Debug, Serialize)]
pub struct TradesEnvelope {
    pub ok: bool,
    pub data: TradesEnvelopeBody,
}

#[derive(Debug, Serialize)]
pub struct TradesEnvelopeBody {
    pub data: Vec<DbTradeListRow>,
    pub count: i64,
}

pub async fn get_trades(
    State(state): State<AppState>,
    Query(pairs): Query<Vec<(String, String)>>,
) -> Result<Json<TradesEnvelope>, ApiError> {
    let first = get_number_parameter("first", &pairs)?;
    let skip = get_number_parameter("skip", &pairs)?;
    let (data, count) = state.trades.list_trades(first, skip).await?;
    Ok(Json(TradesEnvelope {
        ok: true,
        data: TradesEnvelopeBody { data, count },
    }))
}

#[derive(Debug, Serialize)]
pub struct TradeEnvelope {
    pub ok: bool,
    pub data: Trade,
}

pub async fn get_trade(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<TradeEnvelope>, ApiError> {
    let data = state.trades.get_trade(&id).await?;
    Ok(Json(TradeEnvelope { ok: true, data }))
}

#[derive(Debug, Serialize)]
pub struct TradeAcceptedEnvelope {
    pub ok: bool,
    pub data: serde_json::Value,
}

pub async fn get_trade_accepted_event(
    State(state): State<AppState>,
    Path(hashed_signature): Path<String>,
    Query(pairs): Query<Vec<(String, String)>>,
) -> Result<Json<TradeAcceptedEnvelope>, ApiError> {
    let timestamp = get_number_parameter("timestamp", &pairs)?
        .unwrap_or_else(|| chrono::Utc::now().timestamp_millis());
    let p = Params::new(&pairs);
    let caller = p.get_string("caller", Some("")).unwrap_or_default();
    let data = state
        .trades
        .get_trade_accepted_event(&hashed_signature, timestamp, &caller)
        .await?;
    Ok(Json(TradeAcceptedEnvelope { ok: true, data }))
}
