use catalyrst_fed::consumer::{decode_signed, preverify_signed, spawn_gossip_consumer};
use catalyrst_fed::{GossipEnvelope, Scope, Signed, TypedMessage};

use crate::fed::apply;
use crate::fed::messages::{PlaceFavorite, PlaceReport, PlaceVote};
use crate::fed::replay;
use crate::AppState;

pub async fn spawn(state: AppState) {
    let gossip = state.gossip.clone();
    spawn_gossip_consumer(gossip, Scope::Places, move |env| {
        let state = state.clone();
        async move { apply_envelope(&state, &env).await }
    })
    .await;
}

async fn apply_envelope(state: &AppState, env: &GossipEnvelope) -> Result<(), String> {
    let origin = env.origin_peer.as_deref();
    match env.primary_type.as_str() {
        PlaceFavorite::PRIMARY_TYPE => {
            let signed = decode_signed::<PlaceFavorite>(env)?;
            let signer = preverify(state, &signed).await?;
            apply::apply_favorite(state, &signed, &signer, origin)
                .await
                .map_err(|e| e.to_string())?;
        }
        PlaceVote::PRIMARY_TYPE => {
            let signed = decode_signed::<PlaceVote>(env)?;
            let signer = preverify(state, &signed).await?;
            apply::apply_vote(state, &signed, &signer, origin)
                .await
                .map_err(|e| e.to_string())?;
        }
        PlaceReport::PRIMARY_TYPE => {
            let signed = decode_signed::<PlaceReport>(env)?;
            let signer = preverify(state, &signed).await?;
            apply::apply_report(state, &signed, &signer, origin)
                .await
                .map_err(|e| e.to_string())?;
        }
        other => return Err(format!("unknown primary_type '{other}'")),
    }
    Ok(())
}

async fn preverify<T: TypedMessage>(
    state: &AppState,
    signed: &Signed<T>,
) -> Result<String, String> {
    preverify_signed(
        signed,
        &state.domain.name,
        |signer, nonce, signed_at| async move {
            replay::check_and_record(state.places.writer_pool(), &signer, &nonce, signed_at).await
        },
    )
    .await
}
