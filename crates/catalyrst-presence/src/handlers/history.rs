use axum::extract::{Query, State};
use axum::http::StatusCode;
use axum::Json;
use serde::Deserialize;
use serde_json::{json, Value};

use crate::AppState;

const DEFAULT_LIMIT: i64 = 200;
const MAX_LIMIT: i64 = 5000;

fn db_err(e: sqlx::Error) -> (StatusCode, Json<Value>) {
    tracing::error!(error = %e, "presence query failed");
    (
        StatusCode::INTERNAL_SERVER_ERROR,
        Json(json!({"error": "query failed"})),
    )
}

pub async fn current(
    State(state): State<AppState>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    let snap = state.queries.current().await.map_err(db_err)?;
    Ok(Json(json!({ "current": snap })))
}

pub async fn current_scenes(
    State(state): State<AppState>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    let rows = state.queries.current_scenes().await.map_err(db_err)?;
    Ok(Json(json!({ "scenes": rows })))
}

pub async fn current_worlds(
    State(state): State<AppState>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    let rows = state.queries.current_worlds().await.map_err(db_err)?;
    Ok(Json(json!({ "worlds": rows })))
}

#[derive(Debug, Deserialize)]
pub struct SceneHistoryQuery {
    pub pointer: String,
    pub limit: Option<i64>,
}

pub async fn scene_history(
    State(state): State<AppState>,
    Query(q): Query<SceneHistoryQuery>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    let limit = clamp_limit(q.limit);
    let rows = state
        .queries
        .scene_history(&q.pointer, limit)
        .await
        .map_err(db_err)?;
    Ok(Json(json!({ "pointer": q.pointer, "history": rows })))
}

#[derive(Debug, Deserialize)]
pub struct WorldHistoryQuery {
    pub world: String,
    pub limit: Option<i64>,
}

pub async fn world_history(
    State(state): State<AppState>,
    Query(q): Query<WorldHistoryQuery>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    let limit = clamp_limit(q.limit);
    let rows = state
        .queries
        .world_history(&q.world, limit)
        .await
        .map_err(db_err)?;
    Ok(Json(json!({ "world": q.world, "history": rows })))
}

fn clamp_limit(limit: Option<i64>) -> i64 {
    limit.unwrap_or(DEFAULT_LIMIT).clamp(1, MAX_LIMIT)
}
