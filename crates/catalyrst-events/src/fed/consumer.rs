//! Federation gossip consumer apply-loop (events.md §3, 00-primitives.md §2).
//!
//! The publish side lives in `handlers::{profile_settings,schedules}` (local
//! moderator writes emit a [`GossipEnvelope`] on `fed.events.actions`). This is
//! the receive side: for every remote envelope it re-runs the FULL local-write
//! verification path — decode → recover → verify (skew+ecrecover) → domain →
//! replay → authority — before applying. Gossip is never trusted because a peer
//! forwarded it.
//!
//! No-op (returns without spawning) when gossip reaches no peers (single-node
//! default); live under `FED_GOSSIP=nats` with peers.

use catalyrst_fed::{GossipEnvelope, Scope, Signed, TypedMessage};

use crate::fed::apply;
use crate::fed::authority::{is_moderator, require_moderator, settings_write_allowed};
use crate::fed::messages::{ProfileSettingsUpdate, ScheduleUpsert};
use crate::fed::replay;
use crate::AppState;

pub async fn spawn(state: AppState) {
    let rx = match state.gossip.subscribe(Scope::Events).await {
        Ok(Some(rx)) => rx,
        Ok(None) => {
            tracing::info!(
                "events gossip consumer not started (transport reaches no peers; \
                 peers reconcile via snapshot pull)"
            );
            return;
        }
        Err(e) => {
            tracing::error!(error = %e, "events gossip subscribe failed; consumer not started");
            return;
        }
    };
    tracing::info!("events gossip consumer started (fed.events.actions)");
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
                "events gossip envelope rejected"
            );
        }
    }
    tracing::warn!("events gossip consumer channel closed; loop exiting");
}

async fn apply_envelope(state: &AppState, env: &GossipEnvelope) -> Result<(), String> {
    if env.scope != Scope::Events {
        return Err(format!("unexpected scope {:?}", env.scope));
    }
    let origin = env.origin_peer.as_deref();
    match env.primary_type.as_str() {
        ProfileSettingsUpdate::PRIMARY_TYPE => {
            let signed = decode::<ProfileSettingsUpdate>(env)?;
            let signer = preverify(state, &signed).await?;
            // A self-edit needs no moderator status; editing another user does.
            let target = &signed.message.target;
            let mod_status = if target.eq_ignore_ascii_case(&signer) {
                false
            } else {
                is_moderator(&state.pool, &signer)
                    .await
                    .map_err(|e| e.to_string())?
            };
            if !settings_write_allowed(&signer, target, mod_status) {
                return Err("signer not authorized to edit target settings".into());
            }
            apply::apply_profile_settings(&state.pool, &signed, &signer, origin)
                .await
                .map_err(|e| e.to_string())?;
        }
        ScheduleUpsert::PRIMARY_TYPE => {
            let signed = decode::<ScheduleUpsert>(env)?;
            let signer = preverify(state, &signed).await?;
            require_moderator(&state.pool, &signer)
                .await
                .map_err(|e| e.to_string())?;
            apply::apply_schedule(&state.pool, &signed, &signer, origin)
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
    replay::check_and_record(&state.pool, &signer, &signed.nonce, signed.signed_at)
        .await
        .map_err(|e| format!("replay: {e}"))?;
    Ok(signer)
}
