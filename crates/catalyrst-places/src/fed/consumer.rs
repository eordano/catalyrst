//! Federation gossip consumer apply-loop (places.md §4, 00-primitives.md §2).
//!
//! The publish side is wired in `handlers::federation` (local writes emit a
//! [`GossipEnvelope`] on `fed.places.actions`). This module is the receive
//! side: it subscribes to the same subject, and for every remote envelope it
//! re-runs the FULL local-write verification path before applying —
//!
//!   1. deserialise the inner `Signed<T>` from `signed_json` by `primary_type`,
//!   2. recover the signer from the signature + `verify` (skew + ecrecover,
//!      00-primitives.md §2.1/§2.2),
//!   3. domain-name check against this catalyst's place domain,
//!   4. replay check (§2.2) against the shared `seen_nonces` table,
//!   5. apply via the same `fed::apply::*` machinery a local write uses, with
//!      `origin_peer = Some(..)` so the report-anchor relaxation (apply.rs)
//!      applies and the action is logged with its source peer.
//!
//! Gossip is NEVER trusted because a peer forwarded it — a forged or replayed
//! envelope is rejected exactly as a forged local request would be.
//!
//! Single-node correctness: with no peers, [`GossipPublisher::subscribe`]
//! returns `None` (NoopPublisher) and no loop is spawned — a harmless no-op, as
//! the task requires. With `FED_GOSSIP=nats` + peers, the loop is live.

use catalyrst_fed::{GossipEnvelope, Scope, Signed, TypedMessage};

use crate::fed::apply;
use crate::fed::messages::{PlaceFavorite, PlaceReport, PlaceVote};
use crate::fed::replay;
use crate::AppState;

/// Spawn the places gossip apply-loop if the transport reaches peers.
/// Returns immediately; the loop runs on a detached task for the process
/// lifetime. A no-op (returns without spawning) when gossip is disabled.
pub async fn spawn(state: AppState) {
    let rx = match state.gossip.subscribe(Scope::Places).await {
        Ok(Some(rx)) => rx,
        Ok(None) => {
            tracing::info!(
                "places gossip consumer not started (transport reaches no peers; \
                 peers reconcile via snapshot pull)"
            );
            return;
        }
        Err(e) => {
            tracing::error!(error = %e, "places gossip subscribe failed; consumer not started");
            return;
        }
    };
    tracing::info!("places gossip consumer started (fed.places.actions)");
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
                "places gossip envelope rejected"
            );
        }
    }
    tracing::warn!("places gossip consumer channel closed; loop exiting");
}

/// Re-verify (decode → recover → verify → domain → replay) then apply.
async fn apply_envelope(state: &AppState, env: &GossipEnvelope) -> Result<(), String> {
    if env.scope != Scope::Places {
        return Err(format!("unexpected scope {:?}", env.scope));
    }
    let origin = env.origin_peer.as_deref();
    match env.primary_type.as_str() {
        PlaceFavorite::PRIMARY_TYPE => {
            let signed = decode::<PlaceFavorite>(env)?;
            let signer = preverify(state, &signed).await?;
            apply::apply_favorite(state, &signed, &signer, origin)
                .await
                .map_err(|e| e.to_string())?;
        }
        PlaceVote::PRIMARY_TYPE => {
            let signed = decode::<PlaceVote>(env)?;
            let signer = preverify(state, &signed).await?;
            apply::apply_vote(state, &signed, &signer, origin)
                .await
                .map_err(|e| e.to_string())?;
        }
        PlaceReport::PRIMARY_TYPE => {
            let signed = decode::<PlaceReport>(env)?;
            let signer = preverify(state, &signed).await?;
            apply::apply_report(state, &signed, &signer, origin)
                .await
                .map_err(|e| e.to_string())?;
        }
        other => return Err(format!("unknown primary_type '{other}'")),
    }
    Ok(())
}

fn decode<T: TypedMessage + serde::de::DeserializeOwned>(
    env: &GossipEnvelope,
) -> Result<Signed<T>, String> {
    serde_json::from_value::<Signed<T>>(env.signed_json.clone())
        .map_err(|e| format!("decode Signed<{}>: {e}", T::PRIMARY_TYPE))
}

/// The same checks `handlers::federation::preflight` runs for a local request,
/// minus the HTTP auth-chain header (a gossiped action carries only the
/// signature). Returns the recovered wallet signer.
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
    replay::check_and_record(
        state.places.writer_pool(),
        &signer,
        &signed.nonce,
        signed.signed_at,
    )
    .await
    .map_err(|e| format!("replay: {e}"))?;
    Ok(signer)
}
