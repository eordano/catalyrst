use std::collections::HashMap;
use std::sync::{Arc, Mutex, OnceLock};
use std::time::{Duration, Instant};

use axum::extract::{Request, State};
use axum::http::StatusCode;
use axum::response::Response;
use bytes::Bytes;
use serde_json::{json, Value};

use crate::errors::{AppError, AppResult, InvalidRequestError};
use crate::query_params::{
    camel_to_snake, parse_query_string, qs_get_array, qs_get_bool, qs_get_number, qs_get_string,
    to_query_string,
};
use crate::state::{
    AppState, CacheEntry, DeploymentQueryOptions, DEPLOYMENTS_CACHE_MAX_ENTRIES,
    DEPLOYMENTS_CACHE_TTL,
};

fn checked_f64_to_i64(v: f64) -> Option<i64> {
    if !v.is_finite() {
        return None;
    }
    if v < i64::MIN as f64 || v > i64::MAX as f64 {
        return None;
    }
    Some(v as i64)
}

const DEPLOYMENTS_CACHE_SWEEP_INTERVAL: Duration = Duration::from_secs(60);

fn deployments_cache_last_sweep() -> &'static Mutex<Option<Instant>> {
    static LAST: OnceLock<Mutex<Option<Instant>>> = OnceLock::new();
    LAST.get_or_init(|| Mutex::new(None))
}

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
    let query_string = request.uri().query().unwrap_or("");

    let cache_key = normalize_query_string(query_string);
    if let Some(entry) = state.deployments_cache.get(&cache_key) {
        if !entry.is_expired(DEPLOYMENTS_CACHE_TTL) {
            let cached_bytes = entry.bytes.clone();
            drop(entry);
            return Ok(Response::builder()
                .status(StatusCode::OK)
                .header("content-type", "application/json")
                .header("cache-control", "max-age=5")
                .header("x-cache", "HIT")
                .body(axum::body::Body::from(cached_bytes))
                .unwrap());
        }
        drop(entry);
    }
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

    let entity_ids = qs_get_array(&params, "entityId");

    let pointers: Vec<String> = qs_get_array(&params, "pointer")
        .into_iter()
        .map(|p| p.to_lowercase())
        .collect();

    let deployed_by: Vec<String> = qs_get_array(&params, "deployedBy")
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

    let only_currently_pointed = qs_get_bool(&params, "onlyCurrentlyPointed");
    let offset = qs_get_number(&params, "offset");
    let limit = qs_get_number(&params, "limit");
    let from = qs_get_number(&params, "from");
    let to = qs_get_number(&params, "to");
    let last_id = qs_get_string(&params, "lastId").map(|s| s.to_lowercase());

    let from = if from.is_none()
        && to.is_none()
        && last_id.is_none()
        && entity_ids.is_empty()
        && pointers.is_empty()
        && deployed_by.is_empty()
    {
        let window_days = std::env::var("DEPLOYMENTS_DEFAULT_WINDOW_DAYS")
            .ok()
            .and_then(|s| s.parse::<i64>().ok())
            .unwrap_or(30);
        if window_days > 0 {
            let now_ms = chrono::Utc::now().timestamp_millis();
            Some(now_ms - window_days * 86_400_000)
        } else {
            None
        }
    } else {
        from
    };

    let fields_param = qs_get_string(&params, "fields");
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

    let sorting_field_param = qs_get_string(&params, "sortingField");
    let sorting_field = if let Some(ref sf) = sorting_field_param {
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

    let options = DeploymentQueryOptions {
        entity_types,
        entity_ids,
        pointers,
        deployed_by,
        from,
        to,
        only_currently_pointed,
        fields: fields.clone(),
        sorting_field,
        sorting_order,
        offset,
        limit,
        last_id,
    };

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

    let deployments: Vec<Value> = result
        .deployments
        .into_iter()
        .map(|dep| filter_deployment_fields(&dep, &fields))
        .collect();

    let mut pagination = json!({
        "offset": result.pagination.offset,
        "limit": result.pagination.limit,
        "moreData": result.pagination.more_data,
    });

    if result.pagination.more_data && !deployments.is_empty() {
        let last_deployment = &deployments[deployments.len() - 1];
        let next = calculate_next_relative_path(&options, last_deployment);
        pagination["next"] = Value::String(next);
    } else if let Some(ref next) = result.pagination.next {
        pagination["next"] = Value::String(next.clone());
    }

    if let Some(ref lid) = result.pagination.last_id {
        pagination["lastId"] = Value::String(lid.clone());
    }

    let response_json = json!({
        "deployments": deployments,
        "filters": result.filters,
        "pagination": pagination,
    });

    let response_bytes = Bytes::from(serde_json::to_vec(&response_json).unwrap_or_default());

    if state.deployments_cache.len() >= DEPLOYMENTS_CACHE_MAX_ENTRIES {
        let do_sweep = {
            let mut guard = deployments_cache_last_sweep().lock().unwrap();
            let due = guard
                .map(|t| t.elapsed() >= DEPLOYMENTS_CACHE_SWEEP_INTERVAL)
                .unwrap_or(true);
            if due {
                *guard = Some(Instant::now());
            }
            due
        };
        if do_sweep {
            state
                .deployments_cache
                .retain(|_, v| !v.is_expired(DEPLOYMENTS_CACHE_TTL));
        }
    }
    if state.deployments_cache.len() < DEPLOYMENTS_CACHE_MAX_ENTRIES {
        state.deployments_cache.insert(
            cache_key,
            CacheEntry {
                bytes: response_bytes.clone(),
                inserted_at: Instant::now(),
            },
        );
    }

    Ok(Response::builder()
        .status(StatusCode::OK)
        .header("content-type", "application/json")
        .header("cache-control", "max-age=5")
        .header("x-cache", "MISS")
        .body(axum::body::Body::from(response_bytes))
        .unwrap())
}

fn filter_deployment_fields(dep: &Value, fields: &[String]) -> Value {
    let obj = match dep.as_object() {
        Some(o) => o,
        None => return dep.clone(),
    };

    let mut result = serde_json::Map::new();

    for key in &[
        "entityType",
        "entityId",
        "entityTimestamp",
        "deployedBy",
        "entityVersion",
    ] {
        if let Some(v) = obj.get(*key) {
            result.insert(key.to_string(), v.clone());
        }
    }

    if fields.iter().any(|f| f == "pointers") {
        if let Some(v) = obj.get("pointers") {
            result.insert("pointers".to_string(), v.clone());
        }
    }

    if fields.iter().any(|f| f == "content") {
        if let Some(v) = obj.get("content") {
            result.insert("content".to_string(), v.clone());
        }
    }

    if fields.iter().any(|f| f == "metadata") {
        if let Some(v) = obj.get("metadata") {
            result.insert("metadata".to_string(), v.clone());
        }
    }

    if fields.iter().any(|f| f == "auditInfo") {
        if let Some(v) = obj.get("auditInfo") {
            result.insert("auditInfo".to_string(), v.clone());
        }
    }

    let local_ts = obj
        .get("auditInfo")
        .and_then(|ai| ai.get("localTimestamp"))
        .cloned();
    if let Some(ts) = local_ts {
        result.insert("localTimestamp".to_string(), ts);
    }

    Value::Object(result)
}

fn calculate_next_relative_path(
    options: &DeploymentQueryOptions,
    last_deployment: &Value,
) -> String {
    let field = options
        .sorting_field
        .as_deref()
        .unwrap_or("local_timestamp");
    let order = options.sorting_order.as_deref().unwrap_or("DESC");

    let timestamp = if field == "local_timestamp" {
        last_deployment.get("localTimestamp").or_else(|| {
            last_deployment
                .get("auditInfo")
                .and_then(|ai| ai.get("localTimestamp"))
        })
    } else {
        last_deployment.get("entityTimestamp")
    };

    let timestamp_str = match timestamp {
        Some(Value::Number(n)) => {
            if let Some(i) = n.as_i64() {
                i.to_string()
            } else if let Some(f) = n.as_f64() {
                match checked_f64_to_i64(f) {
                    Some(i) => i.to_string(),
                    None => {
                        tracing::warn!(
                            raw_timestamp = f,
                            "Non-finite/out-of-range f64 timestamp in last deployment; \
                             omitting from `next` cursor"
                        );
                        String::new()
                    }
                }
            } else {
                n.to_string()
            }
        }
        _ => String::new(),
    };

    let last_entity_id = last_deployment
        .get("entityId")
        .and_then(|v| v.as_str())
        .unwrap_or("");

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
