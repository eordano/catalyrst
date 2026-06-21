use std::sync::Arc;

use axum::extract::{Path, Request, State};
use axum::response::IntoResponse;
use axum::Json;
use serde_json::json;

use crate::errors::{AppError, AppResult, InvalidRequestError};
use crate::query_params::{parse_pagination, parse_query_string};
use crate::state::AppState;

fn is_hex_address(s: &str) -> bool {
    let rest = match s.strip_prefix("0x").or_else(|| s.strip_prefix("0X")) {
        Some(r) => r,
        None => return false,
    };
    rest.len() == 40 && rest.bytes().all(|b| b.is_ascii_hexdigit())
}

fn invalid_collection_urn_message(urn: &str) -> String {
    format!("Invalid URN format: {urn}")
}

fn is_valid_collection_urn(urn: &str) -> bool {
    let p: Vec<&str> = urn.split(':').collect();
    if p.len() < 5 || p[0] != "urn" || p[1] != "decentraland" {
        return false;
    }

    if p[2] == "off-chain" {
        return (p[3] == "base-avatars" || p[3] == "base-emotes") && !p[4].is_empty();
    }

    match p[3] {
        "collections-v1" => !p[4].is_empty(),
        "collections-v2" => is_hex_address(p[4]),

        "collections-thirdparty" => p.len() == 5 && !p[4].is_empty(),
        _ => false,
    }
}

pub async fn get_entities_by_collection(
    State(state): State<Arc<AppState>>,
    Path(collection_urn): Path<String>,
    request: Request,
) -> AppResult<impl IntoResponse> {
    if !is_valid_collection_urn(&collection_urn) {
        return Err(
            InvalidRequestError::new(invalid_collection_urn_message(&collection_urn)).into(),
        );
    }

    let query_string = request.uri().query().unwrap_or("");
    let params = parse_query_string(query_string);
    let pagination = parse_pagination(&params, 1000).map_err(InvalidRequestError::new)?;

    let result = state
        .database
        .active_entities_by_prefix(&collection_urn, pagination.offset, pagination.limit)
        .await
        .map_err(|e| AppError::Internal(e.to_string()))?;

    let entities: Vec<_> = result
        .entities
        .into_iter()
        .filter(|e| {
            e.get("id")
                .and_then(|id| id.as_str())
                .map(|id| !state.denylist.is_denylisted(id))
                .unwrap_or(true)
        })
        .collect();

    Ok(Json(json!({
        "total": result.total,
        "entities": entities,
    })))
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::http::StatusCode;
    use axum::response::IntoResponse;

    #[test]
    fn invalid_collection_urn_message_matches_upstream_text() {
        assert_eq!(
            invalid_collection_urn_message("not-a-real-urn"),
            "Invalid URN format: not-a-real-urn"
        );
    }

    #[test]
    fn not_a_real_urn_is_rejected() {
        assert!(!is_valid_collection_urn("not-a-real-urn"));
    }

    #[tokio::test]
    async fn invalid_collection_urn_response_is_400_with_upstream_body() {
        let err: AppError =
            InvalidRequestError::new(invalid_collection_urn_message("not-a-real-urn")).into();
        let resp = err.into_response();
        assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
        let bytes = axum::body::to_bytes(resp.into_body(), usize::MAX)
            .await
            .unwrap();
        let body: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(
            body,
            json!({ "error": "Invalid URN format: not-a-real-urn" })
        );
    }
}
