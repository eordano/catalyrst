use axum::body::Bytes;
use axum::extract::{Path, State};
use axum::http::{HeaderMap, StatusCode};
use axum::Json;
use catalyrst_fed::{FedError, RateLimitDecision, Signed, TypedMessage};
use serde::de::DeserializeOwned;
use serde::Deserialize;
use serde_json::json;
use uuid::Uuid;

use crate::auth_chain::require_signer;
use crate::fed::apply;
use crate::fed::authority::{can_grant, community_exists, load_role, require_min_role, Role};
use crate::fed::messages::{
    CommunityBan, CommunityCreate, CommunityDelete, CommunityJoin, CommunityLeave,
    CommunityPlaceRemove, CommunityPlacesAdd, CommunityPost, CommunityPostDelete,
    CommunityPostLike, CommunityPostUnlike, CommunityRequestStatusUpdate, CommunityRole,
    CommunityUnban, CommunityUpdate,
};
use crate::fed::ids::{community_id_hex, community_uuid_from_hex};
use crate::http::ApiError;
use crate::AppState;

fn into_resp(t: (StatusCode, Json<serde_json::Value>)) -> axum::response::Response {
    use axum::response::IntoResponse;
    t.into_response()
}

fn err_json(code: StatusCode, message: impl Into<String>) -> (StatusCode, Json<serde_json::Value>) {
    let m = message.into();
    (code, Json(json!({ "ok": false, "message": m })))
}

/// Best-effort gossip emission for a locally-applied signed action.
///
/// Per `docs/federation/00-primitives.md §2.1` the wire format is the verbatim
/// `Signed<T>` JSON; we wrap it in a `GossipEnvelope` and publish on the
/// communities subject. A transport failure NEVER fails the request — the
/// action is already durable in Postgres and recoverable by peers via the
/// `/federation/communities/changes` snapshot-pull path (`communities.md §5`).
async fn emit_gossip<T>(state: &AppState, signed: &Signed<T>, sig_hash: &str, signer: &str)
where
    T: TypedMessage + serde::Serialize,
{
    let env = match catalyrst_fed::GossipEnvelope::local(
        catalyrst_fed::Scope::Communities,
        signed,
        sig_hash.to_string(),
        signer.to_ascii_lowercase(),
    ) {
        Ok(e) => e,
        Err(e) => {
            tracing::warn!(error = %e, "failed to build gossip envelope");
            return;
        }
    };
    if let Err(e) = state.gossip.publish(&env).await {
        tracing::warn!(error = %e, signature_hash = %sig_hash, "gossip publish failed (action is durable; peers reconcile via snapshot pull)");
    }
}

fn ok_json(sig_hash: String) -> (StatusCode, Json<serde_json::Value>) {
    (
        StatusCode::OK,
        Json(json!({ "ok": true, "signature_hash": sig_hash })),
    )
}

fn ok_json_with(sig_hash: String, extra: serde_json::Value) -> (StatusCode, Json<serde_json::Value>) {
    let mut base = json!({ "ok": true, "signature_hash": sig_hash });
    if let (Some(b), Some(e)) = (base.as_object_mut(), extra.as_object()) {
        for (k, v) in e {
            b.insert(k.clone(), v.clone());
        }
    }
    (StatusCode::OK, Json(base))
}

fn parse_signed<T: TypedMessage + DeserializeOwned>(
    body: &[u8],
) -> Result<Signed<T>, (StatusCode, Json<serde_json::Value>)> {
    serde_json::from_slice::<Signed<T>>(body).map_err(|e| {
        err_json(
            StatusCode::BAD_REQUEST,
            format!("invalid Signed<{}>: {}", T::PRIMARY_TYPE, e),
        )
    })
}

async fn preflight<T: TypedMessage + DeserializeOwned>(
    state: &AppState,
    headers: &HeaderMap,
    method: &str,
    path: &str,
    body: &[u8],
) -> Result<(Signed<T>, String), (StatusCode, Json<serde_json::Value>)> {
    let outer_signer = require_signer(headers, method, path).map_err(|e| {
        err_json(StatusCode::UNAUTHORIZED, format!("auth chain: {}", e))
    })?;

    let signed: Signed<T> = parse_signed(body)?;

    let now = chrono::Utc::now().timestamp();
    if let Err(e) = signed.verify(&outer_signer, now) {
        return Err(err_json(
            StatusCode::UNAUTHORIZED,
            format!("signature verify: {}", e),
        ));
    }

    if !signed.domain.name.eq_ignore_ascii_case(&state.domain.name) {
        return Err(err_json(
            StatusCode::BAD_REQUEST,
            format!("domain mismatch: expected {}", state.domain.name),
        ));
    }

    if let Err(e) = state
        .replay
        .check_and_record(&outer_signer, &signed.nonce, signed.signed_at)
        .await
    {
        return Err(match e {
            FedError::DuplicateNonce { .. } => err_json(StatusCode::CONFLICT, e.to_string()),
            _ => err_json(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()),
        });
    }

    if matches!(state.limiter.check(&outer_signer), RateLimitDecision::Deny) {
        return Err(err_json(StatusCode::TOO_MANY_REQUESTS, "rate limit exceeded"));
    }

    Ok((signed, outer_signer))
}

fn map_apply_err(e: ApiError) -> (StatusCode, Json<serde_json::Value>) {
    let (code, message) = match e {
        ApiError::Http(catalyrst_types::HttpError { code, message }) => (code, message),
        ApiError::Database(de) => {
            tracing::error!(error = %de, "apply database error");
            (500, "database error".to_string())
        }
        other => (500, other.to_string()),
    };
    let status = StatusCode::from_u16(code).unwrap_or(StatusCode::INTERNAL_SERVER_ERROR);
    (status, Json(json!({ "ok": false, "message": message })))
}

fn uuid_from_path(s: &str) -> Result<Uuid, (StatusCode, Json<serde_json::Value>)> {
    Uuid::parse_str(s).map_err(|_| err_json(StatusCode::BAD_REQUEST, "invalid uuid"))
}

pub async fn create_community(
    State(state): State<AppState>,
    headers: HeaderMap,
    body: Bytes,
) -> axum::response::Response {
    if crate::handlers::client::is_federation_envelope(&body) {
        return into_resp(fed_create_community(State(state), headers, body).await);
    }
    crate::handlers::client::create_community(State(state), headers, body).await
}

async fn fed_create_community(
    State(state): State<AppState>,
    headers: HeaderMap,
    body: Bytes,
) -> (StatusCode, Json<serde_json::Value>) {
    let (signed, signer) =
        match preflight::<CommunityCreate>(&state, &headers, "post", "/v1/communities", &body)
            .await
        {
            Ok(x) => x,
            Err(e) => return e,
        };

    if let Err(e) = crate::validate::validate_name(&signed.message.name) {
        return err_json(StatusCode::BAD_REQUEST, e);
    }
    if let Err(e) = crate::validate::validate_description(&signed.message.description) {
        return err_json(StatusCode::BAD_REQUEST, e);
    }

    let expected_id = community_id_hex(&signer, &signed.message.name, &signed.nonce);

    match apply::apply_create(&state.pool, &signed, &signer).await {
        Ok(out) => {
            emit_gossip(&state, &signed, &out.signature_hash, &signer).await;
            ok_json_with(
                out.signature_hash,
                json!({ "community_id": out.community_id, "id": out.uuid, "expected_id": expected_id }),
            )
        }
        Err(e) => map_apply_err(e),
    }
}

async fn run_community_update(
    state: AppState,
    headers: HeaderMap,
    id: String,
    body: Bytes,
    method: &str,
) -> (StatusCode, Json<serde_json::Value>) {
    let uuid = match uuid_from_path(&id) {
        Ok(u) => u,
        Err(e) => return e,
    };
    let path = format!("/v1/communities/{}", id);
    let (signed, signer) =
        match preflight::<CommunityUpdate>(&state, &headers, method, &path, &body).await {
            Ok(x) => x,
            Err(e) => return e,
        };
    if community_uuid_from_hex(&signed.message.community_id) != uuid {
        return err_json(StatusCode::BAD_REQUEST, "community_id in body does not match path");
    }
    if let Err(e) = crate::validate::validate_name_opt(signed.message.name.as_deref()) {
        return err_json(StatusCode::BAD_REQUEST, e);
    }
    if let Err(e) = crate::validate::validate_description_opt(signed.message.description.as_deref()) {
        return err_json(StatusCode::BAD_REQUEST, e);
    }
    match require_min_role(&state.pool, &signed.message.community_id, &signer, Role::Admin).await {
        Ok(_) => {}
        Err(e) => return map_apply_err(e),
    }
    match apply::apply_update(&state.pool, &signed).await {
        Ok(sig) => {
            emit_gossip(&state, &signed, &sig, &signer).await;
            ok_json(sig)
        }
        Err(e) => map_apply_err(e),
    }
}

pub async fn update_community(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
    body: Bytes,
) -> axum::response::Response {
    if crate::handlers::client::is_federation_envelope(&body) {
        return into_resp(run_community_update(state, headers, id, body, "put").await);
    }
    crate::handlers::client::update_community(State(state), headers, Path(id), body).await
}

pub async fn update_community_partially(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
    body: Bytes,
) -> axum::response::Response {
    if crate::handlers::client::is_federation_envelope(&body) {
        return into_resp(run_community_update(state, headers, id, body, "patch").await);
    }
    crate::handlers::client::update_community_partially(State(state), headers, Path(id), body).await
}

pub async fn delete_community(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
    body: Bytes,
) -> axum::response::Response {
    if !crate::handlers::client::is_federation_envelope(&body) {
        return crate::handlers::client::delete_community(State(state), headers, Path(id)).await;
    }
    into_resp(fed_delete_community(State(state), headers, Path(id), body).await)
}

async fn fed_delete_community(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
    body: Bytes,
) -> (StatusCode, Json<serde_json::Value>) {
    let uuid = match uuid_from_path(&id) {
        Ok(u) => u,
        Err(e) => return e,
    };
    let path = format!("/v1/communities/{}", id);
    let (signed, signer) =
        match preflight::<CommunityDelete>(&state, &headers, "delete", &path, &body).await {
            Ok(x) => x,
            Err(e) => return e,
        };
    if community_uuid_from_hex(&signed.message.community_id) != uuid {
        return err_json(StatusCode::BAD_REQUEST, "community_id mismatch");
    }
    match require_min_role(&state.pool, &signed.message.community_id, &signer, Role::Owner).await {
        Ok(_) => {}
        Err(e) => return map_apply_err(e),
    }
    match apply::apply_delete(&state.pool, &signed).await {
        Ok(sig) => {
            emit_gossip(&state, &signed, &sig, &signer).await;
            ok_json(sig)
        }
        Err(e) => map_apply_err(e),
    }
}

pub async fn add_member(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
    body: Bytes,
) -> axum::response::Response {
    if !crate::handlers::client::is_federation_envelope(&body) {
        return crate::handlers::client::add_member(State(state), headers, Path(id)).await;
    }
    into_resp(fed_add_member(State(state), headers, Path(id), body).await)
}

async fn fed_add_member(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
    body: Bytes,
) -> (StatusCode, Json<serde_json::Value>) {
    let uuid = match uuid_from_path(&id) {
        Ok(u) => u,
        Err(e) => return e,
    };
    let path = format!("/v1/communities/{}/members", id);
    let (signed, signer) =
        match preflight::<CommunityJoin>(&state, &headers, "post", &path, &body).await {
            Ok(x) => x,
            Err(e) => return e,
        };
    if community_uuid_from_hex(&signed.message.community_id) != uuid {
        return err_json(StatusCode::BAD_REQUEST, "community_id mismatch");
    }
    match community_exists(&state.pool, &signed.message.community_id).await {
        Ok(true) => {}
        Ok(false) => return err_json(StatusCode::NOT_FOUND, "community not found"),
        Err(e) => return map_apply_err(e),
    }
    match load_role(&state.pool, &signed.message.community_id, &signer).await {
        Ok(Role::Banned) => return err_json(StatusCode::FORBIDDEN, "banned from community"),
        Ok(_) => {}
        Err(e) => return map_apply_err(e),
    }
    match apply::apply_join(&state.pool, &signed, &signer).await {
        Ok(sig) => {
            emit_gossip(&state, &signed, &sig, &signer).await;
            ok_json(sig)
        }
        Err(e) => map_apply_err(e),
    }
}

#[derive(Debug, Deserialize)]
pub struct PathIdAddr {
    pub id: String,
    pub address: String,
}

pub async fn remove_member(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(PathIdAddr { id, address }): Path<PathIdAddr>,
    body: Bytes,
) -> axum::response::Response {
    if !crate::handlers::client::is_federation_envelope(&body) {
        return crate::handlers::client::remove_member(
            State(state),
            headers,
            Path(crate::handlers::client::PathIdAddr { id, address }),
        )
        .await;
    }
    into_resp(fed_remove_member(State(state), headers, Path(PathIdAddr { id, address }), body).await)
}

async fn fed_remove_member(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(PathIdAddr { id, address }): Path<PathIdAddr>,
    body: Bytes,
) -> (StatusCode, Json<serde_json::Value>) {
    let uuid = match uuid_from_path(&id) {
        Ok(u) => u,
        Err(e) => return e,
    };
    let path = format!("/v1/communities/{}/members/{}", id, address);
    let (signed, signer) =
        match preflight::<CommunityLeave>(&state, &headers, "delete", &path, &body).await {
            Ok(x) => x,
            Err(e) => return e,
        };
    if community_uuid_from_hex(&signed.message.community_id) != uuid {
        return err_json(StatusCode::BAD_REQUEST, "community_id mismatch");
    }
    if !signed.message.member.eq_ignore_ascii_case(&signer) {
        return err_json(StatusCode::FORBIDDEN, "may only leave on behalf of self");
    }
    if !address.eq_ignore_ascii_case(&signer) {
        return err_json(StatusCode::FORBIDDEN, "path address must match signer");
    }
    match apply::apply_leave(&state.pool, &signed, &signer).await {
        Ok(sig) => {
            emit_gossip(&state, &signed, &sig, &signer).await;
            ok_json(sig)
        }
        Err(e) => map_apply_err(e),
    }
}

pub async fn update_member_role(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(PathIdAddr { id, address }): Path<PathIdAddr>,
    body: Bytes,
) -> axum::response::Response {
    if !crate::handlers::client::is_federation_envelope(&body) {
        return crate::handlers::client::update_member_role(
            State(state),
            headers,
            Path(crate::handlers::client::PathIdAddr { id, address }),
            body,
        )
        .await;
    }
    into_resp(fed_update_member_role(State(state), headers, Path(PathIdAddr { id, address }), body).await)
}

async fn fed_update_member_role(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(PathIdAddr { id, address }): Path<PathIdAddr>,
    body: Bytes,
) -> (StatusCode, Json<serde_json::Value>) {
    let uuid = match uuid_from_path(&id) {
        Ok(u) => u,
        Err(e) => return e,
    };
    let path = format!("/v1/communities/{}/members/{}", id, address);
    let (signed, signer) =
        match preflight::<CommunityRole>(&state, &headers, "patch", &path, &body).await {
            Ok(x) => x,
            Err(e) => return e,
        };
    if community_uuid_from_hex(&signed.message.community_id) != uuid {
        return err_json(StatusCode::BAD_REQUEST, "community_id mismatch");
    }
    if !signed.message.target.eq_ignore_ascii_case(&address) {
        return err_json(StatusCode::BAD_REQUEST, "target must match path address");
    }
    let actor = match require_min_role(&state.pool, &signed.message.community_id, &signer, Role::Admin).await {
        Ok(r) => r,
        Err(e) => return map_apply_err(e),
    };
    let new_role = match Role::parse(&signed.message.role) {
        Some(r) if r != Role::None => r,
        _ => return err_json(StatusCode::BAD_REQUEST, "invalid role"),
    };
    if !can_grant(actor, new_role) {
        return err_json(StatusCode::FORBIDDEN, "actor cannot grant this role");
    }
    match apply::apply_role(&state.pool, &signed, &signer).await {
        Ok(sig) => {
            emit_gossip(&state, &signed, &sig, &signer).await;
            ok_json(sig)
        }
        Err(e) => map_apply_err(e),
    }
}

pub async fn ban_member(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(PathIdAddr { id, address }): Path<PathIdAddr>,
    body: Bytes,
) -> axum::response::Response {
    if !crate::handlers::client::is_federation_envelope(&body) {
        return crate::handlers::client::ban_member(
            State(state),
            headers,
            Path(crate::handlers::client::PathIdAddr { id, address }),
        )
        .await;
    }
    into_resp(fed_ban_member(State(state), headers, Path(PathIdAddr { id, address }), body).await)
}

async fn fed_ban_member(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(PathIdAddr { id, address }): Path<PathIdAddr>,
    body: Bytes,
) -> (StatusCode, Json<serde_json::Value>) {
    let uuid = match uuid_from_path(&id) {
        Ok(u) => u,
        Err(e) => return e,
    };
    let path = format!("/v1/communities/{}/members/{}/bans", id, address);
    let (signed, signer) =
        match preflight::<CommunityBan>(&state, &headers, "post", &path, &body).await {
            Ok(x) => x,
            Err(e) => return e,
        };
    if community_uuid_from_hex(&signed.message.community_id) != uuid {
        return err_json(StatusCode::BAD_REQUEST, "community_id mismatch");
    }
    if !signed.message.target.eq_ignore_ascii_case(&address) {
        return err_json(StatusCode::BAD_REQUEST, "target must match path address");
    }
    let actor = match require_min_role(&state.pool, &signed.message.community_id, &signer, Role::Mod).await {
        Ok(r) => r,
        Err(e) => return map_apply_err(e),
    };
    let target_role = match load_role(&state.pool, &signed.message.community_id, &signed.message.target).await {
        Ok(r) => r,
        Err(e) => return map_apply_err(e),
    };
    if target_role >= actor {
        return err_json(StatusCode::FORBIDDEN, "cannot ban a peer or superior");
    }
    match apply::apply_ban(&state.pool, &signed, &signer).await {
        Ok(sig) => {
            emit_gossip(&state, &signed, &sig, &signer).await;
            ok_json(sig)
        }
        Err(e) => map_apply_err(e),
    }
}

pub async fn unban_member(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(PathIdAddr { id, address }): Path<PathIdAddr>,
    body: Bytes,
) -> axum::response::Response {
    if !crate::handlers::client::is_federation_envelope(&body) {
        return crate::handlers::client::unban_member(
            State(state),
            headers,
            Path(crate::handlers::client::PathIdAddr { id, address }),
        )
        .await;
    }
    into_resp(fed_unban_member(State(state), headers, Path(PathIdAddr { id, address }), body).await)
}

async fn fed_unban_member(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(PathIdAddr { id, address }): Path<PathIdAddr>,
    body: Bytes,
) -> (StatusCode, Json<serde_json::Value>) {
    let uuid = match uuid_from_path(&id) {
        Ok(u) => u,
        Err(e) => return e,
    };
    let path = format!("/v1/communities/{}/members/{}/bans", id, address);
    let (signed, signer) =
        match preflight::<CommunityUnban>(&state, &headers, "delete", &path, &body).await {
            Ok(x) => x,
            Err(e) => return e,
        };
    if community_uuid_from_hex(&signed.message.community_id) != uuid {
        return err_json(StatusCode::BAD_REQUEST, "community_id mismatch");
    }
    if !signed.message.target.eq_ignore_ascii_case(&address) {
        return err_json(StatusCode::BAD_REQUEST, "target must match path address");
    }
    match require_min_role(&state.pool, &signed.message.community_id, &signer, Role::Mod).await {
        Ok(_) => {}
        Err(e) => return map_apply_err(e),
    }
    match apply::apply_unban(&state.pool, &signed, &signer).await {
        Ok(sig) => {
            emit_gossip(&state, &signed, &sig, &signer).await;
            ok_json(sig)
        }
        Err(e) => map_apply_err(e),
    }
}

pub async fn add_places(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
    body: Bytes,
) -> axum::response::Response {
    if !crate::handlers::client::is_federation_envelope(&body) {
        return crate::handlers::client::add_places(State(state), headers, Path(id), body).await;
    }
    into_resp(fed_add_places(State(state), headers, Path(id), body).await)
}

async fn fed_add_places(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
    body: Bytes,
) -> (StatusCode, Json<serde_json::Value>) {
    let uuid = match uuid_from_path(&id) {
        Ok(u) => u,
        Err(e) => return e,
    };
    let path = format!("/v1/communities/{}/places", id);
    let (signed, signer) =
        match preflight::<CommunityPlacesAdd>(&state, &headers, "post", &path, &body).await {
            Ok(x) => x,
            Err(e) => return e,
        };
    if community_uuid_from_hex(&signed.message.community_id) != uuid {
        return err_json(StatusCode::BAD_REQUEST, "community_id mismatch");
    }
    match require_min_role(&state.pool, &signed.message.community_id, &signer, Role::Admin).await {
        Ok(_) => {}
        Err(e) => return map_apply_err(e),
    }
    match apply::apply_places_add(&state.pool, &signed, &signer).await {
        Ok(sig) => {
            emit_gossip(&state, &signed, &sig, &signer).await;
            ok_json(sig)
        }
        Err(e) => map_apply_err(e),
    }
}

#[derive(Debug, Deserialize)]
pub struct PathIdPlace {
    pub id: String,
    #[serde(rename = "placeId")]
    pub place_id: String,
}

pub async fn remove_place(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(PathIdPlace { id, place_id }): Path<PathIdPlace>,
    body: Bytes,
) -> axum::response::Response {
    if !crate::handlers::client::is_federation_envelope(&body) {
        return crate::handlers::client::remove_place(
            State(state),
            headers,
            Path(crate::handlers::client::PathIdPlace { id, place_id }),
        )
        .await;
    }
    into_resp(fed_remove_place(State(state), headers, Path(PathIdPlace { id, place_id }), body).await)
}

async fn fed_remove_place(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(PathIdPlace { id, place_id }): Path<PathIdPlace>,
    body: Bytes,
) -> (StatusCode, Json<serde_json::Value>) {
    let uuid = match uuid_from_path(&id) {
        Ok(u) => u,
        Err(e) => return e,
    };
    let path = format!("/v1/communities/{}/places/{}", id, place_id);
    let (signed, signer) =
        match preflight::<CommunityPlaceRemove>(&state, &headers, "delete", &path, &body).await {
            Ok(x) => x,
            Err(e) => return e,
        };
    if community_uuid_from_hex(&signed.message.community_id) != uuid {
        return err_json(StatusCode::BAD_REQUEST, "community_id mismatch");
    }
    if signed.message.place_id != place_id {
        return err_json(StatusCode::BAD_REQUEST, "place_id mismatch");
    }
    match require_min_role(&state.pool, &signed.message.community_id, &signer, Role::Admin).await {
        Ok(_) => {}
        Err(e) => return map_apply_err(e),
    }
    match apply::apply_place_remove(&state.pool, &signed, &signer).await {
        Ok(sig) => {
            emit_gossip(&state, &signed, &sig, &signer).await;
            ok_json(sig)
        }
        Err(e) => map_apply_err(e),
    }
}

pub async fn create_post(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
    body: Bytes,
) -> axum::response::Response {
    if !crate::handlers::client::is_federation_envelope(&body) {
        return crate::handlers::client::create_post(State(state), headers, Path(id), body).await;
    }
    into_resp(fed_create_post(State(state), headers, Path(id), body).await)
}

async fn fed_create_post(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
    body: Bytes,
) -> (StatusCode, Json<serde_json::Value>) {
    let uuid = match uuid_from_path(&id) {
        Ok(u) => u,
        Err(e) => return e,
    };
    let path = format!("/v1/communities/{}/posts", id);
    let (signed, signer) =
        match preflight::<CommunityPost>(&state, &headers, "post", &path, &body).await {
            Ok(x) => x,
            Err(e) => return e,
        };
    if community_uuid_from_hex(&signed.message.community_id) != uuid {
        return err_json(StatusCode::BAD_REQUEST, "community_id mismatch");
    }
    match require_min_role(&state.pool, &signed.message.community_id, &signer, Role::Member).await {
        Ok(_) => {}
        Err(e) => return map_apply_err(e),
    }
    let content_hash_lc = signed.message.content_hash.to_ascii_lowercase();
    let body_present = crate::content_store::is_valid_hash(&content_hash_lc)
        && state.content_store.exists(&content_hash_lc);
    if !body_present {
        tracing::debug!(
            community_id = %signed.message.community_id,
            content_hash = %signed.message.content_hash,
            signer = %signer,
            "CommunityPost accepted before content body is locally present; federation pull will fetch it"
        );
    }
    match apply::apply_post(&state.pool, &signed, &signer).await {
        Ok(sig) => {
            emit_gossip(&state, &signed, &sig, &signer).await;
            ok_json_with(sig, json!({ "content_body_local": body_present }))
        }
        Err(e) => map_apply_err(e),
    }
}

#[derive(Debug, Deserialize)]
pub struct PathIdPost {
    pub id: String,
    #[serde(rename = "postId")]
    pub post_id: String,
}

pub async fn delete_post(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(PathIdPost { id, post_id }): Path<PathIdPost>,
    body: Bytes,
) -> axum::response::Response {
    if !crate::handlers::client::is_federation_envelope(&body) {
        return crate::handlers::client::delete_post(
            State(state),
            headers,
            Path(crate::handlers::client::PathIdPost { id, post_id }),
        )
        .await;
    }
    into_resp(fed_delete_post(State(state), headers, Path(PathIdPost { id, post_id }), body).await)
}

async fn fed_delete_post(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(PathIdPost { id, post_id }): Path<PathIdPost>,
    body: Bytes,
) -> (StatusCode, Json<serde_json::Value>) {
    let _ = match uuid_from_path(&id) {
        Ok(u) => u,
        Err(e) => return e,
    };
    let path = format!("/v1/communities/{}/posts/{}", id, post_id);
    let (signed, signer) =
        match preflight::<CommunityPostDelete>(&state, &headers, "delete", &path, &body).await {
            Ok(x) => x,
            Err(e) => return e,
        };
    let post_meta = post_meta(&state.pool, &signed.message.post_id).await;
    let community_id_for_post = post_meta.as_ref().map(|(c, _)| c.clone()).unwrap_or_default();
    let author = post_meta.map(|(_, a)| a);
    if signed.message.community_id != community_id_for_post || signed.message.post_id != post_id {
        // || (not &&): reject if EITHER the path post_id or the signed community_id
        // disagrees with the stored post. With && a mod of community A could delete
        // a post in community B by supplying B's real post_id + A's community_id.
        // Empty community_id_for_post (unknown post) also correctly fails here.
        return err_json(StatusCode::BAD_REQUEST, "post_id / community_id mismatch");
    }
    let is_author = author
        .as_deref()
        .map(|a| a.eq_ignore_ascii_case(&signer))
        .unwrap_or(false);
    if !is_author {
        match require_min_role(&state.pool, &signed.message.community_id, &signer, Role::Mod).await {
            Ok(_) => {}
            Err(e) => return map_apply_err(e),
        }
    }
    match apply::apply_post_delete(&state.pool, &signed).await {
        Ok(sig) => {
            emit_gossip(&state, &signed, &sig, &signer).await;
            ok_json(sig)
        }
        Err(e) => map_apply_err(e),
    }
}

async fn post_meta(pool: &sqlx::PgPool, post_sig_hash: &str) -> Option<(String, String)> {
    sqlx::query_as::<_, (String, String)>(
        "SELECT community_id, author FROM community_posts_log WHERE signature_hash = $1",
    )
    .bind(post_sig_hash)
    .fetch_optional(pool)
    .await
    .ok()
    .flatten()
}

pub async fn like_post(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(PathIdPost { id, post_id }): Path<PathIdPost>,
    body: Bytes,
) -> axum::response::Response {
    if !crate::handlers::client::is_federation_envelope(&body) {
        return crate::handlers::client::like_post(
            State(state),
            headers,
            Path(crate::handlers::client::PathIdPost { id, post_id }),
        )
        .await;
    }
    into_resp(fed_like_post(State(state), headers, Path(PathIdPost { id, post_id }), body).await)
}

async fn fed_like_post(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(PathIdPost { id, post_id }): Path<PathIdPost>,
    body: Bytes,
) -> (StatusCode, Json<serde_json::Value>) {
    let _ = match uuid_from_path(&id) {
        Ok(u) => u,
        Err(e) => return e,
    };
    let path = format!("/v1/communities/{}/posts/{}/like", id, post_id);
    let (signed, signer) =
        match preflight::<CommunityPostLike>(&state, &headers, "post", &path, &body).await {
            Ok(x) => x,
            Err(e) => return e,
        };
    if signed.message.post_id != post_id {
        return err_json(StatusCode::BAD_REQUEST, "post_id mismatch");
    }
    match require_min_role(&state.pool, &signed.message.community_id, &signer, Role::Member).await {
        Ok(_) => {}
        Err(e) => return map_apply_err(e),
    }
    match apply::apply_post_like(&state.pool, &signed, &signer).await {
        Ok(sig) => {
            emit_gossip(&state, &signed, &sig, &signer).await;
            ok_json(sig)
        }
        Err(e) => map_apply_err(e),
    }
}

pub async fn unlike_post(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(PathIdPost { id, post_id }): Path<PathIdPost>,
    body: Bytes,
) -> axum::response::Response {
    if !crate::handlers::client::is_federation_envelope(&body) {
        return crate::handlers::client::unlike_post(
            State(state),
            headers,
            Path(crate::handlers::client::PathIdPost { id, post_id }),
        )
        .await;
    }
    into_resp(fed_unlike_post(State(state), headers, Path(PathIdPost { id, post_id }), body).await)
}

async fn fed_unlike_post(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(PathIdPost { id, post_id }): Path<PathIdPost>,
    body: Bytes,
) -> (StatusCode, Json<serde_json::Value>) {
    let _ = match uuid_from_path(&id) {
        Ok(u) => u,
        Err(e) => return e,
    };
    let path = format!("/v1/communities/{}/posts/{}/like", id, post_id);
    let (signed, signer) =
        match preflight::<CommunityPostUnlike>(&state, &headers, "delete", &path, &body).await {
            Ok(x) => x,
            Err(e) => return e,
        };
    if signed.message.post_id != post_id {
        return err_json(StatusCode::BAD_REQUEST, "post_id mismatch");
    }
    match require_min_role(&state.pool, &signed.message.community_id, &signer, Role::Member).await {
        Ok(_) => {}
        Err(e) => return map_apply_err(e),
    }
    match apply::apply_post_unlike(&state.pool, &signed, &signer).await {
        Ok(sig) => {
            emit_gossip(&state, &signed, &sig, &signer).await;
            ok_json(sig)
        }
        Err(e) => map_apply_err(e),
    }
}

#[derive(Debug, Deserialize)]
pub struct CreateRequestBody {
    #[serde(rename = "targetedAddress", default)]
    pub targeted_address: Option<String>,
    #[serde(rename = "type")]
    pub kind: String,
}

pub async fn create_request(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
    body: Bytes,
) -> (StatusCode, Json<serde_json::Value>) {
    let community_uuid = match uuid_from_path(&id) {
        Ok(u) => u,
        Err(e) => return e,
    };
    let path = format!("/v1/communities/{}/requests", id);
    let signer = match require_signer(&headers, "post", &path) {
        Ok(s) => s,
        Err(e) => return err_json(StatusCode::UNAUTHORIZED, format!("auth chain: {}", e)),
    };
    let req: CreateRequestBody = match serde_json::from_slice(&body) {
        Ok(r) => r,
        Err(e) => return err_json(StatusCode::BAD_REQUEST, format!("invalid body: {}", e)),
    };

    let kind = match req.kind.as_str() {
        "invite" => "invite",
        "request_to_join" => "request_to_join",
        other => {
            return err_json(
                StatusCode::BAD_REQUEST,
                format!("invalid request type: {}", other),
            )
        }
    };

    let exists: Option<bool> = sqlx::query_scalar(
        "SELECT active FROM communities WHERE id = $1",
    )
    .bind(community_uuid)
    .fetch_optional(&state.pool)
    .await
    .ok()
    .flatten();
    match exists {
        Some(true) => {}
        Some(false) => return err_json(StatusCode::BAD_REQUEST, "Community is not active"),
        None => return err_json(StatusCode::NOT_FOUND, format!("Community not found: {}", id)),
    }

    let member_address = req
        .targeted_address
        .as_deref()
        .map(|s| s.to_lowercase())
        .unwrap_or_else(|| signer.to_lowercase());
    if kind == "request_to_join" && member_address != signer.to_lowercase() {
        return err_json(
            StatusCode::UNAUTHORIZED,
            "You can only create a join request for yourself",
        );
    }

    let request_id = Uuid::new_v4();
    let inserted = sqlx::query_as::<_, (Uuid, Uuid, String, String, String)>(
        "INSERT INTO community_requests (id, community_id, member_address, status, type) \
         VALUES ($1, $2, $3, 'pending', $4) \
         RETURNING id, community_id, member_address, status, type",
    )
    .bind(request_id)
    .bind(community_uuid)
    .bind(&member_address)
    .bind(kind)
    .fetch_one(&state.pool)
    .await;

    match inserted {
        Ok((id, community_id, member_address, status, kind)) => (
            StatusCode::OK,
            Json(json!({
                "data": {
                    "id": id,
                    "communityId": community_id,
                    "memberAddress": member_address,
                    "type": kind,
                    "status": status,
                }
            })),
        ),
        Err(e) => {
            tracing::error!(error = %e, "failed to create community request");
            err_json(StatusCode::INTERNAL_SERVER_ERROR, "failed to create request")
        }
    }
}

#[derive(Debug, Deserialize)]
pub struct PathIdReq {
    pub id: String,
    #[serde(rename = "requestId")]
    pub request_id: String,
}

pub async fn update_request_status(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(PathIdReq { id, request_id }): Path<PathIdReq>,
    body: Bytes,
) -> axum::response::Response {
    if !crate::handlers::client::is_federation_envelope(&body) {
        return crate::handlers::client::update_request_status(
            State(state),
            headers,
            Path(crate::handlers::client::PathIdReq { id, request_id }),
            body,
        )
        .await;
    }
    into_resp(fed_update_request_status(State(state), headers, Path(PathIdReq { id, request_id }), body).await)
}

async fn fed_update_request_status(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(PathIdReq { id, request_id }): Path<PathIdReq>,
    body: Bytes,
) -> (StatusCode, Json<serde_json::Value>) {
    let uuid = match uuid_from_path(&id) {
        Ok(u) => u,
        Err(e) => return e,
    };
    let path = format!("/v1/communities/{}/requests/{}", id, request_id);
    let (signed, signer) =
        match preflight::<CommunityRequestStatusUpdate>(&state, &headers, "patch", &path, &body)
            .await
        {
            Ok(x) => x,
            Err(e) => return e,
        };
    if community_uuid_from_hex(&signed.message.community_id) != uuid {
        return err_json(StatusCode::BAD_REQUEST, "community_id mismatch");
    }
    if signed.message.request_id != request_id {
        return err_json(StatusCode::BAD_REQUEST, "request_id mismatch");
    }
    match require_min_role(&state.pool, &signed.message.community_id, &signer, Role::Mod).await {
        Ok(_) => {}
        Err(e) => return map_apply_err(e),
    }
    match apply::apply_request_status(&state.pool, &signed, &signer).await {
        Ok(sig) => {
            emit_gossip(&state, &signed, &sig, &signer).await;
            ok_json(sig)
        }
        Err(e) => map_apply_err(e),
    }
}

#[derive(Deserialize)]
pub struct MemberCommunitiesByIdsBody {
    #[serde(rename = "communityIds", default)]
    community_ids: Vec<String>,
}

/// `POST /v1/members/{address}/communities` — admin batch read mirroring
/// upstream `social-service-ea` `getMemberCommunitiesByIdsHandler`
/// (`src/controllers/handlers/http/get-member-communities-by-ids-handler.ts`).
/// Bearer-token authenticated, called by sibling services (e.g.
/// worlds-content-server) to validate a set of community IDs: returns only the
/// communities from the input set that are visible to `address` (active, not
/// banned, listed-or-member). Response shape matches upstream:
/// `{ data: { communities: [{ id }] } }`.
pub async fn member_communities_by_ids(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(address): Path<String>,
    body: Option<Json<MemberCommunitiesByIdsBody>>,
) -> (StatusCode, Json<serde_json::Value>) {
    let bearer = headers
        .get("authorization")
        .and_then(|v| v.to_str().ok())
        .and_then(|s| s.strip_prefix("Bearer "));
    match (&state.admin_token, bearer) {
        (Some(expected), Some(got)) if expected == got => (),
        _ => return err_json(StatusCode::UNAUTHORIZED, "admin bearer token required"),
    }

    let community_ids = body.map(|Json(b)| b.community_ids).unwrap_or_default();
    // Parse the requested ids as UUIDs (the canonical `communities.id` form).
    // Unparseable ids cannot match any community and are dropped, matching
    // upstream's "filter the input set" behaviour.
    let uuids: Vec<Uuid> = community_ids
        .iter()
        .filter_map(|s| Uuid::parse_str(s).ok())
        .collect();

    let visible = match state
        .communities
        .visible_communities_by_ids(&uuids, &address)
        .await
    {
        Ok(v) => v,
        Err(e) => return map_apply_err(e),
    };

    let communities: Vec<serde_json::Value> = visible
        .into_iter()
        .map(|id| json!({ "id": id.to_string() }))
        .collect();

    (
        StatusCode::OK,
        Json(json!({ "data": { "communities": communities } })),
    )
}
