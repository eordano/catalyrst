use axum::extract::{OriginalUri, Path, Query, State};
use axum::http::{HeaderMap, Method, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::Json;
use serde::Serialize;
use serde_json::Value;

use crate::auth::auth_address_verified;
use crate::entity_id::{resolve_entity_type, EntityType};
use crate::handlers::categories::category_i18n_en;
use crate::handlers::destinations::{
    decorate_next_event, enrich, list_destinations, parse_with_options, Destination,
};
use crate::handlers::federation::lookup_entity;
use crate::http::errors::ApiError;
use crate::http::response::{ApiData, ApiDataTotal};
use crate::ports::places::{CategoryTarget, PlaceRow, PlacesComponent};
use crate::AppState;

#[derive(Debug, Clone, Serialize, utoipa::ToSchema)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export, export_to = "places/"))]
pub struct InteractionSummary {
    pub likes: i32,
    pub dislikes: i32,
    pub favorites: i32,
    pub user_like: bool,
    pub user_dislike: bool,
    pub user_favorite: bool,
}

impl From<&PlaceRow> for InteractionSummary {
    fn from(p: &PlaceRow) -> Self {
        Self {
            likes: p.likes,
            dislikes: p.dislikes,
            favorites: p.favorites,
            user_like: p.user_like,
            user_dislike: p.user_dislike,
            user_favorite: p.user_favorite,
        }
    }
}

#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct ApiJsonList {
    pub ok: bool,
    #[schema(value_type = Vec<Object>)]
    pub data: Vec<Value>,
}

impl ApiJsonList {
    pub fn ok(data: Vec<Value>) -> Self {
        Self { ok: true, data }
    }
}

#[derive(Debug, Clone, Serialize, utoipa::ToSchema)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export, export_to = "places/"))]
pub struct DestinationCategory {
    pub name: String,
    #[cfg_attr(feature = "ts", ts(type = "number"))]
    pub count: i64,
    pub i18n: CategoryI18n,
}

#[derive(Debug, Clone, Serialize, utoipa::ToSchema)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export, export_to = "places/"))]
pub struct CategoryI18n {
    pub en: String,
}

#[utoipa::path(
    get,
    path = "/destinations",
    tag = "v1",
    params(("limit" = Option<i64>, Query),
        ("offset" = Option<i64>, Query),
        ("ids" = Option<Vec<String>>, Query),
        ("positions" = Option<Vec<String>>, Query),
        ("pointer" = Option<Vec<String>>, Query),
        ("world_names" = Option<Vec<String>>, Query),
        ("names" = Option<Vec<String>>, Query),
        ("kinds" = Option<Vec<String>>, Query),
        ("only_places" = Option<String>, Query),
        ("only_worlds" = Option<String>, Query),
        ("categories" = Option<Vec<String>>, Query),
        ("only_highlighted" = Option<String>, Query),
        ("only_favorites" = Option<String>, Query),
        ("search" = Option<String>, Query),
        ("owner" = Option<String>, Query),
        ("creator_address" = Option<String>, Query),
        ("sdk" = Option<String>, Query),
        ("only_sdk7" = Option<String>, Query),
        ("order_by" = Option<String>, Query),
        ("order" = Option<String>, Query),
        ("with" = Option<Vec<String>>, Query),
        ("with_live_events" = Option<String>, Query),
        ("with_connected_users" = Option<String>, Query),
        ("with_realms_detail" = Option<String>, Query)),
    responses(
        (status = 200, body = ApiDataTotal<Destination>),
        (status = 400, body = catalyrst_types::ApiErrorBody),
        (status = 500, body = catalyrst_types::ApiErrorBody)
    )
)]
pub async fn get_v1_destinations_list(
    State(state): State<AppState>,
    method: Method,
    OriginalUri(uri): OriginalUri,
    headers: HeaderMap,
    Query(pairs): Query<Vec<(String, String)>>,
) -> Result<Json<ApiDataTotal<Destination>>, ApiError> {
    list_destinations(&state, &headers, method.as_str(), uri.path(), &pairs).await
}

pub async fn find_destination(
    places: &PlacesComponent,
    id: &str,
) -> Result<Option<PlaceRow>, ApiError> {
    match resolve_entity_type(id) {
        EntityType::Place => Ok(places
            .find_by_id(&id.to_lowercase())
            .await?
            .filter(|p| !p.disabled && !p.world)),
        EntityType::World => Ok(places
            .find_world_by_id(id)
            .await?
            .filter(|w| !w.disabled && w.world && w.show_in_places)),
    }
}

#[utoipa::path(
    get,
    path = "/destinations/{id}",
    tag = "v1",
    params(("id" = String, Path),
        ("with" = Option<Vec<String>>, Query),
        ("with_live_events" = Option<String>, Query),
        ("with_connected_users" = Option<String>, Query),
        ("with_realms_detail" = Option<String>, Query)),
    responses(
        (status = 200, body = ApiData<Destination>),
        (status = 400, body = catalyrst_types::ApiErrorBody),
        (status = 404, body = catalyrst_types::ApiErrorBody),
        (status = 500, body = catalyrst_types::ApiErrorBody)
    )
)]
pub async fn get_v1_destination(
    State(state): State<AppState>,
    method: Method,
    OriginalUri(uri): OriginalUri,
    headers: HeaderMap,
    Query(pairs): Query<Vec<(String, String)>>,
    Path(id): Path<String>,
) -> Result<Json<ApiData<Destination>>, ApiError> {
    let flags = parse_with_options(&pairs)?;
    let user = crate::auth::auth_address_optional(&headers, method.as_str(), uri.path()).await;
    let Some(row) = find_destination(&state.places, &id).await? else {
        return Err(ApiError::not_found(format!("Destination not found: {id}")));
    };
    let mut rows = vec![row];
    state
        .places
        .apply_user_interactions(user.as_deref(), &mut rows)
        .await;
    enrich(&state, &mut rows, &flags).await;
    let mut out: Vec<Destination> = rows.into_iter().map(Destination::from).collect();
    decorate_next_event(&state, &mut out, &flags).await;
    let destination = out
        .pop()
        .ok_or_else(|| ApiError::not_found(format!("Destination not found: {id}")))?;
    Ok(Json(ApiData::ok(destination)))
}

#[utoipa::path(
    get,
    path = "/destinations/{id}/events",
    tag = "v1",
    params(("id" = String, Path)),
    responses(
        (status = 200, body = ApiJsonList),
        (status = 400, body = catalyrst_types::ApiErrorBody),
        (status = 401, body = catalyrst_types::ApiErrorBody),
        (status = 500, body = catalyrst_types::ApiErrorBody),
        (status = 503, body = catalyrst_types::ApiErrorBody)
    )
)]
pub async fn get_v1_destination_events(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
) -> Result<Response, ApiError> {
    let (status, body) = state.events.destination_events(&id, &headers).await?;
    let status = StatusCode::from_u16(status).unwrap_or(StatusCode::BAD_GATEWAY);
    Ok((status, Json(body)).into_response())
}

async fn interaction_actor(
    headers: &HeaderMap,
    method: &Method,
    uri: &axum::http::Uri,
) -> Result<String, ApiError> {
    let signer = auth_address_verified(headers, method.as_str(), uri.path()).await?;
    Ok(signer.as_str().to_lowercase())
}

async fn resolve_destination_entity(
    state: &AppState,
    entity_id: &str,
    user: &str,
) -> Result<(PlaceRow, EntityType), ApiError> {
    let entity_type = resolve_entity_type(entity_id);
    let is_world = entity_type == EntityType::World;
    let lookup_id = if is_world {
        entity_id.to_string()
    } else {
        entity_id.to_lowercase()
    };
    let mut entity = match lookup_entity(&state.places, &lookup_id, is_world).await? {
        Some(entity) => entity,
        None if is_world => {
            return Err(ApiError::not_found(format!(
                "Not found world \"{}\"",
                entity_id
            )))
        }
        None => {
            return Err(ApiError::not_found(format!(
                "Not found entity \"{}\"",
                entity_id
            )))
        }
    };
    state
        .places
        .apply_user_interactions(Some(user), std::slice::from_mut(&mut entity))
        .await;
    Ok((entity, entity_type))
}

pub async fn set_destination_favorite(
    state: &AppState,
    entity_id: &str,
    user: &str,
    favorite: bool,
) -> Result<InteractionSummary, ApiError> {
    let (entity, _) = resolve_destination_entity(state, entity_id, user).await?;
    let mut summary = InteractionSummary::from(&entity);
    if favorite == entity.user_favorite {
        return Ok(summary);
    }
    let (favorites, user_favorite) = state
        .places
        .set_favorite(
            &entity.id,
            user,
            favorite,
            entity.favorites,
            entity.user_favorite,
        )
        .await?;
    summary.favorites = favorites;
    summary.user_favorite = user_favorite;
    Ok(summary)
}

pub async fn set_destination_like(
    state: &AppState,
    entity_id: &str,
    user: &str,
    like: Option<bool>,
) -> Result<InteractionSummary, ApiError> {
    let (entity, _) = resolve_destination_entity(state, entity_id, user).await?;
    let mut summary = InteractionSummary::from(&entity);
    let current = if entity.user_like {
        Some(true)
    } else if entity.user_dislike {
        Some(false)
    } else {
        None
    };
    if current == like {
        return Ok(summary);
    }
    let user_activity = match like {
        Some(_) => crate::snapshot::fetch_score(user).await,
        None => 0.0,
    };
    let (likes, dislikes, user_like, user_dislike) = state
        .places
        .set_like(
            &entity.id,
            user,
            like,
            user_activity,
            entity.likes,
            entity.dislikes,
            entity.user_like,
            entity.user_dislike,
        )
        .await?;
    summary.likes = likes;
    summary.dislikes = dislikes;
    summary.user_like = user_like;
    summary.user_dislike = user_dislike;
    Ok(summary)
}

#[utoipa::path(
    put,
    path = "/destinations/{id}/favorites",
    tag = "v1",
    params(("id" = String, Path)),
    responses(
        (status = 200, body = ApiData<InteractionSummary>),
        (status = 401, body = catalyrst_types::ApiErrorBody),
        (status = 404, body = catalyrst_types::ApiErrorBody),
        (status = 500, body = catalyrst_types::ApiErrorBody),
        (status = 503, body = catalyrst_types::ApiErrorBody)
    )
)]
pub async fn put_v1_destination_favorites(
    State(state): State<AppState>,
    method: Method,
    OriginalUri(uri): OriginalUri,
    headers: HeaderMap,
    Path(id): Path<String>,
) -> Result<Json<ApiData<InteractionSummary>>, ApiError> {
    let user = interaction_actor(&headers, &method, &uri).await?;
    let summary = set_destination_favorite(&state, &id, &user, true).await?;
    Ok(Json(ApiData::ok(summary)))
}

#[utoipa::path(
    delete,
    path = "/destinations/{id}/favorites",
    tag = "v1",
    params(("id" = String, Path)),
    responses(
        (status = 200, body = ApiData<InteractionSummary>),
        (status = 401, body = catalyrst_types::ApiErrorBody),
        (status = 404, body = catalyrst_types::ApiErrorBody),
        (status = 500, body = catalyrst_types::ApiErrorBody),
        (status = 503, body = catalyrst_types::ApiErrorBody)
    )
)]
pub async fn delete_v1_destination_favorites(
    State(state): State<AppState>,
    method: Method,
    OriginalUri(uri): OriginalUri,
    headers: HeaderMap,
    Path(id): Path<String>,
) -> Result<Json<ApiData<InteractionSummary>>, ApiError> {
    let user = interaction_actor(&headers, &method, &uri).await?;
    let summary = set_destination_favorite(&state, &id, &user, false).await?;
    Ok(Json(ApiData::ok(summary)))
}

pub fn parse_like_body(body: Option<&Value>) -> Result<bool, ApiError> {
    let invalid = || ApiError::bad_request("Invalid likes body. Expected { like: boolean }.");
    let object = body.and_then(Value::as_object).ok_or_else(invalid)?;
    if object.len() != 1 {
        return Err(invalid());
    }
    object
        .get("like")
        .and_then(Value::as_bool)
        .ok_or_else(invalid)
}

#[utoipa::path(
    put,
    path = "/destinations/{id}/likes",
    tag = "v1",
    params(("id" = String, Path)),
    request_body = serde_json::Value,
    responses(
        (status = 200, body = ApiData<InteractionSummary>),
        (status = 400, body = catalyrst_types::ApiErrorBody),
        (status = 401, body = catalyrst_types::ApiErrorBody),
        (status = 404, body = catalyrst_types::ApiErrorBody),
        (status = 500, body = catalyrst_types::ApiErrorBody),
        (status = 503, body = catalyrst_types::ApiErrorBody)
    )
)]
pub async fn put_v1_destination_likes(
    State(state): State<AppState>,
    method: Method,
    OriginalUri(uri): OriginalUri,
    headers: HeaderMap,
    Path(id): Path<String>,
    body: Option<Json<Value>>,
) -> Result<Json<ApiData<InteractionSummary>>, ApiError> {
    let user = interaction_actor(&headers, &method, &uri).await?;
    let like = parse_like_body(body.as_ref().map(|Json(v)| v))?;
    let summary = set_destination_like(&state, &id, &user, Some(like)).await?;
    Ok(Json(ApiData::ok(summary)))
}

#[utoipa::path(
    delete,
    path = "/destinations/{id}/likes",
    tag = "v1",
    params(("id" = String, Path)),
    responses(
        (status = 200, body = ApiData<InteractionSummary>),
        (status = 401, body = catalyrst_types::ApiErrorBody),
        (status = 404, body = catalyrst_types::ApiErrorBody),
        (status = 500, body = catalyrst_types::ApiErrorBody),
        (status = 503, body = catalyrst_types::ApiErrorBody)
    )
)]
pub async fn delete_v1_destination_likes(
    State(state): State<AppState>,
    method: Method,
    OriginalUri(uri): OriginalUri,
    headers: HeaderMap,
    Path(id): Path<String>,
) -> Result<Json<ApiData<InteractionSummary>>, ApiError> {
    let user = interaction_actor(&headers, &method, &uri).await?;
    let summary = set_destination_like(&state, &id, &user, None).await?;
    Ok(Json(ApiData::ok(summary)))
}

pub fn destination_category(name: String, count: i64) -> DestinationCategory {
    let en = category_i18n_en(&name)
        .map(str::to_string)
        .unwrap_or_else(|| name.clone());
    DestinationCategory {
        name,
        count,
        i18n: CategoryI18n { en },
    }
}

#[utoipa::path(
    get,
    path = "/categories",
    tag = "v1",
    params(("target" = Option<String>, Query)),
    responses(
        (status = 200, body = ApiJsonList),
        (status = 500, body = catalyrst_types::ApiErrorBody),
        (status = 503, body = catalyrst_types::ApiErrorBody)
    )
)]
pub async fn get_v1_categories(
    State(state): State<AppState>,
    Query(pairs): Query<Vec<(String, String)>>,
) -> Result<Json<ApiJsonList>, ApiError> {
    let target = pairs
        .iter()
        .find(|(k, _)| k == "target")
        .map(|(_, v)| v.as_str())
        .unwrap_or("destinations");
    if target == "events" {
        let data = state.events.event_categories().await?;
        return Ok(Json(ApiJsonList::ok(data)));
    }
    let counts = state.places.category_counts(CategoryTarget::All).await?;
    let data = counts
        .into_iter()
        .map(|(name, count)| destination_category(name, count))
        .map(|c| serde_json::to_value(c).unwrap_or(Value::Null))
        .collect();
    Ok(Json(ApiJsonList::ok(data)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn like_body_accepts_exactly_a_boolean_like() {
        assert!(parse_like_body(Some(&json!({ "like": true }))).unwrap());
        assert!(!parse_like_body(Some(&json!({ "like": false }))).unwrap());
    }

    #[test]
    fn like_body_rejects_everything_else() {
        assert!(parse_like_body(None).is_err());
        assert!(parse_like_body(Some(&json!({}))).is_err());
        assert!(parse_like_body(Some(&json!({ "like": "yes" }))).is_err());
        assert!(parse_like_body(Some(&json!({ "like": null }))).is_err());
        assert!(parse_like_body(Some(&json!({ "like": true, "extra": 1 }))).is_err());
        assert!(parse_like_body(Some(&json!([true]))).is_err());
    }

    #[test]
    fn interaction_summary_takes_the_six_counters() {
        let row = crate::ports::places::PlaceRow {
            id: "uuid-1".to_string(),
            title: None,
            description: None,
            image: None,
            owner: None,
            positions: vec![],
            base_position: "0,0".to_string(),
            contact_name: None,
            contact_email: None,
            content_rating: None,
            disabled: false,
            disabled_at: None,
            disabled_reason: None,
            created_at: None,
            updated_at: None,
            favorites: 3,
            likes: 2,
            dislikes: 1,
            categories: vec![],
            highlighted: false,
            highlighted_image: None,
            ranking: None,
            exclude_from_ranking: false,
            sdk: None,
            creator_address: None,
            world_id: None,
            deployment_id: None,
            deployed_at: None,
            world: false,
            world_name: None,
            is_private: false,
            show_in_places: true,
            single_player: false,
            skybox_time: None,
            user_favorite: true,
            user_like: false,
            user_dislike: true,
            user_count: None,
            user_visits: 0,
            like_rate: None,
            like_score: None,
            live: None,
            live_event_name: None,
            connected_addresses: None,
            realms_detail: None,
        };
        let v = serde_json::to_value(InteractionSummary::from(&row)).unwrap();
        assert_eq!(
            v,
            json!({
                "likes": 2, "dislikes": 1, "favorites": 3,
                "user_like": false, "user_dislike": true, "user_favorite": true
            })
        );
    }

    #[test]
    fn destination_categories_fall_back_to_the_name_for_the_label() {
        let known = destination_category("art".to_string(), 4);
        assert_eq!(known.i18n.en, "\u{1F3A8} Art");
        let unknown = destination_category("zzz".to_string(), 0);
        assert_eq!(unknown.i18n.en, "zzz");
        let v = serde_json::to_value(unknown).unwrap();
        assert_eq!(
            v,
            json!({ "name": "zzz", "count": 0, "i18n": { "en": "zzz" } })
        );
    }
}
