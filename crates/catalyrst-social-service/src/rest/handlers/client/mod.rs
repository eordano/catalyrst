use axum::body::Bytes;
use axum::http::{header, HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::Json;
use serde_json::json;
use uuid::Uuid;

use catalyrst_authenticated_principal::{AuthorityNotEstablished, VerifiedWalletAddress};

use crate::rest::auth_chain::require_signer;
use crate::rest::community_membership_authority::{
    load_standing_from_community_members, status_and_message_for_refusal,
    CommunityMembershipStanding, CommunityMembershipTier,
};
use crate::rest::handlers::permissions::Permission;
use crate::rest::ports::places_api::PlacesError;
use crate::rest::AppState;

mod communities;
mod members;
mod places;
mod posts;
mod requests;

pub use communities::{
    create_community, delete_community, update_community, update_community_partially, PatchBody,
};
pub use members::{
    add_member, ban_member, remove_member, unban_member, update_member_role, PathIdAddr, RoleBody,
};
pub use places::{add_places, remove_place, PathIdPlace, PlacesBody};
pub use posts::{create_post, delete_post, like_post, unlike_post, PathIdPost, PostBody};
pub use requests::{update_request_status, PathIdReq, RequestStatusBody};

pub fn is_federation_envelope(body: &[u8]) -> bool {
    let Ok(v) = serde_json::from_slice::<serde_json::Value>(body) else {
        return false;
    };
    let Some(obj) = v.as_object() else {
        return false;
    };
    obj.contains_key("domain") && obj.contains_key("message") && obj.contains_key("signature")
}

struct MultipartFields {
    name: Option<String>,
    description: Option<String>,
    privacy: Option<String>,
    visibility: Option<String>,
    place_ids: Vec<String>,
    thumbnail: Option<Vec<u8>>,
}

fn boundary(headers: &HeaderMap) -> Option<String> {
    let ct = headers.get(header::CONTENT_TYPE)?.to_str().ok()?;
    multer::parse_boundary(ct).ok()
}

async fn parse_multipart(boundary: String, body: Bytes) -> Result<MultipartFields, Response> {
    let stream = futures_util::stream::once(async move { Ok::<Bytes, std::io::Error>(body) });
    let mut mp = multer::Multipart::new(stream, boundary);
    let mut out = MultipartFields {
        name: None,
        description: None,
        privacy: None,
        visibility: None,
        place_ids: Vec::new(),
        thumbnail: None,
    };
    loop {
        let field = match mp.next_field().await {
            Ok(Some(f)) => f,
            Ok(None) => break,
            Err(e) => {
                return Err(err(
                    StatusCode::BAD_REQUEST,
                    format!("invalid multipart: {}", e),
                ))
            }
        };
        let fname = field.name().unwrap_or("").to_string();
        match fname.as_str() {
            "thumbnail" => {
                let data = field.bytes().await.unwrap_or_default();
                out.thumbnail = if data.is_empty() {
                    None
                } else {
                    Some(data.to_vec())
                };
            }
            "name" => out.name = Some(field.text().await.unwrap_or_default()),
            "description" => out.description = Some(field.text().await.unwrap_or_default()),
            // Raw, not case-folded: parse_privacy/parse_visibility must see `Private` or `ALL`
            // as-sent so miscased values are refused (upstream #487) -- folding here resolved
            // them to the accepted literals before the validator ever ran.
            "privacy" => out.privacy = Some(field.text().await.unwrap_or_default()),
            "visibility" => out.visibility = Some(field.text().await.unwrap_or_default()),
            "placeIds" => {
                let raw = field.text().await.unwrap_or_default();
                if let Ok(parsed) = serde_json::from_str::<Vec<String>>(&raw) {
                    out.place_ids = parsed;
                } else if !raw.trim().is_empty() {
                    out.place_ids = raw
                        .split(',')
                        .map(|s| s.trim().to_string())
                        .filter(|s| !s.is_empty())
                        .collect();
                }
            }
            _ => {
                let _ = field.bytes().await;
            }
        }
    }
    Ok(out)
}

fn err(code: StatusCode, message: impl Into<String>) -> Response {
    (
        code,
        Json(json!(catalyrst_types::ApiErrorBody::new(message))),
    )
        .into_response()
}

async fn auth(
    headers: &HeaderMap,
    method: &str,
    path: &str,
) -> Result<catalyrst_crypto::Signer, Response> {
    require_signer(headers, method, path)
        .await
        .map_err(|e| crate::rest::handlers::error::signed_fetch_gate(e).into_response())
}

/// Mint the shared verified-wallet type from a signer this request's signed fetch already
/// produced.
///
/// The mint takes [`catalyrst_crypto::Signer`] by value -- that is the chokepoint -- so this
/// clones, because the call sites still need the `Signer` for their message text. Cloning
/// an already-verified value forges nothing.
fn verified_wallet_of_the_caller(signer: &catalyrst_crypto::Signer) -> VerifiedWalletAddress {
    VerifiedWalletAddress::from_verified_signed_fetch(signer.clone())
}

fn parse_uuid(s: &str) -> Result<Uuid, Response> {
    Uuid::parse_str(s).map_err(|_| err(StatusCode::BAD_REQUEST, "invalid community id"))
}

fn map_db<T>(r: Result<T, sqlx::Error>) -> Result<T, Response> {
    r.map_err(|e| {
        tracing::error!(error = %e, "communities client-write database error");
        err(StatusCode::INTERNAL_SERVER_ERROR, "database error")
    })
}

fn map_api(e: crate::rest::http::ApiError) -> Response {
    match e {
        crate::rest::http::ApiError::Http(h) => {
            let code = StatusCode::from_u16(h.code).unwrap_or(StatusCode::INTERNAL_SERVER_ERROR);
            err(code, h.message)
        }
        other => err(StatusCode::INTERNAL_SERVER_ERROR, other.to_string()),
    }
}

/// Rejects a thumbnail whose bytes are not a bounded, signature-valid PNG/JPEG/GIF/WebP.
///
/// Callers MUST run this on any uploaded thumbnail before the community-authorization / DB
/// write, so an arbitrary blob never reaches [`store_thumbnail`] and the content store. Port of
/// upstream #444.
fn validate_thumbnail_field(bytes: &[u8]) -> Result<(), Response> {
    crate::rest::thumbnail_signature::validate_thumbnail(bytes)
        .map(|_| ())
        .map_err(|e| err(StatusCode::BAD_REQUEST, e.message()))
}

async fn store_thumbnail<'a, E>(
    executor: E,
    store: &crate::rest::content_store::ContentStore,
    community_id: Uuid,
    bytes: &[u8],
) -> Result<(), Response>
where
    E: sqlx::PgExecutor<'a>,
{
    let hash = store.put(bytes).await.map_err(|e| match e {
        crate::rest::content_store::ContentError::TooLarge { max } => err(
            StatusCode::PAYLOAD_TOO_LARGE,
            format!("thumbnail exceeds {} bytes", max),
        ),
        other => {
            tracing::error!(error = %other, "failed to store community thumbnail");
            err(
                StatusCode::INTERNAL_SERVER_ERROR,
                "failed to store thumbnail",
            )
        }
    })?;
    map_db(
        sqlx::query(
            "INSERT INTO community_ranking_metrics (community_id, has_thumbnail, thumbnail_hash, updated_at) \
             VALUES ($1, TRUE, $2, now()) \
             ON CONFLICT (community_id) DO UPDATE SET has_thumbnail = TRUE, thumbnail_hash = EXCLUDED.thumbnail_hash, updated_at = now()",
        )
        .bind(community_id)
        .bind(&hash)
        .execute(executor)
        .await,
    )?;
    Ok(())
}

/// The spelling this path writes into `community_members.role`.
///
/// Note it is **not**
/// [`CommunityMembershipTier::as_canonical_stored_role_text`]:
/// this table spells a moderator `"moderator"` while `community_role_current` spells the
/// same tier `"mod"`. Two tables, two spellings, both parsed by the one parse.
fn role_text_as_written_into_the_community_members_table(
    tier: CommunityMembershipTier,
) -> &'static str {
    match tier {
        CommunityMembershipTier::OwnerOfThisCommunity => "owner",
        CommunityMembershipTier::ModeratorOfThisCommunity => "moderator",
        CommunityMembershipTier::OrdinaryMemberOfThisCommunity => "member",
        CommunityMembershipTier::BannedFromThisCommunity => "banned",
        CommunityMembershipTier::NotAMemberOfThisCommunity => "none",
    }
}

/// Render an [`AuthorityNotEstablished`] as this module's `Response`, keeping the status
/// and the wording this call site has always answered with when the principal simply
/// lacks the authority.
fn refusal_response(
    refusal: &AuthorityNotEstablished,
    status_when_the_principal_lacks_the_authority: StatusCode,
    message_when_the_principal_lacks_the_authority: impl FnOnce() -> String,
) -> Response {
    let (status, message) = status_and_message_for_refusal(
        refusal,
        status_when_the_principal_lacks_the_authority,
        message_when_the_principal_lacks_the_authority,
    );
    err(status, message)
}

/// Read a wallet's standing from `community_members` for the UUID-keyed client paths.
///
/// **Deliberate behaviour change (BC-1).** The `load_role_uuid` this replaces ended its
/// query with `.ok().flatten()`, so a SQL fault read as "not a member". Now a fault is a
/// 500, exactly as [`is_banned_uuid`] already does for the sibling `community_bans`
/// lookup. The fail-open this closes is on the *target* of a moderation action: with the
/// actor's own lookup succeeding and the target's failing, the target was demoted to "not
/// a member", and `can_act_on_member`'s `!is_member(target)` escape then permitted the
/// action against a community owner.
async fn load_client_standing(
    state: &AppState,
    community_id: Uuid,
    wallet_address: &str,
) -> Result<CommunityMembershipStanding, Response> {
    load_standing_from_community_members(&state.pool, community_id, wallet_address)
        .await
        .map_err(|refusal| {
            refusal_response(&refusal, StatusCode::FORBIDDEN, || {
                "Forbidden: authority could not be established".to_string()
            })
        })
}

/// Ban-status lookup for the UUID-keyed client write paths.
///
/// SQL errors propagate (as a 500 via [`map_db`]) instead of reading as "not
/// banned". This matches the federation write gate
/// [`crate::rest::fed::authority::FederatedCommunityWriteAuthority`],
/// which fails closed on the same logical check: there the query error is surfaced rather
/// than swallowed, so a DB fault denies the write instead of admitting a banned signer. A
/// missing row still means "not banned", exactly as an absent `community_role_current`
/// row means "not a member" there.
async fn is_banned_uuid(
    state: &AppState,
    community_id: Uuid,
    signer: &str,
) -> Result<bool, Response> {
    let banned: Option<bool> = map_db(
        sqlx::query_scalar(
            "SELECT active FROM community_bans WHERE community_id = $1 AND banned_address = $2",
        )
        .bind(community_id)
        .bind(signer.to_lowercase())
        .fetch_optional(&state.pool)
        .await,
    )?;
    Ok(banned.unwrap_or(false))
}

/// The right of a signed-fetch end user to write to one community, proven against the
/// **`community_members`** table plus the `community_bans` table.
///
/// The federation path's equivalent is
/// [`crate::rest::fed::authority::FederatedCommunityWriteAuthority`],
/// which reads a *different* table. The two can disagree; naming them separately is how
/// that stays visible. Neither is convertible into the other.
///
/// # Why the standing it carries is currently unread
///
/// Every call site discards it, exactly as they discarded the `Role` the
/// `require_min_role_uuid` / `require_permission_uuid` pair returned before this type
/// existed -- none of them ever used it. It is kept on the witness rather than dropped
/// because the next step for this crate is moving the identifiers onto the witness so a
/// write function loses its `&str` parameters, and that step wants the standing here. The
/// `allow` is the honest marker for "reserved and not yet read", not for "unused".
#[derive(Debug)]
#[allow(dead_code)]
pub(crate) struct ClientCommunityWriteAuthority {
    standing: CommunityMembershipStanding,
}

impl ClientCommunityWriteAuthority {
    /// Behaviour-preserving replacement for `require_min_role_uuid`: same
    /// `community_bans` pre-check, same 403s, same wording.
    pub(crate) async fn resolve_requiring_at_least(
        state: &AppState,
        community_id: Uuid,
        signer: &str,
        minimum: CommunityMembershipTier,
    ) -> Result<Self, Response> {
        if is_banned_uuid(state, community_id, signer).await? {
            return Err(err(
                StatusCode::FORBIDDEN,
                "Forbidden: banned from this community",
            ));
        }
        let standing = load_client_standing(state, community_id, signer).await?;
        if !standing.tier_is_at_least(minimum) {
            return Err(err(
                StatusCode::FORBIDDEN,
                format!(
                    "Forbidden: signer role {} below required {}",
                    standing.tier().as_canonical_stored_role_text(),
                    minimum.as_canonical_stored_role_text()
                ),
            ));
        }
        Ok(Self { standing })
    }

    /// Behaviour-preserving replacement for `require_permission_uuid`: same
    /// `community_bans` pre-check, same 403 then 401, same wording.
    pub(crate) async fn resolve_requiring_capability(
        state: &AppState,
        community_id: Uuid,
        signer: &str,
        capability: Permission,
        action: &str,
    ) -> Result<Self, Response> {
        if is_banned_uuid(state, community_id, signer).await? {
            return Err(err(
                StatusCode::FORBIDDEN,
                "Forbidden: banned from this community",
            ));
        }
        let standing = load_client_standing(state, community_id, signer).await?;
        if !standing.holds_capability_within_this_community(capability) {
            return Err(err(
                StatusCode::UNAUTHORIZED,
                format!("The user {} doesn't have permission to {}", signer, action),
            ));
        }
        Ok(Self { standing })
    }

    /// The tier this authority was proven at. See the note on the struct for why nothing
    /// reads it yet.
    #[allow(dead_code)]
    pub(crate) fn tier(&self) -> CommunityMembershipTier {
        self.standing.tier()
    }
}

/// Whether a wallet may like or unlike a post in one community.
///
/// **Deliberate behaviour change (BC-3).** All three of this function's lookups used to
/// swallow their errors -- `is_private` through `.unwrap_or(false)`, the role through
/// `load_role_uuid`, and the ban through `.ok().flatten().unwrap_or(false)` -- and each
/// swallow failed *open*: a dead database made a private community read as public, a
/// member read as a non-member, and a banned wallet read as unbanned. The ban swallow here
/// is the same query and the same mistake that `is_banned_uuid` fixed forty lines above;
/// it survived only because this function does not call that helper. All three now
/// propagate as a 500, matching the rest of this module.
async fn validate_like_unlike_access(
    state: &AppState,
    community_id: Uuid,
    signer: &str,
) -> Result<(), Response> {
    let private = state
        .communities
        .is_private(community_id)
        .await
        .map_err(|e| {
            tracing::error!(error = %e, %community_id, "community privacy lookup failed");
            err(StatusCode::INTERNAL_SERVER_ERROR, "database error")
        })?;
    let standing = load_client_standing(state, community_id, signer).await?;
    if private && standing.tier() == CommunityMembershipTier::NotAMemberOfThisCommunity {
        return Err(err(
            StatusCode::UNAUTHORIZED,
            format!(
                "{} is not a member of private community {}. You need to be a member to like/unlike posts in this community.",
                signer, community_id
            ),
        ));
    }
    if is_banned_uuid(state, community_id, signer).await? {
        return Err(err(
            StatusCode::UNAUTHORIZED,
            format!(
                "{} is banned from community {}. You cannot like/unlike posts in this community.",
                signer, community_id
            ),
        ));
    }
    Ok(())
}

async fn validate_places_ownership(
    state: &AppState,
    place_ids: &[String],
    signer: &str,
) -> Result<(), Response> {
    if place_ids.is_empty() || !state.places_api.is_configured() {
        return Ok(());
    }
    match state.places_api.validate_ownership(place_ids, signer).await {
        Ok(_) => Ok(()),
        Err(PlacesError::NotOwner(msg)) => Err(err(StatusCode::UNAUTHORIZED, msg)),
        Err(PlacesError::Unconfigured) => Ok(()),
        Err(PlacesError::Upstream(msg)) => {
            tracing::error!(error = %msg, "places ownership validation failed");
            Err(err(
                StatusCode::BAD_GATEWAY,
                "failed to validate place ownership",
            ))
        }
    }
}
