use catalyrst_fed::{GossipEnvelope, Scope, Signed, TypedMessage};

use crate::fed::apply;
use crate::fed::messages::{PlaceFavorite, PlaceReport, PlaceVote};
use crate::fed::replay;
use crate::AppState;

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
