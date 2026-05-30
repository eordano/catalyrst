//! Places federation write path (docs/federation/places.md).
//!
//! Lifecycle for a place opinion (favorite / vote-like / report):
//!   1. verify the `Signed<T>` envelope (00-primitives.md §2.1) — done by the
//!      caller via `Signed::verify`,
//!   2. replay-check the nonce (§2.2) — [`crate::fed::replay`],
//!   3. namespace-anchor check: the `place_id` must resolve to a known place
//!      in the local store (places.md §3 "Namespace anchor"),
//!   4. append to the `signed_actions_places` log + materialise the current
//!      view (user_favorites / user_likes / place_reports_local),
//!   5. emit the action to gossip (places.md §4: NATS subject
//!      `fed.places.actions`).
//!
//! This module owns steps 3-4; the handler owns 1-2 and 5 so it can shape the
//! HTTP response. Remote envelopes arriving over gossip re-enter at step 1.

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

/// Namespace anchor (places.md §3): reject any signed action whose `place_id`
/// does not resolve to a place in the local store. Returns the resolved row.
async fn resolve_place(state: &AppState, place_id: &str) -> Result<PlaceRow, ApiError> {
    state
        .places
        .find_by_id(place_id)
        .await?
        .ok_or_else(|| ApiError::not_found(format!("Not found place \"{}\"", place_id)))
}

pub struct Applied {
    pub signature_hash: String,
    /// false if this signature_hash was already logged (idempotent replay of an
    /// already-applied action, e.g. re-delivered gossip).
    pub fresh: bool,
}

/// Apply a signed favorite toggle. `origin_peer` is None for local clients.
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

    // materialise current view (last-write-wins is naturally handled by the
    // toggle semantics of set_favorite over user_favorites).
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
    Ok((Applied { signature_hash: sig_hash, fresh }, count, user_favorite))
}

/// Apply a signed vote/like. score: +1 like, -1 dislike, 0 withdraw.
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

    let (likes, dislikes, user_like, user_dislike) = state
        .places
        .set_like(
            &signed.message.place_id,
            signer,
            like_req,
            place.likes,
            place.dislikes,
            place.user_like,
            place.user_dislike,
        )
        .await?;
    Ok((
        Applied { signature_hash: sig_hash, fresh },
        likes,
        dislikes,
        user_like,
        user_dislike,
    ))
}

/// Apply a signed abuse report. Advisory-only per places.md §3/§8 — logged but
/// surfacing of foreign reports stays per-catalyst.
pub async fn apply_report(
    state: &AppState,
    signed: &Signed<PlaceReport>,
    signer: &str,
    origin_peer: Option<&str>,
) -> Result<Applied, ApiError> {
    // place_id is anchored, but we tolerate reports on places not yet synced
    // locally (the report is advisory); only enforce anchor for local clients.
    if origin_peer.is_none() {
        resolve_place(state, &signed.message.place_id).await?;
    }
    let sig_hash = hex32(&signed.hash());
    let reason = serde_json::to_value(signed.message.reason)
        .unwrap_or_else(|_| json!("other"));

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

    // also persist into the legacy place_reports_local for the existing
    // /report read surface.
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
            "", // no signed_url for federation-native reports
            &sig_hash,
            &payload,
        )
        .await?;

    Ok(Applied { signature_hash: sig_hash, fresh })
}
