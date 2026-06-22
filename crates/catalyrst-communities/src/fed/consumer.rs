//! Federation gossip consumer apply-loop. Off by default (communities reconcile
//! via HTTP-snapshot pull); live under `FED_GOSSIP=nats`. Remote envelopes are
//! never trusted for being forwarded by a peer: each runs the identical
//! verify/replay/rate-limit + authority gate as the matching HTTP write handler.

use catalyrst_fed::{GossipEnvelope, RateLimitDecision, Scope, Signed, TypedMessage};

use crate::fed::apply;
use crate::fed::authority::{
    community_exists, community_is_private, load_role, require_min_role, Role,
};
use crate::fed::messages::{
    CommunityBan, CommunityCreate, CommunityDelete, CommunityJoin, CommunityLeave,
    CommunityPlaceRemove, CommunityPlacesAdd, CommunityPost, CommunityPostDelete,
    CommunityPostLike, CommunityPostUnlike, CommunityRequestStatusUpdate, CommunityRole,
    CommunityUnban, CommunityUpdate,
};
use crate::handlers::permissions::{
    can_act_on_member, can_delete_post, can_like_post, has_permission, is_member, Permission,
};
use crate::AppState;

/// Spawn the communities gossip apply-loop if the transport reaches peers.
pub async fn spawn(state: AppState) {
    let rx = match state.gossip.subscribe(Scope::Communities).await {
        Ok(Some(rx)) => rx,
        Ok(None) => {
            tracing::info!(
                "communities gossip consumer not started (transport reaches no peers; \
                 peers reconcile via HTTP-snapshot pull — communities.md §5)"
            );
            return;
        }
        Err(e) => {
            tracing::error!(error = %e, "communities gossip subscribe failed; consumer not started");
            return;
        }
    };
    tracing::info!("communities gossip consumer started (fed.communities.actions)");
    tokio::spawn(run(state, rx));
}

async fn run(state: AppState, mut rx: tokio::sync::mpsc::Receiver<GossipEnvelope>) {
    while let Some(env) = rx.recv().await {
        if let Err(e) = apply_envelope(&state, &env).await {
            tracing::warn!(
                error = %e,
                primary_type = %env.primary_type,
                signature_hash = %env.signature_hash,
                origin_peer = env.origin_peer.as_deref().unwrap_or("?"),
                "communities gossip envelope rejected"
            );
        }
    }
    tracing::warn!("communities gossip consumer channel closed; loop exiting");
}

fn decode<T: TypedMessage + serde::de::DeserializeOwned>(
    env: &GossipEnvelope,
) -> Result<Signed<T>, String> {
    serde_json::from_value::<Signed<T>>(env.signed_json.clone())
        .map_err(|e| format!("decode Signed<{}>: {e}", T::PRIMARY_TYPE))
}

/// Verify the envelope's inner signature, domain, replay, and rate-limit — the
/// same checks `writes::preflight` runs, minus the HTTP auth-chain header (gossip
/// carries only the signature). Returns the recovered wallet.
async fn preverify<T: TypedMessage + serde::de::DeserializeOwned>(
    state: &AppState,
    signed: &Signed<T>,
) -> Result<String, String> {
    let signer = signed
        .signer()
        .map_err(|e| format!("signer recover: {e}"))?;
    let now = chrono::Utc::now().timestamp();
    signed
        .verify(&signer, now)
        .map_err(|e| format!("verify: {e}"))?;
    if !signed.domain.name.eq_ignore_ascii_case(&state.domain.name) {
        return Err(format!("domain mismatch: expected {}", state.domain.name));
    }
    state
        .replay
        .check_and_record(&signer, &signed.nonce, signed.signed_at)
        .await
        .map_err(|e| format!("replay: {e}"))?;
    if matches!(state.limiter.check(&signer), RateLimitDecision::Deny) {
        return Err("rate limit exceeded".to_string());
    }
    Ok(signer)
}

/// `validatePermission(permission)` over the hex community id — resolve the
/// signer's role, deny `banned`, then enforce the static permission matrix.
async fn gate_permission(
    pool: &sqlx::PgPool,
    community_id: &str,
    signer: &str,
    permission: Permission,
) -> Result<(), String> {
    let role = load_role(pool, community_id, signer)
        .await
        .map_err(|e| e.to_string())?;
    if role == Role::Banned {
        return Err("signer is banned".to_string());
    }
    if !has_permission(role, permission) {
        return Err(format!(
            "signer {} lacks permission {:?}",
            role.as_str(),
            permission
        ));
    }
    Ok(())
}

/// `validatePermissionsToLikeAndUnlikePost` (posts.ts): any non-banned user may
/// like/unlike in a PUBLIC community (role `none` included); in a PRIVATE
/// community the signer must be a member.
async fn gate_like(pool: &sqlx::PgPool, community_id: &str, signer: &str) -> Result<(), String> {
    let role = load_role(pool, community_id, signer)
        .await
        .map_err(|e| e.to_string())?;
    let private = community_is_private(pool, community_id)
        .await
        .map_err(|e| e.to_string())?
        .ok_or_else(|| "community does not exist".to_string())?;
    if !can_like_post(role, private) {
        return Err(format!(
            "{} cannot like/unlike posts in community {}",
            signer, community_id
        ));
    }
    Ok(())
}

/// `communityPlaces.validateOwnership(placeIds, owner)` — the signer must own
/// every place. Skipped when the places API is unconfigured (no oracle).
async fn gate_places_ownership(
    state: &AppState,
    place_ids: &[String],
    signer: &str,
) -> Result<(), String> {
    use crate::ports::places_api::PlacesError;
    if place_ids.is_empty() || !state.places_api.is_configured() {
        return Ok(());
    }
    match state.places_api.validate_ownership(place_ids, signer).await {
        Ok(_) | Err(PlacesError::Unconfigured) => Ok(()),
        Err(PlacesError::NotOwner(msg)) => Err(msg),
        Err(PlacesError::Upstream(msg)) => Err(format!("place ownership check failed: {msg}")),
    }
}

async fn apply_envelope(state: &AppState, env: &GossipEnvelope) -> Result<(), String> {
    if env.scope != Scope::Communities {
        return Err(format!("unexpected scope {:?}", env.scope));
    }
    let pool = &state.pool;
    let me = |e: crate::http::ApiError| e.to_string();

    match env.primary_type.as_str() {
        CommunityCreate::PRIMARY_TYPE => {
            let signed = decode::<CommunityCreate>(env)?;
            let signer = preverify(state, &signed).await?;
            // Upstream `createCommunity` gate: the owner must hold at least one
            // claimed DCL name. The HTTP fed path enforces this (writes.rs:254);
            // the gossip consumer must too, else a nameless address can create a
            // community via federation (authorization bypass).
            if let Some(false) = state.profiles.has_owned_name(&signer).await {
                return Err(format!("The user {} doesn't have any names", signer));
            }
            apply::apply_create(pool, &signed, &signer)
                .await
                .map_err(me)?;
        }
        CommunityUpdate::PRIMARY_TYPE => {
            let signed = decode::<CommunityUpdate>(env)?;
            let signer = preverify(state, &signed).await?;
            // Same permission ladder as `run_community_update`: always
            // `edit_info`; `edit_name` on a name change; `edit_settings` on a
            // privacy/visibility change.
            gate_permission(
                pool,
                &signed.message.community_id,
                &signer,
                Permission::EditInfo,
            )
            .await?;
            if signed.message.name.is_some() {
                gate_permission(
                    pool,
                    &signed.message.community_id,
                    &signer,
                    Permission::EditName,
                )
                .await?;
            }
            if signed.message.private.is_some() || signed.message.unlisted.is_some() {
                gate_permission(
                    pool,
                    &signed.message.community_id,
                    &signer,
                    Permission::EditSettings,
                )
                .await?;
            }
            apply::apply_update(pool, &signed).await.map_err(me)?;
        }
        CommunityDelete::PRIMARY_TYPE => {
            let signed = decode::<CommunityDelete>(env)?;
            let signer = preverify(state, &signed).await?;
            require_min_role(pool, &signed.message.community_id, &signer, Role::Owner)
                .await
                .map_err(me)?;
            apply::apply_delete(pool, &signed).await.map_err(me)?;
        }
        CommunityJoin::PRIMARY_TYPE => {
            let signed = decode::<CommunityJoin>(env)?;
            let signer = preverify(state, &signed).await?;
            if !community_exists(pool, &signed.message.community_id)
                .await
                .map_err(me)?
            {
                return Err("community does not exist".to_string());
            }
            if load_role(pool, &signed.message.community_id, &signer)
                .await
                .map_err(me)?
                == Role::Banned
            {
                return Err("signer is banned".to_string());
            }
            apply::apply_join(pool, &signed, &signer)
                .await
                .map_err(me)?;
        }
        CommunityLeave::PRIMARY_TYPE => {
            let signed = decode::<CommunityLeave>(env)?;
            let signer = preverify(state, &signed).await?;
            // Upstream `validatePermissionToLeaveCommunity`: an owner cannot
            // leave their own community without transferring ownership first.
            if load_role(pool, &signed.message.community_id, &signer)
                .await
                .map_err(me)?
                == Role::Owner
            {
                return Err(format!(
                    "the owner cannot leave the community {}",
                    signed.message.community_id
                ));
            }
            apply::apply_leave(pool, &signed, &signer)
                .await
                .map_err(me)?;
        }
        CommunityRole::PRIMARY_TYPE => {
            let signed = decode::<CommunityRole>(env)?;
            let signer = preverify(state, &signed).await?;
            // Mirror upstream `validatePermissionToUpdateMemberRole` over the
            // three-role matrix (owner/moderator/member; no `admin` tier): the
            // assigned role must be member|moderator, the actor cannot change
            // their own role, must hold `assign_roles` (owner-only) and be able
            // to act on the target.
            if !matches!(
                Role::parse(&signed.message.role),
                Some(Role::Member) | Some(Role::Mod)
            ) {
                return Err(format!("invalid role '{}'", signed.message.role));
            }
            if signed.message.target.eq_ignore_ascii_case(&signer) {
                return Err("a user cannot update their own role".to_string());
            }
            let actor_role = load_role(pool, &signed.message.community_id, &signer)
                .await
                .map_err(me)?;
            let target_role = load_role(pool, &signed.message.community_id, &signed.message.target)
                .await
                .map_err(me)?;
            if !has_permission(actor_role, Permission::AssignRoles)
                || !can_act_on_member(actor_role, target_role)
            {
                return Err(format!(
                    "actor {} cannot assign roles for this member",
                    actor_role.as_str()
                ));
            }
            apply::apply_role(pool, &signed, &signer)
                .await
                .map_err(me)?;
        }
        CommunityBan::PRIMARY_TYPE => {
            let signed = decode::<CommunityBan>(env)?;
            let signer = preverify(state, &signed).await?;
            require_min_role(pool, &signed.message.community_id, &signer, Role::Mod)
                .await
                .map_err(me)?;
            apply::apply_ban(pool, &signed, &signer).await.map_err(me)?;
        }
        CommunityUnban::PRIMARY_TYPE => {
            let signed = decode::<CommunityUnban>(env)?;
            let signer = preverify(state, &signed).await?;
            // Upstream `validatePermissionToUnbanMemberFromCommunity`: `ban_players`
            // plus the superiority clause (bypassed for the usual `banned` target).
            let actor_role = load_role(pool, &signed.message.community_id, &signer)
                .await
                .map_err(me)?;
            let target_role = load_role(pool, &signed.message.community_id, &signed.message.target)
                .await
                .map_err(me)?;
            if !has_permission(actor_role, Permission::BanPlayers)
                || (!can_act_on_member(actor_role, target_role) && is_member(target_role))
            {
                return Err(format!(
                    "{} doesn't have permission to unban {}",
                    signer, signed.message.target
                ));
            }
            apply::apply_unban(pool, &signed, &signer)
                .await
                .map_err(me)?;
        }
        CommunityPlacesAdd::PRIMARY_TYPE => {
            let signed = decode::<CommunityPlacesAdd>(env)?;
            let signer = preverify(state, &signed).await?;
            // Upstream `validateAndAddPlaces`: `add_places` then place ownership.
            gate_permission(
                pool,
                &signed.message.community_id,
                &signer,
                Permission::AddPlaces,
            )
            .await?;
            gate_places_ownership(state, &signed.message.place_ids, &signer).await?;
            apply::apply_places_add(pool, &signed, &signer)
                .await
                .map_err(me)?;
        }
        CommunityPlaceRemove::PRIMARY_TYPE => {
            let signed = decode::<CommunityPlaceRemove>(env)?;
            let signer = preverify(state, &signed).await?;
            // Upstream `removePlace`: validate ownership; non-owners then need
            // `remove_places` (owners always may).
            gate_places_ownership(
                state,
                std::slice::from_ref(&signed.message.place_id),
                &signer,
            )
            .await?;
            if load_role(pool, &signed.message.community_id, &signer)
                .await
                .map_err(me)?
                != Role::Owner
            {
                gate_permission(
                    pool,
                    &signed.message.community_id,
                    &signer,
                    Permission::RemovePlaces,
                )
                .await?;
            }
            apply::apply_place_remove(pool, &signed, &signer)
                .await
                .map_err(me)?;
        }
        CommunityPost::PRIMARY_TYPE => {
            let signed = decode::<CommunityPost>(env)?;
            let signer = preverify(state, &signed).await?;
            // Upstream `validatePermissionToCreatePost` = `create_posts`:
            // owner/moderator only (plain members do not have it).
            gate_permission(
                pool,
                &signed.message.community_id,
                &signer,
                Permission::CreatePosts,
            )
            .await?;
            apply::apply_post(pool, &signed, &signer)
                .await
                .map_err(me)?;
        }
        CommunityPostDelete::PRIMARY_TYPE => {
            let signed = decode::<CommunityPostDelete>(env)?;
            let signer = preverify(state, &signed).await?;
            // Upstream `validatePermissionToDeletePost`: the deleter must hold
            // `delete_posts` (owner/moderator — a member author CANNOT delete
            // their own post); a moderator may delete only their OWN posts.
            let author: Option<String> = sqlx::query_as::<_, (String,)>(
                "SELECT author FROM community_posts_log WHERE signature_hash = $1",
            )
            .bind(&signed.message.post_id)
            .fetch_optional(pool)
            .await
            .map_err(|e| e.to_string())?
            .map(|(a,)| a);
            let is_author = author
                .as_deref()
                .map(|a| a.eq_ignore_ascii_case(&signer))
                .unwrap_or(false);
            let role = load_role(pool, &signed.message.community_id, &signer)
                .await
                .map_err(me)?;
            if !can_delete_post(role, is_author) {
                return Err(format!(
                    "{} doesn't have permission to delete posts from the community",
                    signer
                ));
            }
            apply::apply_post_delete(pool, &signed).await.map_err(me)?;
        }
        CommunityPostLike::PRIMARY_TYPE => {
            let signed = decode::<CommunityPostLike>(env)?;
            let signer = preverify(state, &signed).await?;
            gate_like(pool, &signed.message.community_id, &signer).await?;
            apply::apply_post_like(pool, &signed, &signer)
                .await
                .map_err(me)?;
        }
        CommunityPostUnlike::PRIMARY_TYPE => {
            let signed = decode::<CommunityPostUnlike>(env)?;
            let signer = preverify(state, &signed).await?;
            gate_like(pool, &signed.message.community_id, &signer).await?;
            apply::apply_post_unlike(pool, &signed, &signer)
                .await
                .map_err(me)?;
        }
        CommunityRequestStatusUpdate::PRIMARY_TYPE => {
            let signed = decode::<CommunityRequestStatusUpdate>(env)?;
            let signer = preverify(state, &signed).await?;
            require_min_role(pool, &signed.message.community_id, &signer, Role::Mod)
                .await
                .map_err(me)?;
            apply::apply_request_status(pool, &signed, &signer)
                .await
                .map_err(me)?;
        }
        other => return Err(format!("unknown primary_type '{other}'")),
    }
    Ok(())
}
