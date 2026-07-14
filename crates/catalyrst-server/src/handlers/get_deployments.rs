use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

use axum::extract::{Request, State};
use axum::http::StatusCode;
use axum::response::Response;
use bytes::Bytes;

use crate::errors::{AppError, AppResult, InvalidRequestError};
use crate::query_params::{
    camel_to_snake, parse_query_string, qs_get_array, qs_get_bool, qs_get_number, qs_get_string,
    to_query_string, QueryParams,
};
use crate::state::{AppState, DeploymentQueryOptions};
use crate::wire_types::{ControllerDeployment, DeploymentsResponse, HistoryPagination};

const DEFAULT_FIELDS: &[&str] = &["pointers", "content", "metadata"];

const VALID_SORTING_FIELDS: &[&str] = &["local_timestamp", "entity_timestamp"];

const VALID_SORTING_ORDERS: &[&str] = &["ASC", "DESC"];

const VALID_DEPLOYMENT_FIELDS: &[&str] = &["pointers", "content", "metadata", "auditInfo"];

const MAX_DEPLOYMENT_FILTER_VALUES: usize = 1000;

fn normalize_query_string(qs: &str) -> String {
    let mut pairs: Vec<&str> = qs.split('&').filter(|s| !s.is_empty()).collect();
    pairs.sort_unstable();
    pairs.join("&")
}

pub async fn get_deployments(
    State(state): State<Arc<AppState>>,
    request: Request,
) -> AppResult<Response> {
    let query_string = request.uri().query().unwrap_or("").to_string();
    let cache_key = normalize_query_string(&query_string);

    let fetched = Arc::new(AtomicBool::new(false));
    let fetched_in_fetch = fetched.clone();
    let state_in_fetch = state.clone();

    let response_bytes = state
        .deployments_cache
        .get_or_fetch(cache_key, move || async move {
            fetched_in_fetch.store(true, Ordering::Relaxed);
            fetch_deployments_response(state_in_fetch, &query_string).await
        })
        .await?;

    let cache_status = if fetched.load(Ordering::Relaxed) {
        "MISS"
    } else {
        "HIT"
    };

    Ok(Response::builder()
        .status(StatusCode::OK)
        .header("content-type", "application/json")
        .header("cache-control", "max-age=5")
        .header("x-cache", cache_status)
        .body(axum::body::Body::from(response_bytes))
        .unwrap())
}

async fn fetch_deployments_response(state: Arc<AppState>, query_string: &str) -> AppResult<Bytes> {
    let params = parse_query_string(query_string);

    let options = parse_deployment_query_options(&params)?;

    let timeout_secs: u64 = std::env::var("DEPLOYMENTS_QUERY_TIMEOUT_SECS")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(10);
    let result = tokio::time::timeout(
        Duration::from_secs(timeout_secs),
        state.database.get_deployments(&options),
    )
    .await
    .map_err(|_| {
        AppError::ServiceUnavailable(
            "deployments query exceeded server-side time budget; narrow the time range or filters"
                .into(),
        )
    })?
    .map_err(|e| AppError::Internal(e.to_string()))?;

    let deployments: Vec<ControllerDeployment> = result
        .deployments
        .into_iter()
        .map(|dep| project_deployment_fields(dep, &options.fields))
        .collect();

    let next = if result.pagination.more_data && !deployments.is_empty() {
        Some(calculate_next_relative_path(
            &options,
            &deployments[deployments.len() - 1],
        ))
    } else {
        result.pagination.next.clone()
    };

    let pagination = HistoryPagination {
        offset: result.pagination.offset,
        limit: result.pagination.limit,
        more_data: result.pagination.more_data,
        next,
        last_id: result.pagination.last_id.clone(),
    };

    let response = DeploymentsResponse {
        deployments,
        filters: result.filters,
        pagination,
    };

    Ok(Bytes::from(
        serde_json::to_vec(&response).unwrap_or_default(),
    ))
}

fn parse_deployment_query_options(params: &QueryParams) -> AppResult<DeploymentQueryOptions> {
    let mut entity_types: Vec<String> = Vec::new();
    for raw in qs_get_array(params, "entityType") {
        match crate::query_params::parse_entity_type(&raw) {
            Some(canonical) => entity_types.push(canonical.to_string()),
            None => {
                return Err(InvalidRequestError::new("Found an unrecognized entity type").into())
            }
        }
    }

    let entity_ids = qs_get_array(params, "entityId");

    let pointers: Vec<String> = qs_get_array(params, "pointer")
        .into_iter()
        .map(|p| p.to_lowercase())
        .collect();

    let deployed_by: Vec<String> = qs_get_array(params, "deployedBy")
        .into_iter()
        .map(|a| a.to_lowercase())
        .collect();

    if entity_ids.len() > MAX_DEPLOYMENT_FILTER_VALUES
        || pointers.len() > MAX_DEPLOYMENT_FILTER_VALUES
        || entity_types.len() > MAX_DEPLOYMENT_FILTER_VALUES
    {
        return Err(InvalidRequestError::new(format!(
            "Too many filter values; the maximum allowed per filter is {}",
            MAX_DEPLOYMENT_FILTER_VALUES
        ))
        .into());
    }

    let only_currently_pointed = qs_get_bool(params, "onlyCurrentlyPointed");
    let offset = qs_get_number(params, "offset");
    let limit = qs_get_number(params, "limit");
    let from = qs_get_number(params, "from");
    let to = qs_get_number(params, "to");
    let last_id = qs_get_string(params, "lastId").map(|s| s.to_lowercase());

    let fields_param = qs_get_string(params, "fields");
    let fields: Vec<String> = if let Some(ref f) = fields_param {
        if f.trim().is_empty() {
            DEFAULT_FIELDS.iter().map(|s| s.to_string()).collect()
        } else {
            f.split(',')
                .filter(|s| VALID_DEPLOYMENT_FIELDS.contains(&s.trim()))
                .map(|s| s.trim().to_string())
                .collect()
        }
    } else {
        DEFAULT_FIELDS.iter().map(|s| s.to_string()).collect()
    };

    let sorting_field_param = qs_get_string(params, "sortingField");
    let sorting_field = if let Some(ref sf) = sorting_field_param {
        let snake = camel_to_snake(sf);
        if !VALID_SORTING_FIELDS.contains(&snake.as_str()) {
            return Err(InvalidRequestError::new("Found an unrecognized sort field param").into());
        }
        Some(snake)
    } else {
        None
    };

    let sorting_order = if let Some(ref so) = qs_get_string(params, "sortingOrder") {
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

    Ok(DeploymentQueryOptions {
        entity_types,
        entity_ids,
        pointers,
        deployed_by,
        from,
        to,
        only_currently_pointed,
        fields,
        sorting_field,
        sorting_order,
        offset,
        limit,
        last_id,
    })
}

fn project_deployment_fields(
    mut dep: ControllerDeployment,
    fields: &[String],
) -> ControllerDeployment {
    if !fields.iter().any(|f| f == "pointers") {
        dep.pointers = None;
    }
    if !fields.iter().any(|f| f == "content") {
        dep.content = None;
    }
    if !fields.iter().any(|f| f == "metadata") {
        dep.metadata = None;
    }
    if !fields.iter().any(|f| f == "auditInfo") {
        dep.audit_info = None;
    }
    dep
}

fn calculate_next_relative_path(
    options: &DeploymentQueryOptions,
    last_deployment: &ControllerDeployment,
) -> String {
    let field = options
        .sorting_field
        .as_deref()
        .unwrap_or("local_timestamp");
    let order = options.sorting_order.as_deref().unwrap_or("DESC");

    let timestamp = if field == "local_timestamp" {
        last_deployment.local_timestamp
    } else {
        last_deployment.entity_timestamp
    };

    let timestamp_str = timestamp.to_string();
    let last_entity_id = last_deployment.entity_id.as_str();

    let mut next_params: HashMap<String, Vec<String>> = HashMap::new();

    if !options.entity_types.is_empty() {
        next_params.insert("entityType".to_string(), options.entity_types.clone());
    }

    if !options.entity_ids.is_empty() {
        next_params.insert("entityId".to_string(), options.entity_ids.clone());
    }

    if !options.pointers.is_empty() {
        next_params.insert("pointer".to_string(), options.pointers.clone());
    }

    if !options.deployed_by.is_empty() {
        next_params.insert("deployedBy".to_string(), options.deployed_by.clone());
    }

    if options.only_currently_pointed == Some(true) {
        next_params.insert("onlyCurrentlyPointed".to_string(), vec!["true".to_string()]);
    }

    if order == "ASC" {
        if !timestamp_str.is_empty() {
            next_params.insert("from".to_string(), vec![timestamp_str]);
        }
        if let Some(to_val) = options.to {
            next_params.insert("to".to_string(), vec![to_val.to_string()]);
        }
    } else {
        if !timestamp_str.is_empty() {
            next_params.insert("to".to_string(), vec![timestamp_str]);
        }
        if let Some(from_val) = options.from {
            next_params.insert("from".to_string(), vec![from_val.to_string()]);
        }
    }

    let is_default_fields = options.fields.len() == DEFAULT_FIELDS.len()
        && options
            .fields
            .iter()
            .all(|f| DEFAULT_FIELDS.contains(&f.as_str()));
    if !is_default_fields {
        let fields_str = options.fields.join(",");
        if !fields_str.is_empty() {
            next_params.insert("fields".to_string(), vec![fields_str]);
        }
    }

    next_params.insert("sortingField".to_string(), vec![field.to_string()]);
    next_params.insert("sortingOrder".to_string(), vec![order.to_string()]);

    if !last_entity_id.is_empty() {
        next_params.insert("lastId".to_string(), vec![last_entity_id.to_string()]);
    }

    if let Some(lim) = options.limit {
        next_params.insert("limit".to_string(), vec![lim.to_string()]);
    }

    format!("?{}", to_query_string(&next_params))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn no_implicit_time_window_when_client_sends_no_time_filters() {
        let params = parse_query_string("entityType=emote&limit=3&sortingOrder=DESC");
        let options = parse_deployment_query_options(&params).unwrap();
        assert_eq!(options.from, None);
        assert_eq!(options.to, None);
    }

    #[test]
    fn no_implicit_time_window_on_empty_query() {
        let params = parse_query_string("");
        let options = parse_deployment_query_options(&params).unwrap();
        assert_eq!(options.from, None);
        assert_eq!(options.to, None);
    }

    #[test]
    fn client_provided_time_filters_are_preserved() {
        let params = parse_query_string("from=1&to=2&limit=3");
        let options = parse_deployment_query_options(&params).unwrap();
        assert_eq!(options.from, Some(1));
        assert_eq!(options.to, Some(2));
        assert_eq!(options.limit, Some(3));
    }

    use std::sync::atomic::AtomicUsize;

    use async_trait::async_trait;
    use axum::body::Body;
    use serde_json::Value;

    use crate::state::{
        Database, DatabaseError, DeploymentQueryResult, PaginationResult,
        PointerChangesQueryOptions, PointerChangesQueryResult, PrefixQueryResult,
    };
    use crate::test_support;
    use crate::wire_types::DeploymentsFilters;

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
            self.calls.fetch_add(1, Ordering::SeqCst);
            Ok(DeploymentQueryResult {
                deployments: Vec::new(),
                filters: DeploymentsFilters {
                    pointers: Vec::new(),
                    entity_types: Vec::new(),
                    entity_ids: Vec::new(),
                    from: None,
                    to: None,
                    only_currently_pointed: None,
                    deployed_by: Vec::new(),
                },
                pagination: PaginationResult {
                    offset: 0,
                    limit: 100,
                    more_data: false,
                    next: None,
                    last_id: None,
                },
            })
        }

        async fn get_pointer_changes(
            &self,
            _options: &PointerChangesQueryOptions,
        ) -> Result<PointerChangesQueryResult, DatabaseError> {
            Err(DatabaseError::Unsupported(
                "unused in this test".to_string(),
            ))
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

    fn deployments_request(query: &str) -> Request {
        Request::builder()
            .uri(format!("/deployments?{query}"))
            .body(Body::empty())
            .unwrap()
    }

    #[tokio::test]
    async fn second_identical_query_is_served_from_cache() {
        let calls = Arc::new(AtomicUsize::new(0));
        let state = test_support::app_state_with_database(Arc::new(CountingDatabase {
            calls: calls.clone(),
        }));

        let first = get_deployments(
            State(state.clone()),
            deployments_request("entityType=scene&limit=10"),
        )
        .await
        .expect("first call must succeed");
        assert_eq!(first.headers().get("x-cache").unwrap(), "MISS");
        assert_eq!(calls.load(Ordering::SeqCst), 1);

        let second = get_deployments(
            State(state.clone()),
            deployments_request("entityType=scene&limit=10"),
        )
        .await
        .expect("second call must succeed");
        assert_eq!(second.headers().get("x-cache").unwrap(), "HIT");
        assert_eq!(
            calls.load(Ordering::SeqCst),
            1,
            "identical query within TTL must not re-hit the database"
        );
    }

    #[tokio::test]
    async fn reordered_query_params_still_hit_the_cache() {
        let calls = Arc::new(AtomicUsize::new(0));
        let state = test_support::app_state_with_database(Arc::new(CountingDatabase {
            calls: calls.clone(),
        }));

        get_deployments(
            State(state.clone()),
            deployments_request("entityType=scene&limit=10"),
        )
        .await
        .expect("first call must succeed");
        get_deployments(
            State(state.clone()),
            deployments_request("limit=10&entityType=scene"),
        )
        .await
        .expect("second call must succeed");

        assert_eq!(
            calls.load(Ordering::SeqCst),
            1,
            "reordered params must normalize to the same cache key"
        );
    }

    #[tokio::test]
    async fn clearing_the_cache_forces_a_refetch() {
        let calls = Arc::new(AtomicUsize::new(0));
        let state = test_support::app_state_with_database(Arc::new(CountingDatabase {
            calls: calls.clone(),
        }));

        get_deployments(
            State(state.clone()),
            deployments_request("entityType=scene"),
        )
        .await
        .expect("first call must succeed");
        state.deployments_cache.clear();
        let after_clear = get_deployments(
            State(state.clone()),
            deployments_request("entityType=scene"),
        )
        .await
        .expect("call after clear must succeed");

        assert_eq!(after_clear.headers().get("x-cache").unwrap(), "MISS");
        assert_eq!(calls.load(Ordering::SeqCst), 2);
    }
}
