use catalyrst_fed::consumer::{decode_signed, preverify_signed, spawn_gossip_consumer};
use catalyrst_fed::{GossipEnvelope, Scope, Signed, TypedMessage};

use crate::fed::apply;
use crate::fed::authority::{is_moderator, require_moderator, settings_write_allowed};
use crate::fed::messages::{ProfileSettingsUpdate, ScheduleUpsert};
use crate::fed::replay;
use crate::AppState;

pub async fn spawn(state: AppState) {
    let gossip = state.gossip.clone();
    spawn_gossip_consumer(gossip, Scope::Events, move |env| {
        let state = state.clone();
        async move { apply_envelope(&state, &env).await }
    })
    .await;
}

async fn apply_envelope(state: &AppState, env: &GossipEnvelope) -> Result<(), String> {
    let origin = env.origin_peer.as_deref();
    match env.primary_type.as_str() {
        ProfileSettingsUpdate::PRIMARY_TYPE => {
            let signed = decode_signed::<ProfileSettingsUpdate>(env)?;
            let signer = preverify(state, &signed).await?;

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
            let signed = decode_signed::<ScheduleUpsert>(env)?;
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

async fn preverify<T: TypedMessage>(
    state: &AppState,
    signed: &Signed<T>,
) -> Result<String, String> {
    preverify_signed(
        signed,
        &state.domain.name,
        |signer, nonce, signed_at| async move {
            replay::check_and_record(&state.pool, &signer, &nonce, signed_at).await
        },
    )
    .await
}
