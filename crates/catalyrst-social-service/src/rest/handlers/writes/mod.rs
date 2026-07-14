use axum::http::{HeaderMap, StatusCode};
use axum::Json;
use catalyrst_fed::{FedError, RateLimitDecision, Signed, TypedMessage};
use serde::de::DeserializeOwned;
use serde_json::json;
use uuid::Uuid;

use catalyrst_authenticated_principal::{AuthorityNotEstablished, VerifiedWalletAddress};

use crate::rest::auth_chain::require_signer;
use crate::rest::community_membership_authority::{
    load_standing_from_community_role_current, status_and_message_for_refusal_using_its_own_detail,
    CommunityMembershipTier,
};
use crate::rest::fed::authority::community_is_private;
use crate::rest::handlers::permissions::{can_like_post, Permission};
use crate::rest::http::ApiError;
use crate::rest::AppState;

mod communities;
mod members;
mod places;
mod posts;
mod requests;

pub use communities::{
    __path_create_community, __path_delete_community, __path_update_community,
    __path_update_community_partially, create_community, delete_community, update_community,
    update_community_partially,
};
pub use members::{
    __path_add_member, __path_ban_member, __path_member_communities_by_ids, __path_remove_member,
    __path_unban_member, __path_update_member_role, add_member, ban_member,
    member_communities_by_ids, remove_member, unban_member, update_member_role,
    MemberCommunitiesByIdsBody, PathIdAddr,
};
pub use places::{__path_add_places, __path_remove_place, add_places, remove_place, PathIdPlace};
pub use posts::{
    __path_create_post, __path_delete_post, __path_like_post, __path_unlike_post, create_post,
    delete_post, like_post, unlike_post, PathIdPost,
};
pub use requests::{
    __path_create_request, __path_update_request_status, create_request, update_request_status,
    CreateRequestBody, PathIdReq,
};

fn into_resp(t: (StatusCode, Json<serde_json::Value>)) -> axum::response::Response {
    use axum::response::IntoResponse;
    t.into_response()
}

fn err_json(code: StatusCode, message: impl Into<String>) -> (StatusCode, Json<serde_json::Value>) {
    (
        code,
        Json(json!(catalyrst_types::ApiErrorBody::new(message))),
    )
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
) -> Result<(Signed<T>, catalyrst_crypto::Signer), (StatusCode, Json<serde_json::Value>)> {
    let outer_signer = require_signer(headers, method, path)
        .await
        .map_err(crate::rest::handlers::error::signed_fetch_gate_json)?;

    let signed: Signed<T> = parse_signed(body)?;

    let now = chrono::Utc::now().timestamp();
    if let Err(e) = signed.verify(outer_signer.as_str(), now) {
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
        .check_and_record(outer_signer.as_str(), &signed.nonce, signed.signed_at)
        .await
    {
        return Err(match e {
            FedError::DuplicateNonce { .. } => err_json(StatusCode::CONFLICT, e.to_string()),
            _ => err_json(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()),
        });
    }

    if matches!(
        state.limiter.check(outer_signer.as_str()),
        RateLimitDecision::Deny
    ) {
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
    (
        status,
        Json(json!(catalyrst_types::ApiErrorBody::new(message))),
    )
}

fn uuid_from_path(s: &str) -> Result<Uuid, (StatusCode, Json<serde_json::Value>)> {
    Uuid::parse_str(s).map_err(|_| err_json(StatusCode::BAD_REQUEST, "invalid uuid"))
}

/// Mint the shared verified-wallet type from a signer this request's signed fetch already
/// produced. See the identical note in `crate::rest::handlers::client`.
fn verified_wallet_of_the_caller(signer: &catalyrst_crypto::Signer) -> VerifiedWalletAddress {
    VerifiedWalletAddress::from_verified_signed_fetch(signer.clone())
}

/// Render a refusal for this module's `(StatusCode, Json)` handlers.
///
/// Every federation write gate uses the authority's own detail, because merging three
/// separately-worded refusals into one predicate means no single call-site sentence is
/// still accurate. The status each gate answers is unchanged. The client write paths,
/// whose messages a UI actually shows, keep theirs byte for byte -- see
/// `crate::rest::handlers::client::refusal_response`.
fn map_refusal_using_its_own_detail(
    refusal: &AuthorityNotEstablished,
    status_when_the_principal_lacks_the_authority: StatusCode,
) -> (StatusCode, Json<serde_json::Value>) {
    let (status, message) = status_and_message_for_refusal_using_its_own_detail(
        refusal,
        status_when_the_principal_lacks_the_authority,
    );
    err_json(status, message)
}

/// The federation write path's capability gate, proven against `community_role_current`.
///
/// Kept as a function rather than folded into
/// [`FederatedCommunityWriteAuthority`] because its
/// two refusals answer two different statuses -- 403 for a banned wallet, 401 for a wallet
/// whose tier lacks the capability -- and collapsing them into one
/// `AuthorityNotEstablished::RefusedLacksAuthority` would lose that
/// distinction on the wire. Behaviour-preserving; only the role read moved.
async fn require_permission(
    state: &AppState,
    community_id: &str,
    signer: &str,
    permission: Permission,
    action: &str,
) -> Result<CommunityMembershipTier, (StatusCode, Json<serde_json::Value>)> {
    let standing =
        match load_standing_from_community_role_current(&state.pool, community_id, signer).await {
            Ok(s) => s,
            Err(refusal) => {
                return Err(map_refusal_using_its_own_detail(
                    &refusal,
                    StatusCode::FORBIDDEN,
                ))
            }
        };
    if standing.tier() == CommunityMembershipTier::BannedFromThisCommunity {
        return Err(err_json(
            StatusCode::FORBIDDEN,
            "Forbidden: banned from this community",
        ));
    }
    if !standing.holds_capability_within_this_community(permission) {
        return Err(err_json(
            StatusCode::UNAUTHORIZED,
            format!("The user {} doesn't have permission to {}", signer, action),
        ));
    }
    Ok(standing.tier())
}

async fn require_like_permission(
    state: &AppState,
    community_id: &str,
    signer: &str,
) -> Result<(), (StatusCode, Json<serde_json::Value>)> {
    let standing =
        match load_standing_from_community_role_current(&state.pool, community_id, signer).await {
            Ok(s) => s,
            Err(refusal) => {
                return Err(map_refusal_using_its_own_detail(
                    &refusal,
                    StatusCode::FORBIDDEN,
                ))
            }
        };
    let private = match community_is_private(&state.pool, community_id).await {
        Ok(Some(p)) => p,
        Ok(None) => return Err(err_json(StatusCode::NOT_FOUND, "community not found")),
        Err(e) => return Err(map_apply_err(e)),
    };
    if !can_like_post(standing.tier(), private) {
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
