use std::sync::Arc;

use axum::extract::{Request, State};
use axum::response::IntoResponse;
use axum::Json;

use crate::errors::{AppResult, InvalidRequestError};
use crate::query_params::{parse_query_string, qs_get_array};
use crate::state::AppState;
use crate::wire_types::AvailableContentItem;

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

    let existence = state.storage.exist_multiple(&available_cids).await?;

    let result: Vec<AvailableContentItem> = available_cids
        .iter()
        .map(|cid| AvailableContentItem {
            cid: cid.clone(),
            available: existence.get(cid).copied().unwrap_or(false),
        })
        .collect();

    Ok(Json(result))
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::http::StatusCode;
    use axum::response::IntoResponse;
    use serde_json::json;

    const TEST_CID: &str = "bafkreie4eisvkzyjuqrcendydk6vikqs2vco5lmib4nlzsxtjzofiqy2pa";

    fn request_for(cid: &str) -> Request {
        Request::builder()
            .uri(format!("/available-content?cid={cid}"))
            .body(axum::body::Body::empty())
            .unwrap()
    }

    #[tokio::test]
    async fn storage_fault_errors_instead_of_answering_absent() {
        let state = crate::test_support::app_state_with_storage(std::sync::Arc::new(
            crate::test_support::FaultyStorage,
        ));
        let err = match get_available_content(State(state), request_for(TEST_CID)).await {
            Ok(_) => panic!("a storage fault must error, not report \"available\": false"),
            Err(err) => err,
        };
        assert_eq!(
            err.into_response().status(),
            StatusCode::INTERNAL_SERVER_ERROR
        );
    }

    #[tokio::test]
    async fn missing_content_reports_available_false() {
        let state = crate::test_support::app_state_with_storage(std::sync::Arc::new(
            crate::test_support::EmptyStorage,
        ));
        let response = get_available_content(State(state), request_for(TEST_CID))
            .await
            .expect("a provable miss resolves normally")
            .into_response();
        assert_eq!(response.status(), StatusCode::OK);

        let bytes = axum::body::to_bytes(response.into_body(), 4096)
            .await
            .unwrap();
        let body: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(body, json!([{ "cid": TEST_CID, "available": false }]));
    }
}
