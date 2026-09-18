use std::sync::Arc;

use axum::extract::{Request, State};
use axum::http::{header::CONTENT_TYPE, StatusCode};
use axum::response::{IntoResponse, Response};
use bytes::Bytes;

use crate::errors::{AppError, AppResult, InvalidRequestError};
use crate::handlers::get_deployments::normalize_query_string;
use crate::query_params::{
    camel_to_snake, parse_query_string, qs_get_array, qs_get_bool, qs_get_number, qs_get_string,
};
use crate::state::{AppState, PointerChangesQueryOptions};
use crate::wire_types::{HistoryPagination, PointerChangeDelta, PointerChangesResponse};

const VALID_SORTING_FIELDS: &[&str] = &["local_timestamp", "entity_timestamp"];

const VALID_SORTING_ORDERS: &[&str] = &["ASC", "DESC"];

/// Peers poll this continuously with identical queries; the 5 s deployments memo (cleared on every
/// commit) serves them one keyset scan per window instead of one per poll.
pub async fn get_pointer_changes(
    State(state): State<Arc<AppState>>,
    request: Request,
) -> AppResult<impl IntoResponse> {
    let query_string = request.uri().query().unwrap_or("").to_string();
    let cache_key = format!("pc:{}", normalize_query_string(&query_string));
    let state_in_fetch = state.clone();
    let body = state
        .deployments_cache
        .get_or_fetch(cache_key, move || async move {
            fetch_pointer_changes(&state_in_fetch, &query_string).await
        })
        .await?;
    Ok(Response::builder()
        .status(StatusCode::OK)
        .header(CONTENT_TYPE, "application/json")
        .body(axum::body::Body::from(body))
        .unwrap())
}

async fn fetch_pointer_changes(state: &AppState, query_string: &str) -> AppResult<Bytes> {
    let params = parse_query_string(query_string);

    let mut entity_types: Vec<String> = Vec::new();
    for raw in qs_get_array(&params, "entityType") {
        match crate::query_params::parse_entity_type(&raw) {
            Some(canonical) => entity_types.push(canonical.to_string()),
            None => {
                return Err(InvalidRequestError::new("Found an unrecognized entity type").into())
            }
        }
    }
    let from = qs_get_number(&params, "from");
    let to = qs_get_number(&params, "to");
    let offset = qs_get_number(&params, "offset");
    let limit = qs_get_number(&params, "limit");
    let last_id = qs_get_string(&params, "lastId").map(|s| s.to_lowercase());
    let include_auth_chain = qs_get_bool(&params, "includeAuthChain").unwrap_or(false);

    let sorting_field = if let Some(ref sf) = qs_get_string(&params, "sortingField") {
        let snake = camel_to_snake(sf);
        if !VALID_SORTING_FIELDS.contains(&snake.as_str()) {
            return Err(InvalidRequestError::new("Found an unrecognized sort field param").into());
        }
        Some(snake)
    } else {
        None
    };

    let sorting_order = if let Some(ref so) = qs_get_string(&params, "sortingOrder") {
        if !VALID_SORTING_ORDERS.contains(&so.as_str()) {
            return Err(InvalidRequestError::new("Found an unrecognized sort order param").into());
        }
        Some(so.clone())
    } else {
        None
    };

    if let Some(off) = offset {
        if off > 5000 {
            return Err(InvalidRequestError::new(
                "Offset can't be higher than 5000. Please use the 'next' property for pagination.",
            )
            .into());
        }
    }

    let options = PointerChangesQueryOptions {
        entity_types,
        from,
        to,
        include_auth_chain,
        sorting_field,
        sorting_order,
        offset,
        limit,
        last_id,
    };

    let result = state
        .database
        .get_pointer_changes(&options)
        .await
        .map_err(|e| AppError::Internal(e.to_string()))?;

    let deltas: Vec<PointerChangeDelta> = result
        .deltas
        .into_iter()
        .filter(|delta| !state.denylist.is_denylisted(&delta.entity_id))
        .collect();

    let mut next: Option<String> = None;
    if result.pagination.more_data {
        if let Some(last) = deltas.last() {
            let ts = last.local_timestamp;
            let id = &last.entity_id;
            let mut qs: Vec<String> = Vec::new();
            for et in &options.entity_types {
                qs.push(format!("entityType={}", et));
            }
            if let Some(from) = options.from {
                qs.push(format!("from={}", from));
            }
            qs.push(format!("to={}", ts));
            qs.push(format!("limit={}", result.pagination.limit));
            qs.push(format!("lastId={}", id));
            next = Some(format!("?{}", qs.join("&")));
        }
    }

    let pagination = HistoryPagination {
        offset: result.pagination.offset,
        limit: result.pagination.limit,
        more_data: result.pagination.more_data,
        next,
        last_id: None,
    };

    serde_json::to_vec(&PointerChangesResponse {
        deltas,
        filters: result.filters,
        pagination,
    })
    .map(Bytes::from)
    .map_err(|e| AppError::Internal(e.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};

    use async_trait::async_trait;
    use axum::body::Body;
    use serde_json::Value;

    use crate::state::{
        Database, DatabaseError, DeploymentQueryOptions, DeploymentQueryResult, PaginationResult,
        PointerChangesQueryResult, PrefixQueryResult,
    };
    use crate::test_support;
    use crate::wire_types::PointerChangesFilters;

    struct CountingDatabase {
        calls: Arc<AtomicUsize>,
    }

    #[async_trait]
    impl Database for CountingDatabase {
        async fn active_entities_by_pointers(
            &self,
            _pointers: &[String],
        ) -> Result<Vec<Value>, DatabaseError> {
            Ok(Vec::new())
        }

        async fn active_entities_by_ids(
            &self,
            _ids: &[String],
        ) -> Result<Vec<Value>, DatabaseError> {
            Ok(Vec::new())
        }

        async fn active_entities_by_prefix(
            &self,
            _prefix: &str,
            _offset: i64,
            _limit: i64,
        ) -> Result<PrefixQueryResult, DatabaseError> {
            Ok(PrefixQueryResult {
                total: 0,
                entities: Vec::new(),
            })
        }

        async fn active_entity_ids_by_content_hash(
            &self,
            _hash: &str,
        ) -> Result<Vec<String>, DatabaseError> {
            Ok(Vec::new())
        }

        async fn get_deployments(
            &self,
            _options: &DeploymentQueryOptions,
        ) -> Result<DeploymentQueryResult, DatabaseError> {
            Err(DatabaseError::Unsupported(
                "unused in this test".to_string(),
            ))
        }

        async fn get_pointer_changes(
            &self,
            options: &PointerChangesQueryOptions,
        ) -> Result<PointerChangesQueryResult, DatabaseError> {
            self.calls.fetch_add(1, Ordering::SeqCst);
            Ok(PointerChangesQueryResult {
                deltas: Vec::new(),
                filters: PointerChangesFilters {
                    entity_types: options.entity_types.clone(),
                    from: options.from,
                    to: options.to,
                    include_auth_chain: options.include_auth_chain,
                },
                pagination: PaginationResult {
                    offset: 0,
                    limit: options.limit.unwrap_or(500),
                    more_data: false,
                    next: None,
                    last_id: None,
                },
            })
        }

        async fn get_failed_deployments(&self) -> Result<Vec<Value>, DatabaseError> {
            Ok(Vec::new())
        }

        async fn get_audit_info(
            &self,
            _entity_type: &str,
            _entity_id: &str,
        ) -> Result<Option<Value>, DatabaseError> {
            Ok(None)
        }

        async fn find_entity_by_pointer(
            &self,
            _pointer: &str,
        ) -> Result<Option<Value>, DatabaseError> {
            Ok(None)
        }
    }

    fn pointer_changes_request(query: &str) -> Request {
        Request::builder()
            .uri(format!("/pointer-changes?{query}"))
            .body(Body::empty())
            .unwrap()
    }

    async fn body_json(response: Response) -> Value {
        let bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        serde_json::from_slice(&bytes).unwrap()
    }

    #[tokio::test]
    async fn identical_polls_within_the_window_share_one_scan() {
        let calls = Arc::new(AtomicUsize::new(0));
        let state = test_support::app_state_with_database(Arc::new(CountingDatabase {
            calls: calls.clone(),
        }));

        let first = get_pointer_changes(
            State(state.clone()),
            pointer_changes_request("entityType=scene&limit=10"),
        )
        .await
        .expect("first poll")
        .into_response();
        assert_eq!(first.status(), StatusCode::OK);
        assert_eq!(
            first.headers().get(CONTENT_TYPE).unwrap(),
            "application/json"
        );
        let body = body_json(first).await;
        assert_eq!(body["filters"]["entityTypes"], serde_json::json!(["scene"]));
        assert_eq!(body["pagination"]["limit"], 10);
        assert_eq!(body["deltas"], serde_json::json!([]));

        get_pointer_changes(
            State(state.clone()),
            pointer_changes_request("limit=10&entityType=scene"),
        )
        .await
        .expect("second poll");
        assert_eq!(calls.load(Ordering::SeqCst), 1);

        get_pointer_changes(
            State(state.clone()),
            pointer_changes_request("entityType=scene&limit=20"),
        )
        .await
        .expect("different window");
        assert_eq!(calls.load(Ordering::SeqCst), 2);

        state.deployments_cache.clear();
        get_pointer_changes(
            State(state.clone()),
            pointer_changes_request("entityType=scene&limit=10"),
        )
        .await
        .expect("after a deployment landed");
        assert_eq!(calls.load(Ordering::SeqCst), 3);
    }

    #[tokio::test]
    async fn invalid_params_are_rejected_before_the_cache() {
        let calls = Arc::new(AtomicUsize::new(0));
        let state = test_support::app_state_with_database(Arc::new(CountingDatabase {
            calls: calls.clone(),
        }));
        let err = get_pointer_changes(
            State(state.clone()),
            pointer_changes_request("entityType=nope"),
        )
        .await
        .err()
        .expect("unknown entity type is a 400");
        assert_eq!(err.into_response().status(), StatusCode::BAD_REQUEST);
        assert_eq!(calls.load(Ordering::SeqCst), 0);
    }
}
