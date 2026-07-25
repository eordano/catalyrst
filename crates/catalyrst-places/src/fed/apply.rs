use catalyrst_fed::Signed;
use serde_json::json;

use crate::fed::messages::{PlaceFavorite, PlaceFavoriteAction, PlaceReport, PlaceVote};
use crate::http::errors::ApiError;
use crate::ports::places::PlaceRow;
use crate::AppState;

fn hex32(bytes: &[u8; 32]) -> String {
    let mut s = String::with_capacity(64);
    for b in bytes {
        s.push_str(&format!("{:02x}", b));
    }
    s
}

async fn resolve_place(state: &AppState, place_id: &str) -> Result<PlaceRow, ApiError> {
    state
        .places
        .find_by_id(place_id)
        .await?
        .ok_or_else(|| ApiError::not_found(format!("Not found place \"{}\"", place_id)))
}

pub struct Applied {
    pub signature_hash: String,

    pub fresh: bool,
}

pub async fn apply_favorite(
    state: &AppState,
    signed: &Signed<PlaceFavorite>,
    signer: &str,
    origin_peer: Option<&str>,
) -> Result<(Applied, i32, bool), ApiError> {
    let place = resolve_place(state, &signed.message.place_id).await?;
    let sig_hash = hex32(&signed.hash());
    let favorite = signed.message.action == PlaceFavoriteAction::Add;

    let fresh = state
        .places
        .record_signed_action(
            &sig_hash,
            signer,
            &signed.message.place_id,
            if favorite { "favorite" } else { "unfavorite" },
            &json!({ "place_id": signed.message.place_id, "action": if favorite {"add"} else {"remove"}, "signed_at": signed.message.signed_at }),
            signed.signed_at,
            origin_peer,
        )
        .await?;

    let (count, user_favorite) = state
        .places
        .set_favorite(
            &signed.message.place_id,
            signer,
            favorite,
            place.favorites,
            place.user_favorite,
        )
        .await?;
    Ok((
        Applied {
            signature_hash: sig_hash,
            fresh,
        },
        count,
        user_favorite,
    ))
}

pub async fn apply_vote(
    state: &AppState,
    signed: &Signed<PlaceVote>,
    signer: &str,
    origin_peer: Option<&str>,
) -> Result<(Applied, i32, i32, bool, bool), ApiError> {
    let place = resolve_place(state, &signed.message.place_id).await?;
    let sig_hash = hex32(&signed.hash());
    let like_req = match signed.message.score {
        s if s > 0 => Some(true),
        s if s < 0 => Some(false),
        _ => None,
    };
    let action_type = match like_req {
        Some(true) => "vote_up",
        Some(false) => "vote_down",
        None => "vote_withdraw",
    };

    let fresh = state
        .places
        .record_signed_action(
            &sig_hash,
            signer,
            &signed.message.place_id,
            action_type,
            &json!({ "place_id": signed.message.place_id, "score": signed.message.score, "signed_at": signed.message.signed_at }),
            signed.signed_at,
            origin_peer,
        )
        .await?;

    let user_activity = match like_req {
        Some(_) => crate::snapshot::fetch_score(signer).await,
        None => 0.0,
    };
    let (likes, dislikes, user_like, user_dislike) = state
        .places
        .set_like(
            &signed.message.place_id,
            signer,
            like_req,
            user_activity,
            place.likes,
            place.dislikes,
            place.user_like,
            place.user_dislike,
        )
        .await?;
    Ok((
        Applied {
            signature_hash: sig_hash,
            fresh,
        },
        likes,
        dislikes,
        user_like,
        user_dislike,
    ))
}

pub async fn apply_report(
    state: &AppState,
    signed: &Signed<PlaceReport>,
    signer: &str,
    origin_peer: Option<&str>,
) -> Result<Applied, ApiError> {
    if origin_peer.is_none() {
        resolve_place(state, &signed.message.place_id).await?;
    }
    let sig_hash = hex32(&signed.hash());
    let reason = serde_json::to_value(signed.message.reason).unwrap_or_else(|_| json!("other"));

    let fresh = state
        .places
        .record_signed_action(
            &sig_hash,
            signer,
            &signed.message.place_id,
            "report",
            &json!({ "place_id": signed.message.place_id, "reason": reason, "signed_at": signed.message.signed_at }),
            signed.signed_at,
            origin_peer,
        )
        .await?;

    let payload = json!({
        "entity_id": signed.message.place_id,
        "reason": reason,
        "signed": true,
        "signature_hash": sig_hash,
    });
    state
        .places
        .record_report(
            Some(&signed.message.place_id),
            signer,
            "",
            &sig_hash,
            &payload,
        )
        .await?;

    Ok(Applied {
        signature_hash: sig_hash,
        fresh,
    })
}
