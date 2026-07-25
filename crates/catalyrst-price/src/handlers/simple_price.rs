use axum::extract::{Query, State};
use axum::http::header::{
    HeaderValue, CACHE_CONTROL, CONTENT_TYPE, RETRY_AFTER, X_CONTENT_TYPE_OPTIONS,
};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::Json;
use serde_json::{json, Map, Number, Value};

use crate::ports::prices::PriceSnapshot;
use crate::AppState;

struct PriceQuery {
    ids: Vec<String>,
    vs: Vec<String>,
    include_market_cap: bool,
    include_24hr_vol: bool,
    include_24hr_change: bool,
    include_last_updated_at: bool,
    precision: Option<u32>,
}

pub enum PriceError {
    MissingVsCurrencies,
    InvalidPrecision,
    Internal,
}

impl IntoResponse for PriceError {
    fn into_response(self) -> Response {
        match self {
            PriceError::MissingVsCurrencies => (
                StatusCode::UNPROCESSABLE_ENTITY,
                Json(json!({ "error": "Missing parameter vs_currencies" })),
            )
                .into_response(),
            PriceError::InvalidPrecision => (
                StatusCode::BAD_REQUEST,
                Json(json!({ "error": "Invalid value for precision" })),
            )
                .into_response(),
            PriceError::Internal => {
                let body = Json(json!({
                    "status": {
                        "error_code": 500,
                        "error_message": "Internal Server Error"
                    }
                }));
                let mut resp = (StatusCode::INTERNAL_SERVER_ERROR, body).into_response();
                resp.headers_mut()
                    .insert(RETRY_AFTER, HeaderValue::from_static("30"));
                resp
            }
        }
    }
}

fn parse_bool(v: &str) -> bool {
    matches!(v.trim().to_ascii_lowercase().as_str(), "true" | "1")
}

fn parse_query(pairs: Vec<(String, String)>) -> Result<PriceQuery, PriceError> {
    let mut q = PriceQuery {
        ids: Vec::new(),
        vs: Vec::new(),
        include_market_cap: false,
        include_24hr_vol: false,
        include_24hr_change: false,
        include_last_updated_at: false,
        precision: None,
    };
    let mut saw_vs = false;
    for (k, v) in pairs {
        match k.as_str() {
            "ids" => q.ids.extend(
                v.split(',')
                    .map(|s| s.trim().to_ascii_lowercase())
                    .filter(|s| !s.is_empty()),
            ),
            "vs_currencies" => {
                saw_vs = true;
                q.vs.extend(
                    v.split(',')
                        .map(|s| s.trim().to_ascii_lowercase())
                        .filter(|s| !s.is_empty()),
                );
            }
            "include_market_cap" => q.include_market_cap = parse_bool(&v),
            "include_24hr_vol" => q.include_24hr_vol = parse_bool(&v),
            "include_24hr_change" => q.include_24hr_change = parse_bool(&v),
            "include_last_updated_at" => q.include_last_updated_at = parse_bool(&v),
            "precision" => {
                let raw = v.trim();
                if raw.eq_ignore_ascii_case("full") {
                    q.precision = None;
                } else if let Ok(n) = raw.parse::<i64>() {
                    if !(0..=18).contains(&n) {
                        return Err(PriceError::InvalidPrecision);
                    }
                    q.precision = Some(n as u32);
                }
            }
            _ => {}
        }
    }
    if !saw_vs || q.vs.is_empty() {
        return Err(PriceError::MissingVsCurrencies);
    }
    Ok(q)
}

pub async fn simple_price(
    State(state): State<AppState>,
    Query(pairs): Query<Vec<(String, String)>>,
) -> Result<Response, PriceError> {
    let q = parse_query(pairs)?;

    let snapshot = state.prices.latest_coingecko().await.map_err(|err| {
        tracing::error!(%err, "failed to read latest price snapshot");
        PriceError::Internal
    })?;

    let mut out = Map::new();
    for id in &q.ids {
        if id != "decentraland" {
            continue;
        }
        let mut inner = Map::new();
        if let Some(ref snap) = snapshot {
            for cur in &q.vs {
                if let Some(num) = spot(snap, cur, q.precision) {
                    inner.insert(cur.clone(), num);
                    if q.include_market_cap {
                        if let Some(v) = usd_only(cur, snap.market_cap_usd, q.precision) {
                            inner.insert(format!("{cur}_market_cap"), v);
                        }
                    }
                    if q.include_24hr_vol {
                        if let Some(v) = usd_only(cur, snap.volume_24h_usd, q.precision) {
                            inner.insert(format!("{cur}_24h_vol"), v);
                        }
                    }
                    if q.include_24hr_change {
                        if let Some(v) = usd_only(cur, snap.price_change_24h_pct, q.precision) {
                            inner.insert(format!("{cur}_24h_change"), v);
                        }
                    }
                }
            }
            if q.include_last_updated_at {
                let ts = snap.source_updated_at.unwrap_or(snap.taken_at).timestamp();
                inner.insert("last_updated_at".to_string(), Value::Number(ts.into()));
            }
        }
        out.insert(id.clone(), Value::Object(inner));
    }

    let mut resp = Json(Value::Object(out)).into_response();
    let headers = resp.headers_mut();
    headers.insert(
        CONTENT_TYPE,
        HeaderValue::from_static("application/json; charset=utf-8"),
    );
    headers.insert(
        CACHE_CONTROL,
        HeaderValue::from_static("max-age=30, public, must-revalidate, s-maxage=60"),
    );
    headers.insert(X_CONTENT_TYPE_OPTIONS, HeaderValue::from_static("nosniff"));
    Ok(resp)
}

fn spot(snap: &PriceSnapshot, cur: &str, precision: Option<u32>) -> Option<Value> {
    let raw = match cur {
        "usd" => snap.mana_usd,
        "eth" => snap.mana_eth,
        "btc" => snap.mana_btc,
        _ => None,
    }?;
    number(raw, precision)
}

fn usd_only(cur: &str, raw: Option<f64>, precision: Option<u32>) -> Option<Value> {
    if cur != "usd" {
        return None;
    }
    number(raw?, precision)
}

fn number(raw: f64, precision: Option<u32>) -> Option<Value> {
    let v = match precision {
        Some(p) => {
            let factor = 10f64.powi(p as i32);
            (raw * factor).round() / factor
        }
        None => raw,
    };
    Number::from_f64(v).map(Value::Number)
}
