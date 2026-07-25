use std::sync::Arc;

use axum::extract::{Request, State};
use axum::response::IntoResponse;
use axum::Json;
use serde_json::json;

use crate::errors::{AppResult, InvalidRequestError};
use crate::query_params::{parse_query_string, qs_get_array};
use crate::state::AppState;

const MAX_AVAILABLE_CONTENT_CIDS: usize = 1000;

pub async fn get_available_content(
    State(state): State<Arc<AppState>>,
    request: Request,
) -> AppResult<impl IntoResponse> {
    let query_string = request.uri().query().unwrap_or("");
    let params = parse_query_string(query_string);
    let cids = qs_get_array(&params, "cid");

    if cids.is_empty() {
        return Err(InvalidRequestError::new("Please set at least one cid.").into());
    }
    if cids.len() > MAX_AVAILABLE_CONTENT_CIDS {
        return Err(InvalidRequestError::new(format!(
            "Too many cids requested; the maximum allowed is {}.",
            MAX_AVAILABLE_CONTENT_CIDS
        ))
        .into());
    }

    let available_cids: Vec<String> = cids
        .into_iter()
        .filter(|cid| !state.denylist.is_denylisted(cid))
        .collect();

    let existence = state.storage.exist_multiple(&available_cids).await;

    let result: Vec<serde_json::Value> = available_cids
        .iter()
        .map(|cid| {
            json!({
                "cid": cid,
                "available": existence.get(cid).copied().unwrap_or(false),
            })
        })
        .collect();

    Ok(Json(result))
}
