use std::future::Future;
use std::sync::Arc;

use serde::de::DeserializeOwned;

use crate::error::FedError;
use crate::gossip::{subject_actions, GossipEnvelope, GossipPublisher};
use crate::session::Scope;
use crate::sig::{Signed, TypedMessage};

pub async fn spawn_gossip_consumer<D, Fut>(
    gossip: Arc<dyn GossipPublisher>,
    scope: Scope,
    dispatch: D,
) where
    D: Fn(GossipEnvelope) -> Fut + Send + Sync + 'static,
    Fut: Future<Output = Result<(), String>> + Send + 'static,
{
    let label = scope.as_str();
    let rx = match gossip.subscribe(scope).await {
        Ok(Some(rx)) => rx,
        Ok(None) => {
            tracing::info!(
                "{label} gossip consumer not started (transport reaches no peers; \
                 peers reconcile via snapshot pull)"
            );
            return;
        }
        Err(e) => {
            tracing::error!(error = %e, "{label} gossip subscribe failed; consumer not started");
            return;
        }
    };
    tracing::info!(
        "{} gossip consumer started ({})",
        label,
        subject_actions(scope)
    );
    tokio::spawn(run(scope, rx, dispatch));
}

async fn run<D, Fut>(scope: Scope, mut rx: tokio::sync::mpsc::Receiver<GossipEnvelope>, dispatch: D)
where
    D: Fn(GossipEnvelope) -> Fut + Send + Sync + 'static,
    Fut: Future<Output = Result<(), String>> + Send + 'static,
{
    let label = scope.as_str();
    while let Some(env) = rx.recv().await {
        let primary_type = env.primary_type.clone();
        let signature_hash = env.signature_hash.clone();
        let origin_peer = env.origin_peer.clone();
        let result = if env.scope != scope {
            Err(format!("unexpected scope {:?}", env.scope))
        } else {
            dispatch(env).await
        };
        if let Err(e) = result {
            tracing::warn!(
                error = %e,
                primary_type = %primary_type,
                signature_hash = %signature_hash,
                origin_peer = origin_peer.as_deref().unwrap_or("?"),
                "{label} gossip envelope rejected"
            );
        }
    }
    tracing::warn!("{label} gossip consumer channel closed; loop exiting");
}

pub fn decode_signed<T: TypedMessage + DeserializeOwned>(
    env: &GossipEnvelope,
) -> Result<Signed<T>, String> {
    serde_json::from_value::<Signed<T>>(env.signed_json.clone())
        .map_err(|e| format!("decode Signed<{}>: {e}", T::PRIMARY_TYPE))
}

pub async fn preverify_signed<T, F, Fut>(
    signed: &Signed<T>,
    domain_name: &str,
    replay: F,
) -> Result<String, String>
where
    T: TypedMessage,
    F: FnOnce(String, [u8; 16], i64) -> Fut,
    Fut: Future<Output = Result<(), FedError>>,
{
    let signer = signed
        .signer()
        .map_err(|e| format!("signer recover: {e}"))?;
    let now = chrono::Utc::now().timestamp();
    signed
        .verify(&signer, now)
        .map_err(|e| format!("verify: {e}"))?;
    if !signed.domain.name.eq_ignore_ascii_case(domain_name) {
        return Err(format!("domain mismatch: expected {domain_name}"));
    }
    replay(signer.clone(), signed.nonce, signed.signed_at)
        .await
        .map_err(|e| format!("replay: {e}"))?;
    Ok(signer)
}
