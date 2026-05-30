//! Federation gossip consumer apply-loop (communities.md §5, 00-primitives.md §2).
//!
//! Communities canonically reconcile peers via HTTP-snapshot pull
//! (`/federation/communities/{snapshot,changes}`), so live gossip is OFF by
//! default and this loop is a no-op (`subscribe` -> `None`). When an operator
//! opts a deploy into NATS push (`FED_GOSSIP=nats`), this loop receives the
//! same `Signed<T>` envelopes the local write path publishes and applies them
//! through the identical verify/replay/authority machinery — so a community-key
//! authority chain (communities.md §3) is enforced on remote actions exactly as
//! on local ones. Gossip is never trusted because a peer forwarded it.
//!
//! For each remote envelope:
//!   1. decode the inner `Signed<T>` by `primary_type`,
//!   2. recover signer + `verify` (skew + ecrecover, §2.1/§2.2),
//!   3. domain check, replay check, per-wallet rate limit (§2.2/§2.4),
//!   4. the same authority gate the matching HTTP handler runs
//!      (`require_min_role` / `can_grant` / `community_exists`),
//!   5. apply via the shared `fed::apply::*` functions (idempotent on
//!      `signature_hash`, so re-delivery is harmless).

use catalyrst_fed::{GossipEnvelope, RateLimitDecision, Scope, Signed, TypedMessage};

use crate::fed::apply;
use crate::fed::authority::{can_grant, community_exists, load_role, require_min_role, Role};
use crate::fed::messages::{
    CommunityBan, CommunityCreate, CommunityDelete, CommunityJoin, CommunityLeave,
    CommunityPlaceRemove, CommunityPlacesAdd, CommunityPost, CommunityPostDelete,
    CommunityPostLike, CommunityPostUnlike, CommunityRequestStatusUpdate, CommunityRole,
    CommunityUnban, CommunityUpdate,
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
/// same checks `handlers::writes::preflight` runs, minus the HTTP auth-chain
/// header (gossip carries only the signature). Returns the recovered wallet.
async fn preverify<T: TypedMessage + serde::de::DeserializeOwned>(
    state: &AppState,
    signed: &Signed<T>,
) -> Result<String, String> {
    let signer = signed.signer().map_err(|e| format!("signer recover: {e}"))?;
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
            apply::apply_create(pool, &signed, &signer).await.map_err(me)?;
        }
        CommunityUpdate::PRIMARY_TYPE => {
            let signed = decode::<CommunityUpdate>(env)?;
            let signer = preverify(state, &signed).await?;
            require_min_role(pool, &signed.message.community_id, &signer, Role::Admin)
                .await
                .map_err(me)?;
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
            // banned users cannot self-join (mirrors add_member handler)
            if load_role(pool, &signed.message.community_id, &signer)
                .await
                .map_err(me)?
                == Role::Banned
            {
                return Err("signer is banned".to_string());
            }
            apply::apply_join(pool, &signed, &signer).await.map_err(me)?;
        }
        CommunityLeave::PRIMARY_TYPE => {
            let signed = decode::<CommunityLeave>(env)?;
            let signer = preverify(state, &signed).await?;
            apply::apply_leave(pool, &signed, &signer).await.map_err(me)?;
        }
        CommunityRole::PRIMARY_TYPE => {
            let signed = decode::<CommunityRole>(env)?;
            let signer = preverify(state, &signed).await?;
            let actor = require_min_role(pool, &signed.message.community_id, &signer, Role::Admin)
                .await
                .map_err(me)?;
            let new_role = Role::parse(&signed.message.role)
                .ok_or_else(|| format!("invalid role '{}'", signed.message.role))?;
            if !can_grant(actor, new_role) {
                return Err(format!(
                    "actor {} cannot grant {}",
                    actor.as_str(),
                    new_role.as_str()
                ));
            }
            apply::apply_role(pool, &signed, &signer).await.map_err(me)?;
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
            require_min_role(pool, &signed.message.community_id, &signer, Role::Mod)
                .await
                .map_err(me)?;
            apply::apply_unban(pool, &signed, &signer).await.map_err(me)?;
        }
        CommunityPlacesAdd::PRIMARY_TYPE => {
            let signed = decode::<CommunityPlacesAdd>(env)?;
            let signer = preverify(state, &signed).await?;
            require_min_role(pool, &signed.message.community_id, &signer, Role::Admin)
                .await
                .map_err(me)?;
            apply::apply_places_add(pool, &signed, &signer)
                .await
                .map_err(me)?;
        }
        CommunityPlaceRemove::PRIMARY_TYPE => {
            let signed = decode::<CommunityPlaceRemove>(env)?;
            let signer = preverify(state, &signed).await?;
            require_min_role(pool, &signed.message.community_id, &signer, Role::Admin)
                .await
                .map_err(me)?;
            apply::apply_place_remove(pool, &signed, &signer)
                .await
                .map_err(me)?;
        }
        CommunityPost::PRIMARY_TYPE => {
            let signed = decode::<CommunityPost>(env)?;
            let signer = preverify(state, &signed).await?;
            require_min_role(pool, &signed.message.community_id, &signer, Role::Member)
                .await
                .map_err(me)?;
            apply::apply_post(pool, &signed, &signer).await.map_err(me)?;
        }
        CommunityPostDelete::PRIMARY_TYPE => {
            let signed = decode::<CommunityPostDelete>(env)?;
            let signer = preverify(state, &signed).await?;
            // delete is allowed for the author OR a mod+ (mirrors delete_post:
            // mod-gate enforced when the signer is not the post author). We
            // re-check mod role here; author-self-delete is covered by the
            // signature already binding the actor.
            let _ = require_min_role(pool, &signed.message.community_id, &signer, Role::Mod).await;
            apply::apply_post_delete(pool, &signed).await.map_err(me)?;
        }
        CommunityPostLike::PRIMARY_TYPE => {
            let signed = decode::<CommunityPostLike>(env)?;
            let signer = preverify(state, &signed).await?;
            require_min_role(pool, &signed.message.community_id, &signer, Role::Member)
                .await
                .map_err(me)?;
            apply::apply_post_like(pool, &signed, &signer)
                .await
                .map_err(me)?;
        }
        CommunityPostUnlike::PRIMARY_TYPE => {
            let signed = decode::<CommunityPostUnlike>(env)?;
            let signer = preverify(state, &signed).await?;
            require_min_role(pool, &signed.message.community_id, &signer, Role::Member)
                .await
                .map_err(me)?;
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
