use axum::extract::{Path, Query, State};
use axum::Json;
use serde::Serialize;

use crate::http::pagination::get_number_parameter;
use crate::http::params::Params;
use crate::http::response::ApiError;
use crate::ports::trades::{DbTradeListRow, Trade};
use crate::AppState;

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
    // `first`/`skip` are honored only when the trades_pagination flag is on;
    // otherwise list_trades ignores them and returns the full table (parity).
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
