use catalyrst_fed::{GossipEnvelope, RateLimitDecision, Scope, Signed, TypedMessage};

use crate::rest::community_membership_authority::{
    load_standing_from_community_role_current, operator_log_message_for_refusal,
    CommunityBanAuthority, CommunityMembershipTier, CommunityUnbanAuthority,
};
use crate::rest::fed::apply;
use crate::rest::fed::authority::{
    community_exists, community_is_private, FederatedCommunityWriteAuthority,
};
use crate::rest::fed::messages::{
    CommunityBan, CommunityCreate, CommunityDelete, CommunityJoin, CommunityLeave,
    CommunityPlaceRemove, CommunityPlacesAdd, CommunityPost, CommunityPostDelete,
    CommunityPostLike, CommunityPostUnlike, CommunityRequestStatusUpdate, CommunityRole,
    CommunityUnban, CommunityUpdate,
};
use crate::rest::handlers::permissions::{
    can_act_on_member, can_delete_post, can_like_post, has_permission, Permission,
};
use crate::rest::AppState;

pub async fn spawn(state: AppState) {
    let rx = match state.gossip.subscribe(Scope::Communities).await {
        Ok(Some(rx)) => rx,
        Ok(None) => {
            tracing::info!(
                "communities gossip consumer not started (transport reaches no peers; \
                 peers reconcile via HTTP-snapshot pull \u{2014} communities.md \u{A7}5)"
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

async fn gate_permission(
    pool: &sqlx::PgPool,
    community_id: &str,
    signer: &str,
    permission: Permission,
) -> Result<(), String> {
    let standing = load_standing_from_community_role_current(pool, community_id, signer)
        .await
        .map_err(|refusal| operator_log_message_for_refusal(&refusal))?;
    if standing.tier() == CommunityMembershipTier::BannedFromThisCommunity {
        return Err("signer is banned".to_string());
    }
    if !standing.holds_capability_within_this_community(permission) {
        return Err(format!(
            "signer {} lacks permission {:?}",
            standing.tier().as_canonical_stored_role_text(),
            permission
        ));
    }
    Ok(())
}

async fn gate_like(pool: &sqlx::PgPool, community_id: &str, signer: &str) -> Result<(), String> {
    let standing = load_standing_from_community_role_current(pool, community_id, signer)
        .await
        .map_err(|refusal| operator_log_message_for_refusal(&refusal))?;
    let private = community_is_private(pool, community_id)
        .await
        .map_err(|e| e.to_string())?
        .ok_or_else(|| "community does not exist".to_string())?;
    if !can_like_post(standing.tier(), private) {
        return Err(format!(
            "{} cannot like/unlike posts in community {}",
            signer, community_id
        ));
    }
    Ok(())
}

async fn gate_places_ownership(
    state: &AppState,
    place_ids: &[String],
    signer: &str,
) -> Result<(), String> {
    use crate::rest::ports::places_api::PlacesError;
    if place_ids.is_empty() || !state.places_api.is_configured() {
        return Ok(());
    }
    match state.places_api.validate_ownership(place_ids, signer).await {
        Ok(_) | Err(PlacesError::Unconfigured) => Ok(()),
        Err(PlacesError::NotOwner(msg)) => Err(msg),
        Err(PlacesError::Upstream(msg)) => Err(format!("place ownership check failed: {msg}")),
    }
}

pub async fn apply_envelope(state: &AppState, env: &GossipEnvelope) -> Result<(), String> {
    if env.scope != Scope::Communities {
        return Err(format!("unexpected scope {:?}", env.scope));
    }
    let pool = &state.pool;
    let me = |e: crate::rest::http::ApiError| e.to_string();

    match env.primary_type.as_str() {
        CommunityCreate::PRIMARY_TYPE => {
            let signed = decode::<CommunityCreate>(env)?;
            let signer = preverify(state, &signed).await?;

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
            FederatedCommunityWriteAuthority::resolve_requiring_at_least(
                pool,
                &signed.message.community_id,
                &signer,
                CommunityMembershipTier::OwnerOfThisCommunity,
            )
            .await
            .map_err(|refusal| operator_log_message_for_refusal(&refusal))?;
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
            if load_standing_from_community_role_current(
                pool,
                &signed.message.community_id,
                &signer,
            )
            .await
            .map_err(|refusal| operator_log_message_for_refusal(&refusal))?
            .tier()
                == CommunityMembershipTier::BannedFromThisCommunity
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

            if load_standing_from_community_role_current(
                pool,
                &signed.message.community_id,
                &signer,
            )
            .await
            .map_err(|refusal| operator_log_message_for_refusal(&refusal))?
            .tier()
                == CommunityMembershipTier::OwnerOfThisCommunity
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

            if !matches!(
                CommunityMembershipTier::parse_role_text_supplied_in_a_request(
                    &signed.message.role
                ),
                Some(CommunityMembershipTier::OrdinaryMemberOfThisCommunity)
                    | Some(CommunityMembershipTier::ModeratorOfThisCommunity)
            ) {
                return Err(format!("invalid role '{}'", signed.message.role));
            }
            if signed.message.target.eq_ignore_ascii_case(&signer) {
                return Err("a user cannot update their own role".to_string());
            }
            let actor_standing = load_standing_from_community_role_current(
                pool,
                &signed.message.community_id,
                &signer,
            )
            .await
            .map_err(|refusal| operator_log_message_for_refusal(&refusal))?;
            let target_standing = load_standing_from_community_role_current(
                pool,
                &signed.message.community_id,
                &signed.message.target,
            )
            .await
            .map_err(|refusal| operator_log_message_for_refusal(&refusal))?;
            if !has_permission(actor_standing.tier(), Permission::AssignRoles)
                || !can_act_on_member(actor_standing.tier(), target_standing.tier())
            {
                return Err(format!(
                    "actor {} cannot assign roles for this member",
                    actor_standing.tier().as_canonical_stored_role_text()
                ));
            }
            apply::apply_role(pool, &signed, &signer)
                .await
                .map_err(me)?;
        }
        CommunityBan::PRIMARY_TYPE => {
            let signed = decode::<CommunityBan>(env)?;
            let signer = preverify(state, &signed).await?;
            CommunityBanAuthority::resolve_from_gossip_envelope_relayed_by_a_peer_catalyst_server(
                pool,
                &signer,
                &signed.message.community_id,
                &signed.message.target,
            )
            .await
            .map_err(|refusal| operator_log_message_for_refusal(&refusal))?;
            apply::apply_ban(pool, &signed, &signer).await.map_err(me)?;
            state
                .evict_from_private_community_voice(
                    crate::rest::fed::ids::community_uuid_from_hex(&signed.message.community_id),
                    &signed.message.target.to_lowercase(),
                )
                .await;
        }
        CommunityUnban::PRIMARY_TYPE => {
            let signed = decode::<CommunityUnban>(env)?;
            let signer = preverify(state, &signed).await?;

            CommunityUnbanAuthority::resolve_from_gossip_envelope_relayed_by_a_peer_catalyst_server(
                pool,
                &signer,
                &signed.message.community_id,
                &signed.message.target,
            )
            .await
            .map_err(|refusal| match refusal {
                // Keep the wording this arm has always logged for a genuine refusal...
                catalyrst_authenticated_principal::AuthorityNotEstablished::
                    RefusedLacksAuthority { .. } => format!(
                    "{} doesn't have permission to unban {}",
                    signer, signed.message.target
                ),
                // ...but never let an outage wear it. That collapse is the defect the
                // typed refusal exists to prevent.
                other => operator_log_message_for_refusal(&other),
            })?;
            apply::apply_unban(pool, &signed, &signer)
                .await
                .map_err(me)?;
        }
        CommunityPlacesAdd::PRIMARY_TYPE => {
            let signed = decode::<CommunityPlacesAdd>(env)?;
            let signer = preverify(state, &signed).await?;

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

            gate_places_ownership(
                state,
                std::slice::from_ref(&signed.message.place_id),
                &signer,
            )
            .await?;
            if load_standing_from_community_role_current(
                pool,
                &signed.message.community_id,
                &signer,
            )
            .await
            .map_err(|refusal| operator_log_message_for_refusal(&refusal))?
            .tier()
                != CommunityMembershipTier::OwnerOfThisCommunity
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

            let author: Option<String> = sqlx::query_as::<_, (String,)>(
                "SELECT author FROM community_posts_log \
                  WHERE signature_hash = $1 AND community_id = $2",
            )
            .bind(&signed.message.post_id)
            .bind(&signed.message.community_id)
            .fetch_optional(pool)
            .await
            .map_err(|e| e.to_string())?
            .map(|(a,)| a);
            let is_author = author
                .as_deref()
                .map(|a| a.eq_ignore_ascii_case(&signer))
                .unwrap_or(false);
            let standing = load_standing_from_community_role_current(
                pool,
                &signed.message.community_id,
                &signer,
            )
            .await
            .map_err(|refusal| operator_log_message_for_refusal(&refusal))?;
            if !can_delete_post(standing.tier(), is_author) {
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
            FederatedCommunityWriteAuthority::resolve_requiring_at_least(
                pool,
                &signed.message.community_id,
                &signer,
                CommunityMembershipTier::ModeratorOfThisCommunity,
            )
            .await
            .map_err(|refusal| operator_log_message_for_refusal(&refusal))?;
            apply::apply_request_status(pool, &signed, &signer)
                .await
                .map_err(me)?;
        }
        other => return Err(format!("unknown primary_type '{other}'")),
    }
    Ok(())
}
