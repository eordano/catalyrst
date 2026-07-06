use axum::extract::{Query, State};
use axum::Json;
use serde::Deserialize;
use serde_json::{json, Value};

use crate::http::ApiError;
use crate::ports::worlds::{
    OrderDirection, WorldInfoRow, WorldsListFilters, WorldsListOptions, WorldsOrderBy,
};
use crate::AppState;

const MAX_SEARCH_TERM_LENGTH: usize = 64;

#[derive(Debug, Deserialize)]
pub struct WorldsQuery {
    #[serde(default)]
    pub limit: Option<i64>,
    #[serde(default)]
    pub offset: Option<i64>,
    #[serde(default)]
    pub authorized_deployer: Option<String>,
    #[serde(default)]
    pub search: Option<String>,
    #[serde(default)]
    pub has_deployed_scenes: Option<String>,
    #[serde(default)]
    pub sort: Option<String>,
    #[serde(default)]
    pub order: Option<String>,
}

pub async fn get_worlds(
    State(state): State<AppState>,
    Query(q): Query<WorldsQuery>,
) -> Result<Json<Value>, ApiError> {
    let (limit, offset) = clamp_pagination(q.limit, q.offset);

    let deployer = match q.authorized_deployer.as_deref() {
        Some(d) if !d.is_empty() => {
            if !catalyrst_types::is_eth_address(d) {
                return Err(ApiError::bad_request(format!(
                    "Invalid authorized_deployer address: {d}. Must be a valid Ethereum address."
                )));
            }
            Some(d.to_lowercase())
        }
        _ => None,
    };

    let search = match q.search.as_deref().map(str::trim).filter(|s| !s.is_empty()) {
        Some(s) if s.len() > MAX_SEARCH_TERM_LENGTH => {
            return Err(ApiError::bad_request(format!(
                "Invalid search parameter: must be at most {MAX_SEARCH_TERM_LENGTH} characters."
            )))
        }
        Some(s) => Some(s.to_string()),
        None => None,
    };

    let has_deployed_scenes = q.has_deployed_scenes.as_deref().map(|v| v == "true");

    let order_by = match q.sort.as_deref().unwrap_or("name") {
        "name" => WorldsOrderBy::Name,
        "last_deployed_at" => WorldsOrderBy::LastDeployedAt,
        other => {
            return Err(ApiError::bad_request(format!(
                "Invalid sort parameter: {other}. Valid values are: name, last_deployed_at"
            )))
        }
    };
    let order_direction = match q.order.as_deref().unwrap_or("asc") {
        "asc" => OrderDirection::Asc,
        "desc" => OrderDirection::Desc,
        other => {
            return Err(ApiError::bad_request(format!(
                "Invalid order parameter: {other}. Valid values are: asc, desc"
            )))
        }
    };

    let filters = WorldsListFilters {
        authorized_deployer: deployer,
        search,
        has_deployed_scenes,
    };
    let options = WorldsListOptions {
        limit,
        offset,
        order_by,
        order_direction,
    };

    let (worlds, total) = state.worlds.list_worlds_public(&filters, &options).await?;
    let worlds: Vec<Value> = worlds.iter().map(world_info_json).collect();

    Ok(Json(json!({ "worlds": worlds, "total": total })))
}

fn world_info_json(w: &WorldInfoRow) -> Value {
    let shape = match (w.min_x, w.max_x, w.min_y, w.max_y) {
        (Some(x1), Some(x2), Some(y1), Some(y2)) => json!({
            "x1": x1, "x2": x2, "y1": y1, "y2": y2,
        }),
        _ => Value::Null,
    };
    json!({
        "name": w.name,
        "owner": w.owner,
        "title": w.title,
        "description": w.description,
        "shape": shape,
        "content_rating": w.content_rating,
        "spawn_coordinates": w.spawn_coordinates,
        "skybox_time": w.skybox_time,
        "categories": w.categories,
        "single_player": w.single_player,
        "show_in_places": w.show_in_places,
        "thumbnail_hash": w.thumbnail_hash,
        "last_deployed_at": w.last_deployed_at.map(|t| t.to_rfc3339()),
        "blocked_since": w.blocked_since.map(|t| t.to_rfc3339()),
        "deployed_scenes": w.deployed_scenes,
    })
}

fn clamp_pagination(limit: Option<i64>, offset: Option<i64>) -> (i64, i64) {
    let limit = limit.unwrap_or(100).clamp(1, 1000);
    let offset = offset.unwrap_or(0).max(0);
    (limit, offset)
}
