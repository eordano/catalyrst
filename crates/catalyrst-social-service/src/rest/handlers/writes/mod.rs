use axum::http::{HeaderMap, StatusCode};
use axum::Json;
use catalyrst_fed::{FedError, RateLimitDecision, Signed, TypedMessage};
use serde::de::DeserializeOwned;
use serde_json::json;
use uuid::Uuid;

use crate::rest::auth_chain::require_signer;
use crate::rest::fed::authority::{community_is_private, load_role, Role};
use crate::rest::handlers::permissions::{can_like_post, has_permission, Permission};
use crate::rest::http::ApiError;
use crate::rest::AppState;

mod communities;
mod members;
mod places;
mod posts;
mod requests;

pub use communities::{
    create_community, delete_community, update_community, update_community_partially,
};
pub use members::{
    add_member, ban_member, member_communities_by_ids, remove_member, unban_member,
    update_member_role, MemberCommunitiesByIdsBody, PathIdAddr,
};
pub use places::{add_places, remove_place, PathIdPlace};
pub use posts::{create_post, delete_post, like_post, unlike_post, PathIdPost};
pub use requests::{create_request, update_request_status, CreateRequestBody, PathIdReq};

fn into_resp(t: (StatusCode, Json<serde_json::Value>)) -> axum::response::Response {
    use axum::response::IntoResponse;
    t.into_response()
}

fn err_json(code: StatusCode, message: impl Into<String>) -> (StatusCode, Json<serde_json::Value>) {
    let m = message.into();
    (code, Json(json!({ "ok": false, "message": m })))
}

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

fn ok_json_with(
    sig_hash: String,
    extra: serde_json::Value,
) -> (StatusCode, Json<serde_json::Value>) {
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
    let outer_signer = require_signer(headers, method, path)
        .map_err(|e| err_json(StatusCode::UNAUTHORIZED, format!("auth chain: {}", e)))?;

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
        return Err(err_json(
            StatusCode::TOO_MANY_REQUESTS,
            "rate limit exceeded",
        ));
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

async fn require_permission(
    state: &AppState,
    community_id: &str,
    signer: &str,
    permission: Permission,
    action: &str,
) -> Result<Role, (StatusCode, Json<serde_json::Value>)> {
    let role = match load_role(&state.pool, community_id, signer).await {
        Ok(r) => r,
        Err(e) => return Err(map_apply_err(e)),
    };
    if role == Role::Banned {
        return Err(err_json(
            StatusCode::FORBIDDEN,
            "Forbidden: banned from this community",
        ));
    }
    if !has_permission(role, permission) {
        return Err(err_json(
            StatusCode::UNAUTHORIZED,
            format!("The user {} doesn't have permission to {}", signer, action),
        ));
    }
    Ok(role)
}

async fn require_like_permission(
    state: &AppState,
    community_id: &str,
    signer: &str,
) -> Result<(), (StatusCode, Json<serde_json::Value>)> {
    let role = match load_role(&state.pool, community_id, signer).await {
        Ok(r) => r,
        Err(e) => return Err(map_apply_err(e)),
    };
    let private = match community_is_private(&state.pool, community_id).await {
        Ok(Some(p)) => p,
        Ok(None) => return Err(err_json(StatusCode::NOT_FOUND, "community not found")),
        Err(e) => return Err(map_apply_err(e)),
    };
    if !can_like_post(role, private) {
        return Err(err_json(
            StatusCode::UNAUTHORIZED,
            format!(
                "{} cannot like/unlike posts in community {}",
                signer, community_id
            ),
        ));
    }
    Ok(())
}

async fn require_owned_name(
    state: &AppState,
    signer: &str,
) -> Result<(), (StatusCode, Json<serde_json::Value>)> {
    if let Some(false) = state.profiles.has_owned_name(signer).await {
        return Err(err_json(
            StatusCode::UNAUTHORIZED,
            format!("The user {} doesn't have any names", signer),
        ));
    }
    Ok(())
}

async fn require_places_ownership(
    state: &AppState,
    place_ids: &[String],
    signer: &str,
) -> Result<(), (StatusCode, Json<serde_json::Value>)> {
    use crate::rest::ports::places_api::PlacesError;
    if place_ids.is_empty() || !state.places_api.is_configured() {
        return Ok(());
    }
    match state.places_api.validate_ownership(place_ids, signer).await {
        Ok(_) => Ok(()),
        Err(PlacesError::NotOwner(msg)) => Err(err_json(StatusCode::UNAUTHORIZED, msg)),
        Err(PlacesError::Unconfigured) => Ok(()),
        Err(PlacesError::Upstream(msg)) => {
            tracing::error!(error = %msg, "places ownership validation failed");
            Err(err_json(
                StatusCode::BAD_GATEWAY,
                "failed to validate place ownership",
            ))
        }
    }
}
