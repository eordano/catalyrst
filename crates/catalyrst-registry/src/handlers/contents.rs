use axum::extract::{Path, Query, State};
use axum::Json;
use serde_json::Value;

use dcl_contents::errors::ApiResult;
use dcl_contents::handlers::{profiles, status, worlds};
use dcl_contents::types::{CompactProfile, EntityStatus, IdsBody, WorldManifest, WorldNameQuery};

use crate::AppState;

pub async fn post_profiles(
    State(state): State<AppState>,
    body: Json<IdsBody>,
) -> ApiResult<Json<Vec<Value>>> {
    profiles::post_profiles(State(state.contents_state.clone()), body).await
}

pub async fn post_profiles_metadata(
    State(state): State<AppState>,
    body: Json<IdsBody>,
) -> ApiResult<Json<Vec<CompactProfile>>> {
    profiles::post_profiles_metadata(State(state.contents_state.clone()), body).await
}

pub async fn get_entity_status(
    State(state): State<AppState>,
    id: Path<String>,
    q: Query<WorldNameQuery>,
) -> ApiResult<Json<EntityStatus>> {
    status::get_entity_status(State(state.contents_state.clone()), id, q).await
}

pub async fn get_world_manifest(
    State(state): State<AppState>,
    world_name: Path<String>,
) -> ApiResult<Json<WorldManifest>> {
    worlds::get_world_manifest(State(state.contents_state.clone()), world_name).await
}
