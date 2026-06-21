use axum::extract::{OriginalUri, Path, State};
use axum::http::{HeaderMap, Method};
use axum::Json;
use catalyrst_fed::{Signed, TypedMessage};
use serde::de::DeserializeOwned;
use serde_json::{json, Value};

use crate::auth::{auth_address_verified, require_admin_bearer, require_bearer_token};
use crate::fed::apply as fed_apply;
use crate::fed::messages::{PlaceFavorite, PlaceReport, PlaceVote};
use crate::fed::replay;
use crate::http::errors::ApiError;
use crate::http::response::ApiData;
use crate::ports::places::PlaceRow;
use crate::AppState;

/// A `Signed<T>` JSON envelope is `{ domain, message, signature, nonce,
/// signed_at }`. We branch on its presence so the legacy auth-address body
/// (`{ favorites: bool }` / `{ like: bool|null }`) still works.
fn is_federation_envelope(body: &Option<Json<Value>>) -> bool {
    body.as_ref()
        .and_then(|Json(v)| v.as_object())
        .map(|o| {
            o.contains_key("domain") && o.contains_key("message") && o.contains_key("signature")
        })
        .unwrap_or(false)
}

/// Verify envelope + domain + replay + per-wallet rate limit. Returns the
/// recovered wallet signer (places.md: post session-delegation resolution).
async fn preflight<T: TypedMessage + DeserializeOwned>(
    state: &AppState,
    headers: &HeaderMap,
    body: &Option<Json<Value>>,
) -> Result<(Signed<T>, String), ApiError> {
    let raw = body
        .as_ref()
        .map(|Json(v)| v.clone())
        .ok_or_else(|| ApiError::bad_request("missing signed body"))?;
    let signed: Signed<T> = serde_json::from_value(raw).map_err(|e| {
        ApiError::bad_request(format!("invalid Signed<{}>: {}", T::PRIMARY_TYPE, e))
    })?;

    // signer is recovered from the signature (00-primitives.md §2.1). The
    // auth-chain header (if present) is cross-checked when available.
    let signer = signed
        .signer()
        .map_err(|e| ApiError::unauthorized(format!("signature verify: {}", e)))?;
    if let Some(addr) = crate::auth::auth_address_optional(headers) {
        if !addr.eq_ignore_ascii_case(&signer) {
            return Err(ApiError::unauthorized(
                "auth-chain signer != envelope signer",
            ));
        }
    }
    let now = chrono::Utc::now().timestamp();
    signed
        .verify(&signer, now)
        .map_err(|e| ApiError::unauthorized(format!("signature verify: {}", e)))?;
    if !signed.domain.name.eq_ignore_ascii_case(&state.domain.name) {
        return Err(ApiError::bad_request(format!(
            "domain mismatch: expected {}",
            state.domain.name
        )));
    }
    replay::check_and_record(
        state.places.writer_pool(),
        &signer,
        &signed.nonce,
        signed.signed_at,
    )
    .await
    .map_err(|e| ApiError::bad_request(format!("replay: {}", e)))?;
    Ok((signed, signer))
}

fn body_bool(body: &Option<Json<Value>>, key: &str) -> Option<bool> {
    body.as_ref()
        .and_then(|Json(v)| v.get(key))
        .and_then(|v| v.as_bool())
}

/// A place entity id is a UUID; world routes reject these so callers use the
/// place-namespaced route (places ac340f2 isPlaceId guard).
fn is_place_uuid(s: &str) -> bool {
    let b = s.as_bytes();
    b.len() == 36
        && b[8] == b'-'
        && b[13] == b'-'
        && b[18] == b'-'
        && b[23] == b'-'
        && s.chars().all(|c| c.is_ascii_hexdigit() || c == '-')
}

/// World routes resolve worlds-only (`find_world_by_id`) with a "world" 404;
/// place routes accept place-UUID or world id (places f55bb30 retrocompat).
async fn resolve_entity(
    state: &AppState,
    entity_id: &str,
    is_world: bool,
) -> Result<PlaceRow, ApiError> {
    if is_world {
        state
            .places
            .find_world_by_id(entity_id)
            .await?
            .ok_or_else(|| ApiError::not_found(format!("Not found world \"{}\"", entity_id)))
    } else {
        state
            .places
            .find_by_id(entity_id)
            .await?
            .ok_or_else(|| ApiError::not_found(format!("Not found entity \"{}\"", entity_id)))
    }
}

fn body_like(body: &Option<Json<Value>>) -> Option<Option<bool>> {
    let v = body.as_ref()?.0.get("like")?;
    if v.is_null() {
        Some(None)
    } else {
        v.as_bool().map(Some)
    }
}

/// Best-effort gossip of a locally-applied signed place opinion (places.md §4).
/// Never fails the request; the action is durable in `signed_actions_places`
/// and recoverable via snapshot.
async fn emit_gossip<T>(state: &AppState, signed: &Signed<T>, sig_hash: &str, signer: &str)
where
    T: TypedMessage + serde::Serialize,
{
    match catalyrst_fed::GossipEnvelope::local(
        catalyrst_fed::Scope::Places,
        signed,
        sig_hash.to_string(),
        signer.to_ascii_lowercase(),
    ) {
        Ok(env) => {
            if let Err(e) = state.gossip.publish(&env).await {
                tracing::warn!(error = %e, signature_hash = %sig_hash, "places gossip publish failed (action durable; peers reconcile via snapshot)");
            }
        }
        Err(e) => tracing::warn!(error = %e, "failed to build places gossip envelope"),
    }
}

pub async fn patch_place_favorites(
    State(state): State<AppState>,
    method: Method,
    OriginalUri(uri): OriginalUri,
    headers: HeaderMap,
    Path(entity_id): Path<String>,
    body: Option<Json<Value>>,
) -> Result<Json<Value>, ApiError> {
    do_patch_favorites(state, method, uri, headers, entity_id, body, false).await
}

async fn do_patch_favorites(
    state: AppState,
    method: Method,
    uri: axum::http::Uri,
    headers: HeaderMap,
    entity_id: String,
    body: Option<Json<Value>>,
    is_world: bool,
) -> Result<Json<Value>, ApiError> {
    if is_federation_envelope(&body) {
        return fed_patch_favorites(&state, &headers, &entity_id, &body).await;
    }
    let user = auth_address_verified(&headers, method.as_str(), uri.path())?;
    let favorites_req = body_bool(&body, "favorites").ok_or_else(|| {
        ApiError::bad_request("Invalid favorites body. Expected { favorites: boolean }.")
    })?;

    let mut entity = resolve_entity(&state, &entity_id, is_world).await?;
    state
        .places
        .apply_user_interactions(Some(&user), std::slice::from_mut(&mut entity))
        .await;

    if favorites_req == entity.user_favorite {
        return Ok(Json(json!({
            "ok": true,
            "data": { "favorites": entity.favorites, "user_favorite": entity.user_favorite }
        })));
    }

    let (favorites, user_favorite) = state
        .places
        .set_favorite(
            &entity.id,
            &user,
            favorites_req,
            entity.favorites,
            entity.user_favorite,
        )
        .await?;
    Ok(Json(json!({
        "ok": true,
        "data": { "favorites": favorites, "user_favorite": user_favorite }
    })))
}

async fn fed_patch_favorites(
    state: &AppState,
    headers: &HeaderMap,
    entity_id: &str,
    body: &Option<Json<Value>>,
) -> Result<Json<Value>, ApiError> {
    let (signed, signer) = preflight::<PlaceFavorite>(state, headers, body).await?;
    if signed.message.place_id != entity_id {
        return Err(ApiError::bad_request(
            "place_id in body does not match path",
        ));
    }
    let (applied, favorites, user_favorite) =
        fed_apply::apply_favorite(state, &signed, &signer, None).await?;
    if applied.fresh {
        emit_gossip(state, &signed, &applied.signature_hash, &signer).await;
    }
    Ok(Json(json!({
        "ok": true,
        "signature_hash": applied.signature_hash,
        "data": { "favorites": favorites, "user_favorite": user_favorite }
    })))
}

async fn fed_patch_likes(
    state: &AppState,
    headers: &HeaderMap,
    entity_id: &str,
    body: &Option<Json<Value>>,
) -> Result<Json<Value>, ApiError> {
    let (signed, signer) = preflight::<PlaceVote>(state, headers, body).await?;
    if signed.message.place_id != entity_id {
        return Err(ApiError::bad_request(
            "place_id in body does not match path",
        ));
    }
    let (applied, likes, dislikes, user_like, user_dislike) =
        fed_apply::apply_vote(state, &signed, &signer, None).await?;
    if applied.fresh {
        emit_gossip(state, &signed, &applied.signature_hash, &signer).await;
    }
    Ok(Json(json!({
        "ok": true,
        "signature_hash": applied.signature_hash,
        "data": { "likes": likes, "dislikes": dislikes, "user_like": user_like, "user_dislike": user_dislike }
    })))
}

pub async fn patch_place_likes(
    State(state): State<AppState>,
    method: Method,
    OriginalUri(uri): OriginalUri,
    headers: HeaderMap,
    Path(entity_id): Path<String>,
    body: Option<Json<Value>>,
) -> Result<Json<Value>, ApiError> {
    do_patch_likes(state, method, uri, headers, entity_id, body, false).await
}

async fn do_patch_likes(
    state: AppState,
    method: Method,
    uri: axum::http::Uri,
    headers: HeaderMap,
    entity_id: String,
    body: Option<Json<Value>>,
    is_world: bool,
) -> Result<Json<Value>, ApiError> {
    if is_federation_envelope(&body) {
        return fed_patch_likes(&state, &headers, &entity_id, &body).await;
    }
    let user = auth_address_verified(&headers, method.as_str(), uri.path())?;
    let like_req = body_like(&body).ok_or_else(|| {
        ApiError::bad_request("Invalid likes body. Expected { like: boolean|null }.")
    })?;

    let mut entity = resolve_entity(&state, &entity_id, is_world).await?;
    state
        .places
        .apply_user_interactions(Some(&user), std::slice::from_mut(&mut entity))
        .await;

    let current = if entity.user_like {
        Some(true)
    } else if entity.user_dislike {
        Some(false)
    } else {
        None
    };
    if current == like_req {
        return Ok(Json(json!({
            "ok": true,
            "data": {
                "likes": entity.likes,
                "dislikes": entity.dislikes,
                "user_like": entity.user_like,
                "user_dislike": entity.user_dislike,
            }
        })));
    }

    // Capture the voter's Snapshot voting power at vote time (port of
    // updateLike's `fetchScore` call). Only fetched when storing a vote — a
    // withdraw (like == null) deletes the row, so no score is needed.
    let user_activity = match like_req {
        Some(_) => crate::snapshot::fetch_score(&user).await,
        None => 0.0,
    };
    let (likes, dislikes, user_like, user_dislike) = state
        .places
        .set_like(
            &entity.id,
            &user,
            like_req,
            user_activity,
            entity.likes,
            entity.dislikes,
            entity.user_like,
            entity.user_dislike,
        )
        .await?;
    Ok(Json(json!({
        "ok": true,
        "data": {
            "likes": likes,
            "dislikes": dislikes,
            "user_like": user_like,
            "user_dislike": user_dislike,
        }
    })))
}

pub async fn fed_post_report(
    state: &AppState,
    headers: &HeaderMap,
    body: &Option<Json<Value>>,
) -> Result<Json<Value>, ApiError> {
    let (signed, signer) = preflight::<PlaceReport>(state, headers, body).await?;
    let applied = fed_apply::apply_report(state, &signed, &signer, None).await?;
    if applied.fresh {
        emit_gossip(state, &signed, &applied.signature_hash, &signer).await;
    }
    Ok(Json(json!({
        "ok": true,
        "signature_hash": applied.signature_hash,
        "data": { "place_id": signed.message.place_id }
    })))
}

pub async fn patch_world_favorites(
    State(state): State<AppState>,
    method: Method,
    OriginalUri(uri): OriginalUri,
    headers: HeaderMap,
    Path(world_id): Path<String>,
    body: Option<Json<Value>>,
) -> Result<Json<Value>, ApiError> {
    if !is_federation_envelope(&body) && is_place_uuid(&world_id) {
        return Err(ApiError::bad_request(format!(
            "Invalid world ID \"{}\". Use /places/:entity_id/favorites for place entities.",
            world_id
        )));
    }
    do_patch_favorites(state, method, uri, headers, world_id, body, true).await
}

pub async fn patch_world_likes(
    State(state): State<AppState>,
    method: Method,
    OriginalUri(uri): OriginalUri,
    headers: HeaderMap,
    Path(world_id): Path<String>,
    body: Option<Json<Value>>,
) -> Result<Json<Value>, ApiError> {
    if !is_federation_envelope(&body) && is_place_uuid(&world_id) {
        return Err(ApiError::bad_request(format!(
            "Invalid world ID \"{}\". Use /places/:entity_id/likes for place entities.",
            world_id
        )));
    }
    do_patch_likes(state, method, uri, headers, world_id, body, true).await
}

fn require_admin(
    state: &AppState,
    headers: &HeaderMap,
    method: &str,
    path: &str,
    action: &str,
) -> Result<(), ApiError> {
    // Bearer parity (admin-console.md §4): the console can't forge a user
    // auth-chain signature, so these routes ALSO accept the admin bearer token
    // in addition to the existing admin signed-fetch path. This is additive —
    // a valid admin signature still authorizes when no bearer is presented.
    if crate::auth::bearer_token(headers).is_some() {
        return require_admin_bearer(headers, state.admin_auth_token.as_deref());
    }
    let user = auth_address_verified(headers, method, path)?;
    if state.admin_addresses.contains(&user) {
        Ok(())
    } else {
        Err(ApiError::forbidden(format!(
            "Only admin allowed to update {action}"
        )))
    }
}

async fn fetch_place(state: &AppState, place_id: &str) -> Result<PlaceRow, ApiError> {
    state
        .places
        .find_by_id(place_id)
        .await?
        .ok_or_else(|| ApiError::not_found(format!("Not found place \"{}\"", place_id)))
}

async fn fetch_world(state: &AppState, world_id: &str) -> Result<PlaceRow, ApiError> {
    state
        .places
        .find_world_by_id(world_id)
        .await?
        .ok_or_else(|| ApiError::not_found(format!("Not found world \"{}\"", world_id)))
}

fn body_ranking(body: &Option<Json<Value>>) -> Result<Option<f64>, ApiError> {
    let v = body
        .as_ref()
        .and_then(|Json(v)| v.get("ranking"))
        .ok_or_else(|| {
            ApiError::bad_request("Invalid ranking body. Expected { ranking: number|null }.")
        })?;
    if v.is_null() {
        Ok(None)
    } else {
        v.as_f64().map(Some).ok_or_else(|| {
            ApiError::bad_request("Invalid ranking body. Expected { ranking: number|null }.")
        })
    }
}

const ALLOWED_RATINGS: [&str; 5] = ["PR", "E", "T", "A", "R"];

fn body_content_rating(body: &Option<Json<Value>>) -> Result<String, ApiError> {
    let v = body
        .as_ref()
        .and_then(|Json(v)| v.get("content_rating"))
        .and_then(|v| v.as_str())
        .ok_or_else(|| ApiError::bad_request("content rating body needed"))?;
    if ALLOWED_RATINGS.contains(&v) {
        Ok(v.to_string())
    } else {
        Err(ApiError::bad_request("content rating body needed"))
    }
}

pub async fn put_place_rating(
    State(state): State<AppState>,
    method: Method,
    OriginalUri(uri): OriginalUri,
    headers: HeaderMap,
    Path(place_id): Path<String>,
    body: Option<Json<Value>>,
) -> Result<Json<ApiData<PlaceRow>>, ApiError> {
    require_admin(&state, &headers, method.as_str(), uri.path(), "rating")?;
    let rating = body_content_rating(&body)?;
    let mut place = fetch_place(&state, &place_id).await?;
    state.places.set_content_rating(&place_id, &rating).await?;
    place.content_rating = Some(rating);
    Ok(Json(ApiData::ok(place)))
}

pub async fn put_place_ranking(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(place_id): Path<String>,
    body: Option<Json<Value>>,
) -> Result<Json<ApiData<PlaceRow>>, ApiError> {
    require_bearer_token(&headers, state.data_team_auth_token.as_deref())?;
    let ranking = body_ranking(&body)?;
    let mut place = fetch_place(&state, &place_id).await?;
    state.places.set_ranking(&place_id, ranking).await?;
    place.ranking = ranking;
    Ok(Json(ApiData::ok(place)))
}

pub async fn put_place_highlight(
    State(state): State<AppState>,
    method: Method,
    OriginalUri(uri): OriginalUri,
    headers: HeaderMap,
    Path(place_id): Path<String>,
    body: Option<Json<Value>>,
) -> Result<Json<ApiData<PlaceRow>>, ApiError> {
    require_admin(&state, &headers, method.as_str(), uri.path(), "highlight")?;
    let highlighted = body
        .as_ref()
        .and_then(|Json(v)| v.get("highlighted"))
        .and_then(|v| v.as_bool())
        .ok_or_else(|| {
            ApiError::bad_request("Invalid highlight body. Expected { highlighted: boolean }.")
        })?;
    let mut place = fetch_place(&state, &place_id).await?;
    state.places.set_highlighted(&place_id, highlighted).await?;
    place.highlighted = highlighted;
    Ok(Json(ApiData::ok(place)))
}

pub async fn put_place_featured(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(place_id): Path<String>,
) -> Result<Json<ApiData<PlaceRow>>, ApiError> {
    require_bearer_token(&headers, state.admin_auth_token.as_deref())?;
    let mut place = fetch_place(&state, &place_id).await?;
    state.places.set_highlighted(&place_id, true).await?;
    place.highlighted = true;
    Ok(Json(ApiData::ok(place)))
}

pub async fn delete_place_featured(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(place_id): Path<String>,
) -> Result<Json<ApiData<PlaceRow>>, ApiError> {
    require_bearer_token(&headers, state.admin_auth_token.as_deref())?;
    let mut place = fetch_place(&state, &place_id).await?;
    state.places.set_highlighted(&place_id, false).await?;
    place.highlighted = false;
    Ok(Json(ApiData::ok(place)))
}

pub async fn put_world_highlight(
    State(state): State<AppState>,
    method: Method,
    OriginalUri(uri): OriginalUri,
    headers: HeaderMap,
    Path(world_id): Path<String>,
    body: Option<Json<Value>>,
) -> Result<Json<ApiData<PlaceRow>>, ApiError> {
    require_admin(&state, &headers, method.as_str(), uri.path(), "highlight")?;
    let highlighted = body
        .as_ref()
        .and_then(|Json(v)| v.get("highlighted"))
        .and_then(|v| v.as_bool())
        .ok_or_else(|| {
            ApiError::bad_request("Invalid highlight body. Expected { highlighted: boolean }.")
        })?;
    let mut world = fetch_world(&state, &world_id).await?;
    state.places.set_highlighted(&world.id, highlighted).await?;
    world.highlighted = highlighted;
    Ok(Json(ApiData::ok(world)))
}

pub async fn put_world_ranking(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(world_id): Path<String>,
    body: Option<Json<Value>>,
) -> Result<Json<ApiData<PlaceRow>>, ApiError> {
    require_bearer_token(&headers, state.data_team_auth_token.as_deref())?;
    let ranking = body_ranking(&body)?;
    let mut world = fetch_world(&state, &world_id).await?;
    state.places.set_ranking(&world.id, ranking).await?;
    world.ranking = ranking;
    Ok(Json(ApiData::ok(world)))
}

pub async fn put_world_rating(
    State(state): State<AppState>,
    method: Method,
    OriginalUri(uri): OriginalUri,
    headers: HeaderMap,
    Path(world_id): Path<String>,
    body: Option<Json<Value>>,
) -> Result<Json<ApiData<PlaceRow>>, ApiError> {
    require_admin(&state, &headers, method.as_str(), uri.path(), "rating")?;
    let rating = body_content_rating(&body)?;
    let mut world = fetch_world(&state, &world_id).await?;
    state.places.set_content_rating(&world.id, &rating).await?;
    world.content_rating = Some(rating);
    Ok(Json(ApiData::ok(world)))
}

pub async fn put_world_featured(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(world_id): Path<String>,
) -> Result<Json<ApiData<PlaceRow>>, ApiError> {
    require_bearer_token(&headers, state.admin_auth_token.as_deref())?;
    let mut world = fetch_world(&state, &world_id).await?;
    state.places.set_highlighted(&world.id, true).await?;
    world.highlighted = true;
    Ok(Json(ApiData::ok(world)))
}

pub async fn delete_world_featured(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(world_id): Path<String>,
) -> Result<Json<ApiData<PlaceRow>>, ApiError> {
    require_bearer_token(&headers, state.admin_auth_token.as_deref())?;
    let mut world = fetch_world(&state, &world_id).await?;
    state.places.set_highlighted(&world.id, false).await?;
    world.highlighted = false;
    Ok(Json(ApiData::ok(world)))
}

#[cfg(test)]
mod tests {
    use super::is_place_uuid;

    #[test]
    fn place_uuid_guard() {
        assert!(is_place_uuid("123e4567-e89b-12d3-a456-426614174000"));
        assert!(!is_place_uuid("my-world.dcl.eth"));
        assert!(!is_place_uuid("123e4567e89b12d3a456426614174000"));
        assert!(!is_place_uuid("123e4567-e89b-12d3-a456-42661417400g"));
        assert!(!is_place_uuid(""));
    }
}
