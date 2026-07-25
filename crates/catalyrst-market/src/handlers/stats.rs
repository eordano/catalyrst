use axum::extract::{Path, Query, State};
use axum::http::header::{CACHE_CONTROL, CONTENT_LENGTH, CONTENT_TYPE, ETAG, LAST_MODIFIED};
use axum::http::HeaderMap;
use axum::response::{IntoResponse, Response};

use crate::http::response::ApiError;
use crate::ports::stats::{parse_category, parse_filters, parse_stat, StatsEnvelope};
use crate::AppState;

const MAX_AGE: u64 = 3600;

pub async fn get_stats(
    State(state): State<AppState>,
    Path((category, stat)): Path<(String, String)>,
    Query(pairs): Query<Vec<(String, String)>>,
) -> Result<Response, ApiError> {
    let cat = parse_category(&category);
    let st = parse_stat(&stat);
    let filters = parse_filters(&pairs)?;
    let data = state.stats.fetch(cat, st, &filters).await?;

    let data_string = serde_json::to_string(&data).unwrap_or_else(|_| "null".to_string());
    let etag_value = format!("W/\"{}-{:x}\"", data_string.len(), fnv1a(&data_string));

    let body = serde_json::to_vec(&StatsEnvelope { data }).unwrap_or_default();
    let content_length = body.len();

    let mut headers = HeaderMap::new();
    headers.insert(
        CACHE_CONTROL,
        format!("public,max-age={MAX_AGE},s-maxage={MAX_AGE}")
            .parse()
            .unwrap(),
    );
    headers.insert(CONTENT_TYPE, "application/json".parse().unwrap());
    headers.insert(LAST_MODIFIED, httpdate_now().parse().unwrap());
    if let Ok(v) = etag_value.parse() {
        headers.insert(ETAG, v);
    }
    headers.insert(CONTENT_LENGTH, content_length.to_string().parse().unwrap());

    Ok((headers, body).into_response())
}

fn fnv1a(s: &str) -> u64 {
    let mut hash: u64 = 0xcbf29ce484222325;
    for b in s.as_bytes() {
        hash ^= *b as u64;
        hash = hash.wrapping_mul(0x100000001b3);
    }
    hash
}

fn httpdate_now() -> String {
    use chrono::Utc;
    Utc::now().format("%a, %d %b %Y %H:%M:%S GMT").to_string()
}
