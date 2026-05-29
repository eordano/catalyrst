use std::sync::Arc;

use axum::extract::{Request, State};
use axum::response::IntoResponse;
use axum::Json;
use serde_json::{json, Value};

use crate::errors::{AppError, AppResult, InvalidRequestError};
use crate::query_params::{camel_to_snake, parse_query_string, qs_get_array, qs_get_bool, qs_get_number, qs_get_string};
use crate::state::{AppState, PointerChangesQueryOptions};

const VALID_SORTING_FIELDS: &[&str] = &["local_timestamp", "entity_timestamp"];

const VALID_SORTING_ORDERS: &[&str] = &["ASC", "DESC"];

pub async fn get_pointer_changes(
    State(state): State<Arc<AppState>>,
    request: Request,
) -> AppResult<impl IntoResponse> {
    let query_string = request.uri().query().unwrap_or("");
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

    let deltas: Vec<Value> = result
        .deltas
        .into_iter()
        .filter(|delta| {
            let entity_id = delta
                .get("entityId")
                .and_then(|v| v.as_str())
                .unwrap_or("");
            !state.denylist.is_denylisted(entity_id)
        })
        .collect();

    let mut pagination = json!({
        "offset": result.pagination.offset,
        "limit": result.pagination.limit,
        "moreData": result.pagination.more_data,
    });

    if result.pagination.more_data {
        if let Some(last) = deltas.last() {
            let last_ts = last.get("localTimestamp").and_then(|v| v.as_i64());
            let last_id = last.get("entityId").and_then(|v| v.as_str());
            if let (Some(ts), Some(id)) = (last_ts, last_id) {
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
                pagination["next"] = Value::String(format!("?{}", qs.join("&")));
            }
        }
    }

    Ok(Json(json!({
        "deltas": deltas,
        "filters": result.filters,
        "pagination": pagination,
    })))
}
