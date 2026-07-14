use axum::extract::{Path, State};
use axum::http::HeaderMap;
use axum::Json;

use crate::auth::require_bearer_token;
use crate::handlers::federation::{fetch_place, fetch_world};
use crate::http::errors::ApiError;
use crate::http::response::ApiData;
use crate::ports::places::{PlaceRow, WorldRow};
use crate::AppState;

// A destination that stays browsable while the automated score leaves it
// alone: featuring moves where it shows, hiding takes it out of browse and
// disabling takes it out of the catalogue, so none of those say "list it
// normally, just do not rank it". Editorial by definition, so the admin token
// alone opens it -- the data team token that writes scores must not.
//
// The read surface is place_indexed, the write reaches `place` alone, so a
// destination served out of place_world_local is findable and unwritable. A
// 200 here means the flag is stored; anything else must say so rather than
// echo the request back.
const NOT_WRITABLE: &str = "ranking exclusion is not writable for this destination on this server";

async fn set_place_exclusion(
    state: &AppState,
    headers: &HeaderMap,
    place_id: &str,
    exclude: bool,
) -> Result<Json<ApiData<PlaceRow>>, ApiError> {
    require_bearer_token(headers, state.admin_auth_token.as_deref())?;
    let mut place = fetch_place(state, place_id).await?;
    if state
        .places
        .set_exclude_from_ranking(place_id, exclude)
        .await?
        == 0
    {
        return Err(ApiError::service_unavailable(NOT_WRITABLE));
    }
    place.exclude_from_ranking = exclude;
    if exclude {
        place.ranking = Some(0.0);
    }
    Ok(Json(ApiData::ok(place)))
}

async fn set_world_exclusion(
    state: &AppState,
    headers: &HeaderMap,
    world_id: &str,
    exclude: bool,
) -> Result<Json<ApiData<WorldRow>>, ApiError> {
    require_bearer_token(headers, state.admin_auth_token.as_deref())?;
    let mut world = fetch_world(state, world_id).await?;
    if state
        .places
        .set_exclude_from_ranking(&world.id, exclude)
        .await?
        == 0
    {
        return Err(ApiError::service_unavailable(NOT_WRITABLE));
    }
    world.exclude_from_ranking = exclude;
    if exclude {
        world.ranking = Some(0.0);
    }
    Ok(Json(ApiData::ok(WorldRow::from(world))))
}

#[utoipa::path(
    put,
    path = "/places/{place_id}/ranking-exclusion",
    tag = "federation",
    params(("place_id" = String, Path)),
    responses(
        (status = 200, body = ApiData<PlaceRow>),
        (status = 401, body = catalyrst_types::ApiErrorBody),
        (status = 404, body = catalyrst_types::ApiErrorBody),
        (status = 503, body = catalyrst_types::ApiErrorBody)
    )
)]
pub async fn put_place_ranking_exclusion(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(place_id): Path<String>,
) -> Result<Json<ApiData<PlaceRow>>, ApiError> {
    set_place_exclusion(&state, &headers, &place_id, true).await
}

#[utoipa::path(
    delete,
    path = "/places/{place_id}/ranking-exclusion",
    tag = "federation",
    params(("place_id" = String, Path)),
    responses(
        (status = 200, body = ApiData<PlaceRow>),
        (status = 401, body = catalyrst_types::ApiErrorBody),
        (status = 404, body = catalyrst_types::ApiErrorBody),
        (status = 503, body = catalyrst_types::ApiErrorBody)
    )
)]
pub async fn delete_place_ranking_exclusion(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(place_id): Path<String>,
) -> Result<Json<ApiData<PlaceRow>>, ApiError> {
    set_place_exclusion(&state, &headers, &place_id, false).await
}

#[utoipa::path(
    put,
    path = "/worlds/{world_id}/ranking-exclusion",
    tag = "federation",
    params(("world_id" = String, Path)),
    responses(
        (status = 200, body = ApiData<WorldRow>),
        (status = 401, body = catalyrst_types::ApiErrorBody),
        (status = 404, body = catalyrst_types::ApiErrorBody),
        (status = 503, body = catalyrst_types::ApiErrorBody)
    )
)]
pub async fn put_world_ranking_exclusion(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(world_id): Path<String>,
) -> Result<Json<ApiData<WorldRow>>, ApiError> {
    set_world_exclusion(&state, &headers, &world_id, true).await
}

#[utoipa::path(
    delete,
    path = "/worlds/{world_id}/ranking-exclusion",
    tag = "federation",
    params(("world_id" = String, Path)),
    responses(
        (status = 200, body = ApiData<WorldRow>),
        (status = 401, body = catalyrst_types::ApiErrorBody),
        (status = 404, body = catalyrst_types::ApiErrorBody),
        (status = 503, body = catalyrst_types::ApiErrorBody)
    )
)]
pub async fn delete_world_ranking_exclusion(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(world_id): Path<String>,
) -> Result<Json<ApiData<WorldRow>>, ApiError> {
    set_world_exclusion(&state, &headers, &world_id, false).await
}
